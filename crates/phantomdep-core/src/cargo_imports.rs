use std::collections::BTreeSet;

use once_cell::sync::Lazy;
use regex::Regex;

/// Match a TOML dependency table header. Captures the kind so callers can pick.
static TABLE_HEADER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^\s*\[(?P<header>(?:dependencies|dev-dependencies|build-dependencies|target\.[^\]]+\.(?:dependencies|dev-dependencies|build-dependencies)))\]",
    )
    .unwrap()
});

/// Match a key in a deps table: `name = "version"` or `name = { ... }` or `name.version = ...`.
static KEY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^\s*(?P<name>[A-Za-z][A-Za-z0-9_-]*)\s*[.=]"#).unwrap()
});

/// Match an inline-table `package = "real-name"` field used to rename a dep.
static PACKAGE_FIELD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"package\s*=\s*"([^"]+)""#).unwrap());

/// Extract the set of crate names declared in a Cargo.toml.
/// Honors `package = "..."` rename so `foo = { package = "real-foo" }` resolves to `real-foo`.
pub fn extract_cargo_deps(source: &str) -> BTreeSet<String> {
    let mut crates: BTreeSet<String> = BTreeSet::new();

    let stripped = strip_toml_comments(source);
    let headers: Vec<(usize, usize)> = TABLE_HEADER_RE
        .find_iter(&stripped)
        .map(|m| (m.start(), m.end()))
        .collect();

    for (i, (_start, end)) in headers.iter().enumerate() {
        let body_start = *end;
        let body_end = headers
            .get(i + 1)
            .map(|(next_start, _)| *next_start)
            .unwrap_or(stripped.len());
        let body = &stripped[body_start..body_end];

        for key_cap in KEY_RE.captures_iter(body) {
            let alias = key_cap.name("name").unwrap().as_str();
            // Look for an inline package = "..." rename on the same logical row.
            // Find the first newline after this key — the row spans until then if no opening { ;
            // for inline-table form `name = { ... }` we need to scan until the matching `}`.
            let key_start = key_cap.get(0).unwrap().start();
            let after_eq = match body[key_start..].find('=') {
                Some(pos) => key_start + pos + 1,
                None => key_start,
            };
            let scan_until = scan_inline_value_end(&body[after_eq..]);
            let row = &body[after_eq..(after_eq + scan_until).min(body.len())];
            let real = PACKAGE_FIELD_RE
                .captures(row)
                .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
                .unwrap_or_else(|| alias.to_string());
            crates.insert(real);
        }
    }

    crates
}

fn strip_toml_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    for line in src.lines() {
        let mut in_str = false;
        let mut quote = b'"';
        let mut chars = line.bytes().enumerate();
        let mut cut = line.len();
        while let Some((i, b)) = chars.next() {
            if in_str {
                if b == b'\\' {
                    let _ = chars.next();
                    continue;
                }
                if b == quote {
                    in_str = false;
                }
                continue;
            }
            if b == b'"' || b == b'\'' {
                in_str = true;
                quote = b;
                continue;
            }
            if b == b'#' {
                cut = i;
                break;
            }
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

/// Find the end of an inline value starting at the first non-whitespace char.
/// Handles `"string"`, `42`, `true`, `{ inline = "table" }`, multiline `[ array ]`.
fn scan_inline_value_end(rest: &str) -> usize {
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if i >= bytes.len() {
        return rest.len();
    }
    match bytes[i] {
        b'{' => find_matching(bytes, i, b'{', b'}'),
        b'[' => find_matching(bytes, i, b'[', b']'),
        _ => {
            // Single-line value: stop at end of line.
            let nl = rest[i..].find('\n').unwrap_or(rest.len() - i);
            i + nl
        }
    }
}

fn find_matching(bytes: &[u8], start: usize, open: u8, close: u8) -> usize {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut quote = b'"';
    let mut i = start;
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
        if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                return i + 1;
            }
        }
        i += 1;
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(s: &str) -> Vec<String> {
        extract_cargo_deps(s).into_iter().collect()
    }

    #[test]
    fn finds_simple_deps() {
        let pkgs = extract(
            r#"
[package]
name = "x"

[dependencies]
serde = "1"
tokio = "1.0"
anyhow = "1"
"#,
        );
        assert_eq!(pkgs, vec!["anyhow", "serde", "tokio"]);
    }

    #[test]
    fn finds_dev_and_build_deps() {
        let pkgs = extract(
            r#"
[dependencies]
serde = "1"

[dev-dependencies]
proptest = "1"

[build-dependencies]
cc = "1"
"#,
        );
        assert_eq!(pkgs, vec!["cc", "proptest", "serde"]);
    }

    #[test]
    fn handles_inline_table_form() {
        let pkgs = extract(
            r#"
[dependencies]
serde = { version = "1", features = ["derive"] }
clap = { version = "4", default-features = false }
"#,
        );
        assert_eq!(pkgs, vec!["clap", "serde"]);
    }

    #[test]
    fn honors_package_rename() {
        let pkgs = extract(
            r#"
[dependencies]
my-alias = { version = "1", package = "real-crate" }
serde = "1"
"#,
        );
        assert_eq!(pkgs, vec!["real-crate", "serde"]);
    }

    #[test]
    fn ignores_comments() {
        let pkgs = extract(
            r#"
[dependencies]
# fake = "1"
serde = "1"  # real
"#,
        );
        assert_eq!(pkgs, vec!["serde"]);
    }

    #[test]
    fn handles_target_specific_deps() {
        let pkgs = extract(
            r#"
[target.'cfg(unix)'.dependencies]
nix = "0.27"

[dependencies]
serde = "1"
"#,
        );
        assert_eq!(pkgs, vec!["nix", "serde"]);
    }
}
