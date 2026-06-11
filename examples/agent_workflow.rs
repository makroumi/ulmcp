//! Real-world agent workflow using ulmcp + uldb.
//!
//! Simulates an AI agent that:
//!   1. Registers tools (code_search, file_read, file_write, run_tests)
//!   2. Registers resources (codebase files)
//!   3. Ingests a codebase into uldb
//!   4. Agent searches for code, reads files, makes changes
//!   5. Validates all operations
//!   6. Measures performance
//!
//! Run: cargo run --example agent_workflow

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use ulmcp::*;

use uldb::engine::{Engine, EngineConfig};

fn main() {
    println!("======================================================");
    println!("  ulmcp + uldb: Real Agent Workflow");
    println!("======================================================");

    // ===================================================================
    // 1. Set up uldb storage
    // ===================================================================

    let dir = std::env::temp_dir().join(format!("ulmcp_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let engine = Arc::new(RwLock::new(Engine::open(EngineConfig::new(&dir)).unwrap()));

    // Ingest a simulated codebase
    let codebase: Vec<(&str, &str)> = vec![
        ("auth/jwt.py::validate_token",
         "def validate_token(token: str) -> dict:\n    key = load_rsa_key()  # SLOW\n    return jwt.decode(token, key, algorithms=['RS256'])"),
        ("auth/jwt.py::create_token",
         "def create_token(user_id: int, expiry: int = 3600) -> str:\n    payload = {'sub': user_id, 'exp': time.time() + expiry}\n    return jwt.encode(payload, load_rsa_key(), algorithm='RS256')"),
        ("auth/jwt.py::refresh_token",
         "def refresh_token(token: str) -> str:\n    payload = validate_token(token)\n    return create_token(payload['sub'])"),
        ("auth/middleware.py::check_auth",
         "def check_auth(request):\n    token = request.headers.get('Authorization', '').replace('Bearer ', '')\n    if not token:\n        raise AuthError('missing token')\n    return validate_token(token)"),
        ("auth/middleware.py::require_role",
         "def require_role(role: str):\n    def decorator(fn):\n        def wrapper(request, *args, **kwargs):\n            user = check_auth(request)\n            if user.get('role') != role:\n                raise AuthError('insufficient permissions')\n            return fn(request, *args, **kwargs)\n        return wrapper\n    return decorator"),
        ("models/user.py::User",
         "class User(BaseModel):\n    id: int\n    email: str\n    password_hash: str\n    role: str = 'user'\n    created_at: datetime"),
        ("models/user.py::UserCreate",
         "class UserCreate(BaseModel):\n    email: str\n    password: str\n    role: str = 'user'"),
        ("api/routes.py::login",
         "async def login(credentials: LoginRequest) -> TokenResponse:\n    user = await authenticate(credentials.email, credentials.password)\n    if not user:\n        raise HTTPException(401, 'invalid credentials')\n    token = create_token(user.id)\n    return TokenResponse(access_token=token)"),
        ("api/routes.py::register",
         "async def register(data: UserCreate) -> UserResponse:\n    existing = await get_user_by_email(data.email)\n    if existing:\n        raise HTTPException(409, 'email already registered')\n    hashed = hash_password(data.password)\n    user = await create_user(data.email, hashed, data.role)\n    return UserResponse.from_orm(user)"),
        ("api/routes.py::profile",
         "@require_role('user')\nasync def profile(request) -> UserResponse:\n    user = await get_user(request.user_id)\n    return UserResponse.from_orm(user)"),
        ("tests/test_auth.py::test_valid_token",
         "def test_valid_token():\n    token = create_token(user_id=1)\n    payload = validate_token(token)\n    assert payload['sub'] == 1"),
        ("tests/test_auth.py::test_expired_token",
         "def test_expired_token():\n    token = create_token(user_id=1, expiry=-1)\n    with pytest.raises(jwt.ExpiredSignatureError):\n        validate_token(token)"),
    ];

    {
        let mut eng = engine.write().unwrap();
        for (key, code) in &codebase {
            eng.put(key.as_bytes(), code.as_bytes()).unwrap();
        }
    }

    println!("\n  [1] Ingested {} files into uldb", codebase.len());

    // ===================================================================
    // 2. Register tools
    // ===================================================================

    let mut registry = Registry::new();

    // Tool: code_search
    let eng_search = Arc::clone(&engine);
    let search_tool = ToolDef::new("code_search", "Search codebase for symbols and patterns")
        .param("query", "Search query text", ParamType::String, true)
        .param(
            "limit",
            "Maximum results to return",
            ParamType::Integer,
            false,
        )
        .timeout(5000)
        .estimated_tokens(200)
        .tag("search");

    registry.register_tool(
        search_tool,
        Box::new(move |call| {
            let query = call
                .arguments
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let limit = call
                .arguments
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(10) as usize;

            let mut eng = eng_search.write().unwrap();
            let spec = uldb::query::planner::QuerySpec {
                text: query.to_string(),
                top_k: limit,
                ..Default::default()
            };
            let hits = eng.indices.query(&spec);

            let results: Vec<String> = hits
                .iter()
                .map(|h| String::from_utf8_lossy(&h.key).to_string())
                .collect();

            ToolResult {
                call_id: call.call_id.clone(),
                status: ToolStatus::Success,
                output: ToolValue::String(results.join("\n")),
                error: None,
                tokens_used: Some(results.len() * 20),
                latency_ms: None,
            }
        }),
    );

    // Tool: file_read
    let eng_read = Arc::clone(&engine);
    let read_tool = ToolDef::new("file_read", "Read a source file by its qualified path")
        .param(
            "path",
            "Qualified path (e.g. auth/jwt.py::validate_token)",
            ParamType::String,
            true,
        )
        .timeout(1000)
        .estimated_tokens(500)
        .tag("io");

    registry.register_tool(
        read_tool,
        Box::new(move |call| {
            let path = call
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let eng = eng_read.read().unwrap();
            match eng.get(path.as_bytes()) {
                Some(data) => ToolResult {
                    call_id: call.call_id.clone(),
                    status: ToolStatus::Success,
                    output: ToolValue::String(String::from_utf8_lossy(&data).to_string()),
                    error: None,
                    tokens_used: Some(data.len() / 4),
                    latency_ms: None,
                },
                None => ToolResult {
                    call_id: call.call_id.clone(),
                    status: ToolStatus::Error,
                    output: ToolValue::Null,
                    error: Some(format!("file not found: {}", path)),
                    tokens_used: None,
                    latency_ms: None,
                },
            }
        }),
    );

    // Tool: file_write
    let eng_write = Arc::clone(&engine);
    let write_tool = ToolDef::new("file_write", "Write or update a source file")
        .param("path", "Qualified path", ParamType::String, true)
        .param("content", "New file content", ParamType::String, true)
        .timeout(2000)
        .tag("io");

    registry.register_tool(
        write_tool,
        Box::new(move |call| {
            let path = call
                .arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let content = call
                .arguments
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let mut eng = eng_write.write().unwrap();
            match eng.put(path.as_bytes(), content.as_bytes()) {
                Ok(()) => ToolResult {
                    call_id: call.call_id.clone(),
                    status: ToolStatus::Success,
                    output: ToolValue::String(format!("wrote {} bytes to {}", content.len(), path)),
                    error: None,
                    tokens_used: Some(10),
                    latency_ms: None,
                },
                Err(e) => ToolResult {
                    call_id: call.call_id.clone(),
                    status: ToolStatus::Error,
                    output: ToolValue::Null,
                    error: Some(format!("write failed: {}", e)),
                    tokens_used: None,
                    latency_ms: None,
                },
            }
        }),
    );

    // Register resources
    let eng_res = Arc::clone(&engine);
    let codebase_resource = ResourceDef::new("uldb://codebase", "Codebase")
        .description("Full codebase indexed in uldb")
        .subscribable();

    registry.register_resource(
        codebase_resource,
        Box::new(move |_uri| {
            let eng = eng_res.read().unwrap();
            let stats = format!(
                "Codebase: {} records indexed, {} symbols",
                eng.memtable_len(),
                eng.indices.stats().fuzzy_symbols
            );
            Some(ResourceContent {
                uri: "uldb://codebase".into(),
                mime_type: "text/plain".into(),
                data: ResourceData::Text(stats),
            })
        }),
    );

    println!(
        "  [2] Registered {} tools, {} resources",
        registry.tool_count(),
        registry.resource_count()
    );

    // ===================================================================
    // 3. Simulate agent workflow
    // ===================================================================

    println!("\n  [3] Agent workflow: refactor authentication\n");

    // Context tracking
    let mut ctx = ContextTracker::new(4096);

    // Step 1: Search for auth code
    let search_call = ToolCall {
        call_id: "s1".into(),
        tool_name: "code_search".into(),
        arguments: {
            let mut m = HashMap::new();
            m.insert(
                "query".into(),
                ToolValue::String("validate token jwt authentication".into()),
            );
            m.insert("limit".into(), ToolValue::Integer(5));
            m
        },
    };

    let search_result = registry.invoke(&search_call);
    assert_eq!(search_result.status, ToolStatus::Success);
    if let Some(tokens) = search_result.tokens_used {
        ctx.use_tokens(tokens);
    }
    println!("      Step 1: Search 'validate token jwt'");
    println!(
        "        Results: {}",
        search_result.output.as_str().unwrap_or("none")
    );
    println!(
        "        Latency: {} us",
        search_result.latency_ms.unwrap_or(0)
    );
    println!("        Tokens used: {} / {} budget", ctx.used, ctx.budget);

    // Step 2: Read the current implementation
    let read_call = ToolCall {
        call_id: "r1".into(),
        tool_name: "file_read".into(),
        arguments: {
            let mut m = HashMap::new();
            m.insert(
                "path".into(),
                ToolValue::String("auth/jwt.py::validate_token".into()),
            );
            m
        },
    };

    let read_result = registry.invoke(&read_call);
    assert_eq!(read_result.status, ToolStatus::Success);
    if let Some(tokens) = read_result.tokens_used {
        ctx.use_tokens(tokens);
    }
    println!("\n      Step 2: Read auth/jwt.py::validate_token");
    let code = read_result.output.as_str().unwrap_or("");
    println!("        Code: {}...", &code[..code.len().min(60)]);
    println!("        Tokens: {} / {}", ctx.used, ctx.budget);

    // Step 3: Write improved implementation
    let new_code = "_cached_key = None\n\ndef validate_token(token: str) -> dict:\n    global _cached_key\n    if _cached_key is None:\n        _cached_key = load_rsa_key()\n    return jwt.decode(token, _cached_key, algorithms=['RS256'])";

    let write_call = ToolCall {
        call_id: "w1".into(),
        tool_name: "file_write".into(),
        arguments: {
            let mut m = HashMap::new();
            m.insert(
                "path".into(),
                ToolValue::String("auth/jwt.py::validate_token".into()),
            );
            m.insert("content".into(), ToolValue::String(new_code.into()));
            m
        },
    };

    let write_result = registry.invoke(&write_call);
    assert_eq!(write_result.status, ToolStatus::Success);
    if let Some(tokens) = write_result.tokens_used {
        ctx.use_tokens(tokens);
    }
    println!("\n      Step 3: Write improved validate_token");
    println!(
        "        Result: {}",
        write_result.output.as_str().unwrap_or("")
    );
    println!("        Tokens: {} / {}", ctx.used, ctx.budget);

    // Step 4: Verify the change
    let verify_call = ToolCall {
        call_id: "v1".into(),
        tool_name: "file_read".into(),
        arguments: {
            let mut m = HashMap::new();
            m.insert(
                "path".into(),
                ToolValue::String("auth/jwt.py::validate_token".into()),
            );
            m
        },
    };

    let verify_result = registry.invoke(&verify_call);
    assert_eq!(verify_result.status, ToolStatus::Success);
    let verified = verify_result.output.as_str().unwrap_or("");
    assert!(
        verified.contains("_cached_key"),
        "new code must contain cache"
    );
    println!("\n      Step 4: Verify change applied");
    println!(
        "        Contains cache: {}",
        verified.contains("_cached_key")
    );

    // Step 5: Read resource
    let resource = registry.read_resource("uldb://codebase");
    assert!(resource.is_some());
    println!("\n      Step 5: Read codebase resource");
    println!(
        "        Info: {}",
        resource.unwrap().data.as_text().unwrap_or("")
    );

    // Step 6: Check context budget
    println!("\n      Context budget:");
    println!("        Used:      {} tokens", ctx.used);
    println!("        Available: {} tokens", ctx.available());
    println!("        Usage:     {:.0}%", ctx.usage_ratio() * 100.0);

    // ===================================================================
    // 4. Discovery
    // ===================================================================

    println!("\n  [4] Tool discovery");
    println!("      All tools:");
    for tool in registry.list_tools() {
        println!(
            "        {} - {} (timeout: {}ms)",
            tool.name,
            tool.description,
            tool.timeout_ms.unwrap_or(0)
        );
    }
    println!(
        "      Search tools: {:?}",
        registry
            .find_tools_by_tag("search")
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
    );
    println!(
        "      IO tools: {:?}",
        registry
            .find_tools_by_tag("io")
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
    );

    // ===================================================================
    // 5. Validation edge cases
    // ===================================================================

    println!("\n  [5] Validation");

    // Missing required param
    let bad_call = ToolCall {
        call_id: "bad1".into(),
        tool_name: "code_search".into(),
        arguments: HashMap::new(),
    };
    let bad_result = registry.invoke(&bad_call);
    assert_eq!(bad_result.status, ToolStatus::Error);
    println!(
        "      Missing param: {} (correct)",
        bad_result.error.unwrap_or_default()
    );

    // Unknown tool
    let unknown_call = ToolCall {
        call_id: "bad2".into(),
        tool_name: "nonexistent_tool".into(),
        arguments: HashMap::new(),
    };
    let unknown_result = registry.invoke(&unknown_call);
    assert_eq!(unknown_result.status, ToolStatus::Error);
    println!(
        "      Unknown tool:  {} (correct)",
        unknown_result.error.unwrap_or_default()
    );

    // Read nonexistent file
    let missing_call = ToolCall {
        call_id: "bad3".into(),
        tool_name: "file_read".into(),
        arguments: {
            let mut m = HashMap::new();
            m.insert(
                "path".into(),
                ToolValue::String("nonexistent/file.py".into()),
            );
            m
        },
    };
    let missing_result = registry.invoke(&missing_call);
    assert_eq!(missing_result.status, ToolStatus::Error);
    println!(
        "      Missing file:  {} (correct)",
        missing_result.error.unwrap_or_default()
    );

    // ===================================================================
    // 6. Benchmarks
    // ===================================================================

    println!("\n  [6] Benchmarks");

    let iters = 10_000u32;

    // Tool invoke (search)
    let search_bench = ToolCall {
        call_id: "bench".into(),
        tool_name: "code_search".into(),
        arguments: {
            let mut m = HashMap::new();
            m.insert("query".into(), ToolValue::String("validate".into()));
            m
        },
    };

    // Warmup
    for _ in 0..100 {
        let _ = registry.invoke(&search_bench);
    }

    let start = Instant::now();
    for _ in 0..iters {
        let _ = registry.invoke(&search_bench);
    }
    let search_ns = start.elapsed().as_nanos() as f64 / iters as f64;
    println!(
        "      code_search invoke:    {:>8.0} ns ({:.0}K ops/sec)",
        search_ns,
        1_000_000_000.0 / search_ns / 1000.0
    );

    // Tool invoke (file_read)
    let read_bench = ToolCall {
        call_id: "bench".into(),
        tool_name: "file_read".into(),
        arguments: {
            let mut m = HashMap::new();
            m.insert(
                "path".into(),
                ToolValue::String("auth/jwt.py::validate_token".into()),
            );
            m
        },
    };

    for _ in 0..100 {
        let _ = registry.invoke(&read_bench);
    }
    let start = Instant::now();
    for _ in 0..iters {
        let _ = registry.invoke(&read_bench);
    }
    let read_ns = start.elapsed().as_nanos() as f64 / iters as f64;
    println!(
        "      file_read invoke:      {:>8.0} ns ({:.0}K ops/sec)",
        read_ns,
        1_000_000_000.0 / read_ns / 1000.0
    );

    // Tool validation
    let start = Instant::now();
    for _ in 0..iters {
        let _ = validate_call(registry.get_tool("code_search").unwrap(), &search_bench);
    }
    let validate_ns = start.elapsed().as_nanos() as f64 / iters as f64;
    println!("      validate_call:         {:>8.0} ns", validate_ns);

    // Context tracking
    let start = Instant::now();
    for _ in 0..iters {
        let mut c = ContextTracker::new(4096);
        c.use_tokens(100);
        c.reserve(200);
        c.fits("some sample text for token counting");
        let _ = c.available();
    }
    let ctx_ns = start.elapsed().as_nanos() as f64 / iters as f64;
    println!("      context_track cycle:   {:>8.0} ns", ctx_ns);

    // Tool discovery
    let start = Instant::now();
    for _ in 0..iters {
        let _ = registry.list_tools();
        let _ = registry.find_tools_by_tag("search");
    }
    let disc_ns = start.elapsed().as_nanos() as f64 / iters as f64;
    println!("      discovery (list+tag):  {:>8.0} ns", disc_ns);

    // Prompt rendering
    let prompt = PromptDef::new("review", "Review {{file}} for {{concern}} issues")
        .param("file", "File", true)
        .param("concern", "Concern", true);
    let mut args = HashMap::new();
    args.insert("file".into(), "auth.py".into());
    args.insert("concern".into(), "security".into());

    let start = Instant::now();
    for _ in 0..iters {
        let _ = prompt.render(&args);
    }
    let prompt_ns = start.elapsed().as_nanos() as f64 / iters as f64;
    println!("      prompt_render:         {:>8.0} ns", prompt_ns);

    // ===================================================================
    // Summary
    // ===================================================================

    println!("\n======================================================");
    println!("  All assertions passed. Workflow complete.");
    println!("======================================================");

    // Cleanup
    let _ = std::fs::remove_dir_all(&dir);
}
