use crate::types::GqlFetchSrcInfoResponse;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use gix_packetline::async_io::StreamingPeekableIter;
use gix_packetline::PacketLineRef;
use reqwest::{header, Client};
use std::collections::HashMap;
use std::fmt::Write;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

const AUR_GIT_UPLOAD_PACK_GET_URL: &str =
    "https://github.com/archlinux/aur.git/info/refs?service=git-upload-pack";
const GITHUB_GRAPHQL_URL: &str = "https://api.github.com/graphql";
const RETRY_AFTER_FINETUNING: i64 = 15;

#[derive(Clone)]
pub struct AurFetcher {
    client: Client,
    github_token: Option<String>,
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

    pub async fn fetch_srcinfo_batch(
        &self,
        commits: impl Iterator<Item = impl AsRef<str>>,
    ) -> Result<impl Iterator<Item = String>> {
        let mut n_commits: usize = 0;
        let mut query = String::new();
        query.push_str(r#"query{repository(owner:"archlinux",name:"aur"){"#);
        for (i, commit) in commits.enumerate() {
            write!(
                query,
                r#"x{}:object(expression:"{}:.SRCINFO"){{... on Blob{{text}}}}"#,
                i,
                commit.as_ref()
            )?;
            n_commits += 1;
        }
        query.push_str(r#"}}"#);

        let request_body = serde_json::json!({
            "query": query
        });

        let graphql_response = loop {
            let mut request_builder = self
                .client
                .post(GITHUB_GRAPHQL_URL)
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::USER_AGENT, &Self::user_agent());
            if let Some(token) = self.github_token() {
                request_builder = request_builder.bearer_auth(token);
            }
            let response = request_builder.json(&request_body).send().await?;

            // Handle standard Retry-After headers
            if let Some(retry_after) = response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
            {
                if let Ok(retry_after_secs) = retry_after.parse::<i64>() {
                    let wait_time = retry_after_secs + RETRY_AFTER_FINETUNING;
                    if wait_time > 0 {
                        info!("Rate limited. Waiting {} seconds...", wait_time);
                        sleep(Duration::from_secs(wait_time as u64)).await;
                    }
                    continue;
                } else if let Ok(date) = DateTime::parse_from_rfc2822(retry_after) {
                    let wait_time =
                        date.timestamp() - Utc::now().timestamp() + RETRY_AFTER_FINETUNING;
                    if wait_time > 0 {
                        info!("Rate limited. Waiting {} seconds...", wait_time);
                        sleep(Duration::from_secs(wait_time as u64)).await;
                    }
                    continue;
                }
            }

            // Handle GitHub-specific rate limit headers
            if response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u32>().ok())
                == Some(0)
            {
                let now = Utc::now().timestamp();
                let rate_limit_reset = response
                    .headers()
                    .get("x-ratelimit-reset")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(now);
                let wait_time = rate_limit_reset - now + RETRY_AFTER_FINETUNING;
                if wait_time > 0 {
                    info!("Rate limited. Waiting {} seconds...", wait_time);
                    sleep(Duration::from_secs(wait_time as u64)).await;
                }
                continue;
            }

            if response.status().is_success() {
                break response.json::<GqlFetchSrcInfoResponse>().await?;
            } else {
                return Err(anyhow!("GitHub API error: {}", response.status()));
            }
        };

        if let Some(errors) = graphql_response.errors {
            return Err(anyhow!("GraphQL errors: {:?}", errors));
        }

        let mut data = graphql_response
            .data
            .ok_or_else(|| anyhow!("No data in GraphQL response"))?;

        let result = (0..n_commits).map(move |i| {
            let key = format!("x{}", i);
            data.repository
                .remove(&key)
                .map(|obj| obj.text)
                .unwrap_or_default()
        });

        Ok(result)
    }
}
