use crate::types::{
    DatabasePackageDetails, DatabasePackageDetailsWithSupplement,
    DatabasePackageInfoWithSupplement, DatabaseSupplementData, SearchType,
};
use anyhow::Result;
use futures::stream::TryStreamExt;
use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};
use std::collections::HashMap;
use tracing::info;

const CURRENT_DB_VERSION: i32 = 2;

#[derive(Clone)]
pub struct DatabaseOps {
    pool: SqlitePool,
}

impl DatabaseOps {
    pub async fn new(db_path: &str) -> Result<Self> {
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(db_path)
                .create_if_missing(true),
        )
        .await?;
        let result = Self { pool };
        result.check_and_migrate().await?;
        result.init_index_tables().await?;
        Ok(result)
    }

    async fn check_and_migrate(&self) -> Result<()> {
        let version: i32 = sqlx::query("PRAGMA user_version")
            .fetch_one(&self.pool)
            .await?
            .get(0);

        let version = match version {
            0 => {
                // check if table `pkg_info` exists to determine if it's an old version
                // for the first version did not set user_version pragma
                let table_exists = sqlx::query(
                    "SELECT COUNT(name) FROM sqlite_master WHERE type='table' AND name='pkg_info'",
                )
                .fetch_one(&self.pool)
                .await?
                .get::<i32, _>(0)
                    != 0;
                if table_exists {
                    1
                } else {
                    0
                }
            }
            x => x,
        };

        if version < CURRENT_DB_VERSION {
            if version > 0 {
                info!(
                    "Database version {} is outdated (current version: {}). Clearing all data...",
                    version, CURRENT_DB_VERSION
                );
                // Drop all tables
                let tables = vec![
                    "branch_commits",
                    "pkg_info",
                    "pkg_depends",
                    "pkg_make_depends",
                    "pkg_opt_depends",
                    "pkg_check_depends",
                    "pkg_provides",
                    "pkg_conflicts",
                    "pkg_replaces",
                    "pkg_groups",
                    "pkg_supplement",
                ];
                for table in tables {
                    sqlx::query(&format!("DROP TABLE IF EXISTS {}", table))
                        .execute(&self.pool)
                        .await?;
                }
            }
            // Set new version
            sqlx::query(&format!("PRAGMA user_version = {}", CURRENT_DB_VERSION))
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    async fn init_index_tables(&self) -> Result<()> {
        let tables = vec![
            r#"CREATE TABLE IF NOT EXISTS branch_commits (
                branch TEXT NOT NULL PRIMARY KEY,
                commit_id TEXT NOT NULL
            )"#,
            r#"CREATE TABLE IF NOT EXISTS pkg_info (
                branch TEXT NOT NULL,
                pkg_name TEXT NOT NULL,
                pkg_desc TEXT,
                version TEXT NOT NULL,
                url TEXT,
                commit_id TEXT NOT NULL,
                is_listed INTEGER DEFAULT 1,
                committed_at INTEGER,
                PRIMARY KEY (branch, pkg_name)
            )"#,
            r#"CREATE TABLE IF NOT EXISTS pkg_depends (
                branch TEXT NOT NULL,
                pkg_name TEXT NOT NULL,
                depend TEXT NOT NULL,
                PRIMARY KEY (branch, pkg_name, depend)
            )"#,
            r#"CREATE TABLE IF NOT EXISTS pkg_make_depends (
                branch TEXT NOT NULL,
                pkg_name TEXT NOT NULL,
                make_depend TEXT NOT NULL,
                PRIMARY KEY (branch, pkg_name, make_depend)
            )"#,
            r#"CREATE TABLE IF NOT EXISTS pkg_opt_depends (
                branch TEXT NOT NULL,
                pkg_name TEXT NOT NULL,
                opt_depend TEXT NOT NULL,
                PRIMARY KEY (branch, pkg_name, opt_depend)
            )"#,
            r#"CREATE TABLE IF NOT EXISTS pkg_check_depends (
                branch TEXT NOT NULL,
                pkg_name TEXT NOT NULL,
                check_depend TEXT NOT NULL,
                PRIMARY KEY (branch, pkg_name, check_depend)
            )"#,
            r#"CREATE TABLE IF NOT EXISTS pkg_provides (
                branch TEXT NOT NULL,
                pkg_name TEXT NOT NULL,
                provide TEXT NOT NULL,
                PRIMARY KEY (branch, pkg_name, provide)
            )"#,
            r#"CREATE TABLE IF NOT EXISTS pkg_conflicts (
                branch TEXT NOT NULL,
                pkg_name TEXT NOT NULL,
                conflict TEXT NOT NULL,
                PRIMARY KEY (branch, pkg_name, conflict)
            )"#,
            r#"CREATE TABLE IF NOT EXISTS pkg_replaces (
                branch TEXT NOT NULL,
                pkg_name TEXT NOT NULL,
                replace TEXT NOT NULL,
                PRIMARY KEY (branch, pkg_name, replace)
            )"#,
            r#"CREATE TABLE IF NOT EXISTS pkg_groups (
                branch TEXT NOT NULL,
                pkg_name TEXT NOT NULL,
                group_name TEXT NOT NULL,
                PRIMARY KEY (branch, pkg_name, group_name)
            )"#,
            r#"CREATE TABLE IF NOT EXISTS pkg_supplement (
                pkgname TEXT NOT NULL PRIMARY KEY,
                version TEXT NOT NULL,
                popularity REAL NOT NULL,
                num_votes INTEGER NOT NULL,
                out_of_date INTEGER,
                maintainer TEXT,
                submitter TEXT,
                co_maintainers TEXT,
                keywords TEXT,
                first_submitted INTEGER,
                last_modified INTEGER
            )"#,
        ];

        for table_sql in tables {
            sqlx::query(table_sql).execute(&self.pool).await?;
        }

        let indexes = vec![
            // Query based on pkg name
            "CREATE INDEX IF NOT EXISTS idx_pkg_info_name ON pkg_info(pkg_name)",
            // Query based on branch
            "CREATE INDEX IF NOT EXISTS idx_pkg_info_branch ON pkg_info(branch)",
            "CREATE INDEX IF NOT EXISTS idx_pkg_depends_branch ON pkg_depends(branch)",
            "CREATE INDEX IF NOT EXISTS idx_pkg_make_depends_branch ON pkg_make_depends(branch)",
            "CREATE INDEX IF NOT EXISTS idx_pkg_opt_depends_branch ON pkg_opt_depends(branch)",
            "CREATE INDEX IF NOT EXISTS idx_pkg_check_depends_branch ON pkg_check_depends(branch)",
            "CREATE INDEX IF NOT EXISTS idx_pkg_provides_branch ON pkg_provides(branch)",
            "CREATE INDEX IF NOT EXISTS idx_pkg_conflicts_branch ON pkg_conflicts(branch)",
            "CREATE INDEX IF NOT EXISTS idx_pkg_replaces_branch ON pkg_replaces(branch)",
            "CREATE INDEX IF NOT EXISTS idx_pkg_groups_branch ON pkg_groups(branch)",
            // For reverse lookups
            "CREATE INDEX IF NOT EXISTS idx_pkg_depends_depend ON pkg_depends(depend)",
            "CREATE INDEX IF NOT EXISTS idx_pkg_make_depends_make_depend ON pkg_make_depends(make_depend)",
            "CREATE INDEX IF NOT EXISTS idx_pkg_opt_depends_opt_depend ON pkg_opt_depends(opt_depend)",
            "CREATE INDEX IF NOT EXISTS idx_pkg_check_depends_check_depend ON pkg_check_depends(check_depend)",
        ];

        for index_sql in indexes {
            sqlx::query(index_sql).execute(&self.pool).await?;
        }

        Ok(())
    }

    pub async fn get_existing_commits(&self) -> Result<HashMap<String, String>> {
        let mut rows =
            sqlx::query("SELECT branch, commit_id FROM branch_commits").fetch(&self.pool);
        let mut commits = HashMap::new();
        while let Some(row) = rows.try_next().await? {
            let branch: String = row.get("branch");
            let commit_id: String = row.get("commit_id");
            commits.insert(branch, commit_id);
        }
        Ok(commits)
    }

    pub async fn begin_transaction(&self) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>> {
        Ok(self.pool.begin().await?)
    }

    pub async fn update_branch_commit_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        branch: &str,
        commit_id: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO branch_commits (branch, commit_id) 
            VALUES (?, ?)
        "#,
        )
        .bind(branch)
        .bind(commit_id)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn clear_index_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        branch: &str,
    ) -> Result<()> {
        let tables = vec![
            "pkg_info",
            "pkg_depends",
            "pkg_make_depends",
            "pkg_opt_depends",
            "pkg_check_depends",
            "pkg_provides",
            "pkg_conflicts",
            "pkg_replaces",
            "pkg_groups",
        ];
        for table in tables {
            let query = format!("DELETE FROM {} WHERE branch = ?", table);
            sqlx::query(&query).bind(branch).execute(&mut **tx).await?;
        }
        Ok(())
    }

    pub async fn update_index_with_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        packages: &[DatabasePackageDetails],
    ) -> Result<()> {
        if packages.is_empty() {
            return Ok(());
        }

        for pkg in packages {
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO pkg_info 
                (branch, pkg_name, pkg_desc, version, url, commit_id, committed_at) 
                VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            )
            .bind(&pkg.branch)
            .bind(&pkg.pkg_name)
            .bind(&pkg.pkg_desc)
            .bind(&pkg.version)
            .bind(&pkg.url)
            .bind(&pkg.commit_id)
            .bind(pkg.committed_at)
            .execute(&mut **tx)
            .await?;

            self.store_array_tx(
                tx,
                &pkg.branch,
                &pkg.pkg_name,
                "pkg_depends",
                "depend",
                &pkg.depends,
            )
            .await?;
            self.store_array_tx(
                tx,
                &pkg.branch,
                &pkg.pkg_name,
                "pkg_make_depends",
                "make_depend",
                &pkg.make_depends,
            )
            .await?;
            self.store_array_tx(
                tx,
                &pkg.branch,
                &pkg.pkg_name,
                "pkg_opt_depends",
                "opt_depend",
                &pkg.opt_depends,
            )
            .await?;
            self.store_array_tx(
                tx,
                &pkg.branch,
                &pkg.pkg_name,
                "pkg_check_depends",
                "check_depend",
                &pkg.check_depends,
            )
            .await?;
            self.store_array_tx(
                tx,
                &pkg.branch,
                &pkg.pkg_name,
                "pkg_provides",
                "provide",
                &pkg.provides,
            )
            .await?;
            self.store_array_tx(
                tx,
                &pkg.branch,
                &pkg.pkg_name,
                "pkg_conflicts",
                "conflict",
                &pkg.conflicts,
            )
            .await?;
            self.store_array_tx(
                tx,
                &pkg.branch,
                &pkg.pkg_name,
                "pkg_replaces",
                "replace",
                &pkg.replaces,
            )
            .await?;
            self.store_array_tx(
                tx,
                &pkg.branch,
                &pkg.pkg_name,
                "pkg_groups",
                "group_name",
                &pkg.groups,
            )
            .await?;
        }

        Ok(())
    }

    async fn store_array_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        branch: &str,
        pkg_name: &str,
        table: &str,
        column: &str,
        items: &[String],
    ) -> Result<()> {
        for item in items {
            let query = format!(
                "INSERT OR IGNORE INTO {} (branch, pkg_name, {}) VALUES (?, ?, ?)",
                table, column
            );
            sqlx::query(&query)
                .bind(branch)
                .bind(pkg_name)
                .bind(item)
                .execute(&mut **tx)
                .await?;
        }
        Ok(())
    }

    pub async fn search_packages(
        &self,
        search_type: SearchType,
        keyword: &str,
    ) -> Result<Vec<DatabasePackageInfoWithSupplement>> {
        let (query, param, count) = match search_type {
            SearchType::Name => (
                r#"
                    SELECT DISTINCT p.*, s.popularity, s.num_votes, s.out_of_date,
                           s.maintainer, s.submitter, s.first_submitted, s.last_modified
                    FROM pkg_info p
                    LEFT JOIN pkg_supplement s ON p.pkg_name = s.pkgname
                    WHERE p.pkg_name LIKE ? AND p.is_listed = 1
                "#,
                format!("%{}%", keyword),
                1,
            ),
            SearchType::NameDesc => (
                r#"
                    SELECT DISTINCT p.*, s.popularity, s.num_votes, s.out_of_date,
                           s.maintainer, s.submitter, s.first_submitted, s.last_modified
                    FROM pkg_info p
                    LEFT JOIN pkg_supplement s ON p.pkg_name = s.pkgname
                    WHERE (p.pkg_name LIKE ? OR p.pkg_desc LIKE ?) AND p.is_listed = 1
                "#,
                format!("%{}%", keyword),
                2,
            ),
            SearchType::Depends => (
                r#"
                    SELECT DISTINCT p.*, s.popularity, s.num_votes, s.out_of_date,
                           s.maintainer, s.submitter, s.first_submitted, s.last_modified
                    FROM pkg_info p
                    LEFT JOIN pkg_supplement s ON p.pkg_name = s.pkgname
                    JOIN pkg_depends d ON p.pkg_name = d.pkg_name AND p.branch = d.branch
                    WHERE d.depend = ? AND p.is_listed = 1
                "#,
                keyword.to_string(),
                1,
            ),
            SearchType::MakeDepends => (
                r#"
                    SELECT DISTINCT p.*, s.popularity, s.num_votes, s.out_of_date,
                           s.maintainer, s.submitter, s.first_submitted, s.last_modified
                    FROM pkg_info p
                    LEFT JOIN pkg_supplement s ON p.pkg_name = s.pkgname
                    JOIN pkg_make_depends md ON p.pkg_name = md.pkg_name AND p.branch = md.branch
                    WHERE md.make_depend = ? AND p.is_listed = 1
                "#,
                keyword.to_string(),
                1,
            ),
            SearchType::OptDepends => (
                r#"
                    SELECT DISTINCT p.*, s.popularity, s.num_votes, s.out_of_date,
                           s.maintainer, s.submitter, s.first_submitted, s.last_modified
                    FROM pkg_info p
                    LEFT JOIN pkg_supplement s ON p.pkg_name = s.pkgname
                    JOIN pkg_opt_depends od ON p.pkg_name = od.pkg_name AND p.branch = od.branch
                    WHERE od.opt_depend = ? AND p.is_listed = 1
                "#,
                keyword.to_string(),
                1,
            ),
            SearchType::CheckDepends => (
                r#"
                    SELECT DISTINCT p.*, s.popularity, s.num_votes, s.out_of_date,
                           s.maintainer, s.submitter, s.first_submitted, s.last_modified
                    FROM pkg_info p
                    LEFT JOIN pkg_supplement s ON p.pkg_name = s.pkgname
                    JOIN pkg_check_depends cd ON p.pkg_name = cd.pkg_name AND p.branch = cd.branch
                    WHERE cd.check_depend = ? AND p.is_listed = 1
                "#,
                keyword.to_string(),
                1,
            ),
        };

        let mut query_builder = sqlx::query(query);
        for _ in 0..count {
            query_builder = query_builder.bind(&param);
        }
        query_builder
            .fetch(&self.pool)
            .map_ok(|row| {
                // Apply the logic from the spec: use time-sensitive fields only if version matches
                let pkg_version: String = row.get("version");
                let supplement_version: Option<String> = row.try_get("s.version").ok();
                let version_matches = supplement_version
                    .as_ref()
                    .map(|v| v == &pkg_version)
                    .unwrap_or(false);

                DatabasePackageInfoWithSupplement {
                    commit_id: row.get("commit_id"),
                    committed_at: row.get("committed_at"),
                    branch: row.get("branch"),
                    pkg_name: row.get("pkg_name"),
                    pkg_desc: row.get("pkg_desc"),
                    version: pkg_version,
                    url: row.get("url"),
                    popularity: row.try_get("popularity").ok(),
                    num_votes: row.try_get("num_votes").ok(),
                    out_of_date: if version_matches {
                        row.try_get("out_of_date").ok().flatten()
                    } else {
                        None
                    },
                    maintainer: row.try_get("maintainer").ok().flatten(),
                    submitter: row.try_get("submitter").ok().flatten(),
                    first_submitted: row.try_get("first_submitted").ok(),
                    last_modified: if version_matches {
                        row.try_get("last_modified").ok()
                    } else {
                        None
                    },
                }
            })
            .try_collect::<Vec<_>>()
            .await
            .map_err(Into::into)
    }

    pub async fn get_package_details(
        &self,
        package_names: &[String],
    ) -> Result<Vec<DatabasePackageDetailsWithSupplement>> {
        if package_names.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = package_names.iter().map(|_| "?".to_string()).collect();
        let placeholders_str = placeholders.join(",");

        let query = format!(
            r#"
            SELECT p.*, s.version as s_version, s.popularity, s.num_votes, s.out_of_date,
                   s.maintainer, s.submitter, s.first_submitted, s.last_modified,
                   s.co_maintainers, s.keywords
            FROM pkg_info p
            LEFT JOIN pkg_supplement s ON p.pkg_name = s.pkgname
            WHERE p.pkg_name IN ({}) AND p.is_listed = 1
            "#,
            placeholders_str
        );

        let mut query_builder = sqlx::query(&query);
        for name in package_names {
            query_builder = query_builder.bind(name);
        }

        query_builder
            .fetch(&self.pool)
            .and_then(
                async |row| -> sqlx::Result<DatabasePackageDetailsWithSupplement> {
                    let pkg_version: String = row.get("version");
                    let supplement_version: Option<String> = row.try_get("s_version").ok();
                    let version_matches = supplement_version
                        .as_ref()
                        .map(|v| v == &pkg_version)
                        .unwrap_or(false);

                    let info = DatabasePackageInfoWithSupplement {
                        commit_id: row.get("commit_id"),
                        committed_at: row.get("committed_at"),
                        branch: row.get("branch"),
                        pkg_name: row.get("pkg_name"),
                        pkg_desc: row.get("pkg_desc"),
                        version: pkg_version,
                        url: row.get("url"),
                        popularity: row.try_get("popularity").ok(),
                        num_votes: row.try_get("num_votes").ok(),
                        out_of_date: if version_matches {
                            row.try_get("out_of_date").ok().flatten()
                        } else {
                            None
                        },
                        maintainer: row.try_get("maintainer").ok().flatten(),
                        submitter: row.try_get("submitter").ok().flatten(),
                        first_submitted: row.try_get("first_submitted").ok(),
                        last_modified: if version_matches {
                            row.try_get("last_modified").ok()
                        } else {
                            None
                        },
                    };

                    let package_name: String = row.get("pkg_name");
                    let pkg_branch: String = row.get("branch");

                    let tables = vec![
                        ("pkg_depends", "depend"),
                        ("pkg_make_depends", "make_depend"),
                        ("pkg_opt_depends", "opt_depend"),
                        ("pkg_check_depends", "check_depend"),
                        ("pkg_provides", "provide"),
                        ("pkg_conflicts", "conflict"),
                        ("pkg_replaces", "replace"),
                        ("pkg_groups", "group_name"),
                    ];

                    let mut depends = Vec::new();
                    let mut make_depends = Vec::new();
                    let mut opt_depends = Vec::new();
                    let mut check_depends = Vec::new();
                    let mut provides = Vec::new();
                    let mut conflicts = Vec::new();
                    let mut replaces = Vec::new();
                    let mut groups = Vec::new();

                    for (table, column) in tables {
                        let query = format!(
                            "SELECT {} FROM {} WHERE pkg_name = ? AND branch = ?",
                            column, table
                        );
                        let values = sqlx::query(&query)
                            .bind(&package_name)
                            .bind(&pkg_branch)
                            .fetch(&self.pool)
                            .map_ok(|row| row.get::<String, _>(column))
                            .try_collect()
                            .await?;

                        match column {
                            "depend" => depends = values,
                            "make_depend" => make_depends = values,
                            "opt_depend" => opt_depends = values,
                            "check_depend" => check_depends = values,
                            "provide" => provides = values,
                            "conflict" => conflicts = values,
                            "replace" => replaces = values,
                            "group_name" => groups = values,
                            _ => {}
                        }
                    }

                    // Parse keywords and co_maintainers from JSON
                    let keywords: Vec<String> = row
                        .try_get::<Option<String>, _>("keywords")
                        .ok()
                        .flatten()
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default();

                    let co_maintainers: Vec<String> = row
                        .try_get::<Option<String>, _>("co_maintainers")
                        .ok()
                        .flatten()
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or_default();

                    Ok(DatabasePackageDetailsWithSupplement {
                        info,
                        depends,
                        make_depends,
                        opt_depends,
                        check_depends,
                        provides,
                        conflicts,
                        replaces,
                        groups,
                        keywords,
                        co_maintainers,
                    })
                },
            )
            .try_collect()
            .await
            .map_err(Into::into)
    }

    pub async fn get_branch_commit_id(&self, branch: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT commit_id FROM branch_commits WHERE branch = ? LIMIT 1")
            .bind(branch)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| r.get("commit_id")))
    }

    pub async fn store_supplement_data(
        &self,
        supplements: &[DatabaseSupplementData],
    ) -> Result<()> {
        if supplements.is_empty() {
            return Ok(());
        }

        let mut tx = self.begin_transaction().await?;
        sqlx::query("DELETE FROM pkg_supplement")
            .execute(&mut *tx)
            .await?;
        for supplement in supplements {
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO pkg_supplement
                (pkgname, version, popularity, num_votes, out_of_date, maintainer,
                 submitter, co_maintainers, keywords, first_submitted, last_modified)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&supplement.pkgname)
            .bind(&supplement.version)
            .bind(supplement.popularity)
            .bind(supplement.num_votes)
            .bind(supplement.out_of_date)
            .bind(&supplement.maintainer)
            .bind(&supplement.submitter)
            .bind(&serde_json::to_string(&supplement.co_maintainers)?)
            .bind(&serde_json::to_string(&supplement.keywords)?)
            .bind(supplement.first_submitted)
            .bind(supplement.last_modified)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        self.update_is_listed_status().await?;

        Ok(())
    }

    async fn update_is_listed_status(&self) -> Result<()> {
        // Get max last_modified from supplement data
        let max_last_modified: Option<i64> =
            sqlx::query("SELECT MAX(last_modified) FROM pkg_supplement")
                .fetch_one(&self.pool)
                .await?
                .get(0);

        if let Some(max_last_modified) = max_last_modified {
            const GAP: i64 = 86400; // 24 hours
            let threshold = max_last_modified - GAP;

            // Mark packages as unlisted if they meet the criteria
            sqlx::query(
                r#"
                UPDATE pkg_info
                SET is_listed = CASE
                    WHEN pkg_name IN (SELECT pkgname FROM pkg_supplement) THEN 1
                    WHEN committed_at IS NOT NULL AND committed_at < ? THEN 0
                    ELSE 1
                END
                "#,
            )
            .bind(threshold)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }
}
