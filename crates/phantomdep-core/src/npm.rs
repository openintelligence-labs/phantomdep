use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Iso8601;

use crate::checker::PackageRecord;
use crate::verdict::Ecosystem;

const NPM_USER_AGENT: &str = concat!("phantomdep/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct NpmClient {
    http: Client,
    base_url: String,
}

impl NpmClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent(NPM_USER_AGENT)
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .context("building reqwest client")?;
        Ok(Self {
            http,
            base_url: "https://registry.npmjs.org".into(),
        })
    }

    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into();
        self
    }

    pub async fn lookup(&self, name: &str) -> Result<PackageRecord> {
        // Scoped names like @org/pkg must URL-encode the slash.
        let encoded = name.replace('/', "%2f");
        let url = format!("{}/{}", self.base_url, encoded);
        let resp = self
            .http
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;

        match resp.status() {
            StatusCode::NOT_FOUND => Ok(PackageRecord::missing(name, Ecosystem::Npm)),
            s if s.is_success() => {
                let payload: NpmPayload = resp.json().await.context("parsing npm JSON payload")?;
                Ok(payload.into_record(name))
            }
            other => Err(anyhow::anyhow!(
                "npm registry returned unexpected status {other} for {name}"
            )),
        }
    }
}

#[derive(Debug, Deserialize)]
struct NpmPayload {
    #[serde(default)]
    time: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    repository: Option<NpmRepository>,
    #[serde(default)]
    homepage: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NpmRepository {
    Url(String),
    Object {
        #[serde(default)]
        url: Option<String>,
    },
}

impl NpmPayload {
    fn into_record(self, name: &str) -> PackageRecord {
        let created_at = self
            .time
            .as_ref()
            .and_then(|m| m.get("created"))
            .and_then(|v| v.as_str())
            .and_then(|s| OffsetDateTime::parse(s, &Iso8601::DEFAULT).ok());

        let repository_url = match self.repository {
            Some(NpmRepository::Url(s)) => Some(s),
            Some(NpmRepository::Object { url }) => url,
            None => self.homepage,
        }
        .map(strip_npm_repo_prefix);

        PackageRecord {
            name: name.to_string(),
            ecosystem: Ecosystem::Npm,
            exists: true,
            created_at,
            downloads_30d: None,
            repository_url,
            starjacked: None,
            provenance_verified: None,
        }
    }
}

/// npm sometimes stores `git+https://...` or `git@github.com:...` — strip the prefix
/// so we have a comparable URL.
fn strip_npm_repo_prefix(s: String) -> String {
    if let Some(rest) = s.strip_prefix("git+") {
        return rest.to_string();
    }
    if let Some(rest) = s.strip_prefix("git://") {
        return format!("https://{rest}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_git_plus_prefix() {
        assert_eq!(
            strip_npm_repo_prefix("git+https://github.com/foo/bar.git".into()),
            "https://github.com/foo/bar.git"
        );
    }

    #[test]
    fn strips_git_protocol() {
        assert_eq!(
            strip_npm_repo_prefix("git://github.com/foo/bar.git".into()),
            "https://github.com/foo/bar.git"
        );
    }
}
