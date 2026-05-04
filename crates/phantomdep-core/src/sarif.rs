//! SARIF 2.1.0 output for GitHub Code Scanning ingestion.
//! Spec: https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html

use serde::Serialize;
use serde_json::{json, Value};

use crate::scan::{Finding, ScanReport};
use crate::verdict::{Action, Verdict};

const TOOL_NAME: &str = "PhantomDep";
const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
const INFORMATION_URI: &str = "https://github.com/openintelligence-labs/phantomdep";

/// Convert a scan report to a SARIF JSON value.
pub fn report_to_sarif(report: &ScanReport) -> Value {
    let rules = sarif_rules();
    let results: Vec<Value> = report
        .findings
        .iter()
        .filter(|f| !matches!(f.bundle.action, Action::Allow))
        .flat_map(|f| finding_to_results(f, &report.root))
        .collect();

    json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": TOOL_NAME,
                    "version": TOOL_VERSION,
                    "informationUri": INFORMATION_URI,
                    "shortDescription": {
                        "text": "Local-first dependency firewall for AI coding agents"
                    },
                    "rules": rules,
                }
            },
            "results": results,
        }]
    })
}

fn sarif_rules() -> Vec<Value> {
    vec![
        rule(
            "phantomdep/phantom",
            "Hallucinated package",
            "Package does not exist on the registry. Likely an LLM hallucination.",
            "error",
        ),
        rule(
            "phantomdep/squatted",
            "Slop-squatted package",
            "Package matches a known LLM-hallucinated name and was registered to capture installs.",
            "error",
        ),
        rule(
            "phantomdep/known-malicious",
            "Known malicious package",
            "Package is listed in a public malicious-packages feed.",
            "error",
        ),
        rule(
            "phantomdep/internal-collision",
            "Internal-name collision (dependency confusion)",
            "Public package matches a known internal/scoped name.",
            "error",
        ),
        rule(
            "phantomdep/api-mismatch",
            "API mismatch",
            "Package exists but does not export the symbols the importing code uses.",
            "warning",
        ),
        rule(
            "phantomdep/lookalike",
            "Lookalike package",
            "Package name is within edit distance of a popular package.",
            "warning",
        ),
        rule(
            "phantomdep/unknown",
            "Unknown package",
            "PhantomDep could not reach the registry; treating as unknown.",
            "note",
        ),
    ]
}

fn rule(id: &str, name: &str, full: &str, level: &str) -> Value {
    json!({
        "id": id,
        "name": name,
        "shortDescription": { "text": name },
        "fullDescription": { "text": full },
        "defaultConfiguration": { "level": level },
        "helpUri": format!("{}#{}", INFORMATION_URI, id.replace('/', "-")),
    })
}

fn finding_to_results(f: &Finding, root: &std::path::Path) -> Vec<Value> {
    let rule_id = match f.bundle.verdict {
        Verdict::Phantom => "phantomdep/phantom",
        Verdict::Squatted => "phantomdep/squatted",
        Verdict::KnownMalicious => "phantomdep/known-malicious",
        Verdict::InternalCollision => "phantomdep/internal-collision",
        Verdict::ApiMismatch => "phantomdep/api-mismatch",
        Verdict::Lookalike => "phantomdep/lookalike",
        Verdict::Unknown => "phantomdep/unknown",
        Verdict::Real => return vec![],
    };
    let level = match f.bundle.action {
        Action::Block => "error",
        Action::Warn => "warning",
        Action::Allow => "note",
    };
    let message_text = build_message(f);

    if f.files.is_empty() {
        return vec![json!({
            "ruleId": rule_id,
            "level": level,
            "message": { "text": message_text },
            "properties": properties(f),
        })];
    }

    f.files
        .iter()
        .map(|file| {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(file)
                .to_string_lossy()
                .into_owned();
            json!({
                "ruleId": rule_id,
                "level": level,
                "message": { "text": message_text.clone() },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": rel }
                    }
                }],
                "properties": properties(f),
            })
        })
        .collect()
}

fn build_message(f: &Finding) -> String {
    let mut s = format!(
        "{} on {}: {} (verdict {:?}, action {:?})",
        f.package,
        f.ecosystem.as_str(),
        verdict_blurb(f.bundle.verdict),
        f.bundle.verdict,
        f.bundle.action,
    );
    if let Some(fix) = f.bundle.fixes.first() {
        s.push_str(&format!(". Did you mean: {}", fix.replacement));
    }
    s
}

fn verdict_blurb(v: Verdict) -> &'static str {
    match v {
        Verdict::Phantom => "package not found on the registry",
        Verdict::Squatted => "matches a known LLM-hallucinated name that was squatted",
        Verdict::KnownMalicious => "listed in a public malicious-packages feed",
        Verdict::InternalCollision => "collides with a known internal/scoped name",
        Verdict::ApiMismatch => "package exists but does not export the symbols used",
        Verdict::Lookalike => "name is within edit distance of a popular package",
        Verdict::Real => "package is real",
        Verdict::Unknown => "could not reach registry; verdict unknown",
    }
}

#[derive(Serialize)]
struct Properties<'a> {
    ecosystem: &'a str,
    package: &'a str,
    confidence: f32,
    risk_score: u8,
    phantom_db_snapshot: Option<&'a str>,
}

fn properties(f: &Finding) -> Value {
    serde_json::to_value(Properties {
        ecosystem: f.ecosystem.as_str(),
        package: &f.package,
        confidence: f.bundle.confidence,
        risk_score: f.bundle.risk_score,
        phantom_db_snapshot: f.bundle.phantom_db_snapshot.as_deref(),
    })
    .unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::EvidenceBundle;
    use crate::scan::{Finding, ScanReport};
    use crate::verdict::{Ecosystem, Verdict};
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn empty_report() -> ScanReport {
        ScanReport {
            root: PathBuf::from("/tmp/scan"),
            files_scanned: 0,
            packages_seen: 0,
            findings: vec![],
        }
    }

    #[test]
    fn empty_report_yields_valid_sarif_skeleton() {
        let v = report_to_sarif(&empty_report());
        assert_eq!(v["version"], "2.1.0");
        assert_eq!(v["runs"][0]["tool"]["driver"]["name"], "PhantomDep");
        assert_eq!(v["runs"][0]["results"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn phantom_finding_becomes_error_result() {
        let mut bundle = EvidenceBundle::new("nope", Ecosystem::Pypi);
        bundle.verdict = Verdict::Phantom;
        bundle.action = Action::Block;
        let mut files = BTreeSet::new();
        files.insert(PathBuf::from("/tmp/scan/app.py"));
        let report = ScanReport {
            root: PathBuf::from("/tmp/scan"),
            files_scanned: 1,
            packages_seen: 1,
            findings: vec![Finding {
                package: "nope".into(),
                ecosystem: Ecosystem::Pypi,
                files,
                bundle,
            }],
        };
        let v = report_to_sarif(&report);
        let results = v["runs"][0]["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["ruleId"], "phantomdep/phantom");
        assert_eq!(results[0]["level"], "error");
        assert_eq!(
            results[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "app.py"
        );
    }

    #[test]
    fn allow_findings_are_excluded_from_sarif() {
        let mut bundle = EvidenceBundle::new("requests", Ecosystem::Pypi);
        bundle.verdict = Verdict::Real;
        bundle.action = Action::Allow;
        let report = ScanReport {
            root: PathBuf::from("/tmp"),
            files_scanned: 1,
            packages_seen: 1,
            findings: vec![Finding {
                package: "requests".into(),
                ecosystem: Ecosystem::Pypi,
                files: BTreeSet::new(),
                bundle,
            }],
        };
        let v = report_to_sarif(&report);
        assert_eq!(v["runs"][0]["results"].as_array().unwrap().len(), 0);
    }
}
