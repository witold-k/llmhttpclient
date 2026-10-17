// SPDX-License-Identifier: Apache-2.0
// Copyright (c) 2026 Witold Kaminski

use llmhttpclient::{AIRequest, AIRequestResult};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

const MESSAGES: &str = r#"[{"role":"user","content":"hello"}]"#;

fn client(url: &str, key: &str) -> AIRequest {
    AIRequest::new("test-model", url, key, false, 123, 0.25)
}

fn read_request(stream: &mut TcpStream) -> (String, Value) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0, "client closed before request headers");
        bytes.extend_from_slice(&buf[..n]);
        if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        assert!(bytes.len() < 1024 * 1024, "oversized request headers");
    };
    let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
    let content_length: usize = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .expect("request Content-Length");
    while bytes.len() - header_end < content_length {
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0, "client closed before request body");
        bytes.extend_from_slice(&buf[..n]);
    }
    let body = serde_json::from_slice(&bytes[header_end..header_end + content_length]).unwrap();
    (headers, body)
}

fn mock_server(
    status: &'static str,
    body: &'static str,
) -> (String, thread::JoinHandle<(String, Value)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/v1/chat/completions",
        listener.local_addr().unwrap()
    );
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
        request
    });
    (url, handle)
}

#[test]
fn posts_expected_payload_and_authorization() {
    let (url, server) = mock_server("200 OK", r#"{"choices":[{"message":{"content":"ok"}}]}"#);
    let result = client(&url, "secret").request(MESSAGES);
    assert!(
        matches!(result, AIRequestResult::Ok(ref value) if value["choices"][0]["message"]["content"] == "ok")
    );
    let (headers, payload) = server.join().unwrap();
    assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1"));
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("content-type: application/json")
    );
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("authorization: bearer secret")
    );
    assert_eq!(
        payload,
        json!({
            "model": "test-model",
            "messages": [{"role":"user","content":"hello"}],
            "max_tokens": 123,
            "temperature": 0.25
        })
    );
}

#[test]
fn omits_authorization_and_grammar_by_default() {
    let (url, server) = mock_server("200 OK", "{}");
    assert!(matches!(
        client(&url, "").request(MESSAGES),
        AIRequestResult::Ok(_)
    ));
    let (headers, payload) = server.join().unwrap();
    assert!(!headers.to_ascii_lowercase().contains("authorization:"));
    assert!(payload.get("grammar").is_none());
}

#[test]
fn includes_optional_grammar() {
    let (url, server) = mock_server("200 OK", "{}");
    let result = client(&url, "")
        .with_grammar("root ::= \"ok\"")
        .request(MESSAGES);
    assert!(matches!(result, AIRequestResult::Ok(_)));
    let (_, payload) = server.join().unwrap();
    assert_eq!(payload["grammar"], "root ::= \"ok\"");
}

#[test]
fn rejects_invalid_messages_before_http() {
    let request = client("http://127.0.0.1:1/", "");
    match request.request("not JSON") {
        AIRequestResult::DeserFailed(original, error) => {
            assert_eq!(original, request);
            assert!(!error.is_empty());
        }
        other => panic!("expected DeserFailed, got {other}"),
    }
}

#[test]
fn reports_json_error_field() {
    let (url, server) = mock_server("200 OK", r#"{"error":{"message":"model unavailable"}}"#);
    match client(&url, "").request(MESSAGES) {
        AIRequestResult::ReportedError(_, error, payload) => {
            assert_eq!(error["message"], "model unavailable");
            assert_eq!(payload["model"], "test-model");
        }
        other => panic!("expected ReportedError, got {other}"),
    }
    server.join().unwrap();
}

#[test]
fn reports_invalid_response_json() {
    let (url, server) = mock_server("200 OK", "not json");
    assert!(matches!(
        client(&url, "").request(MESSAGES),
        AIRequestResult::DecodeFailed(_, _)
    ));
    server.join().unwrap();
}

#[test]
fn reports_http_error() {
    let (url, server) = mock_server("503 Service Unavailable", r#"{"error":"busy"}"#);
    assert!(matches!(
        client(&url, "").request(MESSAGES),
        AIRequestResult::RequestFailed(_, _)
    ));
    server.join().unwrap();
}

#[test]
fn request_serialization_round_trip_preserves_settings() {
    let request = client("https://example.invalid/v1/chat/completions", "secret")
        .with_grammar("root ::= \"ok\"");
    let serialized = serde_json::to_string(&request).unwrap();
    let restored: AIRequest = serde_json::from_str(&serialized).unwrap();
    assert_eq!(request, restored);
    assert!(!format!("{request}").contains("secret"));
}
