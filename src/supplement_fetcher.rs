use crate::types::{DatabaseSupplementData, RpcPackageDetails};
use anyhow::{anyhow, Result};
use flate2::read::GzDecoder;
use reqwest::Client;
use std::io::Read;
use tracing::{info, warn};

pub struct SupplementFetcher {
    client: Client,
}

impl SupplementFetcher {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn fetch_supplement_data(
        &self,
        sources: &[String],
    ) -> Result<Vec<DatabaseSupplementData>> {
        for source in sources {
            if source == "none" {
                continue;
            }

            info!("Attempting to fetch supplement data from: {}", source);
            match self.fetch_from_source(source).await {
                Ok(data) => {
                    info!(
                        "Successfully fetched {} supplement records from {}",
                        data.len(),
                        source
                    );
                    return Ok(data);
                }
                Err(e) => {
                    warn!(
                        "Failed to fetch supplement data from {}: {}. Trying next source...",
                        source, e
                    );
                }
            }
        }

        Err(anyhow!(
            "Failed to fetch supplement data from all provided sources"
        ))
    }

    async fn fetch_from_source(&self, source: &str) -> Result<Vec<DatabaseSupplementData>> {
        let raw_data = if source.starts_with("http://") || source.starts_with("https://") {
            self.fetch_from_url(source).await?
        } else {
            self.fetch_from_file(source).await?
        };

        let decompressed_data = self.decompress_if_needed(&raw_data)?;
        self.parse_json(&decompressed_data)
    }

    async fn fetch_from_url(&self, url: &str) -> Result<Vec<u8>> {
        let response = self.client.get(url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("HTTP error: {}", response.status()));
        }
        Ok(response.bytes().await?.to_vec())
    }

    async fn fetch_from_file(&self, path: &str) -> Result<Vec<u8>> {
        Ok(tokio::fs::read(path).await?)
    }

    fn decompress_if_needed(&self, data: &[u8]) -> Result<Vec<u8>> {
        // Check for gzip magic bytes (1f 8b)
        if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
            info!("Detected gzip compression, decompressing...");
            let mut decoder = GzDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        } else {
            Ok(data.to_vec())
        }
    }

    fn parse_json(&self, data: &[u8]) -> Result<Vec<DatabaseSupplementData>> {
        let aur_data: Vec<RpcPackageDetails> = serde_json::from_slice(data)?;

        Ok(aur_data
            .into_iter()
            .map(|item| DatabaseSupplementData {
                pkgname: item.name,
                version: item.version,
                popularity: item.popularity,
                num_votes: item.num_votes,
                out_of_date: item.out_of_date,
                maintainer: item.maintainer,
                submitter: item.submitter,
                co_maintainers: item.co_maintainers,
                keywords: item.keywords,
                first_submitted: item.first_submitted,
                last_modified: item.last_modified,
            })
            .collect())
    }
}
