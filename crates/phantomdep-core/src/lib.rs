//! PhantomDep core — verdict engine, evidence model, registry checkers.
//!
//! This crate is intentionally I/O-light at the core. The verdict resolver
//! consumes a `PackageRecord` and produces an `EvidenceBundle`; checkers
//! (PyPI, npm, ...) populate the record from registries. Keep them separable
//! so the resolver remains synchronous, deterministic, and unit-testable
//! without network.

pub mod cache;
pub mod cargo_imports;
pub mod checker;
pub mod crates_io;
pub mod deps_dev;
pub mod evidence;
pub mod go_imports;
pub mod go_proxy;
pub mod hook;
pub mod hook_install;
pub mod install_args;
pub mod jsimports;
pub mod lookup;
pub mod lsp;
pub mod markdown;
pub mod mcp;
pub mod npm;
pub mod phantom_db;
pub mod popular;
pub mod pyimports;
pub mod pyproject;
pub mod pypi;
pub mod requirements;
pub mod resolve;
pub mod sarif;
pub mod scan;
pub mod verdict;

pub use cache::PackageCache;
pub use checker::PackageRecord;
pub use crates_io::CratesClient;
pub use deps_dev::DepsDevClient;
pub use evidence::{Evidence, EvidenceBundle, Fix, evidence_short_text};
pub use go_proxy::GoProxyClient;
pub use hook::{HookDecision, HookEvaluation, HookEvent, evaluate as evaluate_hook};
pub use install_args::{Manager, ParsedInstall, parse as parse_install};
pub use lookup::Lookup;
pub use lsp::LspServer;
pub use mcp::McpServer;
pub use requirements::extract_requirements;
pub use markdown::report_to_markdown;
pub use npm::NpmClient;
pub use phantom_db::{PhantomDb, PhantomEntry, PhantomStatus};
pub use pypi::PypiClient;
pub use resolve::Resolver;
pub use sarif::report_to_sarif;
pub use scan::{Finding, ScanReport, scan_path, scan_python_path};
pub use verdict::{Action, Ecosystem, Verdict};
