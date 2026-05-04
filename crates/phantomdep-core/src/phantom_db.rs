use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::verdict::Ecosystem;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhantomEntry {
    pub name: String,
    pub ecosystem: Ecosystem,
    pub status: PhantomStatus,
    #[serde(default)]
    pub first_observed: Option<String>,
    #[serde(default)]
    pub intended_target: Option<String>,
    #[serde(default)]
    pub did_you_mean: Vec<String>,
    #[serde(default)]
    pub evidence_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhantomStatus {
    Phantom,
    Squatted,
    Malicious,
}

#[derive(Debug, Clone, Default)]
pub struct PhantomDb {
    entries: HashMap<(Ecosystem, String), PhantomEntry>,
    snapshot: Option<String>,
}

impl PhantomDb {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn bootstrap() -> Self {
        let mut db = Self {
            snapshot: Some("2026-05-03-bootstrap".into()),
            ..Self::default()
        };

        db.insert(PhantomEntry {
            name: "huggingface-cli".into(),
            ecosystem: Ecosystem::Pypi,
            status: PhantomStatus::Squatted,
            first_observed: Some("2024-02-13".into()),
            intended_target: Some("huggingface_hub[cli]".into()),
            did_you_mean: vec!["huggingface_hub".into(), "huggingface-hub".into()],
            evidence_url: Some("https://www.lasso.security/blog/ai-package-hallucinations".into()),
        });

        db.insert(PhantomEntry {
            name: "ccxt-mexc-futures".into(),
            ecosystem: Ecosystem::Pypi,
            status: PhantomStatus::Malicious,
            first_observed: Some("2025-04-01".into()),
            intended_target: Some("ccxt".into()),
            did_you_mean: vec!["ccxt".into()],
            evidence_url: Some(
                "https://thehackernews.com/2025/04/malicious-pypi-package-targets-mexc.html".into(),
            ),
        });

        db.insert(PhantomEntry {
            name: "soopsocks".into(),
            ecosystem: Ecosystem::Pypi,
            status: PhantomStatus::Malicious,
            first_observed: Some("2025-09-01".into()),
            intended_target: None,
            did_you_mean: vec!["pysocks".into()],
            evidence_url: Some(
                "https://thehackernews.com/2025/10/alert-malicious-pypi-package-soopsocks.html"
                    .into(),
            ),
        });

        db.insert(PhantomEntry {
            name: "react-codeshift".into(),
            ecosystem: Ecosystem::Npm,
            status: PhantomStatus::Squatted,
            first_observed: Some("2026-01-01".into()),
            intended_target: Some("jscodeshift".into()),
            did_you_mean: vec!["jscodeshift".into(), "react-codemod".into()],
            evidence_url: None,
        });

        db
    }

    /// Load Phantom-DB entries from a directory laid out as
    /// `<root>/<ecosystem>/<first-letter>/<name>.json`. Falls back to bootstrap
    /// data on top so a partial directory still works.
    pub fn from_dir(root: &Path) -> Result<Self> {
        let mut db = Self::bootstrap();
        let mut count = 0usize;
        if !root.exists() {
            return Ok(db);
        }
        for ecosystem_entry in std::fs::read_dir(root).context("reading phantom-db root")? {
            let ecosystem_dir = ecosystem_entry?;
            if !ecosystem_dir.file_type()?.is_dir() {
                continue;
            }
            for letter_entry in std::fs::read_dir(ecosystem_dir.path())? {
                let letter_dir = letter_entry?;
                if !letter_dir.file_type()?.is_dir() {
                    continue;
                }
                for file_entry in std::fs::read_dir(letter_dir.path())? {
                    let file = file_entry?;
                    if !file.file_type()?.is_file() {
                        continue;
                    }
                    let path = file.path();
                    if path.extension().and_then(|s| s.to_str()) != Some("json") {
                        continue;
                    }
                    let bytes = std::fs::read(&path)
                        .with_context(|| format!("reading {}", path.display()))?;
                    let entry: PhantomEntry = serde_json::from_slice(&bytes)
                        .with_context(|| format!("parsing {}", path.display()))?;
                    db.insert(entry);
                    count += 1;
                }
            }
        }
        if count > 0 {
            db.snapshot = Some(format!("{}+disk:{}", db.snapshot.as_deref().unwrap_or(""), count));
        }
        Ok(db)
    }

    pub fn insert(&mut self, entry: PhantomEntry) {
        let key = (entry.ecosystem, entry.name.to_ascii_lowercase());
        self.entries.insert(key, entry);
    }

    pub fn lookup(&self, name: &str, ecosystem: Ecosystem) -> Option<&PhantomEntry> {
        self.entries.get(&(ecosystem, name.to_ascii_lowercase()))
    }

    pub fn snapshot(&self) -> Option<&str> {
        self.snapshot.as_deref()
    }

    /// Iterate all entries — used by `phantomdep replay`.
    pub fn entries(&self) -> impl Iterator<Item = &PhantomEntry> {
        self.entries.values()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_finds_huggingface_cli() {
        let db = PhantomDb::bootstrap();
        let entry = db.lookup("huggingface-cli", Ecosystem::Pypi).unwrap();
        assert_eq!(entry.status, PhantomStatus::Squatted);
        assert_eq!(entry.intended_target.as_deref(), Some("huggingface_hub[cli]"));
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let db = PhantomDb::bootstrap();
        assert!(db.lookup("HuggingFace-CLI", Ecosystem::Pypi).is_some());
    }

    #[test]
    fn ecosystem_separates_namespaces() {
        let db = PhantomDb::bootstrap();
        assert!(db.lookup("huggingface-cli", Ecosystem::Npm).is_none());
    }
}
