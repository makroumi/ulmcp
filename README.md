<div align="center">

# ulmcp

**Ulmen Context Protocol: native tool protocol for agentic AI.**

[![Rust Tests](https://img.shields.io/badge/rust_tests-29-brightgreen)]()
[![License](https://img.shields.io/badge/license-BSL--1.1-blue)]()
[![Zero Deps](https://img.shields.io/badge/deps-ulmen--core_only-orange)]()

</div>

---

ulmcp is a native Rust tool protocol that lets AI agents discover, invoke, and manage tools with typed parameters, context window awareness, and token budget tracking. Compatible with MCP (Model Context Protocol) while being faster and more capable.

```rust
use ulmcp::*;
use std::collections::HashMap;

// Define a tool
let search = ToolDef::new("code_search", "Search codebase")
    .param("query", "Search query", ParamType::String, true)
    .param("limit", "Max results", ParamType::Integer, false)
    .timeout(5000)
    .estimated_tokens(200)
    .tag("search");

// Register with handler
let mut registry = Registry::new();
registry.register_tool(search, Box::new(|call| {
    let query = call.arguments.get("query")
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
}));

// Invoke
let call = ToolCall {
    call_id: "c1".into(),
    tool_name: "code_search".into(),
    arguments: {
        let mut m = HashMap::new();
        m.insert("query".into(), ToolValue::String("validate token".into()));
        m
    },
};

let result = registry.invoke(&call);
assert_eq!(result.status, ToolStatus::Success);
```

---
### Why not just MCP?
| Feature | MCP | ulmcp |
|---------|-----|-------|
| Framing | JSON-RPC 2.0 | Native binary (ulmp) + JSON-RPC compat |
| Speed | ~1ms/call overhead | <1us native, ~1ms JSON-RPC compat |
| Auth/TLS | None built-in | TLS 1.3 + HMAC-SHA256 (via ulmp) |
| Streaming | SSE only | Native streaming + SSE compat |
| Type safety | JSON Schema | Rust types with compile-time validation |
| Context tracking | None | Built-in token budget management |
| Tool validation | Runtime JSON | Compile-time + runtime typed params |
| Agent state | External | Built-in via ulmen-core AgentPayload |
| Transport | stdio, HTTP | stdio, HTTP, native binary, WebSocket |

ulmcp speaks MCP when talking to external tools. Internally it uses native binary transport for maximum performance.

---
## Core Concepts
### Tools
Callable functions with typed parameters and results.
``` Rust
let tool = ToolDef::new("web_search", "Search the web")
    .param("query", "Search query", ParamType::String, true)
    .param("max_results", "Limit", ParamType::Integer, false)
    .timeout(10000)
    .estimated_tokens(500)
    .tag("search")
    .tag("web");
```

### Resources
Readable data with URI addressing.
``` Rust
let resource = ResourceDef::new("file:///src/auth.py", "auth.py")
    .description("Authentication module")
    .mime_type("text/x-python")
    .subscribable();
```

### Prompts
Reusable templates with parameter substitution.
``` Rust
let prompt = PromptDef::new("code_review", "Review {{file}} for {{concern}}")
    .param("file", "File to review", true)
    .param("concern", "What to look for", true);

let rendered = prompt.render(&args)?;
```

### Context Tracking
Token budget management across tool calls.
``` Rust
let mut ctx = ContextTracker::new(4096);
ctx.use_tokens(200);           // tool result consumed 200 tokens
ctx.reserve(500);              // reserve space for response
assert!(ctx.fits("some text")); // check if text fits in remaining budget
println!("{}% used", (ctx.usage_ratio() * 100.0) as u32);
```

### Registry
Discover and manage tools and resources.
``` Rust
let mut registry = Registry::new();
registry.register_tool(tool_def, handler);
registry.register_resource(resource_def, reader);

// Discovery
let tools = registry.list_tools();
let search_tools = registry.find_tools_by_tag("search");

// Invoke with validation
let result = registry.invoke(&call);  // validates params, times execution
```

---
## Architecture
``` text
ulmcp
  tool.rs         Tool definitions, typed params, validation
  resource.rs     URI-addressed readable data
  prompt.rs       Template rendering
  context.rs      Token budget tracking
  registry.rs     Tool/resource registry with discovery

  transport/
    native.rs     Binary transport via ulmp (5M frames/sec)
    stdio.rs      MCP-compatible JSON-RPC over stdin/stdout

  server.rs       Accept connections, serve tools/resources
  client.rs       Connect, discover, invoke

  mcp/
    types.rs      MCP JSON-RPC type definitions
    adapter.rs    Translate MCP <-> native types
```

---
## Ecosystem
| Component | Purpose |
|-----------|---------|
| [ulmen](https://github.com/ulmen/ulmen) | Serialization + agent protocol |
| [uldb](https://github.com/ulmen/uldb) | Storage, indexing, caching |
| [ulmp](https://github.com/ulmen/ulmp) | Wire protocol, networking |
| [ulmcp](https://github.com/ulmen/ulmcp) | Tool protocol, context management |
| [ulflow](https://github.com/ulmen/ulflow) | Agent orchestration (coming soon) |

---
## Installation
``` toml
[dependencies]
ulmcp = { git = "https://github.com/makroumi/ulmcp" }
```

---
## Testing
``` bash
cargo test
```
---
## License
Business Source License 1.1. See [LICENSE](https://www.github.com/makroumi/ulmen/blob/main/LICENSE).

Copyright (c) 2026 El Mehdi Makroumi.