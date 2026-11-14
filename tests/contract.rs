use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};
fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lightrag-cli"))
        .args(args)
        .env_remove("LIGHTRAG_API_KEY")
        .env_remove("LIGHTRAG_TOKEN")
        .env_remove("LIGHTRAG_URL")
        .env("LIGHTRAG_CREDENTIALS_FILE", "/nonexistent-test-credentials")
        .output()
        .unwrap()
}
#[test]
fn full_help() {
    let s: serde_json::Value =
        serde_json::from_str(&String::from_utf8(cli(&["operations", "--json"]).stdout).unwrap())
            .unwrap();
    for op in s.as_array().unwrap() {
        let o = cli(&[op["command"].as_str().unwrap(), "--help"]);
        assert!(o.status.success());
        assert!(String::from_utf8_lossy(&o.stdout).contains("Examples:"));
    }
}
#[test]
fn dry_query() {
    let o = cli(&[
        "post-query",
        "--query",
        "test",
        "--include-references",
        "false",
        "--dry-run",
        "--json",
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["body"]["include_references"], false);
    assert_eq!(v["body"]["query"], "test");
}
#[test]
fn no_mutation_without_yes() {
    let o = cli(&["delete-documents"]);
    assert!(!o.status.success());
    assert!(o.stdout.is_empty());
    assert!(String::from_utf8_lossy(&o.stderr).contains("--yes"));
}
#[test]
fn path_encoding() {
    let o = cli(&[
        "get-documents-track-status-track-id",
        "--track-id",
        "a/b ?",
        "--dry-run",
    ]);
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["path"], "/documents/track_status/a%2Fb%20%3F");
}
#[test]
fn http_json_and_errors() {
    for status in [200, 401, 429, 500] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let h = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut buf = [0; 4096];
            let n = s.read(&mut buf).unwrap();
            assert!(n > 0);
            let body = "{\"result\":true}";
            write!(s,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        });
        let o = cli(&["--url", &url, "get-health", "--json"]);
        h.join().unwrap();
        assert_eq!(o.status.success(), status == 200);
        if status == 200 {
            serde_json::from_slice::<serde_json::Value>(&o.stdout).unwrap();
        } else {
            assert!(o.stdout.is_empty());
            assert!(String::from_utf8_lossy(&o.stderr).contains(&format!("HTTP {status}")));
        }
    }
}
#[test]
fn streaming() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let body = "{\"response\":\"one\"}\n{\"response_time\":1}\n";
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut buf = [0; 8192];
        let n = s.read(&mut buf).unwrap();
        assert!(n > 0);
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    let o = cli(&["--url", &url, "post-query-stream", "--query", "test"]);
    h.join().unwrap();
    assert!(o.status.success());
    assert_eq!(o.stdout, body.as_bytes());
}

#[test]
fn supplemental_ollama_fields() {
    let o = cli(&[
        "post-api-chat",
        "--model",
        "test",
        "--messages",
        r#"[{"role":"user","content":"hello"}]"#,
        "--stream",
        "false",
        "--options",
        r#"{"temperature":0.5}"#,
        "--dry-run",
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["body"]["stream"], false);
    assert_eq!(v["body"]["messages"][0]["role"], "user");
}
#[test]
fn server_dry_run_distinct() {
    let o = cli(&[
        "post-documents-source-conflicts-repair",
        "--canonical-source-key",
        "key",
        "--primary-doc-id",
        "doc",
        "--server-dry-run",
        "false",
        "--dry-run",
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["body"]["dry_run"], false);
    assert_eq!(v["dry_run"], true);
}
#[test]
fn null_boolean_fidelity() {
    let o = cli(&[
        "post-query",
        "--query",
        "test",
        "--include-references",
        "null",
        "--dry-run",
    ]);
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(v["body"]["include_references"].is_null());
}
#[test]
fn stream_error_nonzero() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut buf = [0; 8192];
        assert!(s.read(&mut buf).unwrap() > 0);
        let b = "{\"error\":\"failed\"}\n";
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",b.len()).unwrap();
    });
    let o = cli(&["--url", &url, "post-query-stream", "--query", "test"]);
    h.join().unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("error"));
}
fn read_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let mut data = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stream.read(&mut buf).unwrap();
        assert!(n > 0);
        data.extend_from_slice(&buf[..n]);
        if let Some(pos) = data.windows(4).position(|x| x == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&data[..pos]);
            let size = headers
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|n| n.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if data.len() >= pos + 4 + size {
                break;
            }
        }
    }
    String::from_utf8(data).unwrap()
}
#[test]
fn credential_login_and_bearer() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("creds");
    std::fs::write(&file, "tester:p:a\n").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let h = thread::spawn(move || {
        for login in [true, false] {
            let (mut s, _) = listener.accept().unwrap();
            let request = read_request(&mut s);
            let body = if login {
                assert!(request.starts_with("POST /login"));
                assert!(request.contains("username=tester&password=p%3Aa"));
                r#"{"access_token":"mock-token"}"#
            } else {
                assert!(
                    request
                        .to_lowercase()
                        .contains("authorization: bearer mock-token")
                );
                r#"{"status":"ok"}"#
            };
            write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let o = Command::new(env!("CARGO_BIN_EXE_lightrag-cli"))
        .args([
            "--url",
            &url,
            "--credentials-file",
            file.to_str().unwrap(),
            "get-auth-verify",
            "--json",
        ])
        .env_remove("LIGHTRAG_API_KEY")
        .env_remove("LIGHTRAG_TOKEN")
        .output()
        .unwrap();
    h.join().unwrap();
    assert!(o.status.success());
    assert!(!String::from_utf8_lossy(&o.stdout).contains("mock-token"));
}
#[test]
fn multipart_upload() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("paper.txt");
    std::fs::write(&file, "sample-document").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let request = read_request(&mut s);
        assert!(request.contains("multipart/form-data; boundary="));
        assert!(request.contains("filename=\"paper.txt\""));
        assert!(request.contains("sample-document"));
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
    });
    let o = cli(&[
        "--url",
        &url,
        "post-documents-upload",
        "--file",
        file.to_str().unwrap(),
        "--yes",
    ]);
    h.join().unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}
#[test]
fn nullable_string_and_bad_body() {
    let o = cli(&[
        "post-query",
        "--query",
        "hello",
        "--user-prompt",
        "null",
        "--dry-run",
    ]);
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(v["body"]["user_prompt"].is_null());
    let o = cli(&[
        "post-query",
        "--body",
        "[]",
        "--query",
        "hello",
        "--dry-run",
    ]);
    assert!(!o.status.success());
    assert!(!String::from_utf8_lossy(&o.stderr).contains("panicked"));
    let o = cli(&[
        "get-documents-source-conflicts",
        "--limit",
        "9999",
        "--dry-run",
    ]);
    assert!(!o.status.success());
}
#[test]
fn output_stream_errors_still_fail() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("stream.jsonl");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        let b = "{\"error\":\"failed\"}\n";
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",b.len()).unwrap();
    });
    let o = cli(&[
        "--url",
        &url,
        "post-query-stream",
        "--query",
        "hello",
        "--output",
        path.to_str().unwrap(),
    ]);
    h.join().unwrap();
    assert!(!o.status.success());
    assert!(o.stdout.is_empty());
    assert!(std::fs::read_to_string(path).unwrap().contains("error"));
}

fn mocked_responses(
    responses: Vec<(u16, serde_json::Value)>,
    args: Vec<String>,
) -> (std::process::Output, Vec<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let h = thread::spawn(move || {
        let mut requests = vec![];
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            requests.push(read_request(&mut stream));
            let body = body.to_string();
            write!(stream,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
        requests
    });
    let mut a = vec!["--url".to_string(), url];
    a.extend(args);
    let refs = a.iter().map(String::as_str).collect::<Vec<_>>();
    let o = cli(&refs);
    (o, h.join().unwrap())
}
#[test]
fn automatic_document_pages_preserve_filters() {
    let (a, requests) = mocked_responses(
        vec![
            (
                200,
                serde_json::json!({"documents":[{"id":"one"}],"pagination":{"page":1,"has_next":true}}),
            ),
            (
                200,
                serde_json::json!({"documents":[{"id":"one"},{"id":"two"}],"pagination":{"page":2,"has_next":false}}),
            ),
        ],
        [
            "post-documents-paginated",
            "--all",
            "--status-filter",
            "processed",
            "--page-size",
            "10",
            "--json",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    );
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    let v: serde_json::Value = serde_json::from_slice(&a.stdout).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 2);
    assert_eq!(v["duplicates_skipped"], 1);
    assert_eq!(v["complete"], true);
    for r in requests {
        assert!(r.contains("\"status_filter\":\"processed\""));
        assert!(r.contains("\"page_size\":10"));
    }
}
#[test]
fn cursor_pagination_empty_page_and_loop_guard() {
    let (a, r) = mocked_responses(
        vec![
            (200, serde_json::json!({"conflicts":[],"next_cursor":"a?x"})),
            (
                200,
                serde_json::json!({"conflicts":[{"canonical_source_key":"key"}],"next_cursor":null}),
            ),
        ],
        [
            "get-documents-source-conflicts",
            "--all",
            "--limit",
            "2",
            "--json",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    );
    assert!(a.status.success());
    assert!(r[1].contains("cursor=a%3Fx"));
    let (a, _) = mocked_responses(
        vec![
            (
                200,
                serde_json::json!({"conflicts":[],"next_cursor":"same"}),
            ),
            (
                200,
                serde_json::json!({"conflicts":[],"next_cursor":"same"}),
            ),
        ],
        ["get-documents-source-conflicts", "--all", "--json"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );
    assert!(!a.status.success());
    let v: serde_json::Value = serde_json::from_slice(&a.stdout).unwrap();
    assert_eq!(v["complete"], false);
    assert!(v["error"].as_str().unwrap().contains("repeated"));
}
#[test]
fn page_failure_and_limits_preserve_partial() {
    for limit in [false, true] {
        let mut responses = vec![(
            200,
            serde_json::json!({"documents":[{"id":"one"}],"pagination":{"page":1,"has_next":true}}),
        )];
        if !limit {
            responses.push((500, serde_json::json!({"error":"server"})));
        }
        let mut args = vec![
            "post-documents-paginated".to_string(),
            "--all".into(),
            "--json".into(),
        ];
        if limit {
            args.extend(["--max-pages".into(), "1".into()]);
        }
        let (o, _) = mocked_responses(responses, args);
        assert!(!o.status.success());
        let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["complete"], false);
        assert_eq!(v["items"].as_array().unwrap().len(), 1);
        assert_eq!(v["resume"]["page"], 2);
    }
}
#[test]
fn offline_all_and_help_metadata() {
    let o = cli(&["post-documents-paginated", "--all", "--dry-run"]);
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["automatic_pagination"], true);
    let o = cli(&["post-query", "--help"]);
    let h = String::from_utf8_lossy(&o.stdout);
    assert!(h.contains("enum:"));
    assert!(h.contains("default:"));
    assert!(h.contains("required"));
}
#[test]
fn malformed_stream_fails() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        read_request(&mut s);
        let b = "{broken-json}\n";
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",b.len()).unwrap();
    });
    let o = cli(&["--url", &url, "post-query-stream", "--query", "hello"]);
    h.join().unwrap();
    assert!(!o.status.success());
    assert_eq!(o.stdout, b"{broken-json}\n");
}
#[test]
fn multipart_flag_overrides_body() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("new.txt");
    std::fs::write(&file, "new-content").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let h = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let req = read_request(&mut s);
        assert_eq!(req.matches("name=\"file\"").count(), 1);
        assert!(!req.contains("old-path"));
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").unwrap();
    });
    let o = cli(&[
        "--url",
        &url,
        "post-documents-upload",
        "--body",
        r#"{"file":"old-path"}"#,
        "--file",
        file.to_str().unwrap(),
        "--yes",
    ]);
    h.join().unwrap();
    assert!(o.status.success());
}
#[test]
fn item_cap_and_repeated_document_page() {
    let (a, _) = mocked_responses(
        vec![(
            200,
            serde_json::json!({"documents":[{"id":"one"},{"id":"two"}],"pagination":{"page":1,"has_next":false}}),
        )],
        [
            "post-documents-paginated",
            "--all",
            "--max-items",
            "1",
            "--json",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
    );
    assert!(!a.status.success());
    let v: serde_json::Value = serde_json::from_slice(&a.stdout).unwrap();
    assert_eq!(v["items"].as_array().unwrap().len(), 1);
    assert_eq!(v["resume"]["page"], 1);
    assert_eq!(v["resume"]["item_offset"], 1);
    let (a, _) = mocked_responses(
        vec![
            (
                200,
                serde_json::json!({"documents":[{"id":"one"}],"pagination":{"page":1,"has_next":true}}),
            ),
            (
                200,
                serde_json::json!({"documents":[{"id":"one"}],"pagination":{"page":2,"has_next":true}}),
            ),
        ],
        ["post-documents-paginated", "--all", "--json"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    );
    assert!(!a.status.success());
    assert!(String::from_utf8_lossy(&a.stdout).contains("repeated pagination page"));
}
