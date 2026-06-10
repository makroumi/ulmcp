//! Resources: readable data with URI addressing.
//!
//! A Resource is data that an agent can read. Unlike tools (which are
//! invoked), resources are fetched. Each resource has a URI, MIME type,
//! and content.

use std::fmt;

/// A resource definition.
#[derive(Debug, Clone)]
pub struct ResourceDef {
    pub uri: String,
    pub name: String,
    pub description: String,
    pub mime_type: String,
    /// Whether this resource supports subscription for change notifications.
    pub subscribable: bool,
    /// Estimated size in bytes (for context budgeting).
    pub estimated_size: Option<usize>,
}

impl ResourceDef {
    pub fn new(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            description: String::new(),
            mime_type: "text/plain".into(),
            subscribable: false,
            estimated_size: None,
        }
    }

    pub fn description(mut self, d: impl Into<String>) -> Self {
        self.description = d.into();
        self
    }

    pub fn mime_type(mut self, m: impl Into<String>) -> Self {
        self.mime_type = m.into();
        self
    }

    pub fn subscribable(mut self) -> Self {
        self.subscribable = true;
        self
    }
}

/// Content returned when a resource is read.
#[derive(Debug, Clone)]
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: String,
    pub data: ResourceData,
}

/// The actual data of a resource.
#[derive(Debug, Clone)]
pub enum ResourceData {
    Text(String),
    Binary(Vec<u8>),
}

impl ResourceData {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(s) => s.as_bytes(),
            Self::Binary(b) => b,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::Binary(b) => b.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A resource change notification.
#[derive(Debug, Clone)]
pub struct ResourceChange {
    pub uri: String,
    pub change_type: ChangeType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeType {
    Created,
    Updated,
    Deleted,
}

impl fmt::Display for ChangeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Updated => write!(f, "updated"),
            Self::Deleted => write!(f, "deleted"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_def_builder() {
        let r = ResourceDef::new("file:///auth.py", "auth.py")
            .description("Authentication module")
            .mime_type("text/x-python")
            .subscribable();
        assert_eq!(r.uri, "file:///auth.py");
        assert!(r.subscribable);
        assert_eq!(r.mime_type, "text/x-python");
    }

    #[test]
    fn resource_data_text() {
        let d = ResourceData::Text("hello".into());
        assert_eq!(d.as_text(), Some("hello"));
        assert_eq!(d.len(), 5);
        assert!(!d.is_empty());
    }

    #[test]
    fn resource_data_binary() {
        let d = ResourceData::Binary(vec![1, 2, 3]);
        assert_eq!(d.as_bytes(), &[1, 2, 3]);
        assert!(d.as_text().is_none());
    }

    #[test]
    fn change_type_display() {
        assert_eq!(ChangeType::Created.to_string(), "created");
        assert_eq!(ChangeType::Updated.to_string(), "updated");
        assert_eq!(ChangeType::Deleted.to_string(), "deleted");
    }
}
