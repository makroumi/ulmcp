//! ulmcp client: connects to servers, discovers tools, invokes them.
//!
//! The client can talk to both native ulmcp servers and MCP servers.

use crate::mcp::types::*;

/// A client for invoking tools on a remote or local server.
/// Currently supports in-process dispatch via handle_message.
pub struct Client {
    tools: Vec<McpToolDef>,
}

impl Client {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Discover tools from an initialize + tools/list exchange.
    pub fn discover_from_responses(&mut self, tools_list_response: &str) -> Result<(), String> {
        let resp: JsonRpcResponse =
            serde_json::from_str(tools_list_response).map_err(|e| format!("parse error: {}", e))?;

        if let Some(result) = resp.result {
            if let Some(tools) = result.get("tools") {
                let tool_list: Vec<McpToolDef> = serde_json::from_value(tools.clone())
                    .map_err(|e| format!("parse tools: {}", e))?;
                self.tools = tool_list;
            }
        }
        Ok(())
    }

    /// List discovered tools.
    pub fn tools(&self) -> &[McpToolDef] {
        &self.tools
    }

    /// Build a tools/call JSON-RPC request.
    pub fn build_call_request(id: i64, tool_name: &str, arguments: serde_json::Value) -> String {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(JsonRpcId::Number(id)),
            method: "tools/call".into(),
            params: serde_json::json!({
                "name": tool_name,
                "arguments": arguments,
            }),
        };
        serde_json::to_string(&req).unwrap_or_default()
    }

    /// Parse a tools/call response.
    pub fn parse_call_response(response: &str) -> Result<ToolCallResult, String> {
        let resp: JsonRpcResponse =
            serde_json::from_str(response).map_err(|e| format!("parse error: {}", e))?;

        if let Some(err) = resp.error {
            return Err(format!("server error {}: {}", err.code, err.message));
        }

        if let Some(result) = resp.result {
            let tcr: ToolCallResult =
                serde_json::from_value(result).map_err(|e| format!("parse result: {}", e))?;
            Ok(tcr)
        } else {
            Err("no result in response".into())
        }
    }

    /// Build an initialize request.
    pub fn build_initialize_request(client_name: &str) -> String {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(JsonRpcId::Number(0)),
            method: "initialize".into(),
            params: serde_json::to_value(InitializeParams {
                protocol_version: MCP_PROTOCOL_VERSION.into(),
                capabilities: ClientCapabilities::default(),
                client_info: ClientInfo {
                    name: client_name.into(),
                    version: "0.1.0".into(),
                },
            })
            .unwrap(),
        };
        serde_json::to_string(&req).unwrap_or_default()
    }

    /// Build a tools/list request.
    pub fn build_tools_list_request(id: i64) -> String {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(JsonRpcId::Number(id)),
            method: "tools/list".into(),
            params: serde_json::json!({}),
        };
        serde_json::to_string(&req).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::server::handle_message;
    use crate::tool::{ParamType, ToolDef, ToolResult, ToolStatus, ToolValue};

    fn test_registry() -> Registry {
        let mut reg = Registry::new();
        reg.register_tool(
            ToolDef::new("add", "Add two numbers")
                .param("a", "First number", ParamType::Integer, true)
                .param("b", "Second number", ParamType::Integer, true),
            Box::new(|call| {
                let a = call
                    .arguments
                    .get("a")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let b = call
                    .arguments
                    .get("b")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                ToolResult {
                    call_id: call.call_id.clone(),
                    status: ToolStatus::Success,
                    output: ToolValue::Integer(a + b),
                    error: None,
                    tokens_used: None,
                    latency_ms: None,
                }
            }),
        );
        reg
    }

    #[test]
    fn full_mcp_flow() {
        let reg = test_registry();
        let mut client = Client::new();

        // 1. Initialize
        let init_req = Client::build_initialize_request("test-client");
        let init_resp = handle_message(&reg, &init_req).unwrap();
        assert!(init_resp.contains("ulmcp"));

        // 2. List tools
        let list_req = Client::build_tools_list_request(1);
        let list_resp = handle_message(&reg, &list_req).unwrap();
        client.discover_from_responses(&list_resp).unwrap();
        assert_eq!(client.tools().len(), 1);
        assert_eq!(client.tools()[0].name, "add");

        // 3. Call tool
        let call_req = Client::build_call_request(2, "add", serde_json::json!({"a": 3, "b": 4}));
        let call_resp = handle_message(&reg, &call_req).unwrap();
        let result = Client::parse_call_response(&call_resp).unwrap();
        assert!(!result.is_error);
        // Result should contain "7"
        match &result.content[0] {
            ContentBlock::Text { text } => assert!(text.contains("7")),
            _ => panic!("expected text content"),
        }
    }

    #[test]
    fn call_unknown_tool() {
        let reg = test_registry();
        let call_req = Client::build_call_request(1, "nonexistent", serde_json::json!({}));
        let call_resp = handle_message(&reg, &call_req).unwrap();
        let result = Client::parse_call_response(&call_resp).unwrap();
        assert!(result.is_error);
    }
}
