//! Minimal LSP 3.17 server for PhantomDep.
//!
//! Speaks Content-Length-framed JSON-RPC 2.0 over stdio. Implements only the
//! handlers we need for evidence-backed diagnostics + quick-fix code actions:
//!
//!   initialize / initialized / shutdown / exit
//!   textDocument/didOpen / didChange / didSave / didClose
//!   textDocument/publishDiagnostics    (server → client notification)
//!   textDocument/codeAction            (returns "Did you mean ..." quick-fixes)
//!
//! Position model: LSP positions are 0-indexed (line, character) in UTF-16
//! code units. Our import detection is line-and-column based — see
//! `find_import_range` for the source-text scan that turns a package name
//! back into a `Range`.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, Stdin, Stdout};
use tokio::sync::Mutex;

use crate::evidence::EvidenceBundle;
use crate::hook::validate_many;
use crate::jsimports::extract_npm_packages;
use crate::lookup::Lookup;
use crate::phantom_db::PhantomDb;
use crate::pyimports::extract_pypi_packages;
use crate::verdict::{Action, Ecosystem, Verdict};

const SERVER_NAME: &str = "phantomdep-lsp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct Request {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct Response {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

const PARSE_ERROR: i32 = -32700;
const METHOD_NOT_FOUND: i32 = -32601;

pub struct LspServer {
    lookup: Arc<Lookup>,
    db: Arc<PhantomDb>,
    /// Open documents indexed by URI.
    docs: Mutex<HashMap<String, Document>>,
}

#[derive(Clone, Debug)]
struct Document {
    text: String,
    language_id: String,
    /// Cached findings from the last validation pass — used by `codeAction`.
    findings: Vec<DocumentFinding>,
}

#[derive(Clone, Debug)]
struct DocumentFinding {
    package: String,
    range: Range,
    bundle: EvidenceBundle,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl LspServer {
    pub fn new(lookup: Arc<Lookup>, db: Arc<PhantomDb>) -> Self {
        Self {
            lookup,
            db,
            docs: Mutex::new(HashMap::new()),
        }
    }

    pub async fn serve_stdio(&self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let mut reader = LspReader::new(stdin);
        let writer = Arc::new(Mutex::new(LspWriter::new(stdout)));

        loop {
            let frame = match reader.read_frame().await? {
                Some(f) => f,
                None => break, // EOF
            };
            let req: Request = match serde_json::from_slice(&frame) {
                Ok(r) => r,
                Err(err) => {
                    let resp = Response {
                        jsonrpc: "2.0",
                        id: Value::Null,
                        result: None,
                        error: Some(RpcError {
                            code: PARSE_ERROR,
                            message: format!("parse error: {err}"),
                        }),
                    };
                    write_message(&writer, &serde_json::to_value(&resp)?).await?;
                    continue;
                }
            };
            if req.jsonrpc != "2.0" {
                continue;
            }
            self.handle(req, Arc::clone(&writer)).await?;
        }
        Ok(())
    }

    async fn handle(&self, req: Request, writer: Arc<Mutex<LspWriter<Stdout>>>) -> Result<()> {
        let id = req.id.clone();
        let is_request = id.is_some();
        let method = req.method.as_str();

        match method {
            "initialize" => {
                let result = json!({
                    "capabilities": {
                        "textDocumentSync": 1, // Full sync — simple and correct
                        "codeActionProvider": {
                            "codeActionKinds": ["quickfix"],
                            "resolveProvider": false
                        }
                    },
                    "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION }
                });
                respond(writer, id, Some(result), None).await
            }
            "initialized" => Ok(()),
            "shutdown" => respond(writer, id, Some(Value::Null), None).await,
            "exit" => {
                std::process::exit(0);
            }
            "textDocument/didOpen" => {
                let uri = uri(&req.params, "/textDocument/uri").unwrap_or_default();
                let language_id = string(&req.params, "/textDocument/languageId").unwrap_or_default();
                let version = req
                    .params
                    .pointer("/textDocument/version")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let text = string(&req.params, "/textDocument/text").unwrap_or_default();
                self.upsert_doc(uri.clone(), language_id, version, text)
                    .await;
                self.publish_diagnostics(&uri, &writer).await
            }
            "textDocument/didChange" => {
                let uri = uri(&req.params, "/textDocument/uri").unwrap_or_default();
                let version = req
                    .params
                    .pointer("/textDocument/version")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                // Full-sync mode: we expect a single change with the entire text.
                let text = req
                    .params
                    .pointer("/contentChanges/0/text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let language_id = {
                    let docs = self.docs.lock().await;
                    docs.get(&uri).map(|d| d.language_id.clone()).unwrap_or_default()
                };
                self.upsert_doc(uri.clone(), language_id, version, text)
                    .await;
                self.publish_diagnostics(&uri, &writer).await
            }
            "textDocument/didSave" => {
                let uri = uri(&req.params, "/textDocument/uri").unwrap_or_default();
                self.publish_diagnostics(&uri, &writer).await
            }
            "textDocument/didClose" => {
                let uri = uri(&req.params, "/textDocument/uri").unwrap_or_default();
                let mut docs = self.docs.lock().await;
                docs.remove(&uri);
                Ok(())
            }
            "textDocument/codeAction" => {
                let uri = uri(&req.params, "/textDocument/uri").unwrap_or_default();
                let actions = self.code_actions(&uri).await;
                respond(writer, id, Some(Value::Array(actions)), None).await
            }
            _ => {
                if is_request {
                    let err = RpcError {
                        code: METHOD_NOT_FOUND,
                        message: format!("method not found: {method}"),
                    };
                    respond(writer, id, None, Some(err)).await
                } else {
                    Ok(())
                }
            }
        }
    }

    async fn upsert_doc(
        &self,
        uri: String,
        language_id: String,
        _version: i64,
        text: String,
    ) {
        let mut docs = self.docs.lock().await;
        docs.insert(
            uri,
            Document {
                text,
                language_id,
                findings: Vec::new(),
            },
        );
    }

    async fn publish_diagnostics(
        &self,
        uri: &str,
        writer: &Arc<Mutex<LspWriter<Stdout>>>,
    ) -> Result<()> {
        let (text, language_id) = {
            let docs = self.docs.lock().await;
            match docs.get(uri) {
                Some(d) => (d.text.clone(), d.language_id.clone()),
                None => return Ok(()),
            }
        };

        let (ecosystem, names): (Option<Ecosystem>, Vec<String>) = match language_id.as_str() {
            "python" => (Some(Ecosystem::Pypi), extract_pypi_packages(&text).into_iter().collect()),
            "javascript" | "javascriptreact" | "typescript" | "typescriptreact" => (
                Some(Ecosystem::Npm),
                extract_npm_packages(&text).into_iter().collect(),
            ),
            _ => (None, vec![]),
        };

        let Some(ecosystem) = ecosystem else {
            // Unsupported language — clear any prior diagnostics.
            send_diagnostics(writer, uri, &[]).await?;
            return Ok(());
        };

        let bundles = if names.is_empty() {
            vec![]
        } else {
            validate_many(&names, ecosystem, &self.lookup, &self.db)
                .await
                .unwrap_or_default()
        };

        let mut findings: Vec<DocumentFinding> = Vec::new();
        let mut diagnostics: Vec<Value> = Vec::new();
        for bundle in bundles {
            let Some(range) = find_first_range(&text, &bundle.name, ecosystem) else {
                continue;
            };
            let severity = severity_for(bundle.action);
            if severity == 0 {
                continue;
            }
            let mut diag = json!({
                "range": &range,
                "severity": severity,
                "source": "phantomdep",
                "code": code_for(bundle.verdict),
                "message": diagnostic_message(&bundle),
            });
            if let Some(url) = bundle.explain_url.as_deref() {
                diag["codeDescription"] = json!({ "href": url });
            }
            diagnostics.push(diag);

            findings.push(DocumentFinding {
                package: bundle.name.clone(),
                range,
                bundle,
            });
        }

        // Cache findings on the document for subsequent codeAction queries.
        {
            let mut docs = self.docs.lock().await;
            if let Some(doc) = docs.get_mut(uri) {
                doc.findings = findings;
            }
        }

        send_diagnostics(writer, uri, &diagnostics).await
    }

    async fn code_actions(&self, uri: &str) -> Vec<Value> {
        let docs = self.docs.lock().await;
        let Some(doc) = docs.get(uri) else {
            return vec![];
        };
        let mut actions: Vec<Value> = Vec::new();
        for finding in &doc.findings {
            for fix in &finding.bundle.fixes {
                let title = format!(
                    "PhantomDep: replace `{}` with `{}`",
                    finding.package, fix.replacement
                );
                actions.push(json!({
                    "title": title,
                    "kind": "quickfix",
                    "edit": {
                        "changes": {
                            uri: [{
                                "range": &finding.range,
                                "newText": fix.replacement,
                            }]
                        }
                    }
                }));
            }
        }
        actions
    }
}

fn diagnostic_message(b: &EvidenceBundle) -> String {
    let mut s = format!(
        "PhantomDep: `{}` ({}): {:?} (action {:?}, confidence {:.2})",
        b.name,
        b.ecosystem.as_str(),
        b.verdict,
        b.action,
        b.confidence
    );
    if let Some(fix) = b.fixes.first() {
        s.push_str(&format!(". Did you mean `{}`?", fix.replacement));
    }
    s
}

fn code_for(v: Verdict) -> &'static str {
    match v {
        Verdict::Phantom => "phantom",
        Verdict::Squatted => "squatted",
        Verdict::KnownMalicious => "known-malicious",
        Verdict::InternalCollision => "internal-collision",
        Verdict::ApiMismatch => "api-mismatch",
        Verdict::Lookalike => "lookalike",
        Verdict::Unknown => "unknown",
        Verdict::Real => "real",
    }
}

fn severity_for(action: Action) -> u8 {
    match action {
        Action::Block => 1, // Error
        Action::Warn => 2,  // Warning
        Action::Allow => 0, // No diagnostic emitted
    }
}

/// Find the first occurrence of an import-name token in the source text and
/// return its LSP `Range`. Returns None if we can't locate it.
///
/// Heuristic: look for the package name surrounded by `'`, `"`, or `\`` (npm)
/// or after `import`/`from` (python). For npm scoped names (`@org/pkg`) we
/// match the whole token. We *do not* try to be perfect here — a missing
/// range simply means the diagnostic is suppressed for that finding.
pub fn find_first_range(text: &str, name: &str, ecosystem: Ecosystem) -> Option<Range> {
    // For the npm case, the package name may appear as a substring of a longer
    // path (`@org/pkg/sub`). For PyPI, the name we report can differ from the
    // import (e.g. `pyyaml` from `import yaml`). To be safe we look for the
    // raw `name` *and* a few sensible aliases.
    let candidates: Vec<String> = std::iter::once(name.to_string())
        .chain(reverse_resolver_table(name, ecosystem))
        .collect();

    for needle in &candidates {
        if let Some(range) = locate(text, needle) {
            return Some(range);
        }
    }
    None
}

/// Best-effort byte-offset → (line, utf16-character) conversion.
fn locate(text: &str, needle: &str) -> Option<Range> {
    let pos = text.find(needle)?;
    let prefix = &text[..pos];
    let line = prefix.matches('\n').count() as u32;
    let line_start = prefix.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col = utf16_len(&text[line_start..pos]);
    let end_col = col + utf16_len(needle);
    Some(Range {
        start: Position {
            line,
            character: col,
        },
        end: Position {
            line,
            character: end_col,
        },
    })
}

fn utf16_len(s: &str) -> u32 {
    s.chars().map(|c| c.len_utf16() as u32).sum()
}

/// Map a PyPI distribution name back to the import names that resolve to it.
/// Used so the diagnostic range lands on `import yaml` even when our verdict
/// is for the dist `pyyaml`.
fn reverse_resolver_table(pypi_name: &str, eco: Ecosystem) -> Vec<String> {
    if !matches!(eco, Ecosystem::Pypi) {
        return vec![];
    }
    // Mirror of crate::pyimports::IMPORT_TO_PYPI without exposing it. Keep in
    // sync; tested via the integration test.
    const TABLE: &[(&str, &str)] = &[
        ("yaml", "pyyaml"),
        ("PIL", "pillow"),
        ("cv2", "opencv-python"),
        ("sklearn", "scikit-learn"),
        ("skimage", "scikit-image"),
        ("bs4", "beautifulsoup4"),
        ("dateutil", "python-dateutil"),
        ("dotenv", "python-dotenv"),
        ("magic", "python-magic"),
        ("jose", "python-jose"),
        ("levenshtein", "python-levenshtein"),
        ("OpenSSL", "pyopenssl"),
        ("Crypto", "pycryptodome"),
        ("MySQLdb", "mysqlclient"),
        ("psycopg2", "psycopg2-binary"),
        ("ldap", "python-ldap"),
        ("serial", "pyserial"),
        ("git", "gitpython"),
        ("speech_recognition", "SpeechRecognition"),
        ("attr", "attrs"),
        ("google", "google-api-python-client"),
        ("tensorflow_hub", "tensorflow-hub"),
    ];
    TABLE
        .iter()
        .filter_map(|(import, pypi)| {
            if pypi.eq_ignore_ascii_case(pypi_name) {
                Some((*import).to_string())
            } else {
                None
            }
        })
        .collect()
}

// ----------------------------------------------------------------------------
// JSON-RPC framing
// ----------------------------------------------------------------------------

struct LspReader<R> {
    inner: BufReader<R>,
}

impl<R: tokio::io::AsyncRead + Unpin> LspReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner: BufReader::new(inner),
        }
    }

    /// Read one Content-Length-framed message. Returns Ok(None) on EOF.
    async fn read_frame(&mut self) -> Result<Option<Vec<u8>>> {
        let mut content_length: Option<usize> = None;
        let mut header = String::new();

        loop {
            header.clear();
            let n = self.inner.read_line(&mut header).await?;
            if n == 0 {
                return Ok(None);
            }
            let trimmed = header.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break; // end of headers
            }
            if let Some(rest) = trimmed
                .strip_prefix("Content-Length:")
                .or_else(|| trimmed.strip_prefix("content-length:"))
            {
                content_length = rest.trim().parse().ok();
            }
            // Other headers (Content-Type, etc.) are ignored.
        }

        let len = content_length.context("missing Content-Length header")?;
        let mut body = vec![0u8; len];
        self.inner.read_exact(&mut body).await?;
        Ok(Some(body))
    }
}

// Convenient alias used at construction time.
type _Reader = LspReader<Stdin>;

struct LspWriter<W> {
    inner: W,
}

impl<W: tokio::io::AsyncWrite + Unpin> LspWriter<W> {
    fn new(inner: W) -> Self {
        Self { inner }
    }

    async fn write(&mut self, payload: &[u8]) -> Result<()> {
        let header = format!("Content-Length: {}\r\n\r\n", payload.len());
        self.inner.write_all(header.as_bytes()).await?;
        self.inner.write_all(payload).await?;
        self.inner.flush().await?;
        Ok(())
    }
}

async fn write_message(
    writer: &Arc<Mutex<LspWriter<Stdout>>>,
    value: &Value,
) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let mut w = writer.lock().await;
    w.write(&bytes).await
}

async fn respond(
    writer: Arc<Mutex<LspWriter<Stdout>>>,
    id: Option<Value>,
    result: Option<Value>,
    error: Option<RpcError>,
) -> Result<()> {
    if id.is_none() {
        return Ok(());
    }
    let resp = Response {
        jsonrpc: "2.0",
        id: id.unwrap_or(Value::Null),
        result,
        error,
    };
    write_message(&writer, &serde_json::to_value(&resp)?).await
}

async fn send_diagnostics(
    writer: &Arc<Mutex<LspWriter<Stdout>>>,
    uri: &str,
    diagnostics: &[Value],
) -> Result<()> {
    let notification = json!({
        "jsonrpc": "2.0",
        "method": "textDocument/publishDiagnostics",
        "params": {
            "uri": uri,
            "diagnostics": diagnostics,
        }
    });
    write_message(writer, &notification).await
}

fn uri(params: &Value, pointer: &str) -> Option<String> {
    string(params, pointer)
}

fn string(params: &Value, pointer: &str) -> Option<String> {
    params.pointer(pointer).and_then(|v| v.as_str()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locate_finds_python_import() {
        let src = "import requests\nimport numpy\n";
        let r = locate(src, "requests").unwrap();
        assert_eq!(r.start.line, 0);
        assert_eq!(r.start.character, 7);
        assert_eq!(r.end.character, 15);
    }

    #[test]
    fn locate_finds_on_second_line() {
        let src = "import os\nimport requests\n";
        let r = locate(src, "requests").unwrap();
        assert_eq!(r.start.line, 1);
    }

    #[test]
    fn reverse_resolver_finds_yaml_for_pyyaml() {
        let aliases = reverse_resolver_table("pyyaml", Ecosystem::Pypi);
        assert!(aliases.contains(&"yaml".to_string()));
    }

    #[test]
    fn reverse_resolver_empty_for_npm() {
        assert!(reverse_resolver_table("anything", Ecosystem::Npm).is_empty());
    }

    #[test]
    fn find_first_range_uses_reverse_alias() {
        let src = "import yaml\n";
        let r = find_first_range(src, "pyyaml", Ecosystem::Pypi).unwrap();
        // Should land on `yaml` token, not `pyyaml`.
        assert_eq!(r.start.line, 0);
        assert_eq!(r.end.character - r.start.character, 4); // len of "yaml"
    }

    #[test]
    fn find_first_range_falls_back_to_raw_name() {
        let src = "import requests\n";
        let r = find_first_range(src, "requests", Ecosystem::Pypi).unwrap();
        assert_eq!(r.start.character, 7);
    }

    #[test]
    fn utf16_len_handles_multibyte() {
        assert_eq!(utf16_len(""), 0);
        assert_eq!(utf16_len("abc"), 3);
        assert_eq!(utf16_len("café"), 4); // each char is 1 utf16 unit
        assert_eq!(utf16_len("🚀"), 2); // surrogate pair
    }

    #[test]
    fn severity_maps_actions() {
        assert_eq!(severity_for(Action::Block), 1);
        assert_eq!(severity_for(Action::Warn), 2);
        assert_eq!(severity_for(Action::Allow), 0);
    }

    #[tokio::test]
    async fn read_frame_parses_content_length_message() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize"}"#;
        let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let cursor = std::io::Cursor::new(frame.into_bytes());
        let mut reader = LspReader::new(cursor);
        let bytes = reader.read_frame().await.unwrap().unwrap();
        let v: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["method"], "initialize");
    }

    #[tokio::test]
    async fn read_frame_returns_none_on_eof() {
        let cursor = std::io::Cursor::new(Vec::<u8>::new());
        let mut reader = LspReader::new(cursor);
        assert!(reader.read_frame().await.unwrap().is_none());
    }
}
