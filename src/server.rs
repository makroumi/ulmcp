//! ulmcp server: dispatches requests to the registry.
//!
//! Handles both native ulmcp calls and MCP JSON-RPC messages.
//! The server is transport-agnostic: it processes messages as strings
//! and returns response strings. The transport layer handles I/O.

use crate::mcp::adapter;
use crate::mcp::types::*;
use crate::registry::Registry;

/// Process a JSON-RPC message and return a response.
/// This is the core server dispatch loop.
pub fn handle_message(registry: &Registry, message: &str) -> Option<String> {
    let request: JsonRpcRequest = match serde_json::from_str(message) {
        Ok(r) => r,
        Err(e) => {
            let resp = JsonRpcResponse::error(None, PARSE_ERROR, format!("parse error: {}", e));
            return Some(serde_json::to_string(&resp).unwrap_or_default());
        }
    };

    // Notifications (no id) don't get responses
    if request.id.is_none() && request.method == "notifications/initialized" {
        return None;
    }

    let response = dispatch(registry, &request);
    Some(serde_json::to_string(&response).unwrap_or_default())
}

fn dispatch(registry: &Registry, req: &JsonRpcRequest) -> JsonRpcResponse {
    match req.method.as_str() {
        "initialize" => handle_initialize(req),
        "tools/list" => handle_tools_list(registry, req),
        "tools/call" => handle_tools_call(registry, req),
        "resources/list" => handle_resources_list(registry, req),
        "resources/read" => handle_resources_read(registry, req),
        "prompts/list" => handle_prompts_list(req),
        "prompts/get" => handle_prompts_get(req),
        "ping" => JsonRpcResponse::success(req.id.clone(), serde_json::json!({})),
        _ => JsonRpcResponse::error(
            req.id.clone(),
            METHOD_NOT_FOUND,
            format!("method not found: {}", req.method),
        ),
    }
}

fn handle_initialize(req: &JsonRpcRequest) -> JsonRpcResponse {
    let result = InitializeResult {
        protocol_version: MCP_PROTOCOL_VERSION.into(),
        capabilities: ServerCapabilities {
            tools: Some(serde_json::json!({"listChanged": true})),
            resources: Some(serde_json::json!({"subscribe": true, "listChanged": true})),
            prompts: Some(serde_json::json!({"listChanged": true})),
        },
        server_info: ServerInfo {
            name: "ulmcp".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
    };
    JsonRpcResponse::success(req.id.clone(), serde_json::to_value(result).unwrap())
}

fn handle_tools_list(registry: &Registry, req: &JsonRpcRequest) -> JsonRpcResponse {
    let tools: Vec<McpToolDef> = registry
        .list_tools()
        .iter()
        .map(|t| adapter::tool_to_mcp(t))
        .collect();
    JsonRpcResponse::success(req.id.clone(), serde_json::json!({"tools": tools}))
}

fn handle_tools_call(registry: &Registry, req: &JsonRpcRequest) -> JsonRpcResponse {
    let params: ToolCallParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                req.id.clone(),
                INVALID_PARAMS,
                format!("invalid params: {}", e),
            )
        }
    };

    let call_id = match &req.id {
        Some(JsonRpcId::Number(n)) => format!("{}", n),
        Some(JsonRpcId::String(s)) => s.clone(),
        None => "0".into(),
    };

    let native_call = adapter::mcp_to_tool_call(&call_id, &params);
    let result = registry.invoke(&native_call);
    let mcp_result = adapter::result_to_mcp(&result);

    JsonRpcResponse::success(req.id.clone(), serde_json::to_value(mcp_result).unwrap())
}

fn handle_resources_list(registry: &Registry, req: &JsonRpcRequest) -> JsonRpcResponse {
    let resources: Vec<McpResourceDef> = registry
        .list_resources()
        .iter()
        .map(|r| adapter::resource_to_mcp(r))
        .collect();
    JsonRpcResponse::success(req.id.clone(), serde_json::json!({"resources": resources}))
}

fn handle_resources_read(registry: &Registry, req: &JsonRpcRequest) -> JsonRpcResponse {
    let params: ResourceReadParams = match serde_json::from_value(req.params.clone()) {
        Ok(p) => p,
        Err(e) => {
            return JsonRpcResponse::error(
                req.id.clone(),
                INVALID_PARAMS,
                format!("invalid params: {}", e),
            )
        }
    };

    match registry.read_resource(&params.uri) {
        Some(content) => {
            let mcp_result = adapter::resource_content_to_mcp(&content);
            JsonRpcResponse::success(req.id.clone(), serde_json::to_value(mcp_result).unwrap())
        }
        None => JsonRpcResponse::error(
            req.id.clone(),
            INVALID_PARAMS,
            format!("resource not found: {}", params.uri),
        ),
    }
}

fn handle_prompts_list(req: &JsonRpcRequest) -> JsonRpcResponse {
    // TODO: expose prompts from registry
    JsonRpcResponse::success(req.id.clone(), serde_json::json!({"prompts": []}))
}

fn handle_prompts_get(req: &JsonRpcRequest) -> JsonRpcResponse {
    JsonRpcResponse::error(req.id.clone(), INVALID_PARAMS, "prompt not found")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::*;
    use crate::tool::*;

    fn test_registry() -> Registry {
        let mut reg = Registry::new();
        reg.register_tool(
            ToolDef::new("echo", "Echo input").param("text", "Text", ParamType::String, true),
            Box::new(|call| ToolResult {
                call_id: call.call_id.clone(),
                status: ToolStatus::Success,
                output: ToolValue::String(
                    call.arguments
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                ),
                error: None,
                tokens_used: None,
                latency_ms: None,
            }),
        );
        reg.register_resource(
            ResourceDef::new("test://hello", "hello").mime_type("text/plain"),
            Box::new(|_| {
                Some(crate::resource::ResourceContent {
                    uri: "test://hello".into(),
                    mime_type: "text/plain".into(),
                    data: ResourceData::Text("hello world".into()),
                })
            }),
        );
        reg
    }

    #[test]
    fn initialize() {
        let reg = test_registry();
        let msg = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}"#;
        let resp = handle_message(&reg, msg).unwrap();
        assert!(resp.contains("ulmcp"));
        assert!(resp.contains("protocolVersion"));
    }

    #[test]
    fn tools_list() {
        let reg = test_registry();
        let msg = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}"#;
        let resp = handle_message(&reg, msg).unwrap();
        assert!(resp.contains("echo"));
    }

    #[test]
    fn tools_call() {
        let reg = test_registry();
        let msg = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"text":"hello"}}}"#;
        let resp = handle_message(&reg, msg).unwrap();
        assert!(resp.contains("hello"));
    }

    #[test]
    fn tools_call_unknown() {
        let reg = test_registry();
        let msg = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"nonexistent","arguments":{}}}"#;
        let resp = handle_message(&reg, msg).unwrap();
        assert!(resp.contains("not found"));
    }

    #[test]
    fn resources_list() {
        let reg = test_registry();
        let msg = r#"{"jsonrpc":"2.0","id":5,"method":"resources/list","params":{}}"#;
        let resp = handle_message(&reg, msg).unwrap();
        assert!(resp.contains("test://hello"));
    }

    #[test]
    fn resources_read() {
        let reg = test_registry();
        let msg =
            r#"{"jsonrpc":"2.0","id":6,"method":"resources/read","params":{"uri":"test://hello"}}"#;
        let resp = handle_message(&reg, msg).unwrap();
        assert!(resp.contains("hello world"));
    }

    #[test]
    fn unknown_method() {
        let reg = test_registry();
        let msg = r#"{"jsonrpc":"2.0","id":7,"method":"unknown/method","params":{}}"#;
        let resp = handle_message(&reg, msg).unwrap();
        assert!(resp.contains("-32601"));
    }

    #[test]
    fn parse_error() {
        let reg = test_registry();
        let resp = handle_message(&reg, "not json").unwrap();
        assert!(resp.contains("-32700"));
    }

    #[test]
    fn ping() {
        let reg = test_registry();
        let msg = r#"{"jsonrpc":"2.0","id":8,"method":"ping","params":{}}"#;
        let resp = handle_message(&reg, msg).unwrap();
        assert!(resp.contains("result"));
    }

    #[test]
    fn notification_no_response() {
        let reg = test_registry();
        let msg = r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#;
        let resp = handle_message(&reg, msg);
        assert!(resp.is_none());
    }
}
