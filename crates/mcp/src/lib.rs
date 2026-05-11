//! Glimpse MCP server.
//!
//! Implements the Model Context Protocol over a line-delimited JSON-RPC 2.0
//! stdio transport. Exposes three tools to AI agents:
//!
//! - `ocr_at_cursor`     — capture the screen region around the current
//!                         cursor and return the OCR'd text.
//! - `ocr_region`        — capture an arbitrary screen rectangle and return
//!                         the OCR'd text.
//! - `read_clipboard`    — return the current clipboard text.
//!
//! The first call from a given client triggers an external permission check
//! (the [`PermissionCheck`] closure passed to [`run_stdio`]). This lets the
//! embedding daemon prompt the user and remember the answer; the CLI form
//! `glimpse mcp` defaults to allow-all because the user invoked it directly.

mod protocol;
mod tools;

use protocol::{Error as RpcError, ErrorCode, Message, Request, Response, NULL_ID};
pub use tools::{Tool, ToolError};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Server / client identity exchanged during `initialize`.
const SERVER_NAME: &str = "glimpse";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
const PROTOCOL_VERSION: &str = "2024-11-05";

/// User-provided permission gate.
///
/// Called once per *new client session* (keyed by the client name returned in
/// the `initialize` request) before that client's first capture tool fires.
/// Return `true` to allow this session's captures.
pub trait PermissionCheck: Send + Sync + 'static {
    fn allow(&self, client_id: &str) -> bool;
}

impl<F> PermissionCheck for F
where
    F: Fn(&str) -> bool + Send + Sync + 'static,
{
    fn allow(&self, client_id: &str) -> bool {
        (self)(client_id)
    }
}

/// Run the MCP stdio server until stdin closes.
///
/// `permission_check` is consulted exactly once per client session before that
/// session's first capture tool is invoked.
pub async fn run_stdio<P: PermissionCheck>(permission_check: P) -> anyhow::Result<()> {
    let perm = Arc::new(permission_check);
    let mut server = Server::new(perm);

    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut lines = BufReader::new(stdin).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        tracing::trace!(?line, "→ recv");

        let response = match serde_json::from_str::<Message>(&line) {
            Ok(Message::Request(req)) => server.handle_request(req).await,
            Ok(Message::Notification(note)) => {
                server.handle_notification(note);
                continue;
            }
            Err(e) => Response::error(
                NULL_ID,
                RpcError {
                    code: ErrorCode::ParseError,
                    message: format!("parse error: {e}"),
                    data: None,
                },
            ),
        };

        let bytes = serde_json::to_vec(&response)?;
        tracing::trace!(?response, "← send");
        stdout.write_all(&bytes).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }

    Ok(())
}

struct Server<P: PermissionCheck> {
    perm: Arc<P>,
    /// Set to true after the first successful `initialize`.
    initialized: AtomicBool,
    /// Client identity reported in `initialize`. Used as the permission key.
    client_id: tokio::sync::Mutex<String>,
    /// Whether the active session has been granted capture permission.
    capture_allowed: AtomicBool,
    /// Whether we have already asked the gate for this session.
    capture_decided: AtomicBool,
}

impl<P: PermissionCheck> Server<P> {
    fn new(perm: Arc<P>) -> Self {
        Self {
            perm,
            initialized: AtomicBool::new(false),
            client_id: tokio::sync::Mutex::new(String::new()),
            capture_allowed: AtomicBool::new(false),
            capture_decided: AtomicBool::new(false),
        }
    }

    async fn handle_request(&mut self, req: Request) -> Response {
        let id = req.id.clone();
        match req.method.as_str() {
            "initialize" => self.handle_initialize(req).await,
            "tools/list" => self.handle_tools_list(req),
            "tools/call" => self.handle_tools_call(req).await,
            "ping" => Response::ok(id, serde_json::json!({})),
            other => Response::error(
                id,
                RpcError {
                    code: ErrorCode::MethodNotFound,
                    message: format!("method not found: {other}"),
                    data: None,
                },
            ),
        }
    }

    fn handle_notification(&mut self, note: protocol::Notification) {
        match note.method.as_str() {
            "notifications/initialized" => {
                self.initialized.store(true, Ordering::SeqCst);
                tracing::info!("client signaled initialized");
            }
            other => {
                tracing::debug!(method = %other, "ignoring notification");
            }
        }
    }

    async fn handle_initialize(&mut self, req: Request) -> Response {
        #[derive(serde::Deserialize)]
        struct ClientInfo {
            name: String,
            #[allow(dead_code)]
            #[serde(default)]
            version: String,
        }
        #[derive(serde::Deserialize)]
        struct InitParams {
            #[serde(default)]
            #[allow(dead_code)]
            #[serde(rename = "protocolVersion")]
            protocol_version: Option<String>,
            #[serde(rename = "clientInfo")]
            client_info: Option<ClientInfo>,
        }

        let params: InitParams = match req.params {
            Some(v) => match serde_json::from_value(v) {
                Ok(p) => p,
                Err(e) => {
                    return Response::error(
                        req.id,
                        RpcError {
                            code: ErrorCode::InvalidParams,
                            message: format!("invalid initialize params: {e}"),
                            data: None,
                        },
                    )
                }
            },
            None => InitParams {
                protocol_version: None,
                client_info: None,
            },
        };

        let name = params
            .client_info
            .map(|c| c.name)
            .unwrap_or_else(|| "unknown".into());
        *self.client_id.lock().await = name.clone();

        tracing::info!(client = %name, "MCP client connected");

        Response::ok(
            req.id,
            serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION,
                },
            }),
        )
    }

    fn handle_tools_list(&self, req: Request) -> Response {
        Response::ok(
            req.id,
            serde_json::json!({
                "tools": tools::all_tool_definitions(),
            }),
        )
    }

    async fn handle_tools_call(&self, req: Request) -> Response {
        #[derive(serde::Deserialize)]
        struct CallParams {
            name: String,
            #[serde(default)]
            arguments: serde_json::Value,
        }

        let params: CallParams = match req.params {
            Some(v) => match serde_json::from_value(v) {
                Ok(p) => p,
                Err(e) => {
                    return Response::error(
                        req.id,
                        RpcError {
                            code: ErrorCode::InvalidParams,
                            message: format!("invalid tools/call params: {e}"),
                            data: None,
                        },
                    )
                }
            },
            None => {
                return Response::error(
                    req.id,
                    RpcError {
                        code: ErrorCode::InvalidParams,
                        message: "missing tools/call params".into(),
                        data: None,
                    },
                )
            }
        };

        let is_capture = matches!(params.name.as_str(), "ocr_at_cursor" | "ocr_region");
        if is_capture && !self.check_capture_permission().await {
            return tools::error_response(req.id, "capture permission denied for this session");
        }

        match Tool::dispatch(&params.name, &params.arguments).await {
            Ok(text) => tools::ok_response(req.id, text),
            Err(ToolError::NotFound(name)) => Response::error(
                req.id,
                RpcError {
                    code: ErrorCode::MethodNotFound,
                    message: format!("tool not found: {name}"),
                    data: None,
                },
            ),
            Err(e) => tools::error_response(req.id, e.to_string()),
        }
    }

    async fn check_capture_permission(&self) -> bool {
        if self.capture_decided.load(Ordering::SeqCst) {
            return self.capture_allowed.load(Ordering::SeqCst);
        }
        let client = self.client_id.lock().await.clone();
        let allowed = self.perm.allow(&client);
        self.capture_allowed.store(allowed, Ordering::SeqCst);
        self.capture_decided.store(true, Ordering::SeqCst);
        tracing::info!(client = %client, allowed, "capture permission decided");
        allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AllowAll;
    impl PermissionCheck for AllowAll {
        fn allow(&self, _client_id: &str) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn initialize_handshake() {
        let mut server = Server::new(Arc::new(AllowAll));
        let req: Request = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"test","version":"0.0"}}}"#,
        )
        .unwrap();
        let resp = server.handle_request(req).await;
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["id"], 1);
        assert_eq!(val["result"]["serverInfo"]["name"], "glimpse");
        assert_eq!(
            val["result"]["capabilities"]["tools"],
            serde_json::json!({})
        );
    }

    #[tokio::test]
    async fn tools_list_returns_three_tools() {
        let mut server = Server::new(Arc::new(AllowAll));
        let req: Request =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#).unwrap();
        let resp = server.handle_request(req).await;
        let val = serde_json::to_value(&resp).unwrap();
        let tools = val["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"ocr_at_cursor"));
        assert!(names.contains(&"ocr_region"));
        assert!(names.contains(&"read_clipboard"));
        assert_eq!(names.len(), 3);
    }

    #[tokio::test]
    async fn unknown_method_returns_error() {
        let mut server = Server::new(Arc::new(AllowAll));
        let req: Request =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":3,"method":"nope"}"#).unwrap();
        let resp = server.handle_request(req).await;
        let val = serde_json::to_value(&resp).unwrap();
        assert_eq!(val["error"]["code"], ErrorCode::MethodNotFound as i32);
    }

    struct DenyAll;
    impl PermissionCheck for DenyAll {
        fn allow(&self, _client_id: &str) -> bool {
            false
        }
    }

    #[tokio::test]
    async fn capture_denied_when_perm_says_no() {
        let mut server = Server::new(Arc::new(DenyAll));

        // Hand-roll an init so client_id is set.
        let init: Request = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize",
                "params":{"clientInfo":{"name":"hostile","version":"0"}}}"#,
        )
        .unwrap();
        let _ = server.handle_request(init).await;

        let call: Request = serde_json::from_str(
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call",
                "params":{"name":"ocr_at_cursor","arguments":{}}}"#,
        )
        .unwrap();
        let resp = server.handle_request(call).await;
        let val = serde_json::to_value(&resp).unwrap();
        // Tool errors come back as result.isError, not RPC error.
        assert_eq!(val["result"]["isError"], true);
        assert!(val["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("permission denied"));
    }
}
