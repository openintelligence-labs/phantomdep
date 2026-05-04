//! Claude Code PreToolUse hook handler.
//!
//! Reads a hook event JSON on stdin, inspects:
//!
//! - Bash tool calls matching install commands (pip, uv, poetry, npm, pnpm, yarn, cargo, go),
//! - Write/Edit/MultiEdit tool calls touching dependency manifests,
//!
//! and emits a hook decision on stdout. Per the Claude Code hooks spec, this
//! is a synchronous gate: returning a JSON object with `decision: "block"`
//! prevents the tool call.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::cargo_imports::extract_cargo_deps;
use crate::evidence::EvidenceBundle;
use crate::go_imports::extract_gomod_requires;
use crate::install_args::Manager;
use crate::jsimports::extract_npm_packages;
use crate::lookup::Lookup;
use crate::phantom_db::PhantomDb;
use crate::pyimports::extract_pypi_packages;
use crate::pyproject::extract_pyproject_deps;
use crate::requirements::extract_requirements;
use crate::resolve::Resolver;
use crate::verdict::{Action, Ecosystem};

#[derive(Debug, Deserialize)]
pub struct HookEvent {
    /// Tool the hook is firing for (e.g. "Bash", "Write", "Edit", "MultiEdit").
    #[serde(default, rename = "tool_name")]
    pub tool_name: Option<String>,
    #[serde(default, rename = "tool_input")]
    pub tool_input: serde_json::Value,
    // Other fields we don't currently use: hook_event_name, session_id, etc.
}

#[derive(Debug, Serialize)]
pub struct HookDecision {
    /// `block` to deny the tool call; `approve` to explicitly allow; absent for default behaviour.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<&'static str>,
    /// Human-readable reason; surfaced back to Claude when blocking.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// Hook-author opaque metadata; useful for telemetry-free auditing in logs.
    #[serde(rename = "hookSpecificOutput", skip_serializing_if = "Option::is_none")]
    pub hook_specific: Option<serde_json::Value>,
}

impl HookDecision {
    fn allow(reason: impl Into<String>) -> Self {
        Self {
            decision: None,
            reason: reason.into(),
            hook_specific: None,
        }
    }

    fn block(reason: impl Into<String>) -> Self {
        Self {
            decision: Some("block"),
            reason: reason.into(),
            hook_specific: None,
        }
    }
}

/// Outcome of the PreToolUse evaluation, before being serialised.
pub struct HookEvaluation {
    pub decision: HookDecision,
    /// Worst action seen across validated packages (used for exit code).
    pub worst_action: Action,
    /// All bundles that contributed to the decision (for `--verbose` output).
    pub bundles: Vec<EvidenceBundle>,
}

pub async fn evaluate(
    event: HookEvent,
    lookup: Arc<Lookup>,
    db: Arc<PhantomDb>,
) -> Result<HookEvaluation> {
    let tool = event.tool_name.unwrap_or_default();
    match tool.as_str() {
        "Bash" => evaluate_bash(event.tool_input, lookup, db).await,
        "Write" | "Edit" | "MultiEdit" => evaluate_write(event.tool_input, lookup, db).await,
        _ => Ok(HookEvaluation {
            decision: HookDecision::allow("phantomdep: tool not gated"),
            worst_action: Action::Allow,
            bundles: vec![],
        }),
    }
}

async fn evaluate_bash(
    input: serde_json::Value,
    lookup: Arc<Lookup>,
    db: Arc<PhantomDb>,
) -> Result<HookEvaluation> {
    let command = input
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if command.is_empty() {
        return Ok(HookEvaluation {
            decision: HookDecision::allow("phantomdep: empty bash"),
            worst_action: Action::Allow,
            bundles: vec![],
        });
    }

    // Split chained shell commands and inspect each one. Conservative: split
    // on `&&`, `||`, `;`, and `|` so that `pip install x && echo done` still
    // gates on the install.
    let mut all_bundles: Vec<EvidenceBundle> = Vec::new();
    let mut worst = Action::Allow;
    for chunk in split_shell_pipeline(command) {
        let tokens = match shell_words_split(chunk) {
            Some(t) => t,
            None => continue,
        };
        if tokens.is_empty() {
            continue;
        }
        let program = tokens[0].clone();
        let manager = match Manager::from_program(&program) {
            Some(m) => m,
            None => continue,
        };
        let parsed = crate::install_args::parse(manager, &tokens[1..]);
        if parsed.no_packages && parsed.requirement_files.is_empty() {
            continue;
        }
        let mut names = parsed.packages.clone();
        for path in &parsed.requirement_files {
            if let Ok(text) = std::fs::read_to_string(path) {
                for n in extract_requirements(&text) {
                    if !names.contains(&n) {
                        names.push(n);
                    }
                }
            }
        }
        if names.is_empty() {
            continue;
        }
        let bundles = validate_many(&names, parsed.ecosystem, &lookup, &db).await?;
        for b in &bundles {
            if action_rank(b.action) > action_rank(worst) {
                worst = b.action;
            }
        }
        all_bundles.extend(bundles);
    }

    Ok(decide(all_bundles, worst))
}

async fn evaluate_write(
    input: serde_json::Value,
    lookup: Arc<Lookup>,
    db: Arc<PhantomDb>,
) -> Result<HookEvaluation> {
    let file_path = input
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // Write provides `content`, Edit provides `new_string`, MultiEdit provides
    // `edits: [{old_string, new_string}, ...]`. We collect all candidate text
    // into one buffer so the same parser can run.
    let content = collect_write_text(&input);
    if file_path.is_empty() || content.is_empty() {
        return Ok(HookEvaluation {
            decision: HookDecision::allow("phantomdep: nothing to inspect"),
            worst_action: Action::Allow,
            bundles: vec![],
        });
    }
    let content = content.as_str();
    let path = Path::new(file_path);
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    let (ecosystem, names): (Ecosystem, Vec<String>) = match (file_name, ext) {
        ("requirements.txt", _) | ("constraints.txt", _) => (
            Ecosystem::Pypi,
            extract_requirements(content).into_iter().collect(),
        ),
        ("pyproject.toml", _) => (
            Ecosystem::Pypi,
            extract_pyproject_deps(content).into_iter().collect(),
        ),
        ("Cargo.toml", _) => (
            Ecosystem::Cargo,
            extract_cargo_deps(content).into_iter().collect(),
        ),
        ("go.mod", _) => (
            Ecosystem::Go,
            extract_gomod_requires(content).into_iter().collect(),
        ),
        (_, "py" | "pyi") => (
            Ecosystem::Pypi,
            extract_pypi_packages(content).into_iter().collect(),
        ),
        (_, "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts") => (
            Ecosystem::Npm,
            extract_npm_packages(content).into_iter().collect(),
        ),
        (_, "go") => (
            Ecosystem::Go,
            crate::go_imports::extract_go_imports(content)
                .into_iter()
                .collect(),
        ),
        _ => {
            return Ok(HookEvaluation {
                decision: HookDecision::allow("phantomdep: file type not gated"),
                worst_action: Action::Allow,
                bundles: vec![],
            });
        }
    };

    if names.is_empty() {
        return Ok(HookEvaluation {
            decision: HookDecision::allow("phantomdep: no imports detected"),
            worst_action: Action::Allow,
            bundles: vec![],
        });
    }

    let bundles = validate_many(&names, ecosystem, &lookup, &db).await?;
    let mut worst = Action::Allow;
    for b in &bundles {
        if action_rank(b.action) > action_rank(worst) {
            worst = b.action;
        }
    }
    Ok(decide(bundles, worst))
}

/// Maximum concurrent registry lookups inside a single hook/MCP evaluation.
/// Big enough to be fast on a 50-import file, small enough not to hammer
/// PyPI/npm with hundreds of simultaneous requests on a huge manifest.
pub const VALIDATE_CONCURRENCY: usize = 16;

/// Validate a list of package names with bounded concurrency. Used by the
/// PreToolUse hook handler and the MCP `validate_imports` tool.
pub async fn validate_many(
    names: &[String],
    ecosystem: Ecosystem,
    lookup: &Lookup,
    db: &PhantomDb,
) -> Result<Vec<EvidenceBundle>> {
    use futures::stream::{self, StreamExt};

    let resolver = Resolver::new(db);
    let bundles: Vec<EvidenceBundle> = stream::iter(names.iter().cloned())
        .map(|name| async move {
            let record = lookup.lookup(&name, ecosystem).await;
            (name, record)
        })
        .buffer_unordered(VALIDATE_CONCURRENCY)
        .map(|(name, record)| match record {
            Ok(r) => resolver.resolve(&name, ecosystem, r),
            Err(err) => {
                let mut b = EvidenceBundle::new(name.clone(), ecosystem);
                b.verdict = crate::verdict::Verdict::Unknown;
                b.action = Action::Warn;
                b.evidence.push(crate::evidence::Evidence::Note {
                    source: "lookup".into(),
                    message: format!("registry lookup failed: {err}"),
                });
                b
            }
        })
        .collect()
        .await;
    Ok(bundles)
}

fn decide(bundles: Vec<EvidenceBundle>, worst: Action) -> HookEvaluation {
    let blocked: Vec<&EvidenceBundle> = bundles
        .iter()
        .filter(|b| matches!(b.action, Action::Block))
        .collect();
    let warned: Vec<&EvidenceBundle> = bundles
        .iter()
        .filter(|b| matches!(b.action, Action::Warn))
        .collect();

    if !blocked.is_empty() {
        let names: Vec<String> = blocked
            .iter()
            .map(|b| {
                let mut s = format!("{} ({:?})", b.name, b.verdict);
                if let Some(fix) = b.fixes.first() {
                    s.push_str(&format!(" → did you mean: {}", fix.replacement));
                }
                s
            })
            .collect();
        let reason = format!(
            "PhantomDep blocked {} package(s): {}",
            blocked.len(),
            names.join("; ")
        );
        return HookEvaluation {
            decision: HookDecision::block(reason),
            worst_action: worst,
            bundles,
        };
    }

    if !warned.is_empty() {
        let names: Vec<String> = warned.iter().map(|b| b.name.clone()).collect();
        let reason = format!(
            "PhantomDep flagged {} package(s) as WARN: {}",
            warned.len(),
            names.join(", ")
        );
        return HookEvaluation {
            decision: HookDecision::allow(reason),
            worst_action: worst,
            bundles,
        };
    }

    HookEvaluation {
        decision: HookDecision::allow(format!(
            "PhantomDep cleared {} package(s)",
            bundles.len()
        )),
        worst_action: worst,
        bundles,
    }
}

fn action_rank(a: Action) -> u8 {
    match a {
        Action::Block => 2,
        Action::Warn => 1,
        Action::Allow => 0,
    }
}

/// Pull every chunk of *new* text from a Write/Edit/MultiEdit tool input.
/// Old text is intentionally ignored — we only validate what's being added.
fn collect_write_text(input: &serde_json::Value) -> String {
    let mut buf = String::new();
    if let Some(s) = input.get("content").and_then(|v| v.as_str()) {
        buf.push_str(s);
        buf.push('\n');
    }
    if let Some(s) = input.get("new_string").and_then(|v| v.as_str()) {
        buf.push_str(s);
        buf.push('\n');
    }
    if let Some(edits) = input.get("edits").and_then(|v| v.as_array()) {
        for edit in edits {
            if let Some(s) = edit.get("new_string").and_then(|v| v.as_str()) {
                buf.push_str(s);
                buf.push('\n');
            }
        }
    }
    buf
}

/// Split a shell command line at top-level pipeline operators (`;`, `&&`, `||`, `|`).
/// Quoted regions are respected.
fn split_shell_pipeline(command: &str) -> Vec<&str> {
    let bytes = command.as_bytes();
    let mut out: Vec<&str> = Vec::new();
    let mut in_str = false;
    let mut quote = b'"';
    let mut start = 0usize;
    let mut i = 0usize;
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
        let next_is_double = i + 1 < bytes.len()
            && ((b == b'&' && bytes[i + 1] == b'&') || (b == b'|' && bytes[i + 1] == b'|'));
        let split_len = if next_is_double {
            2
        } else if b == b';' || b == b'|' || b == b'\n' {
            1
        } else {
            0
        };
        if split_len > 0 {
            let chunk = command[start..i].trim();
            if !chunk.is_empty() {
                out.push(chunk);
            }
            i += split_len;
            start = i;
            continue;
        }
        i += 1;
    }
    let last = command[start..].trim();
    if !last.is_empty() {
        out.push(last);
    }
    out
}

/// Minimal shell-style tokenizer: respects single + double quotes and backslash escapes.
fn shell_words_split(input: &str) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    let mut in_squote = false;
    let mut in_dquote = false;
    while let Some(c) = chars.next() {
        if in_squote {
            if c == '\'' {
                in_squote = false;
            } else {
                current.push(c);
            }
            continue;
        }
        if in_dquote {
            if c == '\\' {
                if let Some(n) = chars.next() {
                    current.push(n);
                }
            } else if c == '"' {
                in_dquote = false;
            } else {
                current.push(c);
            }
            continue;
        }
        match c {
            '\'' => in_squote = true,
            '"' => in_dquote = true,
            '\\' => {
                if let Some(n) = chars.next() {
                    current.push(n);
                }
            }
            ' ' | '\t' | '\n' => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(c),
        }
    }
    if in_squote || in_dquote {
        return None;
    }
    if !current.is_empty() {
        out.push(current);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_pipeline_basic() {
        let parts = split_shell_pipeline("pip install foo && echo done");
        assert_eq!(parts, vec!["pip install foo", "echo done"]);
    }

    #[test]
    fn split_pipeline_pipe() {
        let parts = split_shell_pipeline("npm i react | tee log");
        assert_eq!(parts, vec!["npm i react", "tee log"]);
    }

    #[test]
    fn split_pipeline_respects_quotes() {
        let parts = split_shell_pipeline(r#"echo "a && b" ; pip install foo"#);
        assert_eq!(parts, vec![r#"echo "a && b""#, "pip install foo"]);
    }

    #[test]
    fn shell_words_handles_quotes() {
        let toks = shell_words_split(r#"pip install "my pkg" 'other'"#).unwrap();
        assert_eq!(toks, vec!["pip", "install", "my pkg", "other"]);
    }

    #[test]
    fn shell_words_unbalanced_returns_none() {
        assert!(shell_words_split(r#"pip install "broken"#).is_none());
    }

    #[test]
    fn collect_write_text_handles_all_shapes() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"content":"first"}"#).unwrap();
        assert!(collect_write_text(&v).contains("first"));

        let v: serde_json::Value =
            serde_json::from_str(r#"{"new_string":"edited"}"#).unwrap();
        assert!(collect_write_text(&v).contains("edited"));

        let v: serde_json::Value = serde_json::from_str(
            r#"{"edits":[{"new_string":"a"},{"new_string":"b"}]}"#,
        )
        .unwrap();
        let collected = collect_write_text(&v);
        assert!(collected.contains("a"));
        assert!(collected.contains("b"));
    }

    #[test]
    fn decide_block_includes_did_you_mean() {
        use crate::evidence::Fix;
        use crate::verdict::Verdict;
        let mut b = EvidenceBundle::new("huggingface-cli", Ecosystem::Pypi);
        b.verdict = Verdict::Squatted;
        b.action = Action::Block;
        b.fixes.push(Fix {
            replacement: "huggingface_hub".into(),
            confidence: 0.9,
        });
        let ev = decide(vec![b], Action::Block);
        assert_eq!(ev.decision.decision, Some("block"));
        assert!(ev.decision.reason.contains("huggingface-cli"));
        assert!(ev.decision.reason.contains("huggingface_hub"));
    }
}
