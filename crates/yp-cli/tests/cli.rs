//! End-to-end tests for the `yp` binary.
//!
//! These drive the real executable rather than the library, so they catch the
//! things unit tests cannot: argument wiring, stdin handling, and -- once
//! `yp hook` lands -- the absolute requirement that the hook writes nothing to
//! stdout.

use std::io::Write;
use std::process::{Command, Output, Stdio};

const YP: &str = env!("CARGO_BIN_EXE_yp");

fn run(args: &[&str]) -> Output {
    Command::new(YP)
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("failed to run yp")
}

fn run_with_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(YP)
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn yp");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    child.wait_with_output().expect("wait for yp")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn scores_a_prompt_given_as_arguments() {
    let out = run(&["score", "refactor parse_args in src/cli.rs"]);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let text = stdout(&out);
    assert!(text.contains("/ 1000"), "got {text}");
    assert!(text.contains("actionability"), "got {text}");
}

#[test]
fn reads_the_prompt_from_stdin_when_no_arguments_are_given() {
    let out = run_with_stdin(&["score"], "refactor parse_args in src/cli.rs");
    assert!(out.status.success());
    assert!(stdout(&out).contains("/ 1000"));
}

#[test]
fn json_output_is_valid_and_carries_every_axis() {
    let out = run(&["score", "--json", "refactor parse_args in src/cli.rs"]);
    assert!(out.status.success());
    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert!(value["total"].is_number());
    assert!(value["grade"].is_string());
    for axis in ["actionability", "clarity", "context"] {
        assert!(value[axis]["earned"].is_number(), "missing axis {axis}");
    }
    assert!(value["grounding"].is_null(), "grounding lands in M3");
}

#[test]
fn oneline_output_is_a_single_line() {
    let out = run(&["score", "--oneline", "refactor parse_args"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert_eq!(text.lines().count(), 1, "got {text:?}");
}

#[test]
fn no_color_output_contains_no_escape_sequences() {
    let out = run(&["score", "--no-color", "refactor parse_args"]);
    assert!(!stdout(&out).contains('\x1b'));
}

#[test]
fn the_same_prompt_always_scores_the_same() {
    let first = stdout(&run(&[
        "score",
        "--json",
        "fix verify_token in src/auth.rs",
    ]));
    for _ in 0..3 {
        let again = stdout(&run(&[
            "score",
            "--json",
            "fix verify_token in src/auth.rs",
        ]));
        assert_eq!(first, again, "score is not deterministic across processes");
    }
}

#[test]
fn an_empty_prompt_is_handled_rather_than_crashing() {
    let out = run_with_stdin(&["score"], "");
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert!(stdout(&out).contains("/ 1000"));
}

#[test]
fn a_specific_prompt_outscores_the_vague_version_of_itself() {
    let score_of = |text: &str| -> f64 {
        let out = run(&["score", "--json", text]);
        let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
        v["total"].as_f64().unwrap()
    };
    let vague = score_of("fix the login handler");
    let specific = score_of(
        "fix verify_token in src/auth/login.rs: it panics on an expired token \
         and should return Err(AuthError::Expired) instead. the test \
         tests/auth.rs::expired_token_is_rejected must pass.",
    );
    assert!(specific > vague, "specific {specific} vs vague {vague}");
}

#[test]
fn conflicting_output_flags_are_rejected() {
    let out = run(&["score", "--json", "--oneline", "anything"]);
    assert!(!out.status.success(), "should have refused both flags");
}
