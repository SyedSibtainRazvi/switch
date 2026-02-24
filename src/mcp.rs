use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::io::{BufRead, BufReader, Read, Write};

use crate::checkpoint::{Checkpoint, CheckpointPayload};
use crate::db;
use crate::git::ContextScope;

const JSON_RPC_VERSION: &str = "2.0";
const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

#[derive(Debug)]
struct JsonRpcError {
    code: i64,
    message: String,
}

impl JsonRpcError {
    fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method not found: {method}"),
        }
    }

    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: message.into(),
        }
    }

    fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: -32000,
            message: message.into(),
        }
    }
}

pub fn run_mcp_server(conn: &Connection) -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::with_capacity(64 * 1024, stdin.lock());
    let mut writer = stdout.lock();

    while let Some(message) = read_mcp_message(&mut reader)? {
        match handle_mcp_message(conn, message) {
            Ok(Some(response)) => write_mcp_message(&mut writer, &response)?,
            Ok(None) => {}
            Err(err) => eprintln!("warning: failed to handle MCP message: {err:#}"),
        }
    }

    Ok(())
}

fn read_mcp_message<R: BufRead + Read>(reader: &mut R) -> Result<Option<Value>> {
    let mut content_length: Option<usize> = None;
    let mut saw_header = false;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            if saw_header {
                return Err(anyhow!("unexpected EOF while reading MCP headers"));
            }
            return Ok(None);
        }
        saw_header = true;

        if line == "\n" || line == "\r\n" {
            break;
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                content_length =
                    Some(value.trim().parse::<usize>().with_context(|| {
                        format!("invalid Content-Length header: {}", value.trim())
                    })?);
            }
        }
    }

    let length = content_length.ok_or_else(|| anyhow!("missing Content-Length header"))?;
    let mut payload = vec![0_u8; length];
    reader.read_exact(&mut payload)?;

    let message: Value = serde_json::from_slice(&payload).context("invalid MCP JSON payload")?;
    Ok(Some(message))
}

fn write_mcp_message<W: Write>(writer: &mut W, payload: &Value) -> Result<()> {
    let body = serde_json::to_vec(payload)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

fn handle_mcp_message(conn: &Connection, message: Value) -> Result<Option<Value>> {
    let message_obj = message
        .as_object()
        .ok_or_else(|| anyhow!("MCP message must be a JSON object"))?;

    let id = message_obj.get("id").cloned();
    let method = message_obj.get("method").and_then(Value::as_str);
    let params = message_obj
        .get("params")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let method = match method {
        Some(method) => method,
        None => {
            if let Some(id) = id {
                let err = JsonRpcError::invalid_request("request is missing a string method");
                return Ok(Some(json!({
                    "jsonrpc": JSON_RPC_VERSION,
                    "id": id,
                    "error": {
                        "code": err.code,
                        "message": err.message,
                    }
                })));
            }
            return Ok(None);
        }
    };

    if let Some(id) = id {
        let response = match handle_mcp_request(conn, method, params) {
            Ok(result) => json!({
                "jsonrpc": JSON_RPC_VERSION,
                "id": id,
                "result": result
            }),
            Err(err) => json!({
                "jsonrpc": JSON_RPC_VERSION,
                "id": id,
                "error": {
                    "code": err.code,
                    "message": err.message,
                }
            }),
        };
        return Ok(Some(response));
    }

    handle_mcp_notification(method);
    Ok(None)
}

fn handle_mcp_notification(method: &str) {
    let _ = method;
}

fn handle_mcp_request(
    conn: &Connection,
    method: &str,
    params: Value,
) -> std::result::Result<Value, JsonRpcError> {
    match method {
        "initialize" => {
            let protocol_version = params
                .as_object()
                .and_then(|v| v.get("protocolVersion"))
                .and_then(Value::as_str)
                .unwrap_or(MCP_PROTOCOL_VERSION);

            Ok(json!({
                "protocolVersion": protocol_version,
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "switch-mcp",
                    "version": env!("CARGO_PKG_VERSION"),
                }
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": mcp_tools(),
        })),
        "tools/call" => handle_mcp_tools_call(conn, params),
        _ => Err(JsonRpcError::method_not_found(method)),
    }
}

fn handle_mcp_tools_call(
    conn: &Connection,
    params: Value,
) -> std::result::Result<Value, JsonRpcError> {
    let params_obj = params
        .as_object()
        .ok_or_else(|| JsonRpcError::invalid_params("tools/call params must be an object"))?;
    let tool_name = required_string_arg(params_obj, "name")
        .map_err(|err| JsonRpcError::invalid_params(err.to_string()))?;
    let arguments = params_obj
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));

    if !arguments.is_object() {
        return Err(JsonRpcError::invalid_params(
            "tools/call arguments must be an object",
        ));
    }

    let tool_result = match tool_name.as_str() {
        "get_context" => mcp_get_context(conn, &arguments),
        "save_context" => mcp_save_context(conn, &arguments),
        "list_context" => mcp_list_context(conn, &arguments),
        _ => Err(anyhow!("unknown tool: {}", tool_name)),
    };

    match tool_result {
        Ok(structured_content) => mcp_tool_success(structured_content)
            .map_err(|err| JsonRpcError::internal(err.to_string())),
        Err(err) => Ok(mcp_tool_error(err)),
    }
}

fn mcp_tool_success(structured_content: Value) -> Result<Value> {
    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&structured_content)?,
            }
        ],
        "structuredContent": structured_content
    }))
}

fn mcp_tool_error(err: anyhow::Error) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": format!("{err:#}"),
            }
        ],
        "isError": true
    })
}

fn mcp_get_context(conn: &Connection, arguments: &Value) -> Result<Value> {
    let args = arguments
        .as_object()
        .ok_or_else(|| anyhow!("get_context arguments must be an object"))?;
    let repo_path = required_string_arg(args, "repo_path")?;
    let branch = required_string_arg(args, "branch")?;

    let checkpoint = db::latest_checkpoint_for_scope(conn, &repo_path, &branch)?;
    if let Some(checkpoint) = checkpoint {
        Ok(json!({
            "found": true,
            "checkpoint": checkpoint_contract_object(&checkpoint),
        }))
    } else {
        Ok(json!({
            "found": false
        }))
    }
}

fn mcp_save_context(conn: &Connection, arguments: &Value) -> Result<Value> {
    let args = arguments
        .as_object()
        .ok_or_else(|| anyhow!("save_context arguments must be an object"))?;
    let repo_path = required_string_arg(args, "repo_path")?;
    let branch = required_string_arg(args, "branch")?;
    let commit_sha = required_string_arg(args, "commit_sha")?;

    let payload = CheckpointPayload {
        done: optional_string_arg(args, "done_text")?,
        next: optional_string_arg(args, "next_text")?,
        blockers: optional_string_arg(args, "blockers_text")?,
        tests: optional_string_arg(args, "tests_text")?,
        files: optional_string_list_arg(args, "files")?,
        session_id: optional_string_arg(args, "session_id")?,
    };

    let scope = ContextScope {
        repo_path,
        branch,
        commit_sha,
        used_repo_fallback: false,
        used_branch_fallback: false,
        used_commit_fallback: false,
    };

    let id = db::save_checkpoint(conn, &scope, &payload)?;

    Ok(json!({
        "ok": true,
        "id": id
    }))
}

fn mcp_list_context(conn: &Connection, arguments: &Value) -> Result<Value> {
    let args = arguments
        .as_object()
        .ok_or_else(|| anyhow!("list_context arguments must be an object"))?;
    let repo_path = required_string_arg(args, "repo_path")?;
    let branch = required_string_arg(args, "branch")?;
    let limit = optional_u32_arg(args, "limit", 20)?;
    if limit == 0 {
        return Err(anyhow!("limit must be at least 1"));
    }

    let items = db::list_checkpoints_for_scope(conn, &repo_path, &branch, limit)?
        .iter()
        .map(checkpoint_list_item)
        .collect::<Vec<Value>>();

    Ok(json!({
        "items": items
    }))
}

fn checkpoint_contract_object(checkpoint: &Checkpoint) -> Value {
    json!({
        "done_text": checkpoint.done_text,
        "next_text": checkpoint.next_text,
        "blockers_text": checkpoint.blockers_text,
        "tests_text": checkpoint.tests_text,
        "files": checkpoint.files,
        "commit_sha": checkpoint.commit_sha,
        "created_at_ms": checkpoint.created_at_ms
    })
}

fn checkpoint_list_item(checkpoint: &Checkpoint) -> Value {
    json!({
        "id": checkpoint.id,
        "repo_path": checkpoint.repo_path,
        "branch": checkpoint.branch,
        "session_id": checkpoint.session_id,
        "done_text": checkpoint.done_text,
        "next_text": checkpoint.next_text,
        "blockers_text": checkpoint.blockers_text,
        "tests_text": checkpoint.tests_text,
        "files": checkpoint.files,
        "commit_sha": checkpoint.commit_sha,
        "created_at_ms": checkpoint.created_at_ms
    })
}

fn required_string_arg(args: &Map<String, Value>, key: &str) -> Result<String> {
    match args.get(key) {
        Some(Value::String(v)) => Ok(v.clone()),
        Some(_) => Err(anyhow!("{key} must be a string")),
        None => Err(anyhow!("{key} is required")),
    }
}

fn optional_string_arg(args: &Map<String, Value>, key: &str) -> Result<Option<String>> {
    match args.get(key) {
        Some(Value::String(v)) => Ok(Some(v.clone())),
        Some(Value::Null) | None => Ok(None),
        Some(_) => Err(anyhow!("{key} must be a string")),
    }
}

fn optional_string_list_arg(args: &Map<String, Value>, key: &str) -> Result<Vec<String>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(values)) => values
            .iter()
            .enumerate()
            .map(|(i, value)| match value {
                Value::String(s) => Ok(s.clone()),
                _ => Err(anyhow!("{key}[{i}] must be a string")),
            })
            .collect(),
        Some(_) => Err(anyhow!("{key} must be an array of strings")),
    }
}

fn optional_u32_arg(args: &Map<String, Value>, key: &str, default: u32) -> Result<u32> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(n)) => {
            let as_u64 = n
                .as_u64()
                .ok_or_else(|| anyhow!("{key} must be a non-negative integer"))?;
            let as_u32 = u32::try_from(as_u64).context("value exceeds u32 range")?;
            Ok(as_u32)
        }
        Some(_) => Err(anyhow!("{key} must be a number")),
    }
}

fn mcp_tools() -> Value {
    json!([
        {
            "name": "get_context",
            "description": "Return the latest checkpoint for repo_path + branch.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "repo_path": { "type": "string", "description": "Absolute path to the git repo root." },
                    "branch": { "type": "string", "description": "Git branch name." }
                },
                "required": ["repo_path", "branch"]
            }
        },
        {
            "name": "save_context",
            "description": "Save a checkpoint for repo_path + branch. At least one of done_text, next_text, blockers_text, tests_text, or files must be provided.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "repo_path": { "type": "string", "description": "Absolute path to the git repo root." },
                    "branch": { "type": "string", "description": "Git branch name." },
                    "commit_sha": { "type": "string", "description": "Current git commit SHA." },
                    "session_id": { "type": "string", "description": "Optional session identifier." },
                    "done_text": { "type": "string", "description": "Summary of completed work." },
                    "next_text": { "type": "string", "description": "What to do next." },
                    "blockers_text": { "type": "string", "description": "Current blockers." },
                    "tests_text": { "type": "string", "description": "Test status or commands." },
                    "files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Relevant file paths."
                    }
                },
                "required": ["repo_path", "branch", "commit_sha"]
            }
        },
        {
            "name": "list_context",
            "description": "List recent checkpoints for repo_path + branch.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "repo_path": { "type": "string", "description": "Absolute path to the git repo root." },
                    "branch": { "type": "string", "description": "Git branch name." },
                    "limit": { "type": "integer", "minimum": 1, "description": "Max checkpoints to return (default: 20)." }
                },
                "required": ["repo_path", "branch"]
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db_path() -> std::path::PathBuf {
        let test_id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "switch-mcp-test-{}-{}-{}",
            std::process::id(),
            db::current_time_ms().expect("time"),
            test_id
        ));
        std::fs::create_dir_all(&base).expect("create temp dir");
        base.join("switch.db")
    }

    #[test]
    fn mcp_tools_list_includes_context_tools() {
        let db_path = temp_db_path();
        let conn = db::open_db(&db_path).expect("open db");

        let response =
            handle_mcp_request(&conn, "tools/list", json!({})).expect("tools/list should succeed");
        let tools = response["tools"]
            .as_array()
            .expect("tools should be an array");
        let names: Vec<&str> = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect();

        assert!(names.contains(&"get_context"));
        assert!(names.contains(&"save_context"));
        assert!(names.contains(&"list_context"));
    }

    #[test]
    fn mcp_save_then_get_and_list_round_trip() {
        let db_path = temp_db_path();
        let conn = db::open_db(&db_path).expect("open db");

        let save = handle_mcp_request(
            &conn,
            "tools/call",
            json!({
                "name": "save_context",
                "arguments": {
                    "repo_path": "/tmp/mcp-repo",
                    "branch": "feature/mcp",
                    "session_id": "claude-1",
                    "done_text": "wired MCP",
                    "next_text": "test integrations",
                    "blockers_text": "none",
                    "tests_text": "cargo test",
                    "files": ["src/main.rs"],
                    "commit_sha": "abc123"
                }
            }),
        )
        .expect("save_context should succeed");

        assert_ne!(save.get("isError").and_then(Value::as_bool), Some(true));
        let save_payload = &save["structuredContent"];
        assert_eq!(save_payload["ok"], true);
        assert!(
            save_payload["id"].as_i64().expect("id should be i64") > 0,
            "id should be positive"
        );

        let get = handle_mcp_request(
            &conn,
            "tools/call",
            json!({
                "name": "get_context",
                "arguments": {
                    "repo_path": "/tmp/mcp-repo",
                    "branch": "feature/mcp"
                }
            }),
        )
        .expect("get_context should succeed");

        let get_payload = &get["structuredContent"];
        assert_eq!(get_payload["found"], true);
        assert_eq!(get_payload["checkpoint"]["done_text"], "wired MCP");
        assert_eq!(get_payload["checkpoint"]["next_text"], "test integrations");
        assert_eq!(get_payload["checkpoint"]["commit_sha"], "abc123");

        let list = handle_mcp_request(
            &conn,
            "tools/call",
            json!({
                "name": "list_context",
                "arguments": {
                    "repo_path": "/tmp/mcp-repo",
                    "branch": "feature/mcp",
                    "limit": 10
                }
            }),
        )
        .expect("list_context should succeed");

        let items = list["structuredContent"]["items"]
            .as_array()
            .expect("items should be an array");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["done_text"], "wired MCP");
    }
}
