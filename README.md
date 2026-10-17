# llmhttpclient

A small synchronous Rust HTTP client for LLM chat-completion endpoints,
extracted from [aiagents](https://github.com/witold-k/aiagents).

The initial implementation intentionally preserves the original `AIRequest`
and `AIRequestResult` API and its `ureq` transport. JVM support is **not**
implemented yet.

## Example

```rust
use llmhttpclient::{AIRequest, AIRequestResult};

let client = AIRequest::new(
    "local-model",
    "http://127.0.0.1:8080/v1/chat/completions",
    "",
    false,
    1024,
    0.2,
);
let messages = r#"[{"role":"user","content":"Hello"}]"#;
match client.request(messages) {
    AIRequestResult::Ok(json) => println!("{json}"),
    other => eprintln!("{other}"),
}
```

The `insecure` argument disables TLS certificate verification; use it only
for trusted endpoints. The client sends a bearer token only when the API key
is nonempty.

## Tests

Run `cargo test`. Integration tests use an in-process loopback HTTP server
and require no external model or internet access.

## Scope

This repository contains the HTTP request client only. The original module
in `aiagents` remains unchanged until that project is explicitly migrated.
