//! End-to-end tests for `yp serve`.
//!
//! These start the real binary on a real socket and speak HTTP to it. The
//! server is hand-rolled, so the things worth testing are the refusals: a
//! route that does not exist, a method that is not allowed, a body that is too
//! large, and above all that no request can reach the filesystem.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const YP: &str = env!("CARGO_BIN_EXE_yp");

/// A running server, killed when the test ends.
struct Server {
    child: Child,
    port: u16,
    _state: std::path::PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self._state);
    }
}

/// Ports are picked by binding one and letting the OS choose, then releasing
/// it. A fixed port would collide with whatever else the machine is running.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    listener.local_addr().expect("addr").port()
}

fn start(args: &[&str], name: &str) -> Server {
    let state = std::env::temp_dir().join(format!("yp-serve-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::fs::create_dir_all(&state).expect("state dir");

    let port = free_port();
    let child = Command::new(YP)
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .args(args)
        .env("YP_STATE_DIR", &state)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn yp serve");

    let server = Server {
        child,
        port,
        _state: state,
    };

    // Indexing happens before the socket opens, so wait for it to answer
    // rather than guessing at a sleep.
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return server;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("server did not start listening on port {port}");
}

/// Send a raw request and return `(status line, body)`.
fn request(port: u16, raw: &str) -> (String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(20)))
        .expect("timeout");
    stream.write_all(raw.as_bytes()).expect("write");
    stream.flush().expect("flush");

    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read");
    let text = String::from_utf8_lossy(&response).into_owned();
    let status = text.lines().next().unwrap_or("").to_string();
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    (status, body)
}

fn get(port: u16, path: &str) -> (String, String) {
    request(
        port,
        &format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"),
    )
}

fn post(port: u16, path: &str, body: &str) -> (String, String) {
    request(
        port,
        &format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\
             Connection: close\r\n\r\n{body}",
            body.len()
        ),
    )
}

#[test]
fn serves_the_page_and_scores_a_prompt() {
    let server = start(&[], "basic");

    let (status, body) = get(server.port, "/");
    assert!(status.contains("200"), "got {status}");
    assert!(body.contains("<textarea"), "page did not render");

    let (status, body) = post(
        server.port,
        "/api/score",
        "refactor parse_args in src/cli.rs",
    );
    assert!(status.contains("200"), "got {status}");
    let score: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert!(score["total"].as_f64().unwrap() > 0.0);
    assert!(score["actionability"]["earned"].is_number());
}

#[test]
fn an_empty_prompt_scores_rather_than_erroring() {
    let server = start(&[], "empty");
    let (status, body) = post(server.port, "/api/score", "");
    assert!(status.contains("200"), "got {status}");
    let score: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(score["total"].as_f64().unwrap(), 0.0);
}

#[test]
fn without_a_repository_the_score_is_marked_as_rescaled() {
    let server = start(&[], "ungrounded");
    let (_, body) = post(server.port, "/api/score", "fix parse_args");
    let score: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert!(score["grounding"].is_null());
    assert_eq!(score["renormalized"], true);

    let (_, info) = get(server.port, "/api/info");
    let info: serde_json::Value = serde_json::from_str(&info).expect("valid JSON");
    assert!(info["repo"].is_null());
}

#[test]
fn with_a_repository_the_score_is_grounded() {
    // Indexes this very repository.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let server = start(&["--repo", &root], "grounded");

    let (_, info) = get(server.port, "/api/info");
    let info: serde_json::Value = serde_json::from_str(&info).expect("valid JSON");
    assert!(info["repo"].is_string(), "got {info}");
    assert!(info["files"].as_u64().unwrap() > 5);

    let (_, body) = post(server.port, "/api/score", "fix simplified_clarity_score");
    let score: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert!(!score["grounding"].is_null(), "should be grounded");
    assert_eq!(score["renormalized"], false);
}

#[test]
fn no_request_can_reach_the_filesystem() {
    // The page is compiled into the binary and nothing else is ever served,
    // so there is no path to traverse. These must all be flat 404s rather
    // than anything that looks like a file.
    let server = start(&[], "traversal");
    for path in [
        "/etc/passwd",
        "/../../../../etc/passwd",
        "/..%2f..%2fetc%2fpasswd",
        "/index.html",
        "/assets/index.html",
        "/Cargo.toml",
        "/.git/config",
    ] {
        let (status, body) = get(server.port, path);
        assert!(status.contains("404"), "{path} gave {status}");
        assert_eq!(body, "not found", "{path} returned content");
    }
}

#[test]
fn unknown_methods_are_refused() {
    let server = start(&[], "methods");
    for method in ["DELETE", "PUT", "PATCH", "TRACE"] {
        let (status, _) = request(
            server.port,
            &format!("{method} / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"),
        );
        assert!(status.contains("405"), "{method} gave {status}");
    }
}

#[test]
fn an_oversized_body_is_refused_before_it_is_read() {
    let server = start(&[], "oversized");
    // Declare far more than the cap without sending it: the server must
    // refuse on the header alone rather than waiting for bytes that will
    // never arrive.
    let (status, _) = request(
        server.port,
        "POST /api/score HTTP/1.1\r\nHost: localhost\r\nContent-Length: 99999999\r\n\
         Connection: close\r\n\r\n",
    );
    assert!(status.contains("400"), "got {status}");
}

#[test]
fn a_malformed_request_does_not_take_the_server_down() {
    let server = start(&[], "malformed");
    for raw in [
        "\r\n\r\n",
        "GET\r\n\r\n",
        "not http at all\r\n\r\n",
        "\0\0\0",
    ] {
        let _ = request(server.port, raw);
    }
    // Still answering afterwards.
    let (status, _) = get(server.port, "/");
    assert!(status.contains("200"), "server died: {status}");
}

#[test]
fn a_query_string_does_not_break_routing() {
    let server = start(&[], "query");
    let (status, _) = get(server.port, "/?utm_source=somewhere");
    assert!(status.contains("200"), "got {status}");
}

#[test]
fn korean_prompts_round_trip_intact() {
    let server = start(&[], "korean");
    let (status, body) = post(
        server.port,
        "/api/score",
        "src/auth/login.rs 의 verify_token 이 만료 토큰에서 panic 나는 것 수정해줘",
    );
    assert!(status.contains("200"), "got {status}");
    let score: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert!(score["total"].as_f64().unwrap() > 0.0);
}
