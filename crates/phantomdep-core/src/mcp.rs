//! MCP (Model Context Protocol) server implementation for PhantomDep.
//!
//! Transport: stdio, newline-delimited JSON-RPC 2.0 messages (per the MCP spec).
//! Protocol version: 2025-06-18.
//!
//! Posture (per architecture §5.5):
//! - Read-only tools only.
//! - Strict schema validation on every argument.
//! - Deterministic outputs (modulo Phantom-DB snapshot).
//! - No shell execution, no filesystem mutation, no telemetry.
//! - Local stdio first; remote HTTP is out of scope for v0.7.

use std::sync::Arc;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::lookup::Lookup;
use crate::phantom_db::PhantomDb;
use crate::pyimports::extract_pypi_packages;
use crate::jsimports::extract_npm_packages;
use crate::resolve::Resolver;
use crate::verdict::Ecosystem;

const PROTOCOL_VERSION: &str = "2025-06-18";
const SERVER_NAME: &str = "phantomdep";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;

pub struct McpServer {
    lookup: Arc<Lookup>,
    db: Arc<PhantomDb>,
}

impl McpServer {
    pub fn new(lookup: Arc<Lookup>, db: Arc<PhantomDb>) -> Self {
        Self { lookup, db }
    }

    /// Run the server on stdio until EOF.
    pub async fn serve_stdio(&self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut stdout = tokio::io::stdout();
        let mut line = String::new();

        loop {
            line.clear();
            let n = reader.read_line(&mut line).await.context("reading stdin")?;
            if n == 0 {
                break; // EOF
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let response_opt = self.handle_line(trimmed).await;
            if let Some(response) = response_opt {
                let mut buf = serde_json::to_vec(&response).context("encoding response")?;
                buf.push(b'\n');
                stdout.write_all(&buf).await.context("writing stdout")?;
                stdout.flush().await.ok();
            }
        }
        Ok(())
    }

    async fn handle_line(&self, line: &str) -> Option<JsonRpcResponse> {
        let req: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(err) => {
                return Some(error_response(
                    Value::Null,
                    INVALID_PARAMS,
                    format!("invalid JSON-RPC: {err}"),
                ));
            }
        };
        if req.jsonrpc != "2.0" {
            return Some(error_response(
                req.id.unwrap_or(Value::Null),
                INVALID_PARAMS,
                "jsonrpc must be \"2.0\"".into(),
            ));
        }
        let id = req.id;
        // Notifications (no id) get no response.
        let is_notification = id.is_none();
        let response = self.dispatch(&req.method, req.params).await;
        if is_notification {
            return None;
        }
        Some(match response {
            Ok(result) => JsonRpcResponse {
                jsonrpc: "2.0",
                id: id.unwrap_or(Value::Null),
                result: Some(result),
                error: None,
            },
            Err(err) => error_response(id.unwrap_or(Value::Null), err.code, err.message),
        })
    }

    async fn dispatch(&self, method: &str, params: Value) -> std::result::Result<Value, RpcErr> {
        match method {
            "initialize" => Ok(self.initialize_response()),
            "initialized" | "notifications/initialized" => Ok(Value::Null),
            "tools/list" => Ok(self.tools_list_response()),
            "tools/call" => self.tools_call(params).await,
            "ping" => Ok(json!({})),
            other => Err(RpcErr {
                code: METHOD_NOT_FOUND,
                message: format!("method not found: {other}"),
            }),
        }
    }

    fn initialize_response(&self) -> Value {
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {
                "tools": { "listChanged": false }
            },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": SERVER_VERSION
            },
            "instructions": "PhantomDep validates every package an AI assistant suggests \
                            before it reaches a manifest or install command. All tools are \
                            read-only and deterministic."
        })
    }

    fn tools_list_response(&self) -> Value {
        json!({
            "tools": [
                {
                    "name": "validate_package",
                    "description": "Validate a single package name against the registry and Phantom-DB. Returns an evidence-backed verdict.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Package name (e.g. `requests`, `@anthropic-ai/sdk`, `github.com/spf13/cobra`)" },
                            "ecosystem": { "type": "string", "enum": ["pypi", "npm", "cargo", "go", "maven"], "description": "Registry ecosystem" }
                        },
                        "required": ["name", "ecosystem"]
                    }
                },
                {
                    "name": "validate_imports",
                    "description": "Extract package imports from source code and validate each one. Returns one verdict per resolved package.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "code": { "type": "string", "description": "Source code as a string" },
                            "language": { "type": "string", "enum": ["python", "javascript", "typescript"], "description": "Source language" }
                        },
                        "required": ["code", "language"]
                    }
                },
                {
                    "name": "suggest_real_alternative",
                    "description": "Given a name PhantomDep flagged as PHANTOM/SQUATTED/LOOKALIKE, suggest plausible real packages.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string" },
                            "ecosystem": { "type": "string", "enum": ["pypi", "npm", "cargo", "go", "maven"] }
                        },
                        "required": ["name", "ecosystem"]
                    }
                },
                {
                    "name": "phantom_db_status",
                    "description": "Return the loaded Phantom-DB snapshot identifier and entry count.",
                    "inputSchema": { "type": "object", "properties": {} }
                }
            ]
        })
    }

    async fn tools_call(&self, params: Value) -> std::result::Result<Value, RpcErr> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RpcErr {
                code: INVALID_PARAMS,
                message: "tools/call: missing `name`".into(),
            })?;
        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        let result_text = match name {
            "validate_package" => self.tool_validate_package(arguments).await?,
            "validate_imports" => self.tool_validate_imports(arguments).await?,
            "suggest_real_alternative" => self.tool_suggest(arguments)?,
            "phantom_db_status" => self.tool_phantom_db_status(),
            other => {
                return Err(RpcErr {
                    code: METHOD_NOT_FOUND,
                    message: format!("unknown tool: {other}"),
                });
            }
        };

        Ok(json!({
            "content": [
                { "type": "text", "text": result_text }
            ],
            "isError": false
        }))
    }

    async fn tool_validate_package(&self, args: Value) -> std::result::Result<String, RpcErr> {
        let name = string_field(&args, "name")?;
        let ecosystem = ecosystem_field(&args, "ecosystem")?;
        let record = self
            .lookup
            .lookup(&name, ecosystem)
            .await
            .map_err(|e| RpcErr {
                code: INTERNAL_ERROR,
                message: format!("registry lookup failed: {e}"),
            })?;
        let bundle = Resolver::new(&self.db).resolve(&name, ecosystem, record);
        serde_json::to_string_pretty(&bundle).map_err(|e| RpcErr {
            code: INTERNAL_ERROR,
            message: format!("encoding bundle: {e}"),
        })
    }

    async fn tool_validate_imports(&self, args: Value) -> std::result::Result<String, RpcErr> {
        let code = string_field(&args, "code")?;
        let language = string_field(&args, "language")?;
        let (ecosystem, names): (Ecosystem, Vec<String>) = match language.as_str() {
            "python" => (
                Ecosystem::Pypi,
                extract_pypi_packages(&code).into_iter().collect(),
            ),
            "javascript" | "typescript" => (
                Ecosystem::Npm,
                extract_npm_packages(&code).into_iter().collect(),
            ),
            other => {
                return Err(RpcErr {
                    code: INVALID_PARAMS,
                    message: format!("unsupported language: {other}"),
                });
            }
        };

        // Bounded-concurrency parallel lookup; same helper the hook uses.
        let bundles = crate::hook::validate_many(&names, ecosystem, &self.lookup, &self.db)
            .await
            .map_err(|e| RpcErr {
                code: INTERNAL_ERROR,
                message: format!("validate_imports failed: {e}"),
            })?;
        serde_json::to_string_pretty(&bundles).map_err(|e| RpcErr {
            code: INTERNAL_ERROR,
            message: format!("encoding bundles: {e}"),
        })
    }

    fn tool_suggest(&self, args: Value) -> std::result::Result<String, RpcErr> {
        let name = string_field(&args, "name")?;
        let ecosystem = ecosystem_field(&args, "ecosystem")?;
        let mut suggestions: Vec<String> = Vec::new();
        if let Some(entry) = self.db.lookup(&name, ecosystem) {
            for s in &entry.did_you_mean {
                suggestions.push(s.clone());
            }
        }
        let needle = name.to_ascii_lowercase();
        for candidate in crate::popular::top_packages(ecosystem) {
            let dist = strsim::damerau_levenshtein(&needle, candidate);
            if dist > 0 && dist <= 2 && !suggestions.iter().any(|s| s == candidate) {
                suggestions.push(candidate.to_string());
            }
        }
        Ok(serde_json::to_string_pretty(&suggestions).unwrap_or_else(|_| "[]".into()))
    }

    fn tool_phantom_db_status(&self) -> String {
        let snapshot = self.db.snapshot().unwrap_or("none");
        format!(
            "{{\"snapshot\":\"{}\",\"server\":\"{}\",\"version\":\"{}\"}}",
            snapshot, SERVER_NAME, SERVER_VERSION
        )
    }
}

struct RpcErr {
    code: i32,
    message: String,
}

fn error_response(id: Value, code: i32, message: String) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message,
            data: None,
        }),
    }
}

fn string_field(args: &Value, name: &str) -> std::result::Result<String, RpcErr> {
    args.get(name)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| RpcErr {
            code: INVALID_PARAMS,
            message: format!("missing or non-string field: {name}"),
        })
}

fn ecosystem_field(args: &Value, name: &str) -> std::result::Result<Ecosystem, RpcErr> {
    let s = string_field(args, name)?;
    s.parse::<Ecosystem>().map_err(|e| RpcErr {
        code: INVALID_PARAMS,
        message: e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::PackageCache;

    async fn make_server() -> McpServer {
        let lookup = Arc::new(Lookup::new(None).unwrap());
        let db = Arc::new(PhantomDb::bootstrap());
        let _ = PackageCache::open_default;
        McpServer::new(lookup, db)
    }

    #[tokio::test]
    async fn initialize_returns_protocol_version() {
        let server = make_server().await;
        let req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let resp = server.handle_line(req).await.unwrap();
        let result = resp.result.unwrap();
        assert_eq!(result["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(result["serverInfo"]["name"], "phantomdep");
    }

    #[tokio::test]
    async fn tools_list_returns_five_tools() {
        let server = make_server().await;
        let req = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let resp = server.handle_line(req).await.unwrap();
        let result = resp.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 4); // 4 tools: validate_package, validate_imports, suggest, status
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"validate_package"));
        assert!(names.contains(&"validate_imports"));
        assert!(names.contains(&"suggest_real_alternative"));
        assert!(names.contains(&"phantom_db_status"));
    }

    #[tokio::test]
    async fn unknown_method_returns_method_not_found() {
        let server = make_server().await;
        let req = r#"{"jsonrpc":"2.0","id":3,"method":"does/not/exist"}"#;
        let resp = server.handle_line(req).await.unwrap();
        let err = resp.error.unwrap();
        assert_eq!(err.code, METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn notification_returns_no_response() {
        let server = make_server().await;
        let req = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let resp = server.handle_line(req).await;
        assert!(resp.is_none());
    }

    #[tokio::test]
    async fn phantom_db_status_tool_works() {
        let server = make_server().await;
        let req = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"phantom_db_status","arguments":{}}}"#;
        let resp = server.handle_line(req).await.unwrap();
        let result = resp.result.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("snapshot"));
    }

    #[tokio::test]
    async fn suggest_returns_known_alternatives() {
        let server = make_server().await;
        let req = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"suggest_real_alternative","arguments":{"name":"huggingface-cli","ecosystem":"pypi"}}}"#;
        let resp = server.handle_line(req).await.unwrap();
        let result = resp.result.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("huggingface_hub"));
    }

    #[tokio::test]
    async fn invalid_jsonrpc_version_rejected() {
        let server = make_server().await;
        let req = r#"{"jsonrpc":"1.0","id":1,"method":"initialize"}"#;
        let resp = server.handle_line(req).await.unwrap();
        assert!(resp.error.is_some());
    }
}
