use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};

use crate::checker::PackageRecord;
use crate::verdict::Ecosystem;

const USER_AGENT: &str = concat!("phantomdep/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone)]
pub struct GoProxyClient {
    http: Client,
    base_url: String,
}

impl GoProxyClient {
    pub fn new() -> Result<Self> {
        let http = Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .context("building reqwest client")?;
        Ok(Self {
            http,
            base_url: "https://proxy.golang.org".into(),
        })
    }

    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into();
        self
    }

    pub async fn lookup(&self, module: &str) -> Result<PackageRecord> {
        let encoded = encode_module_path(module);
        let url = format!("{}/{}/@v/list", self.base_url, encoded);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?;

        match resp.status() {
            StatusCode::NOT_FOUND | StatusCode::GONE => {
                Ok(PackageRecord::missing(module, Ecosystem::Go))
            }
            s if s.is_success() => {
                // We don't currently parse @v/list bodies; existence is enough for a
                // baseline verdict. Repository URL is recoverable from the module path
                // itself (e.g. github.com/...), so we record that.
                let mut record = PackageRecord::missing(module, Ecosystem::Go);
                record.exists = true;
                record.repository_url = repo_url_from_module(module);
                Ok(record)
            }
            other => Err(anyhow::anyhow!(
                "go proxy returned unexpected status {other} for {module}"
            )),
        }
    }
}

/// Go's case-encoding for module paths: uppercase letters become `!<lowercase>`.
fn encode_module_path(module: &str) -> String {
    let mut out = String::with_capacity(module.len());
    for c in module.chars() {
        if c.is_ascii_uppercase() {
            out.push('!');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn repo_url_from_module(module: &str) -> Option<String> {
    let parts: Vec<&str> = module.split('/').collect();
    if parts.len() >= 3 {
        let host = parts[0];
        if host == "github.com" || host == "gitlab.com" || host == "bitbucket.org" {
            return Some(format!("https://{host}/{}/{}", parts[1], parts[2]));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_uppercase() {
        assert_eq!(
            encode_module_path("github.com/AlecAivazis/survey/v2"),
            "github.com/!alec!aivazis/survey/v2"
        );
    }

    #[test]
    fn lowercase_unchanged() {
        assert_eq!(
            encode_module_path("github.com/spf13/cobra"),
            "github.com/spf13/cobra"
        );
    }

    #[test]
    fn extracts_github_repo_url() {
        assert_eq!(
            repo_url_from_module("github.com/spf13/cobra"),
            Some("https://github.com/spf13/cobra".to_string())
        );
    }

    #[test]
    fn extracts_repo_with_subpath() {
        assert_eq!(
            repo_url_from_module("github.com/aws/aws-sdk-go-v2/service/s3"),
            Some("https://github.com/aws/aws-sdk-go-v2".to_string())
        );
    }

    #[test]
    fn no_repo_url_for_custom_domains() {
        assert_eq!(repo_url_from_module("k8s.io/client-go"), None);
    }
}
