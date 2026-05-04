use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

use crate::checker::PackageRecord;
use crate::verdict::Ecosystem;

const USER_AGENT: &str = concat!(
    "phantomdep/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/openintelligence-labs/phantomdep)"
);

#[derive(Debug, Clone)]
pub struct CratesClient {
    http: Client,
    base_url: String,
}

impl CratesClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .context("building reqwest client")?;
        Ok(Self {
            http,
            base_url: "https://crates.io".into(),
        })
    }

    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into();
        self
    }

    pub async fn lookup(&self, name: &str) -> Result<PackageRecord> {
        // crates.io requires a User-Agent identifying the client.
        let url = format!("{}/api/v1/crates/{}", self.base_url, name);
        let resp = self
            .http
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;

        match resp.status() {
            StatusCode::NOT_FOUND => Ok(PackageRecord::missing(name, Ecosystem::Cargo)),
            s if s.is_success() => {
                let payload: CratesPayload = resp
                    .json()
                    .await
                    .context("parsing crates.io JSON payload")?;
                Ok(payload.into_record(name))
            }
            other => Err(anyhow::anyhow!(
                "crates.io returned unexpected status {other} for {name}"
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
struct CratesPayload {
    #[serde(rename = "crate")]
    crate_info: CrateInfo,
}

#[derive(Debug, Deserialize)]
struct CrateInfo {
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    repository: Option<String>,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    downloads: Option<u64>,
    #[serde(default)]
    recent_downloads: Option<u64>,
}

impl CratesPayload {
    fn into_record(self, name: &str) -> PackageRecord {
        let created_at = self
            .crate_info
            .created_at
            .as_deref()
            .and_then(|s| OffsetDateTime::parse(s, &Iso8601::DEFAULT).ok());

        let repository_url = self.crate_info.repository.or(self.crate_info.homepage);
        let downloads_30d = self.crate_info.recent_downloads.or(self.crate_info.downloads);

        PackageRecord {
            name: name.to_string(),
            ecosystem: Ecosystem::Cargo,
            exists: true,
            created_at,
            downloads_30d,
            repository_url,
            starjacked: None,
            provenance_verified: None,
        }
    }
}
