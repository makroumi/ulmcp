//! ulmcp: Ulmen Context Protocol
//!
//! A native tool protocol for agentic AI that is faster and more capable
//! than MCP (Model Context Protocol) while maintaining compatibility.
//!
//! Core concepts:
//!   Tool       - a callable function with typed parameters and results
//!   Resource   - readable data with URI addressing
//!   Prompt     - reusable prompt templates
//!   Context    - token budget tracking and context window management
//!   Registry   - discover and manage tools/resources across servers
//!
//! Transport:
//!   Native     - binary framing via ulmp (5M frames/sec)
//!   Stdio      - MCP-compatible JSON-RPC over stdin/stdout
//!   HTTP/SSE   - MCP-compatible JSON-RPC over HTTP
//!
//! Copyright (c) 2026 El Mehdi Makroumi. All rights reserved.
//! Licensed under BSL-1.1.

#![forbid(unsafe_code)]

pub mod client;
pub mod context;
pub mod mcp;
pub mod prompt;
pub mod registry;
pub mod resource;
pub mod server;
pub mod tool;
pub mod transport;

pub use context::*;
pub use prompt::*;
pub use registry::*;
pub use resource::*;
pub use tool::*;
