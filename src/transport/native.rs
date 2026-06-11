//! Native binary transport.
//!
//! Uses ulmp framing for high-performance transport.
//! In-process dispatch is available directly via server::handle_message.
//!
//! For network transport, use ulmp's TLS connection with the agent_bridge.

/// Marker for native transport capability.
pub struct NativeTransport;

impl NativeTransport {
    /// In-process dispatch: serialize request, handle, deserialize response.
    pub fn call_local(
        registry: &crate::registry::Registry,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let req = crate::mcp::types::JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(crate::mcp::types::JsonRpcId::Number(1)),
            method: method.into(),
            params,
        };
        let msg = serde_json::to_string(&req).map_err(|e| e.to_string())?;
        let resp_str = crate::server::handle_message(registry, &msg)
            .ok_or_else(|| "no response".to_string())?;
        let resp: crate::mcp::types::JsonRpcResponse =
            serde_json::from_str(&resp_str).map_err(|e| e.to_string())?;
        if let Some(err) = resp.error {
            Err(format!("{}: {}", err.code, err.message))
        } else {
            resp.result.ok_or_else(|| "no result".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;
    use crate::tool::*;

    #[test]
    fn local_dispatch() {
        let mut reg = Registry::new();
        reg.register_tool(
            ToolDef::new("greet", "Greet").param("name", "Name", ParamType::String, true),
            Box::new(|call| ToolResult {
                call_id: call.call_id.clone(),
                status: ToolStatus::Success,
                output: ToolValue::String(format!(
                    "hello {}",
                    call.arguments
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("world")
                )),
                error: None,
                tokens_used: None,
                latency_ms: None,
            }),
        );

        let result = NativeTransport::call_local(&reg, "tools/list", serde_json::json!({}));
        assert!(result.is_ok());
        let tools = result.unwrap();
        assert!(tools.to_string().contains("greet"));
    }
}
