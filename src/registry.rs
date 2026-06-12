//! Tool and resource registry with discovery.

use crate::resource::{ResourceContent, ResourceDef};
use crate::tool::{validate_call, ToolCall, ToolDef, ToolResult, ToolStatus, ToolValue};
use std::collections::HashMap;

/// Callback type for tool execution.
pub type ToolHandler = Box<dyn Fn(&ToolCall) -> ToolResult + Send + Sync>;

/// Callback type for resource reading.
pub type ResourceHandler = Box<dyn Fn(&str) -> Option<ResourceContent> + Send + Sync>;

/// Registry of tools and resources.
pub struct Registry {
    tools: HashMap<String, (ToolDef, ToolHandler)>,
    resources: HashMap<String, (ResourceDef, ResourceHandler)>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
            resources: HashMap::new(),
        }
    }

    /// Register a tool with its handler.
    pub fn register_tool(&mut self, def: ToolDef, handler: ToolHandler) {
        self.tools.insert(def.name.clone(), (def, handler));
    }

    /// Register a resource with its handler.
    pub fn register_resource(&mut self, def: ResourceDef, handler: ResourceHandler) {
        self.resources.insert(def.uri.clone(), (def, handler));
    }

    /// List all registered tools.
    pub fn list_tools(&self) -> Vec<&ToolDef> {
        self.tools.values().map(|(def, _)| def).collect()
    }

    /// List all registered resources.
    pub fn list_resources(&self) -> Vec<&ResourceDef> {
        self.resources.values().map(|(def, _)| def).collect()
    }

    /// Get a tool definition by name.
    pub fn get_tool(&self, name: &str) -> Option<&ToolDef> {
        self.tools.get(name).map(|(def, _)| def)
    }

    /// Get a resource definition by URI.
    pub fn get_resource(&self, uri: &str) -> Option<&ResourceDef> {
        self.resources.get(uri).map(|(def, _)| def)
    }

    /// Invoke a tool. Validates the call, executes the handler, returns result.
    pub fn invoke(&self, call: &ToolCall) -> ToolResult {
        let (def, handler) = match self.tools.get(&call.tool_name) {
            Some(entry) => entry,
            None => {
                return ToolResult {
                    call_id: call.call_id.clone(),
                    status: ToolStatus::Error,
                    output: ToolValue::Null,
                    error: Some(format!("tool not found: {:?}", call.tool_name)),
                    tokens_used: None,
                    latency_ms: None,
                }
            }
        };

        // Validate
        if let Err(e) = validate_call(def, call) {
            return ToolResult {
                call_id: call.call_id.clone(),
                status: ToolStatus::Error,
                output: ToolValue::Null,
                error: Some(e),
                tokens_used: None,
                latency_ms: None,
            };
        }

        // Execute with timing
        let start = std::time::Instant::now();
        let mut result = handler(call);
        result.latency_ms = Some(start.elapsed().as_millis() as u64);
        result
    }

    /// Read a resource by URI.
    pub fn read_resource(&self, uri: &str) -> Option<ResourceContent> {
        let (_, handler) = self.resources.get(uri)?;
        handler(uri)
    }

    /// Search tools by tag.
    pub fn find_tools_by_tag(&self, tag: &str) -> Vec<&ToolDef> {
        self.tools
            .values()
            .filter(|(def, _)| def.tags.contains(&tag.to_string()))
            .map(|(def, _)| def)
            .collect()
    }

    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }
    pub fn resource_count(&self) -> usize {
        self.resources.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::ResourceData;
    use crate::tool::ParamType;

    fn make_registry() -> Registry {
        let mut reg = Registry::new();

        let search_tool = ToolDef::new("search", "Search code")
            .param("query", "Search query", ParamType::String, true)
            .tag("search");

        reg.register_tool(
            search_tool,
            Box::new(|call| {
                let query = call
                    .arguments
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                ToolResult {
                    call_id: call.call_id.clone(),
                    status: ToolStatus::Success,
                    output: ToolValue::String(format!("found results for: {}", query)),
                    error: None,
                    tokens_used: Some(50),
                    latency_ms: None,
                }
            }),
        );

        let file_resource = ResourceDef::new("file:///auth.py", "auth.py")
            .description("Auth module")
            .mime_type("text/x-python");

        reg.register_resource(
            file_resource,
            Box::new(|uri| {
                Some(ResourceContent {
                    uri: uri.to_string(),
                    mime_type: "text/x-python".into(),
                    data: ResourceData::Text("def validate(): pass".into()),
                })
            }),
        );

        reg
    }

    #[test]
    fn register_and_list() {
        let reg = make_registry();
        assert_eq!(reg.tool_count(), 1);
        assert_eq!(reg.resource_count(), 1);
        assert_eq!(reg.list_tools().len(), 1);
        assert_eq!(reg.list_resources().len(), 1);
    }

    #[test]
    fn invoke_tool() {
        let reg = make_registry();
        let call = ToolCall {
            call_id: "c1".into(),
            tool_name: "search".into(),
            arguments: {
                let mut m = HashMap::new();
                m.insert("query".into(), ToolValue::String("validate".into()));
                m
            },
        };
        let result = reg.invoke(&call);
        assert_eq!(result.status, ToolStatus::Success);
        assert!(result.output.as_str().unwrap().contains("validate"));
        assert!(result.latency_ms.is_some());
    }

    #[test]
    fn invoke_unknown_tool() {
        let reg = make_registry();
        let call = ToolCall {
            call_id: "c1".into(),
            tool_name: "nonexistent".into(),
            arguments: HashMap::new(),
        };
        let result = reg.invoke(&call);
        assert_eq!(result.status, ToolStatus::Error);
        assert!(result.error.unwrap().contains("not found"));
    }

    #[test]
    fn invoke_invalid_call() {
        let reg = make_registry();
        let call = ToolCall {
            call_id: "c1".into(),
            tool_name: "search".into(),
            arguments: HashMap::new(), // missing required "query"
        };
        let result = reg.invoke(&call);
        assert_eq!(result.status, ToolStatus::Error);
        assert!(result.error.unwrap().contains("missing required"));
    }

    #[test]
    fn read_resource() {
        let reg = make_registry();
        let content = reg.read_resource("file:///auth.py").unwrap();
        assert_eq!(content.mime_type, "text/x-python");
        assert!(content.data.as_text().unwrap().contains("validate"));
    }

    #[test]
    fn read_missing_resource() {
        let reg = make_registry();
        assert!(reg.read_resource("file:///nonexistent").is_none());
    }

    #[test]
    fn find_by_tag() {
        let reg = make_registry();
        let search_tools = reg.find_tools_by_tag("search");
        assert_eq!(search_tools.len(), 1);
        assert_eq!(search_tools[0].name, "search");

        #[test]
        fn invoke_checked_denied() {
            let reg = make_registry();
            let call = ToolCall {
                call_id: "c1".into(),
                tool_name: "search".into(),
                arguments: {
                    let mut m = HashMap::new();
                    m.insert("query".into(), ToolValue::String("test".into()));
                    m
                },
            };
            // Only allow "other_tool", not "search"
            let result = reg.invoke_checked(&call, Some(&["other_tool"]));
            assert_eq!(result.status, ToolStatus::Error);
            assert!(result.error.unwrap().contains("capability denied"));
        }

        #[test]
        fn invoke_checked_allowed() {
            let reg = make_registry();
            let call = ToolCall {
                call_id: "c1".into(),
                tool_name: "search".into(),
                arguments: {
                    let mut m = HashMap::new();
                    m.insert("query".into(), ToolValue::String("test".into()));
                    m
                },
            };
            let result = reg.invoke_checked(&call, Some(&["search", "other"]));
            assert_eq!(result.status, ToolStatus::Success);
        }

        #[test]
        fn invoke_checked_wildcard() {
            let reg = make_registry();
            let call = ToolCall {
                call_id: "c1".into(),
                tool_name: "search".into(),
                arguments: {
                    let mut m = HashMap::new();
                    m.insert("query".into(), ToolValue::String("test".into()));
                    m
                },
            };
            let result = reg.invoke_checked(&call, Some(&["*"]));
            assert_eq!(result.status, ToolStatus::Success);
        }

        #[test]
        fn invoke_checked_no_restriction() {
            let reg = make_registry();
            let call = ToolCall {
                call_id: "c1".into(),
                tool_name: "search".into(),
                arguments: {
                    let mut m = HashMap::new();
                    m.insert("query".into(), ToolValue::String("test".into()));
                    m
                },
            };
            // None = no restrictions
            let result = reg.invoke_checked(&call, None);
            assert_eq!(result.status, ToolStatus::Success);
        }
    }
}
