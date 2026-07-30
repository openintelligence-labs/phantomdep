use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use futures::future::BoxFuture;
use futures::stream::{FuturesUnordered, StreamExt};
use futures::FutureExt;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::cargo_imports::extract_cargo_deps;
use crate::evidence::EvidenceBundle;
use crate::go_imports::{extract_go_imports, extract_gomod_requires};
use crate::jsimports::extract_npm_packages;
use crate::lookup::Lookup;
use crate::phantom_db::PhantomDb;
use crate::pyimports::extract_pypi_packages;
use crate::pyproject::extract_pyproject_deps;
use crate::requirements::extract_requirements;
use crate::resolve::Resolver;
use crate::verdict::{Action, Ecosystem, Verdict};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub root: PathBuf,
    pub files_scanned: usize,
    pub packages_seen: usize,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub package: String,
    pub ecosystem: Ecosystem,
    pub files: BTreeSet<PathBuf>,
    pub bundle: EvidenceBundle,
}

impl ScanReport {
    pub fn worst_action(&self) -> Action {
        self.findings
            .iter()
            .map(|f| f.bundle.action)
            .max_by_key(|a| match a {
                Action::Block => 2,
                Action::Warn => 1,
                Action::Allow => 0,
            })
            .unwrap_or(Action::Allow)
    }

    pub fn count_by_verdict(&self) -> BTreeMap<&'static str, usize> {
        let mut out = BTreeMap::new();
        for f in &self.findings {
            let key = verdict_label(f.bundle.verdict);
            *out.entry(key).or_insert(0) += 1;
        }
        out
    }

    pub fn count_by_ecosystem(&self) -> BTreeMap<&'static str, usize> {
        let mut out = BTreeMap::new();
        for f in &self.findings {
            *out.entry(f.ecosystem.as_str()).or_insert(0) += 1;
        }
        out
    }
}

fn verdict_label(v: Verdict) -> &'static str {
    match v {
        Verdict::Phantom => "PHANTOM",
        Verdict::KnownMalicious => "MALICIOUS",
        Verdict::Squatted => "SQUATTED",
        Verdict::InternalCollision => "COLLISION",
        Verdict::ApiMismatch => "API_MISMATCH",
        Verdict::Lookalike => "LOOKALIKE",
        Verdict::Real => "REAL",
        Verdict::Unknown => "UNKNOWN",
    }
}

/// Multi-ecosystem scan: walks Python + JS/TS files, dispatches to the right
/// parser, resolves each package against its registry. Bounded concurrency.
pub async fn scan_path(
    root: &Path,
    lookup: Arc<Lookup>,
    db: &PhantomDb,
    concurrency: usize,
) -> Result<ScanReport> {
    let imports = collect_imports(root)?;
    let files_scanned = imports.files_scanned;
    let by_pair = imports.by_package; // (Ecosystem, name) → files
    let packages_seen = by_pair.len();

    let resolver = Resolver::new(db);
    let mut findings = Vec::new();

    type Task = BoxFuture<
        'static,
        (
            Ecosystem,
            String,
            BTreeSet<PathBuf>,
            Result<crate::checker::PackageRecord>,
        ),
    >;
    let mut tasks: FuturesUnordered<Task> = FuturesUnordered::new();
    let mut iter = by_pair.into_iter();
    let cap = concurrency.max(1);

    let spawn = |ecosystem: Ecosystem,
                 pkg: String,
                 files: BTreeSet<PathBuf>,
                 lookup: Arc<Lookup>|
     -> Task {
        async move {
            let record = lookup.lookup(&pkg, ecosystem).await;
            (ecosystem, pkg, files, record)
        }
        .boxed()
    };

    for _ in 0..cap {
        if let Some(((ecosystem, pkg), files)) = iter.next() {
            tasks.push(spawn(ecosystem, pkg, files, Arc::clone(&lookup)));
        }
    }

    while let Some((ecosystem, pkg, files, record)) = tasks.next().await {
        if let Some(((next_ecosystem, next_pkg), next_files)) = iter.next() {
            tasks.push(spawn(
                next_ecosystem,
                next_pkg,
                next_files,
                Arc::clone(&lookup),
            ));
        }

        let bundle = match record {
            Ok(r) => resolver.resolve(&pkg, ecosystem, r),
            Err(err) => {
                let mut b = EvidenceBundle::new(pkg.clone(), ecosystem);
                b.verdict = Verdict::Unknown;
                b.action = Action::Warn;
                b.evidence.push(crate::evidence::Evidence::Note {
                    source: "lookup".into(),
                    message: format!("registry lookup failed: {err}"),
                });
                b
            }
        };

        findings.push(Finding {
            package: pkg,
            ecosystem,
            files,
            bundle,
        });
    }

    findings.sort_by_key(|f| {
        (
            -(action_rank(f.bundle.action) as i32),
            f.ecosystem.as_str(),
            f.package.clone(),
        )
    });

    Ok(ScanReport {
        root: root.to_path_buf(),
        files_scanned,
        packages_seen,
        findings,
    })
}

/// Backwards-compatible alias.
pub async fn scan_python_path(
    root: &Path,
    lookup: Arc<Lookup>,
    db: &PhantomDb,
    concurrency: usize,
) -> Result<ScanReport> {
    scan_path(root, lookup, db, concurrency).await
}

fn action_rank(a: Action) -> u8 {
    match a {
        Action::Block => 2,
        Action::Warn => 1,
        Action::Allow => 0,
    }
}

struct CollectedImports {
    files_scanned: usize,
    by_package: BTreeMap<(Ecosystem, String), BTreeSet<PathBuf>>,
}

/// Strategies for parsing a particular file: extract package names of one ecosystem.
enum ParseStrategy {
    PyImports,
    JsImports,
    Requirements,
    Pyproject,
    CargoToml,
    GoMod,
    GoImports,
}

impl ParseStrategy {
    fn ecosystem(&self) -> Ecosystem {
        match self {
            Self::PyImports | Self::Requirements | Self::Pyproject => Ecosystem::Pypi,
            Self::JsImports => Ecosystem::Npm,
            Self::CargoToml => Ecosystem::Cargo,
            Self::GoMod | Self::GoImports => Ecosystem::Go,
        }
    }

    fn extract(&self, source: &str) -> Vec<String> {
        match self {
            Self::PyImports => extract_pypi_packages(source).into_iter().collect(),
            Self::JsImports => extract_npm_packages(source).into_iter().collect(),
            Self::Requirements => extract_requirements(source).into_iter().collect(),
            Self::Pyproject => extract_pyproject_deps(source).into_iter().collect(),
            Self::CargoToml => extract_cargo_deps(source).into_iter().collect(),
            Self::GoMod => extract_gomod_requires(source).into_iter().collect(),
            Self::GoImports => extract_go_imports(source).into_iter().collect(),
        }
    }
}

/// pip-style requirements files: `requirements.txt`, `constraints.txt`, plus
/// the common variants `requirements-dev.txt` / `dev-requirements.txt`.
fn is_requirements_file(file_name: &str) -> bool {
    if !file_name.ends_with(".txt") {
        return false;
    }
    file_name == "constraints.txt"
        || file_name.starts_with("requirements")
        || file_name.ends_with("requirements.txt")
}

fn strategy_for_path(path: &Path) -> Option<ParseStrategy> {
    let file_name = path.file_name()?.to_str()?;
    if file_name == "Cargo.toml" {
        return Some(ParseStrategy::CargoToml);
    }
    if file_name == "go.mod" {
        return Some(ParseStrategy::GoMod);
    }
    if file_name == "pyproject.toml" {
        return Some(ParseStrategy::Pyproject);
    }
    if is_requirements_file(file_name) {
        return Some(ParseStrategy::Requirements);
    }
    let ext = path.extension()?.to_str()?;
    match ext {
        "py" | "pyi" => Some(ParseStrategy::PyImports),
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => {
            Some(ParseStrategy::JsImports)
        }
        "go" => Some(ParseStrategy::GoImports),
        _ => None,
    }
}

fn collect_imports(root: &Path) -> Result<CollectedImports> {
    let mut files_scanned = 0usize;
    let mut by_package: BTreeMap<(Ecosystem, String), BTreeSet<PathBuf>> = BTreeMap::new();

    for entry in WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .build()
        .flatten()
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(strategy) = strategy_for_path(path) else {
            continue;
        };
        let ecosystem = strategy.ecosystem();
        files_scanned += 1;
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        for pkg in strategy.extract(text) {
            by_package
                .entry((ecosystem, pkg))
                .or_default()
                .insert(path.to_path_buf());
        }
    }

    Ok(CollectedImports {
        files_scanned,
        by_package,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_mixed_project() -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "phantomdep-mixedscantest-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("a.py"),
            "import requests\nfrom yaml import safe_load\n",
        )
        .unwrap();
        fs::write(
            dir.join("b.ts"),
            "import React from 'react';\nimport { z } from 'zod';\n",
        )
        .unwrap();
        fs::write(
            dir.join("c.js"),
            "const _ = require('lodash');\nimport '@anthropic-ai/sdk';\n",
        )
        .unwrap();
        dir
    }

    #[tokio::test]
    async fn collects_python_and_js_imports() {
        let dir = write_mixed_project();
        let imports = collect_imports(&dir).unwrap();
        assert_eq!(imports.files_scanned, 3);
        assert!(imports
            .by_package
            .contains_key(&(Ecosystem::Pypi, "requests".into())));
        assert!(imports
            .by_package
            .contains_key(&(Ecosystem::Npm, "react".into())));
        assert!(imports
            .by_package
            .contains_key(&(Ecosystem::Npm, "@anthropic-ai/sdk".into())));
    }

    fn write_manifest_only_project() -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "phantomdep-manifestscantest-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("requirements.txt"),
            "requests\nlangchain_vectorstore_utils_pro==1.0\n",
        )
        .unwrap();
        fs::write(
            dir.join("pyproject.toml"),
            "[project]\nname = \"x\"\ndependencies = [\"fastapi>=0.110\"]\n",
        )
        .unwrap();
        dir
    }

    // Regression test for issue #2: a directory containing only manifests must
    // still be scanned — manifests are first-class scan inputs.
    #[test]
    fn collects_packages_from_manifest_only_directory() {
        let dir = write_manifest_only_project();
        let imports = collect_imports(&dir).unwrap();
        assert_eq!(imports.files_scanned, 2);
        assert!(imports
            .by_package
            .contains_key(&(Ecosystem::Pypi, "requests".into())));
        assert!(imports
            .by_package
            .contains_key(&(Ecosystem::Pypi, "langchain_vectorstore_utils_pro".into())));
        assert!(imports
            .by_package
            .contains_key(&(Ecosystem::Pypi, "fastapi".into())));
    }

    // Single-file scans of a manifest must work too: `phantomdep scan requirements.txt`.
    #[test]
    fn collects_packages_from_single_manifest_file() {
        let dir = write_manifest_only_project();
        let imports = collect_imports(&dir.join("requirements.txt")).unwrap();
        assert_eq!(imports.files_scanned, 1);
        assert!(imports
            .by_package
            .contains_key(&(Ecosystem::Pypi, "langchain_vectorstore_utils_pro".into())));
    }

    #[test]
    fn strategy_recognizes_manifest_names() {
        assert!(matches!(
            strategy_for_path(Path::new("a/requirements.txt")),
            Some(ParseStrategy::Requirements)
        ));
        assert!(matches!(
            strategy_for_path(Path::new("requirements-dev.txt")),
            Some(ParseStrategy::Requirements)
        ));
        assert!(matches!(
            strategy_for_path(Path::new("dev-requirements.txt")),
            Some(ParseStrategy::Requirements)
        ));
        assert!(matches!(
            strategy_for_path(Path::new("constraints.txt")),
            Some(ParseStrategy::Requirements)
        ));
        assert!(matches!(
            strategy_for_path(Path::new("pyproject.toml")),
            Some(ParseStrategy::Pyproject)
        ));
        // Arbitrary text files are still not scan inputs.
        assert!(strategy_for_path(Path::new("notes.txt")).is_none());
        assert!(strategy_for_path(Path::new("README.md")).is_none());
    }

    #[test]
    fn worst_action_picks_block() {
        let report = ScanReport {
            root: PathBuf::from("/tmp"),
            files_scanned: 0,
            packages_seen: 0,
            findings: vec![],
        };
        assert_eq!(report.worst_action(), Action::Allow);

        let mut bundle1 = EvidenceBundle::new("a", Ecosystem::Pypi);
        bundle1.action = Action::Allow;
        let mut bundle2 = EvidenceBundle::new("b", Ecosystem::Npm);
        bundle2.action = Action::Block;
        let report = ScanReport {
            root: PathBuf::from("/tmp"),
            files_scanned: 0,
            packages_seen: 0,
            findings: vec![
                Finding {
                    package: "a".into(),
                    ecosystem: Ecosystem::Pypi,
                    files: BTreeSet::new(),
                    bundle: bundle1,
                },
                Finding {
                    package: "b".into(),
                    ecosystem: Ecosystem::Npm,
                    files: BTreeSet::new(),
                    bundle: bundle2,
                },
            ],
        };
        assert_eq!(report.worst_action(), Action::Block);
    }
}
