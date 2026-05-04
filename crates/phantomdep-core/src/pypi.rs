use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

use crate::checker::PackageRecord;
use crate::verdict::Ecosystem;

const PYPI_USER_AGENT: &str = concat!("phantomdep/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct PypiClient {
    http: Client,
    base_url: String,
}

impl PypiClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent(PYPI_USER_AGENT)
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .context("building reqwest client")?;
        Ok(Self {
            http,
            base_url: "https://pypi.org".into(),
        })
    }

    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into();
        self
    }

    pub async fn lookup(&self, name: &str) -> Result<PackageRecord> {
        let url = format!("{}/pypi/{}/json", self.base_url, name);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;

        match resp.status() {
            StatusCode::NOT_FOUND => Ok(PackageRecord::missing(name, Ecosystem::Pypi)),
            s if s.is_success() => {
                let payload: PypiPayload =
                    resp.json().await.context("parsing PyPI JSON payload")?;
                Ok(payload.into_record(name))
            }
            other => Err(anyhow::anyhow!(
                "PyPI returned unexpected status {other} for {name}"
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
struct PypiPayload {
    info: PypiInfo,
    #[serde(default)]
    releases: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
struct PypiInfo {
    #[serde(default)]
    project_urls: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    home_page: Option<String>,
}

impl PypiPayload {
    fn into_record(self, name: &str) -> PackageRecord {
        let created_at = self
            .releases
            .values()
            .filter_map(|v| v.as_array())
            .flatten()
            .filter_map(|file| file.get("upload_time_iso_8601").and_then(|s| s.as_str()))
            .filter_map(|s| OffsetDateTime::parse(s, &Iso8601::DEFAULT).ok())
            .min();

        let repository_url = self
            .info
            .project_urls
            .as_ref()
            .and_then(|m| {
                ["Source", "Repository", "Homepage", "source", "repository"]
                    .iter()
                    .find_map(|k| m.get(*k).and_then(|v| v.as_str()).map(String::from))
            })
            .or(self.info.home_page);

        PackageRecord {
            name: name.to_string(),
            ecosystem: Ecosystem::Pypi,
            exists: true,
            created_at,
            downloads_30d: None,
            repository_url,
            starjacked: None,
            provenance_verified: None,
        }
    }
}
