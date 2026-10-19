//! End-to-end tests for the `yp` binary.
//!
//! These drive the real executable rather than the library, so they catch the
//! things unit tests cannot: argument wiring, stdin handling, and -- once
//! `yp hook` lands -- the absolute requirement that the hook writes nothing to
//! stdout.

use std::io::Write;
use std::process::{Command, Output, Stdio};

const YP: &str = env!("CARGO_BIN_EXE_yp");

/// A state directory shared by the tests that do not care which one they get.
///
/// Without this the binary reads the developer's real `~/.claude`, so whether
/// a test sees a repository index depends on whose machine it runs on -- which
/// is exactly how two of these tests started failing after an index happened
/// to be built locally.
fn shared_state() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("yp-cli-shared-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("state dir");
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(YP)
        .args(args)
        .env("NO_COLOR", "1")
        .env("YP_STATE_DIR", shared_state())
        .output()
        .expect("failed to run yp")
}

fn run_with_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(YP)
        .args(args)
        .env("NO_COLOR", "1")
        .env("YP_STATE_DIR", shared_state())
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
    // No index in this state directory, so the grounding axis reports itself
    // unavailable and the rest are rescaled.
    assert!(value["grounding"].is_null(), "should be ungrounded here");
    assert_eq!(value["renormalized"], true);
    assert_eq!(value["kind"], "task");
    assert!(value["language"].is_null());
}

/// The showcase prompt in Spanish, which scored 149.1 (F) when it was graded.
const SPANISH: &str = "arregla el bug en src/auth/login.rs para que verify_token devuelva \
                       Err(AuthError::Expired). Los tests deben pasar.";

#[test]
fn score_says_why_a_prompt_is_not_graded() {
    for (prompt, why, kind) in [
        ("yes", "follow-up", "follow_up"),
        ("/compact keep the API notes", "slash command", "command"),
        (SPANISH, "looks like Spanish", "unsupported"),
    ] {
        let out = run(&["score", prompt]);
        assert!(out.status.success(), "stderr: {:?}", out.stderr);
        let text = stdout(&out);
        assert!(text.contains("not graded"), "{prompt}: {text}");
        assert!(text.contains(why), "{prompt}: {text}");
        assert!(!text.contains("/ 1000"), "{prompt} was graded: {text}");

        let oneline = stdout(&run(&["score", "--oneline", prompt]));
        assert_eq!(oneline.lines().count(), 1, "got {oneline:?}");
        assert!(oneline.contains("not graded"), "got {oneline:?}");

        // The number is still computed; consumers decide what to show.
        let json: serde_json::Value =
            serde_json::from_str(&stdout(&run(&["score", "--json", prompt]))).unwrap();
        assert_eq!(json["kind"], kind, "{prompt}");
        assert!(json["total"].is_number(), "{prompt}");
    }
    let json: serde_json::Value =
        serde_json::from_str(&stdout(&run(&["score", "--json", SPANISH]))).unwrap();
    assert_eq!(json["language"], "Spanish");
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

/// Run `yp` with an isolated state directory, so hook/statusline tests never
/// touch the real `~/.claude` and never collide with each other.
fn run_isolated(state: &std::path::Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(YP)
        .args(args)
        .env("NO_COLOR", "1")
        .env("YP_STATE_DIR", state)
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

/// A scratch state directory, removed when the test finishes.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("yp-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn hook_payload(prompt: &str, session: &str) -> String {
    serde_json::json!({
        "session_id": session,
        "hook_event_name": "UserPromptSubmit",
        "cwd": "/tmp/project",
        "prompt": prompt,
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// The one absolute requirement
// ---------------------------------------------------------------------------

#[test]
fn the_hook_writes_absolutely_nothing_to_stdout() {
    // Claude Code injects a UserPromptSubmit hook's stdout straight into the
    // model's context. One stray byte here is a token spent on every prompt
    // the user ever types. This is the test that makes the project's central
    // claim -- zero context cost -- true rather than aspirational.
    let scratch = Scratch::new("zero-stdout");
    for prompt in [
        "fix the login handler",
        "그거 좀 고쳐줘",
        "refactor parse_args in src/cli.rs so it returns Config",
        "```
let x = 1;
```",
        "",
    ] {
        let out = run_isolated(
            scratch.path(),
            &["hook"],
            &hook_payload(prompt, "zero-stdout"),
        );
        assert_eq!(
            out.stdout.len(),
            0,
            "hook wrote {} bytes to stdout for prompt {prompt:?}: {:?}",
            out.stdout.len(),
            String::from_utf8_lossy(&out.stdout)
        );
    }
}

#[test]
fn the_hook_never_blocks_the_prompt_whatever_it_is_fed() {
    // Exit code 2 would erase the user's prompt; any other non-zero code puts
    // an error in their transcript. A scoring tool must do neither, no matter
    // how malformed its input.
    let scratch = Scratch::new("never-blocks");
    for payload in [
        "",
        "not json at all",
        "{}",
        "{\"prompt\": 42}",
        "{\"prompt\": \"ok\", \"session_id\": null}",
        "[1, 2, 3]",
        r#"{"prompt": " "}"#,
    ] {
        let out = run_isolated(scratch.path(), &["hook"], payload);
        assert_eq!(
            out.status.code(),
            Some(0),
            "hook exited {:?} on payload {payload:?}",
            out.status.code()
        );
        assert_eq!(out.stdout.len(), 0, "hook printed on payload {payload:?}");
    }
}

#[test]
fn a_prompt_with_half_a_surrogate_pair_is_still_recorded() {
    // Node writes a lone surrogate, such as half of an emoji cut by a paste,
    // as an escape that serde_json refuses. The hook used to drop the whole
    // prompt, and the status line kept the previous grade.
    let scratch = Scratch::new("lone-surrogate");
    let payload =
        "{\"session_id\": \"lone\", \"prompt\": \"fix the parser in src/cli.rs \\ud83d\"}";
    let out = run_isolated(scratch.path(), &["hook"], payload);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout.len(), 0);
    let sidecar = sidecar_of(scratch.path(), "lone");
    assert_eq!(
        sidecar["latest_prompt"],
        "fix the parser in src/cli.rs \u{FFFD}"
    );
}

// ---------------------------------------------------------------------------
// Hook to status line round trip
// ---------------------------------------------------------------------------

#[test]
fn the_score_reaches_the_status_line_through_the_sidecar() {
    let scratch = Scratch::new("roundtrip");
    let session = "round-trip-session";

    let hook = run_isolated(
        scratch.path(),
        &["hook"],
        &hook_payload("refactor parse_args in src/cli.rs", session),
    );
    assert!(hook.status.success());

    let status = run_isolated(
        scratch.path(),
        &["statusline"],
        &format!("{{\"session_id\": \"{session}\"}}"),
    );
    let line = stdout(&status);
    assert!(line.contains('⟦'), "got {line:?}");
    assert_eq!(line.lines().count(), 1, "got {line:?}");
    // A score, a bar and a grade.
    assert!(line.contains('▓') || line.contains('░'), "got {line:?}");
}

#[test]
fn the_status_line_is_silent_before_the_first_prompt() {
    let scratch = Scratch::new("silent");
    let out = run_isolated(
        scratch.path(),
        &["statusline"],
        "{\"session_id\": \"never-prompted\"}",
    );
    assert!(out.status.success());
    assert_eq!(stdout(&out), "", "should print nothing at all");
}

#[test]
fn the_status_line_shows_the_trend_across_prompts() {
    let scratch = Scratch::new("trend");
    let session = "trend-session";

    run_isolated(scratch.path(), &["hook"], &hook_payload("fix it", session));
    run_isolated(
        scratch.path(),
        &["hook"],
        &hook_payload(
            "fix verify_token in src/auth/login.rs so it returns              Err(AuthError::Expired) instead of panicking; tests/auth.rs must pass",
            session,
        ),
    );

    let out = run_isolated(
        scratch.path(),
        &["statusline"],
        &format!("{{\"session_id\": \"{session}\"}}"),
    );
    let line = stdout(&out);
    assert!(line.contains('▲'), "score went up, expected ▲ in {line:?}");
}

fn sidecar_of(state: &std::path::Path, session: &str) -> serde_json::Value {
    let path = state.join("sessions").join(format!("{session}.json"));
    serde_json::from_str(&std::fs::read_to_string(path).expect("sidecar")).expect("valid JSON")
}

#[test]
fn commands_and_follow_ups_leave_the_last_task_in_place() {
    // "continue", "yes" and "ok do it" each used to replace the last task's
    // score with an F, and "/score" -- seen here before Claude Code expands
    // it -- was recorded and then explained instead of the real prompt.
    let scratch = Scratch::new("not-recorded");
    let session = "kinds-session";
    let task = "refactor parse_args in src/cli.rs so it returns Config";
    run_isolated(scratch.path(), &["hook"], &hook_payload(task, session));

    for prompt in [
        "/score",
        "continue",
        "yes",
        "ok do it",
        "응 계속해줘",
        "/compact keep the API notes",
    ] {
        let out = run_isolated(scratch.path(), &["hook"], &hook_payload(prompt, session));
        assert_eq!(out.status.code(), Some(0), "{prompt}");
        assert_eq!(out.stdout.len(), 0, "{prompt}");
    }

    let sidecar = sidecar_of(scratch.path(), session);
    assert_eq!(sidecar["latest_prompt"], task);
    assert_eq!(sidecar["latest"]["kind"], "task");
    assert!(sidecar["history"].as_array().unwrap().is_empty());

    let line = stdout(&run_isolated(
        scratch.path(),
        &["statusline"],
        &format!("{{\"session_id\": \"{session}\"}}"),
    ));
    assert!(line.contains('▓') || line.contains('░'), "got {line:?}");

    let explained = stdout(&run_isolated(
        scratch.path(),
        &["explain", "--no-color", "--session", session],
        "",
    ));
    assert!(explained.contains("/ 1000"), "got {explained}");
    assert!(explained.contains("objective"), "got {explained}");
}

#[test]
fn a_prompt_in_another_language_is_recorded_but_not_graded() {
    let scratch = Scratch::new("unsupported");
    let session = "unsupported-session";
    run_isolated(
        scratch.path(),
        &["hook"],
        &hook_payload("refactor parse_args in src/cli.rs", session),
    );
    run_isolated(scratch.path(), &["hook"], &hook_payload(SPANISH, session));

    let sidecar = sidecar_of(scratch.path(), session);
    assert_eq!(sidecar["latest"]["kind"], "unsupported");
    assert_eq!(sidecar["latest"]["language"], "Spanish");

    let line = stdout(&run_isolated(
        scratch.path(),
        &["statusline"],
        &format!("{{\"session_id\": \"{session}\"}}"),
    ));
    assert!(line.contains("not graded"), "got {line:?}");
    assert_eq!(line.lines().count(), 1, "got {line:?}");
    assert!(!line.contains('▓') && !line.contains('░'), "got {line:?}");

    let explained = stdout(&run_isolated(
        scratch.path(),
        &["explain", "--no-color", "--session", session],
        "",
    ));
    assert!(explained.contains("not graded"), "got {explained}");
    assert!(explained.contains("Spanish"), "got {explained}");
    assert!(explained.contains("last graded prompt"), "got {explained}");
}

#[test]
fn the_trend_ignores_a_prompt_that_was_not_graded() {
    // High, then unsupported, then lower than the first: that is down, even
    // though it is up on the Spanish prompt's ungraded 149.1.
    let scratch = Scratch::new("trend-skip");
    let session = "trend-skip-session";
    for prompt in [
        "fix verify_token in src/auth/login.rs so it returns \
         Err(AuthError::Expired) instead of panicking; tests/auth.rs must pass",
        SPANISH,
        "fix the login handler",
    ] {
        run_isolated(scratch.path(), &["hook"], &hook_payload(prompt, session));
    }
    let line = stdout(&run_isolated(
        scratch.path(),
        &["statusline"],
        &format!("{{\"session_id\": \"{session}\"}}"),
    ));
    assert!(line.contains('▼'), "expected ▼ in {line:?}");
}

#[test]
fn wrapping_preserves_an_existing_status_line() {
    let scratch = Scratch::new("wrap");
    let session = "wrap-session";
    run_isolated(
        scratch.path(),
        &["hook"],
        &hook_payload("refactor parse_args in src/cli.rs", session),
    );

    let out = run_isolated(
        scratch.path(),
        &["statusline", "--wrap", "echo my-existing-line"],
        &format!("{{\"session_id\": \"{session}\"}}"),
    );
    let line = stdout(&out);
    assert!(line.contains("my-existing-line"), "got {line:?}");
    assert!(line.contains('⟦'), "got {line:?}");
    assert_eq!(line.lines().count(), 1, "wrapping must stay one line");
}

// ---------------------------------------------------------------------------
// yp explain (the /score slash command)
// ---------------------------------------------------------------------------

#[test]
fn explain_shows_the_breakdown_of_the_last_scored_prompt() {
    let scratch = Scratch::new("explain");
    run_isolated(
        scratch.path(),
        &["hook"],
        &hook_payload(
            "refactor parse_args in src/cli.rs so it returns Config",
            "e1",
        ),
    );

    let out = run_isolated(scratch.path(), &["explain", "--no-color"], "");
    let text = stdout(&out);
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    assert!(text.contains("/ 1000"), "got {text}");
    assert!(text.contains("actionability"), "got {text}");
    assert!(text.contains("objective"), "got {text}");
}

#[test]
fn explain_reports_the_move_since_the_previous_prompt() {
    let scratch = Scratch::new("explain-trend");
    run_isolated(scratch.path(), &["hook"], &hook_payload("fix it", "e2"));
    run_isolated(
        scratch.path(),
        &["hook"],
        &hook_payload(
            "fix verify_token in src/auth/login.rs so it returns              Err(AuthError::Expired) instead of panicking; tests/auth.rs must pass",
            "e2",
        ),
    );

    let text = stdout(&run_isolated(
        scratch.path(),
        &["explain", "--no-color"],
        "",
    ));
    assert!(text.contains("up "), "expected an upward delta in {text:?}");
}

#[test]
fn explain_says_so_politely_when_nothing_has_been_scored() {
    let scratch = Scratch::new("explain-empty");
    let out = run_isolated(scratch.path(), &["explain", "--no-color"], "");
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("No scored prompt"), "got {text}");
    assert!(text.contains("yp install"), "got {text}");
}

#[test]
fn explain_reproduces_the_score_the_status_line_showed() {
    // The sidecar stores the number; explain re-scores the stored prompt.
    // Those must agree, or the breakdown would be explaining a different
    // score than the one the user saw.
    let scratch = Scratch::new("explain-agrees");
    let prompt = "refactor parse_args in src/cli.rs so it returns Config; tests must pass";
    run_isolated(scratch.path(), &["hook"], &hook_payload(prompt, "e3"));

    let sidecar = std::fs::read_to_string(scratch.path().join("sessions").join("e3.json"))
        .expect("sidecar should exist");
    let stored: serde_json::Value = serde_json::from_str(&sidecar).unwrap();
    let stored_total = stored["latest"]["total"].as_f64().unwrap();

    let direct = stdout(&run(&["score", "--json", prompt]));
    let direct_total: serde_json::Value = serde_json::from_str(&direct).unwrap();
    let direct_total = direct_total["total"].as_f64().unwrap();

    assert!(
        (stored_total - direct_total).abs() < 0.05,
        "sidecar said {stored_total}, rescoring said {direct_total}"
    );
}

// ---------------------------------------------------------------------------
// The grounding axis
// ---------------------------------------------------------------------------

/// Build an index of this very repository into an isolated state directory.
fn index_this_repo(state: &std::path::Path) {
    let out = run_isolated(state, &["index"], "");
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
}

fn grounded_score(state: &std::path::Path, prompt: &str) -> serde_json::Value {
    let out = run_isolated(state, &["score", "--json", prompt], "");
    serde_json::from_str(&stdout(&out)).expect("valid JSON")
}

#[test]
fn grounding_is_scored_once_an_index_exists() {
    let scratch = Scratch::new("grounded");

    let before = grounded_score(scratch.path(), "fix simplified_clarity_score");
    assert!(before["grounding"].is_null(), "no index yet");
    assert_eq!(before["renormalized"], true);

    index_this_repo(scratch.path());

    let after = grounded_score(scratch.path(), "fix simplified_clarity_score");
    assert!(!after["grounding"].is_null(), "index exists now");
    assert_eq!(after["renormalized"], false);
    assert!(after["grounding"]["earned"].as_f64().unwrap() > 0.0);
}

#[test]
fn a_name_this_repository_defines_outscores_one_it_does_not() {
    let scratch = Scratch::new("resolves");
    index_this_repo(scratch.path());

    let resolution = |prompt: &str| -> f64 {
        let v = grounded_score(scratch.path(), prompt);
        v["grounding"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "resolution")
            .unwrap()["earned"]
            .as_f64()
            .unwrap()
    };

    let real = resolution("fix simplified_clarity_score in crates/yp-core/src/grounding.rs");
    let imaginary = resolution("fix compute_paycheck in payroll_ledger.rs");
    assert!(
        real > imaginary,
        "a real name should resolve better: {real} vs {imaginary}"
    );
}

#[test]
fn a_deictic_with_nothing_to_point_at_is_penalised() {
    let scratch = Scratch::new("deixis");
    index_this_repo(scratch.path());

    let deixis = |prompt: &str| -> f64 {
        let v = grounded_score(scratch.path(), prompt);
        v["grounding"]["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "deixis")
            .unwrap()["earned"]
            .as_f64()
            .unwrap()
    };

    assert!(deixis("그거 좀 고쳐줘") < deixis("simplified_clarity_score 를 고쳐줘"));
}

#[test]
fn indexing_twice_is_skipped_when_recent() {
    let scratch = Scratch::new("maxage");
    index_this_repo(scratch.path());
    let out = run_isolated(scratch.path(), &["index", "--max-age", "3600"], "");
    assert!(stdout(&out).contains("skipping"), "got {}", stdout(&out));
}

#[test]
fn the_hook_still_writes_nothing_with_an_index_present() {
    // The grounding axis reads a file. That must not have introduced any
    // output on the path that has to stay silent.
    let scratch = Scratch::new("zero-stdout-grounded");
    index_this_repo(scratch.path());
    let out = run_isolated(
        scratch.path(),
        &["hook"],
        &hook_payload("fix simplified_clarity_score", "g1"),
    );
    assert_eq!(out.stdout.len(), 0, "hook printed with an index present");
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn scoring_against_an_explicit_index_grounds_against_that_repository() {
    // The --index flag is how the grounding axis gets inspected: it answers
    // "what would this prompt score against *that* codebase".
    let scratch = Scratch::new("explicit-index");
    index_this_repo(scratch.path());

    let index = std::fs::read_dir(scratch.path().join("index"))
        .expect("index dir")
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "tsv"))
        .expect("an index file");

    let out = run_isolated(
        scratch.path(),
        &[
            "score",
            "--json",
            "--index",
            index.to_str().unwrap(),
            "fix simplified_clarity_score",
        ],
        "",
    );
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let value: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert!(!value["grounding"].is_null(), "should be grounded");
    assert_eq!(value["renormalized"], false);
}

#[test]
fn an_unreadable_index_is_refused_rather_than_silently_ignored() {
    // Silently falling back to an ungrounded score would make a typo in the
    // path look like a real measurement.
    let scratch = Scratch::new("bad-index");
    let out = run_isolated(
        scratch.path(),
        &["score", "--index", "no/such/index.tsv", "anything"],
        "",
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("could not read an index"));
}
