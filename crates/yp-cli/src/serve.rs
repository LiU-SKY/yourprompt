//! `yp serve` -- the score in a browser.
//!
//! A deliberately small HTTP server built on `std::net` alone. The surface is
//! three routes and one embedded page; pulling in an async runtime and a web
//! framework to serve that would add megabytes to a binary whose whole pitch
//! is starting in milliseconds on the hook path.
//!
//! Being hand-rolled, it is written to refuse rather than to cope:
//!
//! - Nothing is ever read from the filesystem in response to a request. The
//!   only asset is compiled into the binary, so there is no path to traverse.
//! - Requests are capped at [`MAX_BODY`] and headers at [`MAX_HEADERS`], read
//!   under a timeout, so a slow or enormous client cannot tie up a thread
//!   indefinitely or exhaust memory.
//! - Only `GET /`, `GET /api/info` and `POST /api/score` do anything. Every
//!   other path and method gets a flat 404 or 405.
//! - It binds to loopback unless told otherwise, because a scoring box on a
//!   shared network is still a box that accepts arbitrary text from anyone.
//!
//! Scoring is pure and local: no model is called and nothing is stored, so a
//! request leaves no trace beyond the log line.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use yp_core::Corpus;

use crate::repo::IndexCorpus;

const PAGE: &str = include_str!("../assets/index.html");

/// Longest prompt accepted. Far above any real one, far below anything that
/// would trouble the machine.
const MAX_BODY: usize = 256 * 1024;
/// Header bytes accepted before the request is refused.
const MAX_HEADERS: usize = 16 * 1024;
/// How long a client may take over its request before the thread gives up.
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const WRITE_TIMEOUT: Duration = Duration::from_secs(15);

/// What the server was started with, shared by every connection.
struct Context {
    corpus: Option<IndexCorpus>,
    repo_label: Option<String>,
    files: usize,
}

impl Context {
    fn corpus(&self) -> Option<&dyn Corpus> {
        self.corpus.as_ref().map(|c| c as &dyn Corpus)
    }
}

/// The first line of a request, once it has been found acceptable.
struct Request {
    method: String,
    path: String,
    body: String,
}

fn read_request(stream: &mut TcpStream) -> Result<Request, &'static str> {
    let mut reader = BufReader::new(stream.try_clone().map_err(|_| "could not clone stream")?);

    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|_| "could not read request line")?;
    let mut parts = line.split_whitespace();
    let method = parts.next().ok_or("empty request")?.to_string();
    let path = parts.next().ok_or("no path")?.to_string();

    let mut length = 0usize;
    let mut header_bytes = line.len();
    loop {
        let mut header = String::new();
        let read = reader
            .read_line(&mut header)
            .map_err(|_| "could not read headers")?;
        if read == 0 || header == "\r\n" || header == "\n" {
            break;
        }
        header_bytes += read;
        if header_bytes > MAX_HEADERS {
            return Err("headers too large");
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().map_err(|_| "bad content-length")?;
            }
        }
    }

    if length > MAX_BODY {
        return Err("body too large");
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        reader
            .read_exact(&mut body)
            .map_err(|_| "could not read body")?;
    }
    // Prompts are text. Anything that is not valid UTF-8 was not one.
    let body = String::from_utf8(body).map_err(|_| "body was not valid UTF-8")?;

    Ok(Request { method, path, body })
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\n\
         Connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

/// An absolute, readable form of a path, for showing the user which
/// repository is being scored against.
///
/// `--repo .` should not report itself as ".". Windows canonicalisation
/// returns a verbatim `\?\` prefix, which is correct and unreadable, so it
/// is trimmed for display only.
fn display_path(path: &Path) -> String {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let shown = resolved.display().to_string();
    shown.strip_prefix(r"\?\").unwrap_or(&shown).to_string()
}

/// JSON string escaping, for the two short strings the info route returns.
fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn handle(stream: &mut TcpStream, context: &Context) {
    let request = match read_request(stream) {
        Ok(request) => request,
        Err(reason) => {
            respond(
                stream,
                "400 Bad Request",
                "text/plain; charset=utf-8",
                reason.as_bytes(),
            );
            return;
        }
    };

    // Query strings and fragments are not used by any route; strip them rather
    // than letting them make a path miss.
    let path = request
        .path
        .split(['?', '#'])
        .next()
        .unwrap_or("")
        .to_string();

    match (request.method.as_str(), path.as_str()) {
        ("GET", "/") => respond(
            stream,
            "200 OK",
            "text/html; charset=utf-8",
            PAGE.as_bytes(),
        ),
        ("GET", "/api/info") => {
            let body = match (&context.repo_label, context.files) {
                (Some(repo), files) => {
                    format!("{{\"repo\":\"{}\",\"files\":{}}}", json_escape(repo), files)
                }
                (None, _) => "{\"repo\":null,\"files\":0}".to_string(),
            };
            respond(
                stream,
                "200 OK",
                "application/json; charset=utf-8",
                body.as_bytes(),
            );
        }
        ("POST", "/api/score") => {
            let Some(score) = yp_core::score_with(request.body.trim(), context.corpus()) else {
                respond(
                    stream,
                    "500 Internal Server Error",
                    "text/plain; charset=utf-8",
                    b"language resources unavailable",
                );
                return;
            };
            match serde_json::to_vec(&score) {
                Ok(body) => respond(stream, "200 OK", "application/json; charset=utf-8", &body),
                Err(_) => respond(
                    stream,
                    "500 Internal Server Error",
                    "text/plain; charset=utf-8",
                    b"could not serialise score",
                ),
            }
        }
        ("GET", _) | ("HEAD", _) => respond(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found",
        ),
        _ => respond(
            stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"method not allowed",
        ),
    }
}

pub fn run(bind: String, port: u16, repo: Option<String>) -> ExitCode {
    // Index up front rather than per request: indexing takes seconds and a
    // request must not wait for it.
    let (corpus, repo_label, files) = match repo {
        Some(path) => {
            let root = crate::repo::repo_root(Path::new(&path));
            eprintln!("indexing {}...", root.display());
            match crate::repo::build_for(&root) {
                Ok((root, files, terms)) => {
                    eprintln!("  {files} files, {terms} distinct terms");
                    match crate::repo::load_for(&root) {
                        Some(corpus) => (Some(corpus), Some(display_path(&root)), files),
                        None => {
                            eprintln!("yp: index built but could not be read back");
                            return ExitCode::FAILURE;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("yp: could not index {}: {e}", root.display());
                    return ExitCode::FAILURE;
                }
            }
        }
        None => (None, None, 0),
    };

    let context = Arc::new(Context {
        corpus,
        repo_label,
        files,
    });

    let address = format!("{bind}:{port}");
    let listener = match TcpListener::bind(&address) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("yp: could not bind {address}: {e}");
            return ExitCode::FAILURE;
        }
    };

    eprintln!("yourprompt is serving on http://{address}");
    if bind == "0.0.0.0" {
        eprintln!("  reachable from the network -- anyone who can route here can use it");
    }

    for incoming in listener.incoming() {
        let mut stream = match incoming {
            Ok(stream) => stream,
            Err(e) => {
                eprintln!("yp: connection failed: {e}");
                continue;
            }
        };
        let context = Arc::clone(&context);
        // A thread per connection, each with a deadline. Scoring is
        // microseconds; the only way a thread lingers is a client that will
        // not finish its request, and the timeouts end that.
        std::thread::spawn(move || {
            let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
            let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));
            handle(&mut stream, &context);
            let _ = stream.shutdown(std::net::Shutdown::Both);
        });
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_is_self_contained() {
        // It is served from memory with no filesystem access, so every asset
        // has to be inline. An external reference would simply fail to load.
        assert!(PAGE.contains("<textarea"));
        assert!(!PAGE.contains("<script src="), "external script");
        assert!(
            !PAGE.contains("<link rel=\"stylesheet\""),
            "external stylesheet"
        );
        assert!(!PAGE.contains("http://"), "plain-http reference");
    }

    #[test]
    fn a_relative_repo_path_is_shown_absolutely() {
        let shown = display_path(Path::new("."));
        assert_ne!(shown, ".");
        assert!(
            !shown.starts_with(r"\?\"),
            "verbatim prefix leaked: {shown}"
        );
        assert!(!shown.is_empty());
    }

    #[test]
    fn json_escaping_survives_quotes_and_controls() {
        assert_eq!(json_escape(r#"a"b"#), r#"a\"b"#);
        assert_eq!(json_escape("a\\b"), "a\\\\b");
        assert_eq!(json_escape("a\nb"), "a\\nb");
        assert_eq!(json_escape("a\u{1}b"), "a\\u0001b");
        assert_eq!(json_escape("경로/파일.rs"), "경로/파일.rs");
    }

    #[test]
    fn a_windows_repo_path_stays_valid_json() {
        let body = format!(
            "{{\"repo\":\"{}\"}}",
            json_escape(r"C:\Projects\yourprompt")
        );
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(parsed["repo"], r"C:\Projects\yourprompt");
    }
}
