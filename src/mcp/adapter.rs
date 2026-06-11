//! MCP adapter: translates between native ulmcp types and MCP JSON-RPC.

use std::collections::HashMap;

use crate::mcp::types::{
    ContentBlock, McpPromptDef, McpResourceDef, McpToolDef, ResourceReadResult, ToolCallParams,
    ToolCallResult,
};
use crate::resource::{ResourceContent, ResourceData};
use crate::tool::{ParamType, ToolCall, ToolResult, ToolStatus, ToolValue};

// ---------------------------------------------------------------------------
// Native -> MCP
// ---------------------------------------------------------------------------

pub fn tool_to_mcp(def: &crate::tool::ToolDef) -> McpToolDef {
    let mut properties = serde_json::Map::new();
    let mut required: Vec<serde_json::Value> = Vec::new();

    for param in &def.params {
        let prop = serde_json::json!({
            "type": param_type_to_json_type(&param.param_type),
            "description": param.description,
        });
        properties.insert(param.name.clone(), prop);
        if param.required {
            required.push(serde_json::Value::String(param.name.clone()));
        }
    }

    McpToolDef {
        name: def.name.clone(),
        description: def.description.clone(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required,
        }),
    }
}

pub fn resource_to_mcp(def: &crate::resource::ResourceDef) -> McpResourceDef {
    McpResourceDef {
        uri: def.uri.clone(),
        name: def.name.clone(),
        description: def.description.clone(),
        mime_type: def.mime_type.clone(),
    }
}

pub fn prompt_to_mcp(def: &crate::prompt::PromptDef) -> McpPromptDef {
    use crate::mcp::types::McpPromptArg;
    McpPromptDef {
        name: def.name.clone(),
        description: def.description.clone(),
        arguments: def
            .params
            .iter()
            .map(|p| McpPromptArg {
                name: p.name.clone(),
                description: p.description.clone(),
                required: p.required,
            })
            .collect(),
    }
}

pub fn result_to_mcp(result: &ToolResult) -> ToolCallResult {
    let text = match &result.output {
        ToolValue::Null => {
            if let Some(e) = &result.error {
                e.clone()
            } else {
                "null".into()
            }
        }
        ToolValue::String(s) => s.clone(),
        ToolValue::Integer(i) => i.to_string(),
        ToolValue::Float(f) => format!("{:?}", f),
        ToolValue::Boolean(b) => b.to_string(),
        other => format!("{:?}", other),
    };

    ToolCallResult {
        content: vec![ContentBlock::Text { text }],
        is_error: result.status != ToolStatus::Success,
    }
}

pub fn resource_content_to_mcp(content: &ResourceContent) -> ResourceReadResult {
    let rc = match &content.data {
        ResourceData::Text(text) => crate::mcp::types::ResourceContent {
            uri: content.uri.clone(),
            mime_type: content.mime_type.clone(),
            text: Some(text.clone()),
            blob: None,
        },
        ResourceData::Binary(bytes) => crate::mcp::types::ResourceContent {
            uri: content.uri.clone(),
            mime_type: content.mime_type.clone(),
            text: None,
            blob: Some(base64_encode(bytes)),
        },
    };
    ResourceReadResult { contents: vec![rc] }
}

// ---------------------------------------------------------------------------
// MCP -> Native
// ---------------------------------------------------------------------------

pub fn mcp_to_tool_call(call_id: &str, params: &ToolCallParams) -> ToolCall {
    let mut arguments = HashMap::new();
    if let Some(obj) = params.arguments.as_object() {
        for (k, v) in obj {
            arguments.insert(k.clone(), json_to_tool_value(v));
        }
    }
    ToolCall {
        call_id: call_id.into(),
        tool_name: params.name.clone(),
        arguments,
    }
}

pub fn json_to_tool_value(v: &serde_json::Value) -> ToolValue {
    match v {
        serde_json::Value::Null => ToolValue::Null,
        serde_json::Value::Bool(b) => ToolValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                ToolValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                ToolValue::Float(f)
            } else {
                ToolValue::Null
            }
        }
        serde_json::Value::String(s) => ToolValue::String(s.clone()),
        serde_json::Value::Array(arr) => {
            ToolValue::Array(arr.iter().map(json_to_tool_value).collect())
        }
        serde_json::Value::Object(obj) => {
            let map: HashMap<String, ToolValue> = obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_tool_value(v)))
                .collect();
            ToolValue::Object(map)
        }
    }
}

pub fn tool_value_to_json(v: &ToolValue) -> serde_json::Value {
    match v {
        ToolValue::Null => serde_json::Value::Null,
        ToolValue::String(s) => serde_json::Value::String(s.clone()),
        ToolValue::Integer(i) => serde_json::json!(*i),
        ToolValue::Float(f) => serde_json::json!(*f),
        ToolValue::Boolean(b) => serde_json::Value::Bool(*b),
        ToolValue::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(tool_value_to_json).collect())
        }
        ToolValue::Object(obj) => {
            let map: serde_json::Map<String, serde_json::Value> = obj
                .iter()
                .map(|(k, v)| (k.clone(), tool_value_to_json(v)))
                .collect();
            serde_json::Value::Object(map)
        }
        ToolValue::Bytes(b) => serde_json::Value::String(base64_encode(b)),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn param_type_to_json_type(pt: &ParamType) -> &'static str {
    match pt {
        ParamType::String | ParamType::Enum(_) => "string",
        ParamType::Integer => "integer",
        ParamType::Float => "number",
        ParamType::Boolean => "boolean",
        ParamType::Array(_) => "array",
        ParamType::Object => "object",
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((n >> 18) & 0x3F) as usize] as char);
        out.push(CHARS[((n >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((n >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(n & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::{ResourceContent, ResourceData};
    use crate::tool::{ParamType, ToolDef, ToolResult, ToolStatus, ToolValue};

    #[test]
    fn tool_to_mcp_schema() {
        let tool = ToolDef::new("search", "Search code")
            .param("query", "Query", ParamType::String, true)
            .param("limit", "Limit", ParamType::Integer, false);
        let mcp = tool_to_mcp(&tool);
        assert_eq!(mcp.name, "search");
        let schema = mcp.input_schema.as_object().unwrap();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("query"));
        assert!(props.contains_key("limit"));
        let req = schema["required"].as_array().unwrap();
        assert_eq!(req.len(), 1);
        assert_eq!(req[0], "query");
    }

    #[test]
    fn mcp_to_tool_call_conversion() {
        let params = ToolCallParams {
            name: "search".into(),
            arguments: serde_json::json!({"query": "auth", "limit": 5}),
        };
        let call = mcp_to_tool_call("c1", &params);
        assert_eq!(call.tool_name, "search");
        assert_eq!(call.arguments["query"].as_str(), Some("auth"));
        assert_eq!(call.arguments["limit"].as_i64(), Some(5));
    }

    #[test]
    fn result_to_mcp_success() {
        let r = ToolResult {
            call_id: "c1".into(),
            status: ToolStatus::Success,
            output: ToolValue::String("found it".into()),
            error: None,
            tokens_used: None,
            latency_ms: None,
        };
        let mcp = result_to_mcp(&r);
        assert!(!mcp.is_error);
        match &mcp.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "found it"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn result_to_mcp_error() {
        let r = ToolResult {
            call_id: "c1".into(),
            status: ToolStatus::Error,
            output: ToolValue::Null,
            error: Some("not found".into()),
            tokens_used: None,
            latency_ms: None,
        };
        let mcp = result_to_mcp(&r);
        assert!(mcp.is_error);
        match &mcp.content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "not found"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn resource_content_text() {
        let content = ResourceContent {
            uri: "file:///a.py".into(),
            mime_type: "text/python".into(),
            data: ResourceData::Text("def foo(): pass".into()),
        };
        let mcp = resource_content_to_mcp(&content);
        assert_eq!(mcp.contents[0].text.as_deref(), Some("def foo(): pass"));
        assert!(mcp.contents[0].blob.is_none());
    }

    #[test]
    fn json_value_roundtrip() {
        let orig = ToolValue::Array(vec![
            ToolValue::Integer(1),
            ToolValue::String("a".into()),
            ToolValue::Boolean(true),
            ToolValue::Null,
        ]);
        let json = tool_value_to_json(&orig);
        let back = json_to_tool_value(&json);
        assert_eq!(orig, back);
    }

    #[test]
    fn base64_known_values() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }
}
