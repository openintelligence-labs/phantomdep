//! Minimal `pyproject.toml` dependency extractor.
//!
//! Targets PEP 621 `[project] dependencies = [...]` and `optional-dependencies`,
//! plus Poetry's `[tool.poetry.dependencies]` and `[tool.poetry.group.*.dependencies]`,
//! plus PDM's `[tool.pdm]` style. Conservative regex parser — same posture as
//! the rest of phantomdep-core: when in doubt, include the name.

use std::collections::BTreeSet;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::requirements::extract_requirements;

/// Match the `dependencies = [` start line under `[project]`.
static PROJECT_DEPS_START: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*(?:dependencies|requires|optional-dependencies(?:\.[A-Za-z0-9_-]+)?)\s*=\s*\[")
        .unwrap()
});

/// Match `name = "spec"` or `name = { ... }` lines under a Poetry deps table.
static POETRY_DEP_LINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?m)^\s*([A-Za-z][A-Za-z0-9_.-]*)\s*="#).unwrap());

static POETRY_TABLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?m)^\s*\[tool\.poetry(?:\.group\.[A-Za-z0-9_-]+)?\.dependencies\]\s*$"#,
    )
    .unwrap()
});

/// Extract distribution names from a pyproject.toml file.
pub fn extract_pyproject_deps(source: &str) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();

    // PEP 621: `dependencies = [` array of PEP 508 strings.
    for m in PROJECT_DEPS_START.find_iter(source) {
        let after = &source[m.end()..];
        let end = match find_matching_bracket(after) {
            Some(i) => i,
            None => continue,
        };
        let inner = &after[..end];
        // Strings inside the array; very small parser.
        for spec in extract_quoted_strings(inner) {
            for name in extract_requirements(&spec) {
                names.insert(name);
            }
        }
    }

    // Poetry-style tables.
    for m in POETRY_TABLE.find_iter(source) {
        let after = &source[m.end()..];
        let next_table = next_table_header(after);
        let body = &after[..next_table];
        for cap in POETRY_DEP_LINE.captures_iter(body) {
            let name = cap.get(1).unwrap().as_str();
            if name == "python" {
                continue; // python version constraint, not a package
            }
            names.insert(name.to_ascii_lowercase());
        }
    }

    names
}

fn find_matching_bracket(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 1i32;
    let mut in_str = false;
    let mut quote = b'"';
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == quote {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' {
            in_str = true;
            quote = b;
            i += 1;
            continue;
        }
        if b == b'[' {
            depth += 1;
        } else if b == b']' {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn extract_quoted_strings(inner: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' || b == b'\'' {
            let quote = b;
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            if i <= bytes.len() {
                let raw = &inner[start..i.min(inner.len())];
                out.push(raw.to_string());
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

fn next_table_header(s: &str) -> usize {
    s.find("\n[").map(|i| i + 1).unwrap_or(s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(s: &str) -> Vec<String> {
        extract_pyproject_deps(s).into_iter().collect()
    }

    #[test]
    fn pep621_dependencies_array() {
        let pkgs = extract(
            r#"
[project]
name = "x"
dependencies = [
    "requests>=2.31",
    "fastapi[all]==0.110",
    "uvicorn",
]
"#,
        );
        assert_eq!(pkgs, vec!["fastapi", "requests", "uvicorn"]);
    }

    #[test]
    fn poetry_dependencies_table() {
        let pkgs = extract(
            r#"
[tool.poetry.dependencies]
python = "^3.10"
requests = "^2.31"
fastapi = { version = "^0.110", extras = ["all"] }

[tool.poetry.group.dev.dependencies]
pytest = "^8"
ruff = "^0.5"
"#,
        );
        let mut s = pkgs;
        s.sort();
        assert_eq!(s, vec!["fastapi", "pytest", "requests", "ruff"]);
    }
}
