//! MCP JSON-RPC 处理：Streamable HTTP（POST /mcp）与 SSE 握手（GET /mcp/sse）。

use axum::body::{Body, Bytes};
use axum::http::{header::CONTENT_TYPE, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};

/// 13 个 MCP 工具定义（后续接知识库逻辑）
pub fn mcp_tools() -> Vec<Value> {
    vec![
        tool("search_knowledge_base", "在指定知识库中检索相关内容",
            json!({"type":"object","properties":{
                "query":{"type":"string","description":"检索关键词"},
                "kb_id":{"type":"string","description":"知识库 ID"},
                "top_k":{"type":"integer","description":"返回条数"}
            },"required":["query"]})),
        tool("list_knowledge_bases", "列出所有知识库",
            json!({"type":"object","properties":{}})),
        tool("create_knowledge_base", "创建知识库",
            json!({"type":"object","properties":{
                "name":{"type":"string","description":"知识库名称"},
                "description":{"type":"string","description":"知识库描述"}
            },"required":["name"]})),
        tool("update_knowledge_base", "更新知识库",
            json!({"type":"object","properties":{
                "kb_id":{"type":"string","description":"知识库 ID"},
                "name":{"type":"string","description":"新名称"},
                "description":{"type":"string","description":"新描述"}
            },"required":["kb_id"]})),
        tool("delete_knowledge_base", "删除知识库",
            json!({"type":"object","properties":{
                "kb_id":{"type":"string","description":"知识库 ID"}
            },"required":["kb_id"]})),
        tool("list_documents", "列出知识库中的文档",
            json!({"type":"object","properties":{
                "kb_id":{"type":"string","description":"知识库 ID"}
            },"required":["kb_id"]})),
        tool("upload_document", "上传文档到知识库",
            json!({"type":"object","properties":{
                "kb_id":{"type":"string","description":"知识库 ID"},
                "file_path":{"type":"string","description":"文档路径"}
            },"required":["kb_id","file_path"]})),
        tool("delete_document", "删除文档",
            json!({"type":"object","properties":{
                "kb_id":{"type":"string","description":"知识库 ID"},
                "doc_id":{"type":"string","description":"文档 ID"}
            },"required":["doc_id"]})),
        tool("import_source", "导入外部源",
            json!({"type":"object","properties":{
                "kb_id":{"type":"string","description":"知识库 ID"},
                "source":{"type":"string","description":"源地址"}
            },"required":["kb_id","source"]})),
        tool("build_index", "构建向量索引",
            json!({"type":"object","properties":{
                "kb_id":{"type":"string","description":"知识库 ID"}
            },"required":["kb_id"]})),
        tool("get_index_status", "获取索引构建状态",
            json!({"type":"object","properties":{
                "kb_id":{"type":"string","description":"知识库 ID"}
            },"required":["kb_id"]})),
        tool("ask", "基于知识库的 RAG 问答",
            json!({"type":"object","properties":{
                "kb_id":{"type":"string","description":"知识库 ID"},
                "question":{"type":"string","description":"问题"}
            },"required":["kb_id","question"]})),
        tool("clear_conversations", "清空会话历史",
            json!({"type":"object","properties":{
                "kb_id":{"type":"string","description":"知识库 ID"}
            },"required":["kb_id"]})),
    ]
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({ "name": name, "description": description, "inputSchema": input_schema })
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "yeapi-mcp", "version": "0.1.0" },
        "instructions": "本地知识库 MCP Server，提供检索与 RAG 问答等 13 个工具"
    })
}

fn jsonrpc_result(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn jsonrpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// POST /mcp —— JSON-RPC 分发（Streamable HTTP）
pub async fn handle_mcp(body: Bytes) -> Response {
    let text = String::from_utf8_lossy(&body);
    let parsed: Value = match serde_json::from_str(text.trim()) {
        Ok(v) => v,
        Err(_) => return Json(jsonrpc_error(&Value::Null, -32700, "Parse error")).into_response(),
    };

    let id = parsed.get("id").cloned().unwrap_or(Value::Null);
    let method = parsed.get("method").and_then(|m| m.as_str()).unwrap_or("");

    match method {
        "initialize" => Json(jsonrpc_result(&id, initialize_result())).into_response(),
        "tools/list" => Json(jsonrpc_result(&id, json!({ "tools": mcp_tools() }))).into_response(),
        "tools/call" => {
            let name = parsed
                .pointer("/params/name")
                .and_then(|n| n.as_str())
                .unwrap_or("");
            Json(jsonrpc_result(&id, json!({
                "content": [{ "type": "text", "text": format!("工具 {} 尚未实现", name) }],
                "isError": true
            })))
            .into_response()
        }
        // 通知（如 notifications/initialized）：无需响应
        m if m.starts_with("notifications/") => StatusCode::ACCEPTED.into_response(),
        _ => Json(jsonrpc_error(&id, -32601, "Method not found")).into_response(),
    }
}

/// GET /mcp/sse —— 传统 SSE 传输握手，返回 endpoint 事件
pub async fn handle_mcp_sse() -> Response {
    let session_id = format!("sess_{}", uuid::Uuid::new_v4().simple());
    let body = format!("event: endpoint\ndata: /mcp?session_id={}\n\n", session_id);
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "text/event-stream")
        .header("Cache-Control", "no-cache")
        .body(Body::from(body))
        .unwrap()
}
