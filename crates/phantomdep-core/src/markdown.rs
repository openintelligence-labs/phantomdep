//! Markdown PR-comment renderer for scan reports.
//! Designed to be the body of a sticky GitHub PR comment posted by the Action.

use std::fmt::Write;

use crate::scan::{Finding, ScanReport};
use crate::verdict::{Action, Verdict};

pub fn report_to_markdown(report: &ScanReport) -> String {
    let blocks = report
        .findings
        .iter()
        .filter(|f| f.bundle.action == Action::Block)
        .count();
    let warns = report
        .findings
        .iter()
        .filter(|f| f.bundle.action == Action::Warn)
        .count();
    let allows = report
        .findings
        .iter()
        .filter(|f| f.bundle.action == Action::Allow)
        .count();

    let mut s = String::new();
    let _ = writeln!(s, "## PhantomDep scan");
    let _ = writeln!(s);

    let header_emoji = if blocks > 0 {
        "🛑"
    } else if warns > 0 {
        "⚠️"
    } else {
        "✅"
    };
    let _ = writeln!(
        s,
        "{header_emoji} **{blocks} block · {warns} warn · {allows} allow** across {pkgs} packages in {files} files",
        pkgs = report.packages_seen,
        files = report.files_scanned,
    );
    let _ = writeln!(s);

    if blocks == 0 && warns == 0 {
        let _ = writeln!(
            s,
            "No phantom, squatted, malicious, or lookalike packages detected."
        );
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "<sub>PhantomDep · evidence-backed dependency firewall · [docs](https://github.com/openintelligence-labs/phantomdep)</sub>"
        );
        return s;
    }

    let _ = writeln!(s, "| Verdict | Package | Ecosystem | Did you mean | Files |");
    let _ = writeln!(s, "|---|---|---|---|---|");
    for f in report
        .findings
        .iter()
        .filter(|f| !matches!(f.bundle.action, Action::Allow))
    {
        let did_you_mean = f
            .bundle
            .fixes
            .first()
            .map(|fix| format!("`{}`", fix.replacement))
            .unwrap_or_else(|| "—".into());
        let files_str = files_blurb(f, &report.root);
        let _ = writeln!(
            s,
            "| {} | `{}` | `{}` | {} | {} |",
            verdict_label(f.bundle.verdict),
            f.package,
            f.ecosystem.as_str(),
            did_you_mean,
            files_str,
        );
    }
    let _ = writeln!(s);

    let _ = writeln!(s, "<details><summary>Why these verdicts?</summary>");
    let _ = writeln!(s);
    for f in report
        .findings
        .iter()
        .filter(|f| !matches!(f.bundle.action, Action::Allow))
    {
        let _ = writeln!(
            s,
            "- **`{}`** ({:?}, confidence {:.2})",
            f.package, f.bundle.verdict, f.bundle.confidence
        );
        for ev in &f.bundle.evidence {
            let _ = writeln!(s, "  - {}", crate::evidence_short_text(ev));
        }
    }
    let _ = writeln!(s);
    let _ = writeln!(s, "</details>");
    let _ = writeln!(s);

    let _ = writeln!(
        s,
        "<sub>PhantomDep · evidence-backed dependency firewall · [docs](https://github.com/openintelligence-labs/phantomdep)</sub>"
    );
    s
}

fn verdict_label(v: Verdict) -> &'static str {
    match v {
        Verdict::Phantom => "🛑 PHANTOM",
        Verdict::KnownMalicious => "🛑 MALICIOUS",
        Verdict::Squatted => "🛑 SQUATTED",
        Verdict::InternalCollision => "🛑 COLLISION",
        Verdict::ApiMismatch => "⚠️ API_MISMATCH",
        Verdict::Lookalike => "⚠️ LOOKALIKE",
        Verdict::Real => "✅ REAL",
        Verdict::Unknown => "❔ UNKNOWN",
    }
}

fn files_blurb(f: &Finding, root: &std::path::Path) -> String {
    if f.files.is_empty() {
        return "—".into();
    }
    let mut parts: Vec<String> = f
        .files
        .iter()
        .take(3)
        .map(|p| {
            format!(
                "`{}`",
                p.strip_prefix(root).unwrap_or(p).display()
            )
        })
        .collect();
    if f.files.len() > 3 {
        parts.push(format!("+{} more", f.files.len() - 3));
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evidence::EvidenceBundle;
    use crate::scan::{Finding, ScanReport};
    use crate::verdict::Ecosystem;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    #[test]
    fn empty_report_produces_clean_message() {
        let report = ScanReport {
            root: PathBuf::from("/tmp"),
            files_scanned: 5,
            packages_seen: 3,
            findings: vec![],
        };
        let md = report_to_markdown(&report);
        assert!(md.contains("0 block"));
        assert!(md.contains("No phantom"));
    }

    #[test]
    fn block_finding_renders_table_row() {
        let mut bundle = EvidenceBundle::new("nope", Ecosystem::Pypi);
        bundle.verdict = Verdict::Phantom;
        bundle.action = Action::Block;
        let report = ScanReport {
            root: PathBuf::from("/tmp"),
            files_scanned: 1,
            packages_seen: 1,
            findings: vec![Finding {
                package: "nope".into(),
                ecosystem: Ecosystem::Pypi,
                files: BTreeSet::new(),
                bundle,
            }],
        };
        let md = report_to_markdown(&report);
        assert!(md.contains("1 block"));
        assert!(md.contains("PHANTOM"));
        assert!(md.contains("`nope`"));
    }
}
