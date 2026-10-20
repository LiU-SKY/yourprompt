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

/// `yp`, with nothing from the caller's environment that would change which
/// session it reads: run inside Claude Code, as these tests may be,
/// CLAUDE_CODE_SESSION_ID names the session that is running them.
fn yp() -> Command {
    let mut command = Command::new(YP);
    command
        .env("NO_COLOR", "1")
        .env_remove("CLAUDE_CODE_SESSION_ID");
    command
}

fn run(args: &[&str]) -> Output {
    yp().args(args)
        .env("YP_STATE_DIR", shared_state())
        .output()
        .expect("failed to run yp")
}

fn run_with_stdin(args: &[&str], stdin: &str) -> Output {
    let mut child = yp()
        .args(args)
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

/// What a background agent's report back looks like when Claude Code runs
/// the prompt hook on it.
const TASK_NOTIFICATION: &str = "<task-notification>\n<task-id>agent-a1b</task-id>\n\
                                 <status>completed</status>\n<summary>Agent \"Investigate \
                                 auth bug\" finished</summary>\n<result>Found null pointer in \
                                 src/auth/validate.ts:42.</result>\n</task-notification>";

#[test]
fn score_says_why_a_prompt_is_not_graded() {
    for (prompt, why, kind) in [
        ("yes", "follow-up", "follow_up"),
        ("/compact keep the API notes", "slash command", "command"),
        (SPANISH, "looks like Spanish", "unsupported"),
        (TASK_NOTIFICATION, "Claude Code sent this turn", "automated"),
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
/// touch the real `~/.claude` and never collide with each other. The same
/// directory stands in for Claude Code's own, where the hook looks for the
/// user's commands.
fn run_isolated(state: &std::path::Path, args: &[&str], stdin: &str) -> Output {
    let mut child = yp()
        .args(args)
        .env("YP_STATE_DIR", state)
        .env("CLAUDE_CONFIG_DIR", state)
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
        "looks good to me",
        "알아서 해줘",
        "/review is this safe to merge",
        TASK_NOTIFICATION,
        // A custom command's argument, however much it reads like a
        // sentence about a route; a plan approved without naming work; and
        // thanks as a session ends.
        "/fix test fails on windows",
        "/plan api returns too much data",
        "proceed as planned",
        "수고하셨어요",
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
fn a_long_report_is_left_out_and_markup_the_user_wrote_is_recorded() {
    // Only the first 64 KB of a prompt is scored, and a report longer than
    // that lost the close tag that marks it as Claude Code's: one of 69,326
    // bytes replaced an 809.5 A in the status line with 205.7, an F. A
    // request the user wrapped in a hyphenated tag of their own was taken
    // for Claude Code's and never recorded.
    let scratch = Scratch::new("long-report");
    let session = "long-report-session";
    let task = "refactor parse_args in src/cli.rs so it returns Config";
    run_isolated(scratch.path(), &["hook"], &hook_payload(task, session));

    let report = format!(
        "<task-notification>\n<task-id>a1b2</task-id>\n<status>completed</status>\n\
         <result>{}</result>\n</task-notification>",
        "The agent read src/auth/login.rs and found the bug. ".repeat(1_400)
    );
    assert!(report.len() > 70 * 1024, "{} bytes", report.len());
    let out = run_isolated(scratch.path(), &["hook"], &hook_payload(&report, session));
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout.len(), 0);
    assert_eq!(sidecar_of(scratch.path(), session)["latest_prompt"], task);
    let line = stdout(&run_with_stdin(&["score", "--oneline"], &report));
    assert!(line.contains("Claude Code sent this turn"), "got {line:?}");

    let theirs = "<bug-report>\nThe login form submits twice when you press Enter.\n</bug-report>";
    run_isolated(scratch.path(), &["hook"], &hook_payload(theirs, session));
    let sidecar = sidecar_of(scratch.path(), session);
    assert_eq!(sidecar["latest_prompt"], theirs);
    assert_eq!(sidecar["latest"]["kind"], "task");
    let line = stdout(&run(&["score", "--oneline", theirs]));
    assert!(!line.contains("not graded"), "got {line:?}");
}

#[test]
fn a_command_with_a_file_of_its_own_is_left_out_however_its_argument_reads() {
    // "/triage form submits twice" opens the way a sentence about a form on
    // the /triage route does, and is graded. With a triage command in the
    // project, or a hotfix command in the user's Claude Code directory, it
    // is that command.
    let scratch = Scratch::new("command-files");
    let project = scratch.path().join("project");
    let commands = project.join(".claude").join("commands");
    std::fs::create_dir_all(&commands).unwrap();
    std::fs::write(commands.join("triage.md"), "Triage: $ARGUMENTS\n").unwrap();
    std::fs::create_dir_all(scratch.path().join("commands")).unwrap();
    std::fs::write(
        scratch.path().join("commands").join("hotfix.md"),
        "Fix: $ARGUMENTS\n",
    )
    .unwrap();
    let session = "command-files-session";
    let hook = |prompt: &str| {
        let payload = serde_json::json!({
            "session_id": session,
            "hook_event_name": "UserPromptSubmit",
            "cwd": project,
            "prompt": prompt,
        });
        let out = run_isolated(scratch.path(), &["hook"], &payload.to_string());
        assert_eq!(out.status.code(), Some(0), "{prompt}");
        assert_eq!(out.stdout.len(), 0, "{prompt}");
    };

    let task = "refactor parse_args in src/cli.rs so it returns Config";
    hook(task);
    for prompt in [
        "/triage form submits twice",
        "/hotfix page crashes on submit",
    ] {
        hook(prompt);
        let sidecar = sidecar_of(scratch.path(), session);
        assert_eq!(sidecar["latest_prompt"], task, "{prompt}");
    }
    // With no file of that name the same words are a sentence about a
    // route, and graded.
    let route = "/checkout form submits twice";
    hook(route);
    assert_eq!(sidecar_of(scratch.path(), session)["latest_prompt"], route);
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

/// Mark `dir` as a repository root without making it a repository. yp only
/// looks for `.git`; git refuses a `.git` file that points nowhere, so the
/// index is built by yp's own walk rather than from whatever repository
/// happens to enclose the temporary directory.
fn mark_as_root(dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(".git"), "gitdir: nowhere\n").unwrap();
}

#[test]
fn one_directory_spelt_another_way_still_finds_its_index() {
    // Indexed as Git Bash types the path, then scored from a hook payload
    // spelling it the Windows way: 529.8 grounded became 356.4 ungrounded.
    let scratch = Scratch::new("spelling");
    let project = scratch.path().join("project");
    mark_as_root(&project);
    std::fs::create_dir_all(project.join("src")).unwrap();
    std::fs::write(
        project.join("src").join("auth.rs"),
        "pub fn verify_token(token: &str) -> bool { !token.is_empty() }\n",
    )
    .unwrap();
    let state = scratch.path().join("state");
    let text = project.to_string_lossy().into_owned();
    let (indexed_as, scored_from) = if cfg!(windows) {
        let lower_drive = text[..1].to_ascii_lowercase() + &text[1..];
        (text.replace('\\', "/"), format!("{lower_drive}\\"))
    } else {
        (format!("{text}/"), format!("{text}/./"))
    };

    let out = run_isolated(&state, &["index", "--root", &indexed_as], "");
    assert!(out.status.success(), "stderr: {:?}", out.stderr);
    let payload = serde_json::json!({
        "session_id": "spelling",
        "cwd": scored_from,
        "prompt": "fix verify_token in src/auth.rs so it rejects an empty token",
    });
    run_isolated(&state, &["hook"], &payload.to_string());
    let sidecar = sidecar_of(&state, "spelling");
    assert_eq!(sidecar["latest"]["renormalized"], false, "{sidecar}");
    let indexes = std::fs::read_dir(state.join("index")).unwrap().count();
    assert_eq!(indexes, 1);
}

#[test]
fn an_index_named_before_roots_were_canonical_is_still_found() {
    // An upgrade must not cost grounding until the next rebuild. Earlier
    // versions named an index after the root exactly as it was spelt, here
    // with a trailing separator.
    let scratch = Scratch::new("old-index-name");
    let project = scratch.path().join("project");
    mark_as_root(&project);
    std::fs::write(
        project.join("auth.rs"),
        "pub fn verify_token(token: &str) -> bool { !token.is_empty() }\n",
    )
    .unwrap();
    let state = scratch.path().join("state");
    let out = run_isolated(&state, &["index", "--root", &project.to_string_lossy()], "");
    assert!(out.status.success(), "{out:?}");

    let spelt = format!("{}{}", project.to_string_lossy(), std::path::MAIN_SEPARATOR);
    let index_dir = state.join("index");
    let built = std::fs::read_dir(&index_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let earlier = index_dir.join(yp_index::index_file_name(std::path::Path::new(&spelt)));
    assert_ne!(built, earlier);
    std::fs::rename(&built, &earlier).unwrap();

    let payload = serde_json::json!({
        "session_id": "old-name",
        "cwd": spelt,
        "prompt": "fix verify_token in auth.rs so it rejects an empty token",
    });
    run_isolated(&state, &["hook"], &payload.to_string());
    let sidecar = sidecar_of(&state, "old-name");
    assert_eq!(sidecar["latest"]["renormalized"], false, "{sidecar}");
}

#[test]
fn a_home_directory_under_git_is_not_indexed() {
    // With a .git in the home directory, a session opened anywhere below it
    // used to index up to 20,000 home files.
    let scratch = Scratch::new("home-git");
    let home = scratch.path().join("home");
    let downloads = home.join("Downloads");
    mark_as_root(&home);
    std::fs::create_dir_all(&downloads).unwrap();
    std::fs::write(home.join("notes.txt"), "fn something_at_home() {}\n").unwrap();
    std::fs::write(downloads.join("a.rs"), "fn downloaded() {}\n").unwrap();
    let state = scratch.path().join("state");

    let index = |cwd: &std::path::Path, quiet: bool| {
        let mut command = yp();
        command.arg("index").current_dir(cwd);
        if quiet {
            command.arg("--quiet");
        }
        command
            .env("YP_STATE_DIR", &state)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .output()
            .unwrap()
    };

    let out = index(&home, false);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("home directory"),
        "{out:?}"
    );
    assert!(!state.join("index").exists(), "home was indexed");
    let quiet = index(&home, true);
    assert!(quiet.status.success());
    assert!(
        quiet.stdout.is_empty() && quiet.stderr.is_empty(),
        "{quiet:?}"
    );

    // Below home, the directory itself is indexed, not home.
    let out = index(&downloads, false);
    assert!(out.status.success(), "{out:?}");
    let said = stdout(&out);
    assert!(said.contains("indexed 1 files"), "{said}");
    assert!(said.trim_end().ends_with("Downloads"), "{said}");
}

// ---------------------------------------------------------------------------
// /score picks the session it is typed in
// ---------------------------------------------------------------------------

#[test]
fn explain_explains_the_session_claude_code_says_it_is_in() {
    // Two sessions in one repository: /score typed in the first used to
    // explain the second's "fix the bug", because it was written last.
    let scratch = Scratch::new("explain-session");
    let task = "refactor parse_args in src/cli.rs so it returns Config";
    run_isolated(scratch.path(), &["hook"], &hook_payload(task, "sess-A"));
    run_isolated(
        scratch.path(),
        &["hook"],
        &hook_payload("fix the bug", "sess-B"),
    );
    let total = |session: &str| {
        let total = sidecar_of(scratch.path(), session)["latest"]["total"]
            .as_f64()
            .unwrap();
        format!("{total:.1} / 1000")
    };

    let explain_in = |session: &str| {
        let out = yp()
            .args(["explain", "--no-color"])
            .env("YP_STATE_DIR", scratch.path())
            .env("CLAUDE_CODE_SESSION_ID", session)
            .output()
            .unwrap();
        stdout(&out)
    };
    let a = explain_in("sess-A");
    assert!(a.contains(&total("sess-A")), "{a}");
    assert!(!a.contains(&total("sess-B")), "{a}");
    assert!(explain_in("sess-B").contains(&total("sess-B")));

    let none = explain_in("sess-never-prompted");
    assert!(
        none.contains("No scored prompt found yet in this session"),
        "{none}"
    );
}

#[test]
fn explain_says_when_the_score_did_not_move() {
    // The status line showed "·" while explain said "down 0.0".
    let scratch = Scratch::new("explain-same");
    let task = "In crates/yp-cli/src/hook.rs, change run so it skips scoring when the \
                prompt is longer than 64 KB. Done means: cargo test -p yp-cli passes.";
    for _ in 0..2 {
        run_isolated(scratch.path(), &["hook"], &hook_payload(task, "same"));
    }
    let text = stdout(&run_isolated(
        scratch.path(),
        &["explain", "--no-color", "--session", "same"],
        "",
    ));
    assert!(
        text.contains("same as your previous prompt in this session"),
        "{text}"
    );
    assert!(!text.contains("down 0.0"), "{text}");
}

// ---------------------------------------------------------------------------
// yp install, as Claude Code runs what it writes
// ---------------------------------------------------------------------------

/// The shell Claude Code hands command strings to here: `sh` on macOS and
/// Linux, Git Bash on Windows, found the way `yp` finds it. `None` on Windows
/// without Git Bash.
fn claude_shell() -> Option<std::path::PathBuf> {
    use std::path::{Path, PathBuf};
    if !cfg!(windows) {
        return Some(PathBuf::from("sh"));
    }
    if let Some(bash) = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").map(PathBuf::from) {
        if bash.is_file() {
            return Some(bash);
        }
    }
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        if !dir.join("git.exe").is_file() {
            continue;
        }
        for root in [dir.parent().and_then(Path::parent), dir.parent()]
            .into_iter()
            .flatten()
        {
            let bash = root.join("bin").join("bash.exe");
            if bash.is_file() {
                return Some(bash);
            }
        }
    }
    let bash = PathBuf::from(r"C:\Program Files\Git\bin\bash.exe");
    bash.is_file().then_some(bash)
}

/// Every shell a generated command must run in here: Claude Code's, and
/// bash as well where `sh` is something else.
fn shells() -> Vec<std::path::PathBuf> {
    let mut shells: Vec<_> = claude_shell().into_iter().collect();
    let bash_too = !cfg!(windows)
        && Command::new("bash")
            .args(["-c", "true"])
            .status()
            .is_ok_and(|s| s.success());
    if bash_too {
        shells.push("bash".into());
    }
    shells
}

/// An isolated Claude Code configuration: a settings directory, a state
/// directory and a home, all scratch, and a copy of yp in a directory whose
/// name needs quoting, as `C:\Users\John Smith` does.
struct Setup {
    _scratch: Scratch,
    config: std::path::PathBuf,
    state: std::path::PathBuf,
    home: std::path::PathBuf,
    project: std::path::PathBuf,
    yp: std::path::PathBuf,
}

impl Setup {
    fn new(name: &str) -> Self {
        Setup::with_bin(name, "yp's bin")
    }

    /// The same, with yp in a directory called `bin`.
    fn with_bin(name: &str, bin: &str) -> Self {
        let scratch = Scratch::new(name);
        let base = scratch.path().to_path_buf();
        let bin = base.join(bin);
        let project = base.join("project");
        for dir in ["config", "state", "home"] {
            std::fs::create_dir_all(base.join(dir)).unwrap();
        }
        std::fs::create_dir_all(&bin).unwrap();
        mark_as_root(&project);
        std::fs::write(project.join("lib.rs"), "pub fn parse_args() {}\n").unwrap();
        let yp = bin.join(std::path::Path::new(YP).file_name().unwrap());
        std::fs::copy(YP, &yp).unwrap();
        Setup {
            config: base.join("config"),
            state: base.join("state"),
            home: base.join("home"),
            project,
            yp,
            _scratch: scratch,
        }
    }

    /// `command` with this setup's environment, and nothing of the real one
    /// that would point it at the real ~/.claude.
    fn isolate<'a>(&self, command: &'a mut Command) -> &'a mut Command {
        command
            .env("CLAUDE_CONFIG_DIR", &self.config)
            .env("YP_STATE_DIR", &self.state)
            .env("HOME", &self.home)
            .env("NO_COLOR", "1")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .current_dir(&self.project)
    }

    fn install(&self, args: &[&str]) -> Output {
        let mut command = Command::new(&self.yp);
        command.arg("install").args(args);
        self.isolate(&mut command).output().unwrap()
    }

    fn settings(&self) -> serde_json::Value {
        let text = std::fs::read_to_string(self.config.join("settings.json")).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    /// Run a command string as Claude Code would: through `shell`, or with
    /// no shell at all for the exec form.
    fn run(&self, shell: &std::path::Path, handler: &serde_json::Value, input: &str) -> Output {
        let command = handler["command"].as_str().unwrap();
        let mut process = match handler["args"].as_array() {
            Some(args) => {
                let mut process = Command::new(command);
                process.args(args.iter().map(|a| a.as_str().unwrap()));
                process
            }
            None => {
                let mut process = Command::new(shell);
                process.args(["-c", command]);
                process
            }
        };
        let mut child = self
            .isolate(&mut process)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }
}

/// Our handler for `event`: the one that is not the user's `echo`.
fn our_handler(settings: &serde_json::Value, event: &str) -> serde_json::Value {
    settings["hooks"][event]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|group| group["hooks"].as_array().unwrap().iter())
        .find(|h| !h["command"].as_str().unwrap().starts_with("echo"))
        .cloned()
        .unwrap()
}

#[test]
fn every_command_yp_install_writes_runs_in_the_shell_claude_code_uses() {
    // v0.1.1 wrote unquoted backslash paths, and under Git Bash every one of
    // its commands exited 127, so nothing was ever scored.
    let shells = shells();
    if shells.is_empty() {
        eprintln!("no Git Bash here; Claude Code would use PowerShell");
        return;
    }
    let setup = Setup::new("install-shells");
    // A status line that reads its own stdin: wrapped by v0.1.1, the outer
    // shell ran its $(cat) and the whole line came out blank.
    let theirs = r#"input=$(cat); echo "mine:${#input}""#;
    std::fs::write(
        setup.config.join("settings.json"),
        serde_json::json!({ "statusLine": { "type": "command", "command": theirs } }).to_string(),
    )
    .unwrap();
    let out = setup.install(&[]);
    assert!(out.status.success(), "{out:?}");
    let settings = setup.settings();

    for shell in &shells {
        let session = format!("shell-{}", shell.file_stem().unwrap().to_string_lossy());
        let payload = serde_json::json!({
            "session_id": session,
            "hook_event_name": "UserPromptSubmit",
            "cwd": setup.project,
            "prompt": "refactor parse_args in lib.rs so it returns Config",
        })
        .to_string();
        let hook = setup.run(shell, &our_handler(&settings, "UserPromptSubmit"), &payload);
        assert_eq!(hook.status.code(), Some(0), "{shell:?} hook: {hook:?}");
        assert!(hook.stdout.is_empty(), "{shell:?} hook printed: {hook:?}");
        assert!(
            setup
                .state
                .join("sessions")
                .join(format!("{session}.json"))
                .exists(),
            "{shell:?}: the hook ran but recorded nothing"
        );

        let start = serde_json::json!({ "session_id": session, "hook_event_name": "SessionStart" });
        let index = setup.run(
            shell,
            &our_handler(&settings, "SessionStart"),
            &start.to_string(),
        );
        assert_eq!(index.status.code(), Some(0), "{shell:?} index: {index:?}");
        assert!(
            index.stdout.is_empty() && index.stderr.is_empty(),
            "{index:?}"
        );
        assert!(setup
            .state
            .join("index")
            .read_dir()
            .unwrap()
            .next()
            .is_some());

        let status_json =
            serde_json::json!({ "session_id": session, "cwd": setup.project }).to_string();
        let line = setup.run(shell, &settings["statusLine"], &status_json);
        assert_eq!(
            line.status.code(),
            Some(0),
            "{shell:?} status line: {line:?}"
        );
        let text = stdout(&line);
        assert!(
            text.contains(&format!("mine:{}", status_json.len())),
            "{shell:?}: their status line lost its stdin: {text:?}"
        );
        assert!(text.contains('⟦'), "{shell:?}: no score in {text:?}");
        assert_eq!(text.lines().count(), 1, "{text:?}");
    }
}

#[test]
fn a_hook_run_without_its_arguments_does_not_block_the_prompt() {
    // A Claude Code older than 2.1.139 that skips a hook's `args` hands the
    // bare executable to its shell. clap's usage error exits 2, which would
    // block every prompt.
    let scratch = Scratch::new("bare");
    let out = run_isolated(
        scratch.path(),
        &[],
        &hook_payload("fix the login handler", "bare"),
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(out.stdout.is_empty(), "{out:?}");

    let Some(shell) = shells().into_iter().next() else {
        return;
    };
    let setup = Setup::with_bin("install-bare", "bin");
    assert!(setup.install(&[]).status.success());
    let settings = setup.settings();
    for event in ["UserPromptSubmit", "SessionStart"] {
        let bare = serde_json::json!({ "command": our_handler(&settings, event)["command"] });
        let out = setup.run(&shell, &bare, &hook_payload("fix the login handler", "old"));
        assert_eq!(out.status.code(), Some(0), "{event}: {out:?}");
        assert!(out.stdout.is_empty(), "{event}: {out:?}");
    }
}

#[test]
fn reinstalling_from_a_new_place_points_every_command_there() {
    // cargo install first, then install.ps1 with the first binary gone:
    // v0.1.1 said "already registered" and kept the dead path.
    let setup = Setup::new("install-moved");
    let old_bin = setup.config.parent().unwrap().join("old bin");
    std::fs::create_dir_all(&old_bin).unwrap();
    let old = old_bin.join(setup.yp.file_name().unwrap());
    std::fs::copy(&setup.yp, &old).unwrap();
    let mut first = Command::new(&old);
    first.arg("install");
    assert!(setup.isolate(&mut first).output().unwrap().status.success());
    std::fs::remove_file(&old).unwrap();

    let out = setup.install(&[]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        stdout(&out).matches("which no longer exists").count(),
        3,
        "{out:?}"
    );
    let settings = setup.settings();
    let mut commands = vec![settings["statusLine"]["command"].clone()];
    for event in ["UserPromptSubmit", "SessionStart"] {
        commands.push(our_handler(&settings, event)["command"].clone());
        assert_eq!(settings["hooks"][event].as_array().unwrap().len(), 1);
    }
    // The status line spells "yp's" the way its shell needs it quoted.
    for command in commands {
        let command = command.as_str().unwrap();
        assert!(command.contains("s bin/yp"), "{command}");
        assert!(!command.contains("old bin"), "{command}");
    }
}

#[test]
fn install_twice_then_uninstall_gives_back_the_settings_and_the_backup() {
    let setup = Setup::new("install-roundtrip");
    let original = serde_json::json!({
        "model": "opus",
        "statusLine": {
            "type": "command",
            "command": "bash ~/.claude/statusline-command.sh",
            "padding": 0
        },
        "hooks": {
            "UserPromptSubmit": [
                { "hooks": [ { "type": "command", "command": "echo other-hook" } ] }
            ]
        }
    });
    let original_text = serde_json::to_string_pretty(&original).unwrap();
    std::fs::write(setup.config.join("settings.json"), &original_text).unwrap();
    let backup = setup.config.join("settings.json.yp-backup");
    let score = setup.config.join("commands").join("score.md");

    assert!(setup.install(&[]).status.success());
    let installed = setup.settings();
    assert_eq!(installed["statusLine"]["padding"], 0);
    assert_eq!(
        std::fs::read_to_string(&score).unwrap(),
        include_str!("../../../plugin/commands/score.md")
    );
    let second = setup.install(&[]);
    assert!(second.status.success());
    assert_eq!(
        setup.settings(),
        installed,
        "the second install changed settings"
    );
    assert!(stdout(&second).contains("needed no change"), "{second:?}");
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), original_text);

    let out = setup.install(&["--uninstall"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(setup.settings(), original);
    assert!(!score.exists(), "/score was left behind");
    assert!(!setup.state.join("install.json").exists());
    // Uninstall neither writes the backup nor removes it.
    assert_eq!(std::fs::read_to_string(&backup).unwrap(), original_text);
    assert!(stdout(&out).contains("yp-backup"), "{out:?}");
}

#[test]
fn install_leaves_a_score_command_it_did_not_write() {
    let setup = Setup::new("install-theirs");
    let commands = setup.config.join("commands");
    std::fs::create_dir_all(&commands).unwrap();
    let theirs = "---\ndescription: my own score\n---\nSomething else entirely.\n";
    std::fs::write(commands.join("score.md"), theirs).unwrap();

    let out = setup.install(&[]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("left your own"), "{out:?}");
    assert!(setup.install(&["--uninstall"]).status.success());
    assert_eq!(
        std::fs::read_to_string(commands.join("score.md")).unwrap(),
        theirs
    );
}

#[test]
fn print_only_shows_everything_and_writes_nothing() {
    let setup = Setup::new("install-print");
    let out = setup.install(&["--print-only"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("commands"), "{text}");
    assert!(text.contains("#   !`yp explain --no-color`"), "{text}");
    // Without its comment lines, the output is the JSON it would write.
    let json: String = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let would: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(would["statusLine"]["command"].is_string());
    assert!(!setup.config.join("settings.json").exists());
    assert!(!setup.config.join("commands").exists());
    assert_eq!(setup.state.read_dir().unwrap().count(), 0);
}

#[test]
fn a_status_line_wrapped_by_v011_keeps_working_and_moves_on_reinstall() {
    let shells = shells();
    let Some(shell) = shells.first() else {
        return;
    };
    let setup = Setup::new("install-legacy");
    // What v0.1.1 wrote: the original quoted into our own command line.
    let exe = setup
        .yp
        .to_string_lossy()
        .replace('\\', "/")
        .replace('\'', r"'\''");
    let legacy = format!(r#"'{exe}' statusline --wrap "echo legacy-line""#);
    std::fs::write(
        setup.config.join("settings.json"),
        serde_json::json!({ "statusLine": { "type": "command", "command": legacy } }).to_string(),
    )
    .unwrap();

    let before = setup.run(shell, &setup.settings()["statusLine"], "{}");
    assert!(stdout(&before).contains("legacy-line"), "{before:?}");

    assert!(setup.install(&[]).status.success());
    let settings = setup.settings();
    let command = settings["statusLine"]["command"].as_str().unwrap();
    assert!(command.ends_with("statusline --wrapped"), "{command}");
    let after = setup.run(shell, &settings["statusLine"], "{}");
    assert!(stdout(&after).contains("legacy-line"), "{after:?}");

    assert!(setup.install(&["--uninstall"]).status.success());
    assert_eq!(
        setup.settings()["statusLine"]["command"],
        "echo legacy-line"
    );
}
