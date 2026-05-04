use std::collections::BTreeSet;

use once_cell::sync::Lazy;
use regex::Regex;

/// Node.js built-in modules that should never be flagged as packages.
static NODE_BUILTINS: &[&str] = &[
    "assert", "async_hooks", "buffer", "child_process", "cluster", "console", "constants",
    "crypto", "dgram", "diagnostics_channel", "dns", "domain", "events", "fs", "http",
    "http2", "https", "inspector", "module", "net", "os", "path", "perf_hooks", "process",
    "punycode", "querystring", "readline", "repl", "stream", "string_decoder", "sys",
    "test", "timers", "tls", "trace_events", "tty", "url", "util", "v8", "vm", "wasi",
    "worker_threads", "zlib",
];

/// Regex for ES modules: `import ... from 'pkg'`, `import 'pkg'`, dynamic `import('pkg')`.
static IMPORT_FROM_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)\bimport\b[^'"`;\n]*?\bfrom\s*['"`]([^'"`\n]+)['"`]"#).unwrap()
});
static SIDE_EFFECT_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^\s*import\s*['"`]([^'"`\n]+)['"`]"#).unwrap()
});
static DYNAMIC_IMPORT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\bimport\s*\(\s*['"`]([^'"`\n]+)['"`]"#).unwrap());
static REQUIRE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\brequire\s*\(\s*['"`]([^'"`\n]+)['"`]"#).unwrap());

/// Extract the set of *npm package names* imported by a JS/TS source file.
/// Skips Node builtins, relative paths (./, ../, /), data URIs, and `node:` prefixed imports.
pub fn extract_npm_packages(source: &str) -> BTreeSet<String> {
    // Strip comments + strings handled implicitly: regexes target import statements specifically.
    // We *do* strip line + block comments first to avoid `// import 'fake'` false positives.
    let stripped = strip_comments(source);
    let mut packages: BTreeSet<String> = BTreeSet::new();

    let regexes: &[&Lazy<Regex>] = &[
        &IMPORT_FROM_RE,
        &SIDE_EFFECT_IMPORT_RE,
        &DYNAMIC_IMPORT_RE,
        &REQUIRE_RE,
    ];

    for re in regexes {
        for cap in re.captures_iter(&stripped) {
            if let Some(spec) = cap.get(1).map(|m| m.as_str()) {
                if let Some(pkg) = specifier_to_package(spec) {
                    packages.insert(pkg);
                }
            }
        }
    }

    packages
        .into_iter()
        .filter(|p| !NODE_BUILTINS.iter().any(|b| b == p))
        .collect()
}

/// Convert an import specifier (`@org/pkg/sub`, `lodash/fp`, `./foo`, `node:fs`) into the
/// underlying npm package name, or None if it's not an npm package.
pub fn specifier_to_package(specifier: &str) -> Option<String> {
    let s = specifier.trim();
    if s.is_empty() {
        return None;
    }
    // Relative or absolute paths.
    if s.starts_with('.') || s.starts_with('/') {
        return None;
    }
    // Explicit Node builtins via `node:` prefix.
    if let Some(rest) = s.strip_prefix("node:") {
        let _ = rest; // builtin, never an npm package
        return None;
    }
    // Data / http(s) URIs.
    if s.contains("://") || s.starts_with("data:") {
        return None;
    }

    // Scoped: @org/pkg[/...]  →  @org/pkg
    if let Some(rest) = s.strip_prefix('@') {
        let mut parts = rest.splitn(3, '/');
        let org = parts.next().unwrap_or("");
        let pkg = parts.next();
        return match pkg {
            Some(p) if !p.is_empty() && !org.is_empty() => Some(format!("@{org}/{p}")),
            _ => None,
        };
    }

    // Unscoped: take everything before first slash.
    let pkg = s.split('/').next().unwrap_or(s);
    if pkg.is_empty() {
        None
    } else {
        Some(pkg.to_string())
    }
}

/// Strip JS/TS comments while preserving string literals. We need to be
/// string-aware so that `const x = "// not a comment"` is left intact, and
/// so that an `import 'foo'` literal inside a string isn't accidentally
/// treated as a comment-introducer.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut in_str = false;
    let mut quote = b'"';
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            // Pass strings through unchanged but track end-of-string.
            out.push(b as char);
            if b == b'\\' && i + 1 < bytes.len() {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            if b == quote {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' || b == b'\'' || b == b'`' {
            in_str = true;
            quote = b;
            out.push(b as char);
            i += 1;
            continue;
        }
        // // line comment
        if i + 1 < bytes.len() && b == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // /* block comment */
        if i + 1 < bytes.len() && b == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                if bytes[i] == b'\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        out.push(b as char);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(s: &str) -> Vec<String> {
        extract_npm_packages(s).into_iter().collect()
    }

    #[test]
    fn finds_es_module_imports() {
        let pkgs = extract("import React from 'react';\nimport { z } from 'zod';");
        assert_eq!(pkgs, vec!["react", "zod"]);
    }

    #[test]
    fn handles_scoped_packages() {
        let pkgs = extract(
            "import Anthropic from '@anthropic-ai/sdk';\nimport { x } from '@types/node';",
        );
        assert_eq!(pkgs, vec!["@anthropic-ai/sdk", "@types/node"]);
    }

    #[test]
    fn collapses_subpath_to_package() {
        let pkgs = extract("import { fp } from 'lodash/fp';");
        assert_eq!(pkgs, vec!["lodash"]);
    }

    #[test]
    fn collapses_scoped_subpath() {
        let pkgs = extract("import { x } from '@anthropic-ai/sdk/resources/messages';");
        assert_eq!(pkgs, vec!["@anthropic-ai/sdk"]);
    }

    #[test]
    fn skips_relative_imports() {
        let pkgs = extract(
            "import { foo } from './foo';\nimport { bar } from '../bar';\nimport zod from 'zod';",
        );
        assert_eq!(pkgs, vec!["zod"]);
    }

    #[test]
    fn skips_node_builtins() {
        let pkgs = extract("import fs from 'fs';\nimport path from 'path';\nimport ax from 'axios';");
        assert_eq!(pkgs, vec!["axios"]);
    }

    #[test]
    fn skips_node_prefix() {
        let pkgs = extract("import fs from 'node:fs';\nimport ax from 'axios';");
        assert_eq!(pkgs, vec!["axios"]);
    }

    #[test]
    fn handles_require() {
        let pkgs = extract("const r = require('react');\nconst _ = require('lodash');");
        assert_eq!(pkgs, vec!["lodash", "react"]);
    }

    #[test]
    fn handles_dynamic_import() {
        let pkgs = extract("const x = await import('chalk');");
        assert_eq!(pkgs, vec!["chalk"]);
    }

    #[test]
    fn handles_side_effect_import() {
        let pkgs = extract("import 'reflect-metadata';\nimport { x } from 'rxjs';");
        assert_eq!(pkgs, vec!["reflect-metadata", "rxjs"]);
    }

    #[test]
    fn strings_with_comment_markers_dont_eat_following_imports() {
        // Regression: before string-aware strip, the `//` inside the URL string
        // would be treated as a line comment and eat the rest of the line,
        // including the next import's `from 'react'` clause.
        let pkgs = extract(
            "const url = \"https://example.com/foo\";\nimport React from 'react';\n",
        );
        assert_eq!(pkgs, vec!["react"]);
    }

    #[test]
    fn imports_inside_strings_are_ignored() {
        // The literal `import 'fake'` is inside a string, not real code.
        let pkgs = extract("const txt = \"import 'fake'\";\nimport real from 'react';");
        assert_eq!(pkgs, vec!["react"]);
    }

    #[test]
    fn ignores_imports_in_comments() {
        let pkgs = extract(
            r#"
// import 'fake-pkg';
/* import 'also-fake'; */
import real from 'react';
"#,
        );
        assert_eq!(pkgs, vec!["react"]);
    }

    #[test]
    fn handles_double_and_single_and_template_quotes() {
        let pkgs = extract(
            "import a from \"react\";\nimport b from 'vue';\nconst c = require(`zod`);",
        );
        assert_eq!(pkgs, vec!["react", "vue", "zod"]);
    }
}
