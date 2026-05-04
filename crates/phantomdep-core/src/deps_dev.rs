//! deps.dev v3 unified package metadata client.
//!
//! Per architecture §5.6, deps.dev is the primary unified source. We use it
//! to fill in fields the native registries don't expose (or expose awkwardly):
//! creation time, license, version count, repository URL, OpenSSF Scorecard.
//!
//! Spec: <https://docs.deps.dev/api/v3/>

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::verdict::Ecosystem;

const USER_AGENT: &str = concat!("phantomdep/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct DepsDevClient {
    http: Client,
    base_url: String,
}

impl DepsDevClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .context("building reqwest client")?;
        Ok(Self {
            http,
            base_url: "https://api.deps.dev".into(),
        })
    }

    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into();
        self
    }

    /// Fetch package-level metadata. Returns None if the package is unknown to deps.dev.
    pub async fn package(&self, ecosystem: Ecosystem, name: &str) -> Result<Option<DepsDevPackage>> {
        let system = system_name(ecosystem);
        // deps.dev wants slashes URL-encoded; reqwest does this for query args but not path
        // segments, so encode manually.
        let encoded = url_encode(name);
        let url = format!(
            "{}/v3/systems/{}/packages/{}",
            self.base_url, system, encoded
        );
        let resp = self
            .http
            .get(&url)
            .header("Accept", "application/json")
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;
        match resp.status() {
            StatusCode::NOT_FOUND => Ok(None),
            s if s.is_success() => {
                let payload: DepsDevPackage = resp
                    .json()
                    .await
                    .context("parsing deps.dev package payload")?;
                Ok(Some(payload))
            }
            other => Err(anyhow::anyhow!(
                "deps.dev returned unexpected status {other} for {name}"
            )),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DepsDevPackage {
    #[serde(default, rename = "packageKey")]
    pub package_key: Option<DepsDevPackageKey>,
    #[serde(default)]
    pub versions: Vec<DepsDevVersion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DepsDevPackageKey {
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DepsDevVersion {
    #[serde(default, rename = "versionKey")]
    pub version_key: Option<DepsDevVersionKey>,
    #[serde(default, rename = "publishedAt")]
    pub published_at: Option<String>,
    #[serde(default, rename = "isDefault")]
    pub is_default: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DepsDevVersionKey {
    #[serde(default)]
    pub version: Option<String>,
}

fn system_name(eco: Ecosystem) -> &'static str {
    // deps.dev system names per https://docs.deps.dev/api/v3/
    match eco {
        Ecosystem::Pypi => "pypi",
        Ecosystem::Npm => "npm",
        Ecosystem::Cargo => "cargo",
        Ecosystem::Go => "go",
        Ecosystem::Maven => "maven",
    }
}

fn url_encode(s: &str) -> String {
    // Minimal percent-encoding for path segments. We deliberately avoid pulling
    // in a full URL-encoder dep — deps.dev names are constrained to package
    // names + scoped names like @org/pkg.
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            other => out.push_str(&format!("%{:02X}", other)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encodes_slashes() {
        assert_eq!(url_encode("@scope/pkg"), "%40scope%2Fpkg");
    }

    #[test]
    fn system_names_match_deps_dev() {
        assert_eq!(system_name(Ecosystem::Pypi), "pypi");
        assert_eq!(system_name(Ecosystem::Npm), "npm");
        assert_eq!(system_name(Ecosystem::Cargo), "cargo");
        assert_eq!(system_name(Ecosystem::Go), "go");
        assert_eq!(system_name(Ecosystem::Maven), "maven");
    }
}
