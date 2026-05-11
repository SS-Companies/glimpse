//! JSON-RPC 2.0 wire types for the MCP stdio transport.
//!
//! We deliberately do not model the full MCP surface — only what the three
//! Glimpse tools require: `initialize` request, `tools/list` request,
//! `tools/call` request, `notifications/initialized` notification.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC request ID. We accept both number and string per the spec.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Id {
    Num(i64),
    Str(String),
    /// Used by the server when reporting a top-level parse error with no
    /// recoverable id.
    Null,
}

pub const NULL_ID: Id = Id::Null;

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Request(Request),
    Notification(Notification),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    #[serde(default = "version_2_0")]
    pub jsonrpc: String,
    pub id: Id,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// A JSON-RPC notification is a request with no `id`. Notifications must NOT
/// be replied to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    #[serde(default = "version_2_0")]
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    pub id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
}

impl Response {
    pub fn ok(id: Id, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Id, error: Error) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(into = "i32")]
pub enum ErrorCode {
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
}

impl From<ErrorCode> for i32 {
    fn from(c: ErrorCode) -> i32 {
        c as i32
    }
}

fn version_2_0() -> String {
    "2.0".into()
}
