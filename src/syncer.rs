use crate::{
    app_state::AppState,
    aur_fetcher::AurFetcher,
    database::DatabaseOps,
    srcinfo_parse::ParsedSrcInfo,
    types::{DatabasePackageDetails, DatabasePackageInfo},
};
use anyhow::Result;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

const BATCH_SIZE: usize = 3000;

pub struct Syncer {
    db: DatabaseOps,
    fetcher: AurFetcher,
}

struct SrcInfoTuple {
    branch: String,
    commit: String,
    srcinfo_text: String,
}

impl Syncer {
    pub fn new(app_state: AppState) -> Self {
        let fetcher = AurFetcher::new(app_state.github_token);
        Self {
            db: app_state.db,
            fetcher,
        }
    }

    pub async fn sync(&self) -> Result<()> {
        info!("Starting sync operation...");

        if self.fetcher.github_token().is_none() {
            warn!("⚠ No GitHub token configured. You may hit rate limits.");
        }

        info!("Fetching branch list from AUR Mirror...");
        // Fetch branch list
        let branches = self.fetcher.fetch_branch_list().await?;

        info!(
            "Found {} branches, comparing to existing...",
            branches.len()
        );
        let existing_commits = self.db.get_existing_commits().await?;
        let to_process = branches
            .into_iter()
            .filter(|(branch, commit)| existing_commits.get(branch) != Some(commit))
            .collect::<Vec<_>>();

        info!("Need to process {} updated branches", to_process.len());
        if to_process.is_empty() {
            info!("All branches are up to date");
            return Ok(());
        }

        let (db_sender, mut db_receiver) = mpsc::channel::<SrcInfoTuple>(BATCH_SIZE * 2);

        let fetcher = self.fetcher.clone();
        let fetch_task = tokio::spawn(async move {
            for chunk in to_process.chunks(BATCH_SIZE) {
                let commits = chunk.iter().map(|(_, commit)| commit.as_str());
                match fetcher.fetch_srcinfo_batch(commits).await {
                    Ok(srcinfo_data) => {
                        for ((branch, commit), srcinfo_text) in chunk.iter().zip(srcinfo_data) {
                            if let Err(e) = db_sender
                                .send(SrcInfoTuple {
                                    branch: branch.clone(),
                                    commit: commit.clone(),
                                    srcinfo_text,
                                })
                                .await
                            {
                                error!("Failed to send srcinfo to database task: {}", e);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error fetching batch: {}", e);
                    }
                }
            }
            // Close the sender to signal we're done
            drop(db_sender);
        });

        let mut processed_packages = 0;
        let mut srcinfo_batch: Vec<SrcInfoTuple> = Vec::with_capacity(BATCH_SIZE);
        let mut packages_batch: Vec<DatabasePackageDetails> =
            Vec::with_capacity((BATCH_SIZE + (BATCH_SIZE + 3)) >> 2);
        loop {
            srcinfo_batch.clear();
            packages_batch.clear();

            let count = db_receiver.recv_many(&mut srcinfo_batch, BATCH_SIZE).await;
            if count == 0 {
                break; // Channel closed
            }

            let mut tx = self.db.begin_transaction().await?;
            for SrcInfoTuple {
                branch,
                commit,
                srcinfo_text,
            } in srcinfo_batch.iter()
            {
                self.db.clear_index_with_tx(&mut tx, branch).await?;
                self.db
                    .update_branch_commit_with_tx(&mut tx, branch, commit)
                    .await?;

                let branch_packages = srcinfo_to_db_models(branch, commit, srcinfo_text);

                let before_len = packages_batch.len();
                packages_batch.extend(branch_packages);
                if before_len == packages_batch.len() {
                    warn!(
                        "⚠ No packages found for branch {} ({})",
                        branch,
                        &commit[..8]
                    );
                }
            }

            if !packages_batch.is_empty() {
                self.db
                    .update_index_with_tx(&mut tx, &packages_batch)
                    .await?;
                processed_packages += packages_batch.len();
            }

            tx.commit().await?;

            info!("Processed {} packages", processed_packages);
        }

        fetch_task.await?;

        info!(
            "✅ Sync completed successfully. Processed {} packages",
            processed_packages
        );
        Ok(())
    }
}

fn srcinfo_to_db_models(
    branch: &str,
    commit_id: &str,
    srcinfo: &str,
) -> impl Iterator<Item = DatabasePackageDetails> {
    let branch = branch.to_string();
    let commit_id = commit_id.to_string();
    ParsedSrcInfo::parse(srcinfo)
        .into_iter()
        .map(move |pkg| DatabasePackageDetails {
            info: DatabasePackageInfo {
                branch: branch.clone(),
                commit_id: commit_id.clone(),
                pkg_name: pkg.pkgname.clone(),
                pkg_desc: pkg.first_prop("pkgdesc").map(|s| s.to_string()),
                version: pkg.version(),
                url: pkg.first_prop("url").map(|s| s.to_string()),
            },
            groups: pkg.prop("groups"),
            depends: pkg.flatten_arch_prop("depends"),
            make_depends: pkg.flatten_arch_prop("makedepends"),
            opt_depends: pkg.flatten_arch_prop("optdepends"),
            check_depends: pkg.flatten_arch_prop("checkdepends"),
            provides: pkg.flatten_arch_prop("provides"),
            conflicts: pkg.flatten_arch_prop("conflicts"),
            replaces: pkg.flatten_arch_prop("replaces"),
        })
}
