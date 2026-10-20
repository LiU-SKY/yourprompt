//! Prompts that name things by the thousand, scored against a repository.
//!
//! Every name a prompt uses becomes a referent and every referent a mark, so
//! these are the inputs on which per-name work that is not constant shows: a
//! qualified name tens of thousands of parts long, and tens of thousands of
//! names in a row. Each is timed against a text of the same size that names
//! nothing, so the pair measures how the work grows, not how fast the machine
//! is.

use std::time::{Duration, Instant};

use yp_core::{Corpus, MapCorpus};

fn corpus() -> MapCorpus {
    MapCorpus::new(
        10,
        &[
            ("verify_token", 1, 3, 1),
            ("autherror", 2, 4, 1),
            ("expired", 2, 3, 0),
        ],
    )
}

/// The fastest of three scorings of `text` against `corpus`.
fn time(text: &str, corpus: &dyn Corpus) -> Duration {
    (0..3)
        .map(|_| {
            let start = Instant::now();
            std::hint::black_box(yp_core::score_with(text, Some(corpus)));
            start.elapsed()
        })
        .min()
        .unwrap()
}

/// `fix a::a::…::b`: 21,845 parts in 65,537 bytes, the whole of what is read.
fn chain() -> String {
    format!("fix {}b", "a::".repeat(21_844))
}

#[test]
fn a_qualified_name_of_any_length_costs_what_plain_text_does() {
    // Each part was recorded under the whole name up to it, and each part's
    // mark quoted that name. This chain took the hook 6.7 s against the
    // demo's sample, past its 5 s timeout, and the page almost four minutes
    // before it trapped; here, 5.0 s in a debug build, where the same text
    // with a space after every `::`, naming nothing, took 34 ms.
    let corpus = corpus();
    let named = time(&chain(), &corpus);
    let plain = time(&format!("fix {}b", "a:: ".repeat(16_383)), &corpus);
    assert!(
        named < plain * 20 + Duration::from_millis(100),
        "the chain took {named:?}, the same text naming nothing {plain:?}"
    );
}

#[test]
fn a_qualified_name_of_any_length_leaves_small_marks() {
    // 718 MB of them, when each part's note quoted the name up to it.
    let s = yp_core::score_with(&chain(), Some(&corpus())).unwrap();
    let json = serde_json::to_string(&s.marks).unwrap();
    assert!(json.len() < 1_000_000, "{} bytes of marks", json.len());
}

#[test]
fn names_on_tokens_of_their_own_cost_what_plain_text_does() {
    // A name on a token of its own has a mark of its own, and the token under
    // each name and the mark for it were found by scanning: 64 KB of `,\`,
    // 32,768 paths the repository lacks, took 3.5 s in a debug build that
    // way, where `,;` took 55 ms. The second text of each pair is as many
    // tokens naming nothing.
    let corpus = corpus();
    for (named, plain) in [
        (",\\".repeat(32_768), ",;".repeat(32_768)),
        ("a_b ".repeat(16_384), "a b ".repeat(16_384)),
    ] {
        let (named_time, plain_time) = (time(&named, &corpus), time(&plain, &corpus));
        assert!(
            named_time < plain_time * 20 + Duration::from_millis(100),
            "{:?}...: {named_time:?} against {plain_time:?}",
            &named[..8]
        );
    }
    let s = yp_core::score_with(&"a_b ".repeat(16_384), Some(&corpus)).unwrap();
    assert_eq!(
        s.marks.iter().filter(|m| m.kind == "name:missing").count(),
        16_384
    );
}

#[test]
fn a_qualified_name_is_read_to_its_thirty_second_part() {
    let corpus = corpus();
    let parts: Vec<String> = (1..=40).map(|i| format!("part{i}")).collect();
    let found = yp_core::referents_of(&format!("fix {} now", parts.join("::")), &corpus);
    let texts: Vec<&str> = found.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(texts.len(), 32, "{texts:?}");
    assert_eq!(texts[0], "part1");
    assert_eq!(texts[31], parts[..32].join("::"));

    // The longest name in the crates this workspace depends on is read
    // whole, less the syntax it starts with.
    let real = "super::super::super::Win32::Security::Authentication::Identity::\
                SEC_APPLICATION_PROTOCOL_LIST";
    let found = yp_core::referents_of(&format!("use {real} here"), &corpus);
    assert_eq!(found.len(), 5);
    assert_eq!(found[4].text, real);
}

#[test]
fn a_note_quotes_a_long_name_only_in_part() {
    let corpus = corpus();
    let long = (1..=30)
        .map(|i| format!("segment{i}"))
        .collect::<Vec<_>>()
        .join("::");
    assert!(long.len() > 200);
    let text = format!("fix {long}");
    let s = yp_core::score_with(&text, Some(&corpus)).unwrap();
    let last = s
        .marks
        .iter()
        .find(|m| &text[m.start..m.end] == "segment30")
        .expect("the last part is marked");
    assert!(!last.note.contains(&long[..81]), "{}", last.note);
    assert!(
        last.note.contains(&format!("{}…", &long[..80])),
        "{}",
        last.note
    );
    for m in &s.marks {
        assert!(m.note.chars().count() < 160, "{}", m.note);
    }

    // A name of an ordinary length is quoted whole.
    let text = "return Err(AuthError::Expired) here";
    let s = yp_core::score_with(text, Some(&corpus)).unwrap();
    let expired = s
        .marks
        .iter()
        .find(|m| &text[m.start..m.end] == "Expired")
        .expect("the member is marked");
    assert!(
        expired.note.contains("AuthError::Expired"),
        "{}",
        expired.note
    );
    assert!(!expired.note.contains('…'), "{}", expired.note);
}

#[test]
fn a_snippet_is_marked_by_the_worst_name_in_it() {
    // The names in a code span all sit where the span starts, and the span
    // shows the one the agent would trip on.
    let corpus = corpus();
    let text = "fix `verify_token compute_paycheck parse_args` today";
    let s = yp_core::score_with(text, Some(&corpus)).unwrap();
    let span = s
        .marks
        .iter()
        .find(|m| text[m.start..m.end].starts_with('`'))
        .expect("the span is marked");
    assert_eq!(span.kind, "name:missing");
    assert!(span.note.contains("compute_paycheck"), "{}", span.note);

    let text = "fix `verify_token` today";
    let s = yp_core::score_with(text, Some(&corpus)).unwrap();
    let span = s
        .marks
        .iter()
        .find(|m| text[m.start..m.end].starts_with('`'))
        .expect("the span is marked");
    assert_eq!(span.kind, "name:unique");
}

#[test]
fn tokens_come_in_text_order() {
    // A mark finds the token under a name by binary search, which holds only
    // while each token starts after the one before it.
    let pieces = [
        "fix ",
        "a::",
        "b",
        "`x`",
        "```\ny\n```",
        "`",
        "그거 ",
        "고쳐줘",
        "src/a.rs",
        "C:\\x\\y.rs",
        ",\\",
        "/ ",
        "e.g.",
        "end.",
        "-",
        "--",
        "...",
        "Foo::bar()",
        "😀",
        "\n",
        "\t",
        "é",
        "verify_token",
        "x.y.z",
    ];
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    for _ in 0..2_000 {
        let text: String = (0..next(40)).map(|_| pieces[next(pieces.len())]).collect();
        let tokens = yp_lang::tokenize(&text);
        for w in tokens.windows(2) {
            assert!(
                w[0].span.start < w[1].span.start && w[0].span.end <= w[1].span.start,
                "{text:?}: {:?} then {:?}",
                w[0],
                w[1]
            );
        }
    }
}
