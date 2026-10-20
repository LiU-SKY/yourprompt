//! The two web pages, run rather than only parsed.
//!
//! The hosted demo (`docs/index.html`) and the page `yp serve` embeds
//! (`assets/index.html`) are each one inline script. CI checks that the
//! script parses, which catches a page where nothing works but not one that
//! throws a dropped sentence away or says the wrong thing in a report.
//! `pages.mjs` runs each script in Node against a stand-in for the browser
//! and plays one visitor's action per check; each test here is one check,
//! named for what it guards.
//!
//! Every CI runner has Node. Elsewhere a missing Node skips the checks with a
//! note; on CI it fails them, so they cannot pass by not running.

use std::path::Path;
use std::process::Command;

fn page_check(name: &str) {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("node")
        .arg(here.join("tests").join("pages.mjs"))
        .arg(here.join("../../docs/index.html"))
        .arg(here.join("assets/index.html"))
        .arg(name)
        .output();
    let output = match output {
        Ok(output) => output,
        Err(e) if std::env::var_os("CI").is_none() => {
            eprintln!("skipping the {name} page check: node did not run ({e})");
            return;
        }
        Err(e) => panic!("the page checks need node on CI: {e}"),
    };
    assert!(
        output.status.success(),
        "page check {name} failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn text_dragged_onto_the_prompt_box_is_left_to_it() {
    page_check("text-drop");
}

#[test]
fn a_lone_folder_is_indexed_by_the_paths_inside_it() {
    page_check("lone-folder");
}

#[test]
fn only_a_file_that_came_on_its_own_is_indexed_bare() {
    page_check("bare-files");
}

#[test]
fn removing_files_says_what_went() {
    page_check("removed-note");
}

#[test]
fn a_copied_link_says_what_it_leaves_out() {
    page_check("link-note");
}

#[test]
fn the_marks_follow_the_box_when_it_changes_size() {
    page_check("resize");
}

#[test]
fn a_prompt_the_page_puts_in_the_box_fits_it() {
    page_check("box-fit");
}

#[test]
fn a_dangling_pronoun_points_at_nothing_in_the_code() {
    page_check("pronoun-label");
}

#[test]
fn a_report_on_an_ungraded_prompt_has_no_grounded_row() {
    page_check("ungraded-report");
}

#[test]
fn the_footer_command_does_not_break_inside_its_flag() {
    page_check("footer-code");
}

#[test]
fn the_served_report_says_the_server_scored_it() {
    page_check("served-report");
}
