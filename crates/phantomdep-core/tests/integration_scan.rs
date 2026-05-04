use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use phantomdep_core::{Action, Lookup, PhantomDb, Verdict, scan_python_path};

fn fixture_project() -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "phantomdep-int-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("good.py"),
        "import os\nimport sys\nimport requests\nfrom yaml import safe_load\n",
    )
    .unwrap();
    fs::write(
        dir.join("bad.py"),
        "import super_fast_json_parser_phantomdep_test\nfrom huggingface_cli import login\n",
    )
    .unwrap();
    dir
}

#[tokio::test]
async fn scan_resolves_imports_and_returns_findings() {
    let dir = fixture_project();
    let db = PhantomDb::bootstrap();
    // Use no cache + offline-style — the lookup will hit the network for real
    // packages but that's fine for an integration test against PyPI.
    let lookup = Arc::new(Lookup::new(None).unwrap());
    let report = scan_python_path(&dir, lookup, &db, 8).await.unwrap();

    assert_eq!(report.files_scanned, 2);
    assert!(report.packages_seen >= 4);

    let names: Vec<&str> = report.findings.iter().map(|f| f.package.as_str()).collect();
    assert!(names.contains(&"requests"));
    assert!(names.contains(&"pyyaml"));
    assert!(names.contains(&"super_fast_json_parser_phantomdep_test"));

    let phantom = report
        .findings
        .iter()
        .find(|f| f.package == "super_fast_json_parser_phantomdep_test")
        .unwrap();
    assert_eq!(phantom.bundle.verdict, Verdict::Phantom);
    assert_eq!(phantom.bundle.action, Action::Block);

    assert_eq!(report.worst_action(), Action::Block);
}
