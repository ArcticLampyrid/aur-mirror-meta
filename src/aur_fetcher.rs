use anyhow::{anyhow, Result};
use async_tempfile::TempFile;
use futures::TryStreamExt;
use gix_hash::{oid, ObjectId};
use gix_object::{commit, CommitRefIter, TreeRefIter};
use gix_pack::data;
use gix_pack::data::input::{self, BytesToEntriesIter, EntryDataMode};
use gix_packetline::async_io::{encode, StreamingPeekableIter};
use gix_packetline::read::ProgressAction;
use gix_packetline::PacketLineRef;
use reqwest::{header, Client};
use std::collections::HashMap;
use tokio_util::compat::TokioAsyncReadCompatExt as _;
use tracing::{error, trace};

const AUR_GIT_UPLOAD_PACK_GET_URL: &str =
    "https://github.com/archlinux/aur.git/info/refs?service=git-upload-pack";
const AUR_GIT_UPLOAD_PACK_POST_URL: &str = "https://github.com/archlinux/aur.git/git-upload-pack";

#[derive(Clone)]
pub struct AurFetcher {
    client: Client,
    github_token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FetchedSrcInfo {
    pub srcinfo_text: String,
    pub committed_at: i64,
}

impl AurFetcher {
    pub fn new(github_token: Option<String>) -> Self {
        let client = Client::new();
        Self {
            client,
            github_token,
        }
    }

    pub fn github_token(&self) -> Option<&str> {
        self.github_token.as_deref()
    }

    pub fn user_agent() -> String {
        format!("AUR-Mirror-Meta/{}", env!("CARGO_PKG_VERSION"))
    }

    pub async fn fetch_srcinfo_batch(
        &self,
        commits: impl Iterator<Item = impl AsRef<str>>,
    ) -> Result<impl Iterator<Item = Option<FetchedSrcInfo>>> {
        let commit_ids: Vec<ObjectId> = commits
            .map(|c| ObjectId::from_hex(c.as_ref().as_bytes()).unwrap())
            .collect();
        let commit_data = self
            .fetch_srcinfo_blob_ids_and_timestamps(commit_ids.iter())
            .await?;
        let blob_ids: Vec<_> = commit_data.values().map(|(blob_id, _)| blob_id).collect();
        let mut blobs = self.fetch_srcinfo_blobs(blob_ids.into_iter()).await?;
        Ok(commit_ids.into_iter().map(move |commit_id| {
            commit_data.get(&commit_id).map(|(blob_id, timestamp)| {
                let srcinfo = blobs.remove(blob_id).unwrap_or_default();
                FetchedSrcInfo {
                    srcinfo_text: srcinfo,
                    committed_at: *timestamp,
                }
            })
        }))
    }

    async fn fetch_srcinfo_blob_ids_and_timestamps(
        &self,
        commits: impl Iterator<Item = impl AsRef<oid>>,
    ) -> Result<gix_hashtable::HashMap<ObjectId, (ObjectId, i64)>> {
        let mut request_builder = self
            .client
            .post(AUR_GIT_UPLOAD_PACK_POST_URL)
            .header("Git-Protocol", "version=2")
            .header(header::USER_AGENT, &Self::user_agent());
        if let Some(token) = &self.github_token {
            request_builder = request_builder.basic_auth(token, None::<&str>);
        }
        {
            let mut body = Vec::new();
            encode::text_to_write(b"command=fetch", &mut body).await?;
            encode::text_to_write(b"agent=git/aur-mirror", &mut body).await?;
            encode::delim_to_write(&mut body).await?;
            for commit in commits {
                encode::text_to_write(format!("want {}", commit.as_ref()).as_bytes(), &mut body)
                    .await?;
            }
            encode::text_to_write(b"ofs-delta", &mut body).await?;
            encode::text_to_write(b"deepen 1", &mut body).await?;
            encode::text_to_write(b"filter blob:none", &mut body).await?;
            encode::text_to_write(b"no-progress", &mut body).await?;
            encode::text_to_write(b"done", &mut body).await?;
            encode::flush_to_write(&mut body).await?;
            request_builder = request_builder.body(body);
        }
        let response = request_builder.send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("Failed to fetch commits: {}", response.status()));
        }

        let mut rd = StreamingPeekableIter::new(
            response
                .bytes_stream()
                .map_err(std::io::Error::other)
                .into_async_read(),
            &[PacketLineRef::Flush, PacketLineRef::Delimiter],
            false,
        );

        let mut packfile = TempFile::new().await?;
        read_packfile_from_fetch_response(&mut rd, &mut (&mut packfile).compat()).await?;
        let commit_to_blob_and_timestamp =
            map_commit_id_to_srcinfo_blob_id_and_timestamp(packfile.file_path())?;
        Ok(commit_to_blob_and_timestamp)
    }

    async fn fetch_srcinfo_blobs(
        &self,
        blobs: impl Iterator<Item = impl AsRef<oid>>,
    ) -> Result<gix_hashtable::HashMap<ObjectId, std::string::String>> {
        let mut request_builder = self
            .client
            .post(AUR_GIT_UPLOAD_PACK_POST_URL)
            .header("Git-Protocol", "version=2")
            .header(header::USER_AGENT, &Self::user_agent());
        if let Some(token) = &self.github_token {
            request_builder = request_builder.basic_auth(token, None::<&str>);
        }
        {
            let mut body = Vec::new();
            encode::text_to_write(b"command=fetch", &mut body).await?;
            encode::text_to_write(b"agent=git/aur-mirror", &mut body).await?;
            encode::delim_to_write(&mut body).await?;
            for blob in blobs {
                encode::text_to_write(format!("want {}", blob.as_ref()).as_bytes(), &mut body)
                    .await?;
            }
            encode::text_to_write(b"ofs-delta", &mut body).await?;
            encode::text_to_write(b"no-progress", &mut body).await?;
            encode::text_to_write(b"done", &mut body).await?;
            encode::flush_to_write(&mut body).await?;
            request_builder = request_builder.body(body);
        }
        let response = request_builder.send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("Failed to fetch commits: {}", response.status()));
        }

        let mut rd = StreamingPeekableIter::new(
            response
                .bytes_stream()
                .map_err(std::io::Error::other)
                .into_async_read(),
            &[PacketLineRef::Flush, PacketLineRef::Delimiter],
            false,
        );

        let mut packfile = TempFile::new().await?;
        read_packfile_from_fetch_response(&mut rd, &mut (&mut packfile).compat()).await?;
        let blob_id_to_content_map =
            map_blob_id_to_content(packfile.file_path(), String::from_utf8)?;
        Ok(blob_id_to_content_map)
    }

    pub async fn fetch_branch_list(&self) -> Result<HashMap<String, String>> {
        let mut request_builder = self.client.get(AUR_GIT_UPLOAD_PACK_GET_URL);
        if let Some(token) = &self.github_token {
            request_builder = request_builder.basic_auth(token, None::<&str>);
        }
        let response = request_builder.send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("Failed to fetch refs: {}", response.status()));
        }
        let mut rd = StreamingPeekableIter::new(
            response
                .bytes_stream()
                .map_err(std::io::Error::other)
                .into_async_read(),
            &[PacketLineRef::Flush],
            false,
        );
        let mut branches = HashMap::new();
        while rd.read_line().await.is_some() {
            // skip first part
        }
        rd.reset();

        while let Some(line_res) = rd.read_line().await {
            let line_str = line_res??
                .as_bstr()
                .and_then(|b| std::str::from_utf8(b).ok());
            let line = match line_str {
                Some(l) => l,
                None => continue,
            };
            if let Some((commit_id, branch_name)) = line.split_once(" refs/heads/") {
                let branch_name = branch_name.trim_end();
                if branch_name != "main" {
                    branches.insert(branch_name.to_string(), commit_id.to_string());
                }
            }
        }
        Ok(branches)
    }
}

async fn read_packfile_from_fetch_response<S, D>(
    rd: &mut StreamingPeekableIter<S>,
    dest: &mut D,
) -> anyhow::Result<()>
where
    S: futures::io::AsyncRead + Unpin,
    D: futures::io::AsyncWrite + Unpin,
{
    loop {
        let section_header = rd.read_line().await;
        if section_header.is_none() {
            return Err(anyhow!("Missing section header"));
        }
        let section_header = section_header
            .unwrap()??
            .as_bstr()
            .and_then(|x| std::str::from_utf8(x).ok())
            .map(|x: &str| x.trim())
            .ok_or_else(|| anyhow!("Invalid section header"))?;
        if section_header != "packfile" {
            // read all lines and reset
            while rd.read_line().await.is_some() {
                // skip
            }
            rd.reset();
            continue;
        }
        // packfile section
        futures::io::copy(
            rd.as_read_with_sidebands(|is_error, msg| {
                if is_error {
                    error!("Packfile fetch error: {}", String::from_utf8_lossy(msg));
                    ProgressAction::Interrupt
                } else {
                    trace!("Packfile fetch progress: {}", String::from_utf8_lossy(msg));
                    ProgressAction::Continue
                }
            }),
            dest,
        )
        .await?;
        break;
    }

    Ok(())
}

fn map_commit_id_to_srcinfo_blob_id_and_timestamp(
    packfile_path: &std::path::Path,
) -> anyhow::Result<gix_hashtable::HashMap<ObjectId, (ObjectId, i64)>> {
    let entries_offset = BytesToEntriesIter::new_from_header(
        std::io::BufReader::with_capacity(4096 * 8, std::fs::File::open(packfile_path)?),
        input::Mode::AsIs,
        EntryDataMode::Ignore,
        gix_hash::Kind::Sha1,
    )?
    .filter_map(|x| x.ok().map(|e| e.pack_offset));

    let mut commit_to_tree_and_timestamp =
        gix_hashtable::HashMap::<ObjectId, (ObjectId, i64)>::default();
    let mut tree_to_srcinfo_blob_map = gix_hashtable::HashMap::<ObjectId, ObjectId>::default();

    let pack = data::File::at(packfile_path, gix_hash::Kind::Sha1)?;
    let mut delta_cache = gix_pack::cache::lru::MemoryCappedHashmap::new(1024 * 1024 * 10);
    for pack_offset in entries_offset {
        let entry = pack.entry(pack_offset)?;
        let mut out = Vec::with_capacity(entry.decompressed_size as usize);
        let outcome = pack.decode_entry(
            entry,
            &mut out,
            &mut Default::default(),
            &|_, _| None,
            &mut delta_cache,
        )?;
        let object_id = gix_object::compute_hash(gix_hash::Kind::Sha1, outcome.kind, &out)?;
        match outcome.kind {
            gix_object::Kind::Commit => {
                let mut tree_id = None;
                let mut commit_time = None;

                for token in CommitRefIter::from_bytes(&out) {
                    match token {
                        Ok(commit::ref_iter::Token::Tree { id }) => {
                            tree_id = Some(id);
                        }
                        Ok(commit::ref_iter::Token::Committer { signature }) => {
                            commit_time = signature.time().ok();
                        }
                        _ => {}
                    }
                }

                let tree_id =
                    tree_id.ok_or_else(|| anyhow!("Commit {} missing tree", object_id))?;
                let commit_time =
                    commit_time.ok_or_else(|| anyhow!("Commit {} missing time", object_id))?;
                trace!(
                    "Mapping commit {} to tree {} with timestamp {}",
                    object_id,
                    tree_id,
                    commit_time
                );
                commit_to_tree_and_timestamp.insert(object_id, (tree_id, commit_time.seconds));
            }
            gix_object::Kind::Tree => {
                let srcinfo_blob_id = TreeRefIter::from_bytes(&out)
                    .filter_map(|tree_entry_res| {
                        // find ".SRCINFO" blob
                        if let Ok(tree_entry) = tree_entry_res {
                            if tree_entry.filename == b".SRCINFO" && tree_entry.mode.is_blob() {
                                Some(tree_entry.oid)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .next();
                if let Some(srcinfo_blob_id) = srcinfo_blob_id {
                    trace!(
                        "Mapping tree {} to .SRCINFO blob {}",
                        object_id,
                        srcinfo_blob_id
                    );
                    tree_to_srcinfo_blob_map.insert(object_id, srcinfo_blob_id.to_owned());
                }
            }
            _ => {}
        }
    }

    let mut commit_to_srcinfo_and_timestamp =
        gix_hashtable::HashMap::<ObjectId, (ObjectId, i64)>::default();
    for (commit_id, (tree_id, timestamp)) in commit_to_tree_and_timestamp {
        if let Some(srcinfo_blob_id) = tree_to_srcinfo_blob_map.remove(&tree_id) {
            commit_to_srcinfo_and_timestamp.insert(commit_id, (srcinfo_blob_id, timestamp));
        }
    }
    Ok(commit_to_srcinfo_and_timestamp)
}

fn map_blob_id_to_content<T, E>(
    packfile_path: &std::path::Path,
    content_parser: fn(Vec<u8>) -> Result<T, E>,
) -> anyhow::Result<gix_hashtable::HashMap<ObjectId, T>>
where
    E: std::error::Error,
{
    let entries_offset = BytesToEntriesIter::new_from_header(
        std::io::BufReader::with_capacity(4096 * 8, std::fs::File::open(packfile_path)?),
        input::Mode::AsIs,
        EntryDataMode::Ignore,
        gix_hash::Kind::Sha1,
    )?
    .filter_map(|x| x.ok().map(|e| e.pack_offset));

    let mut blob_id_to_content_map = gix_hashtable::HashMap::<ObjectId, T>::default();

    let pack = data::File::at(packfile_path, gix_hash::Kind::Sha1)?;
    let mut delta_cache = gix_pack::cache::lru::MemoryCappedHashmap::new(1024 * 1024 * 10);
    for pack_offset in entries_offset {
        let entry = pack.entry(pack_offset)?;
        let mut out = Vec::with_capacity(entry.decompressed_size as usize);
        let outcome = pack.decode_entry(
            entry,
            &mut out,
            &mut Default::default(),
            &|_, _| None,
            &mut delta_cache,
        )?;
        let object_id = gix_object::compute_hash(gix_hash::Kind::Sha1, outcome.kind, &out)?;
        if outcome.kind == gix_object::Kind::Blob {
            match content_parser(out) {
                Ok(content) => {
                    blob_id_to_content_map.insert(object_id, content);
                }
                Err(err) => {
                    error!("Failed to parse blob {}: {}", object_id, err);
                }
            }
        }
    }

    Ok(blob_id_to_content_map)
}
