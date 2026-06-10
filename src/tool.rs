//! Tool definitions, invocations, and results.
//!
//! A Tool is a callable function that an LLM agent can invoke.
//! Each tool has a name, description, typed parameters, and
//! returns a typed result.

use std::collections::HashMap;
use std::fmt;

/// A parameter for a tool.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolParam {
    pub name: String,
    pub description: String,
    pub param_type: ParamType,
    pub required: bool,
    pub default: Option<ToolValue>,
}

/// Parameter types.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamType {
    String,
    Integer,
    Float,
    Boolean,
    Array(Box<ParamType>),
    Object,
    /// One of a fixed set of string values.
    Enum(Vec<String>),
}

impl ParamType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Float => "float",
            Self::Boolean => "boolean",
            Self::Array(_) => "array",
            Self::Object => "object",
            Self::Enum(_) => "enum",
        }
    }
}

/// A concrete value passed to or returned from a tool.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolValue {
    Null,
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<ToolValue>),
    Object(HashMap<String, ToolValue>),
    Bytes(Vec<u8>),
}

impl ToolValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(i) => Some(*i),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(b) => Some(*b),
            _ => None,
        }
    }
}

/// A tool definition.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub params: Vec<ToolParam>,
    /// Estimated token cost of calling this tool (for context budgeting).
    pub estimated_tokens: Option<usize>,
    /// Maximum execution time in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Tags for categorization and discovery.
    pub tags: Vec<String>,
}

impl ToolDef {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            params: Vec::new(),
            estimated_tokens: None,
            timeout_ms: None,
            tags: Vec::new(),
        }
    }

    pub fn param(
        mut self,
        name: impl Into<String>,
        desc: impl Into<String>,
        param_type: ParamType,
        required: bool,
    ) -> Self {
        self.params.push(ToolParam {
            name: name.into(),
            description: desc.into(),
            param_type,
            required,
            default: None,
        });
        self
    }

    pub fn timeout(mut self, ms: u64) -> Self {
        self.timeout_ms = Some(ms);
        self
    }

    pub fn estimated_tokens(mut self, tokens: usize) -> Self {
        self.estimated_tokens = Some(tokens);
        self
    }

    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Required parameter names.
    pub fn required_params(&self) -> Vec<&str> {
        self.params
            .iter()
            .filter(|p| p.required)
            .map(|p| p.name.as_str())
            .collect()
    }
}

/// A tool invocation request.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub arguments: HashMap<String, ToolValue>,
}

/// A tool invocation result.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub call_id: String,
    pub status: ToolStatus,
    pub output: ToolValue,
    pub error: Option<String>,
    /// Actual token cost of the result.
    pub tokens_used: Option<usize>,
    /// Execution time in milliseconds.
    pub latency_ms: Option<u64>,
}

/// Status of a tool invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolStatus {
    Success,
    Error,
    Timeout,
    Cancelled,
}

impl fmt::Display for ToolStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Error => write!(f, "error"),
            Self::Timeout => write!(f, "timeout"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Validate a tool call against its definition.
pub fn validate_call(def: &ToolDef, call: &ToolCall) -> Result<(), String> {
    // Check tool name matches
    if call.tool_name != def.name {
        return Err(format!(
            "tool name mismatch: expected {:?}, got {:?}",
            def.name, call.tool_name
        ));
    }

    // Check required parameters are present
    for param in &def.params {
        if param.required && !call.arguments.contains_key(&param.name) {
            return Err(format!("missing required parameter: {:?}", param.name));
        }
    }

    // Check parameter types
    for (name, value) in &call.arguments {
        if let Some(param) = def.params.iter().find(|p| p.name == *name) {
            validate_type(value, &param.param_type, name)?;
        }
        // Unknown params are allowed (forward compatibility)
    }

    Ok(())
}

fn validate_type(value: &ToolValue, expected: &ParamType, name: &str) -> Result<(), String> {
    match (value, expected) {
        (ToolValue::Null, _) => Ok(()), // null is always valid
        (ToolValue::String(_), ParamType::String) => Ok(()),
        (ToolValue::Integer(_), ParamType::Integer) => Ok(()),
        (ToolValue::Float(_), ParamType::Float) => Ok(()),
        (ToolValue::Boolean(_), ParamType::Boolean) => Ok(()),
        (ToolValue::Array(_), ParamType::Array(_)) => Ok(()),
        (ToolValue::Object(_), ParamType::Object) => Ok(()),
        (ToolValue::String(s), ParamType::Enum(variants)) => {
            if variants.contains(s) {
                Ok(())
            } else {
                Err(format!(
                    "parameter {:?}: value {:?} not in enum {:?}",
                    name, s, variants
                ))
            }
        }
        _ => Err(format!(
            "parameter {:?}: expected {}, got {:?}",
            name,
            expected.as_str(),
            value
        )),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn search_tool() -> ToolDef {
        ToolDef::new("code_search", "Search codebase for symbols and patterns")
            .param("query", "Search query", ParamType::String, true)
            .param("limit", "Max results", ParamType::Integer, false)
            .param(
                "language",
                "Filter by language",
                ParamType::Enum(vec!["rust".into(), "python".into(), "javascript".into()]),
                false,
            )
            .timeout(5000)
            .estimated_tokens(200)
            .tag("search")
    }

    #[test]
    fn tool_def_builder() {
        let tool = search_tool();
        assert_eq!(tool.name, "code_search");
        assert_eq!(tool.params.len(), 3);
        assert_eq!(tool.required_params(), vec!["query"]);
        assert_eq!(tool.timeout_ms, Some(5000));
    }

    #[test]
    fn validate_call_valid() {
        let tool = search_tool();
        let call = ToolCall {
            call_id: "c1".into(),
            tool_name: "code_search".into(),
            arguments: {
                let mut m = HashMap::new();
                m.insert("query".into(), ToolValue::String("validate token".into()));
                m.insert("limit".into(), ToolValue::Integer(10));
                m
            },
        };
        assert!(validate_call(&tool, &call).is_ok());
    }

    #[test]
    fn validate_call_missing_required() {
        let tool = search_tool();
        let call = ToolCall {
            call_id: "c1".into(),
            tool_name: "code_search".into(),
            arguments: HashMap::new(),
        };
        let err = validate_call(&tool, &call).unwrap_err();
        assert!(err.contains("missing required"));
    }

    #[test]
    fn validate_call_wrong_type() {
        let tool = search_tool();
        let call = ToolCall {
            call_id: "c1".into(),
            tool_name: "code_search".into(),
            arguments: {
                let mut m = HashMap::new();
                m.insert("query".into(), ToolValue::Integer(42)); // should be string
                m
            },
        };
        let err = validate_call(&tool, &call).unwrap_err();
        assert!(err.contains("expected string"));
    }

    #[test]
    fn validate_call_bad_enum() {
        let tool = search_tool();
        let call = ToolCall {
            call_id: "c1".into(),
            tool_name: "code_search".into(),
            arguments: {
                let mut m = HashMap::new();
                m.insert("query".into(), ToolValue::String("test".into()));
                m.insert("language".into(), ToolValue::String("cobol".into()));
                m
            },
        };
        let err = validate_call(&tool, &call).unwrap_err();
        assert!(err.contains("not in enum"));
    }

    #[test]
    fn validate_call_wrong_name() {
        let tool = search_tool();
        let call = ToolCall {
            call_id: "c1".into(),
            tool_name: "wrong_tool".into(),
            arguments: HashMap::new(),
        };
        assert!(validate_call(&tool, &call).is_err());
    }

    #[test]
    fn tool_value_accessors() {
        assert_eq!(ToolValue::String("hi".into()).as_str(), Some("hi"));
        assert_eq!(ToolValue::Integer(42).as_i64(), Some(42));
        assert_eq!(ToolValue::Float(3.14).as_f64(), Some(3.14));
        assert_eq!(ToolValue::Boolean(true).as_bool(), Some(true));
        assert_eq!(ToolValue::Null.as_str(), None);
    }

    #[test]
    fn tool_result_creation() {
        let result = ToolResult {
            call_id: "c1".into(),
            status: ToolStatus::Success,
            output: ToolValue::String("found 5 results".into()),
            error: None,
            tokens_used: Some(150),
            latency_ms: Some(12),
        };
        assert_eq!(result.status, ToolStatus::Success);
        assert_eq!(result.tokens_used, Some(150));
    }

    #[test]
    fn tool_status_display() {
        assert_eq!(ToolStatus::Success.to_string(), "success");
        assert_eq!(ToolStatus::Error.to_string(), "error");
        assert_eq!(ToolStatus::Timeout.to_string(), "timeout");
        assert_eq!(ToolStatus::Cancelled.to_string(), "cancelled");
    }
}
