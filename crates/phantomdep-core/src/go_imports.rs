use std::collections::BTreeSet;

use once_cell::sync::Lazy;
use regex::Regex;

/// Match a single `require module v1.2.3` line.
static REQUIRE_LINE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^\s*require\s+(?P<module>[^\s]+)\s+v[0-9][^\s]*\s*(?://.*)?$"#).unwrap()
});

/// Match the start of a `require ( ... )` block.
static REQUIRE_BLOCK_START_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^\s*require\s*\(\s*$").unwrap());

/// Match a single line inside a require block: `module v1.2.3`.
static REQUIRE_BLOCK_LINE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?m)^\s*(?P<module>[^\s]+)\s+v[0-9][^\s]*\s*(?://.*)?$"#).unwrap());

/// Match a Go source `import "..."` (single-line) or `import ( ... )` block contents.
static GO_IMPORT_LINE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^\s*import\s+(?:[A-Za-z_][A-Za-z0-9_]*\s+)?"(?P<path>[^"]+)""#).unwrap()
});
static GO_IMPORT_BLOCK_LINE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^\s*(?:[A-Za-z_][A-Za-z0-9_]*\s+)?"(?P<path>[^"\s]+)""#).unwrap()
});
static GO_IMPORT_BLOCK_START_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^\s*import\s*\(").unwrap());
static GO_IMPORT_BLOCK_END_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)^\s*\)\s*$").unwrap());

/// Extract module paths declared in a go.mod file.
pub fn extract_gomod_requires(source: &str) -> BTreeSet<String> {
    let mut modules: BTreeSet<String> = BTreeSet::new();

    for cap in REQUIRE_LINE_RE.captures_iter(source) {
        let m = cap.name("module").unwrap().as_str();
        if !m.is_empty() {
            modules.insert(m.to_string());
        }
    }

    // Walk require ( ... ) blocks.
    for start_match in REQUIRE_BLOCK_START_RE.find_iter(source) {
        let block_start = start_match.end();
        let after = &source[block_start..];
        let end = after
            .find(')')
            .map(|p| block_start + p)
            .unwrap_or(source.len());
        let body = &source[block_start..end];
        for cap in REQUIRE_BLOCK_LINE_RE.captures_iter(body) {
            let m = cap.name("module").unwrap().as_str();
            if !m.is_empty() && !m.starts_with("//") {
                modules.insert(m.to_string());
            }
        }
    }

    modules
        .into_iter()
        .filter(|m| is_external_module(m))
        .collect()
}

/// Extract module paths imported by a .go source file.
/// Returns module-root paths (e.g. `github.com/foo/bar` from `github.com/foo/bar/sub/pkg`).
pub fn extract_go_imports(source: &str) -> BTreeSet<String> {
    let mut paths: BTreeSet<String> = BTreeSet::new();

    for cap in GO_IMPORT_LINE_RE.captures_iter(source) {
        if let Some(p) = cap.name("path") {
            paths.insert(p.as_str().to_string());
        }
    }

    for start in GO_IMPORT_BLOCK_START_RE.find_iter(source) {
        let body_start = start.end();
        let after = &source[body_start..];
        let block_end = GO_IMPORT_BLOCK_END_RE
            .find(after)
            .map(|m| body_start + m.start())
            .unwrap_or(source.len());
        let body = &source[body_start..block_end];
        for cap in GO_IMPORT_BLOCK_LINE_RE.captures_iter(body) {
            if let Some(p) = cap.name("path") {
                paths.insert(p.as_str().to_string());
            }
        }
    }

    paths
        .into_iter()
        .filter(|p| is_external_module(p))
        .map(|p| module_root(&p))
        .collect()
}

/// Stdlib imports have no `.` in the first path segment (e.g. `fmt`, `net/http`).
fn is_external_module(path: &str) -> bool {
    let first = path.split('/').next().unwrap_or("");
    first.contains('.')
}

/// Reduce an import path to its module root.
///
/// For known forges (github.com / gitlab.com / bitbucket.org) the module is the
/// `host/owner/repo` triple, optionally extended with a `/vN` major-version
/// suffix. For other hosts (k8s.io, go.uber.org, golang.org/x, etc.) we cannot
/// infer the module root from the path alone — Go resolves it via the
/// `?go-get=1` redirect dance — so we leave the path unchanged and let the
/// proxy.golang.org existence check decide.
fn module_root(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.is_empty() {
        return path.to_string();
    }
    let host = parts[0];
    let known_forge = matches!(host, "github.com" | "gitlab.com" | "bitbucket.org");
    if known_forge && parts.len() >= 3 {
        let take = if parts.len() >= 4 && is_major_version(parts[3]) {
            4
        } else {
            3
        };
        return parts[..take.min(parts.len())].join("/");
    }
    path.to_string()
}

fn is_major_version(s: &str) -> bool {
    s.starts_with('v')
        && s[1..].chars().all(|c| c.is_ascii_digit())
        && !s[1..].is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vec_extract_gomod(s: &str) -> Vec<String> {
        extract_gomod_requires(s).into_iter().collect()
    }

    fn vec_extract_go(s: &str) -> Vec<String> {
        extract_go_imports(s).into_iter().collect()
    }

    #[test]
    fn parses_single_line_require() {
        let mods = vec_extract_gomod(
            r#"module example.com/me
go 1.21
require github.com/spf13/cobra v1.8.0
"#,
        );
        assert_eq!(mods, vec!["github.com/spf13/cobra"]);
    }

    #[test]
    fn parses_require_block() {
        let mods = vec_extract_gomod(
            r#"module example.com/me
go 1.21
require (
    github.com/spf13/cobra v1.8.0
    github.com/aws/aws-sdk-go-v2 v1.0.0 // indirect
    k8s.io/client-go v0.30.0
)
"#,
        );
        assert_eq!(
            mods,
            vec![
                "github.com/aws/aws-sdk-go-v2",
                "github.com/spf13/cobra",
                "k8s.io/client-go",
            ]
        );
    }

    #[test]
    fn parses_single_import() {
        let imps = vec_extract_go(
            r#"package main

import "github.com/spf13/cobra"
"#,
        );
        assert_eq!(imps, vec!["github.com/spf13/cobra"]);
    }

    #[test]
    fn parses_import_block_and_strips_known_forge_subpath() {
        let imps = vec_extract_go(
            r#"package main

import (
    "fmt"
    "net/http"
    "github.com/spf13/cobra"
    "github.com/aws/aws-sdk-go-v2/service/s3"
    "k8s.io/client-go/kubernetes"
)
"#,
        );
        // github.com paths get stripped to host/owner/repo. Custom hosts
        // (k8s.io, go.uber.org, golang.org/x, ...) keep the full path
        // because the module root can only be resolved via ?go-get=1.
        assert_eq!(
            imps,
            vec![
                "github.com/aws/aws-sdk-go-v2",
                "github.com/spf13/cobra",
                "k8s.io/client-go/kubernetes",
            ]
        );
    }

    #[test]
    fn keeps_major_version_suffix() {
        assert_eq!(
            module_root("github.com/aws/aws-sdk-go-v2/service/s3"),
            "github.com/aws/aws-sdk-go-v2"
        );
        // /v2 as a true major-version suffix at position 3 should be kept
        assert_eq!(
            module_root("github.com/foo/bar/v2/sub/pkg"),
            "github.com/foo/bar/v2"
        );
    }

    #[test]
    fn skips_aliased_imports() {
        let imps = vec_extract_go(
            r#"package main

import (
    foo "github.com/spf13/cobra"
    _ "github.com/sirupsen/logrus"
)
"#,
        );
        let mut sorted = imps;
        sorted.sort();
        assert_eq!(
            sorted,
            vec!["github.com/sirupsen/logrus", "github.com/spf13/cobra"]
        );
    }
}
