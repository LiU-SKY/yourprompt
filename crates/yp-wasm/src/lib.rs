//! `yourprompt` compiled for the browser.
//!
//! A raw `wasm32-unknown-unknown` module with a hand-written C ABI, and no
//! `wasm-bindgen`. The scorer is a pure function from text to JSON, so the
//! whole interface is four exports and a length-prefixed buffer -- adding a
//! binding generator and its toolchain to a project whose case rests on
//! having no dependencies would be a poor trade.
//!
//! # Calling convention
//!
//! Every string crossing the boundary is UTF-8, passed as a pointer and a
//! length. Returned buffers are owned by the caller, which must hand them
//! back to [`yp_free`] -- the module cannot know when JavaScript is finished
//! with them.
//!
//! ```js
//! const ptr = yp_alloc(bytes.length);
//! new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
//! const out = yp_score(ptr, bytes.length, 0, 0);   // returns ptr to [u32 len][bytes]
//! ```
//!
//! # Grounding
//!
//! An index built by `yp index` can be handed over as bytes, so the page can
//! ground scores in a real repository without a server. Pass a zero pointer to
//! score without one.

use std::alloc::{alloc, dealloc, Layout};

use yp_core::{prompt, Corpus, TermFacts};

/// Reserve `len` bytes for the caller to write into.
///
/// # Safety
///
/// The returned pointer is valid for `len` bytes and must eventually be given
/// to [`yp_free`] with the same length.
#[no_mangle]
pub unsafe extern "C" fn yp_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    match Layout::from_size_align(len, 1) {
        Ok(layout) => alloc(layout),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Release a buffer obtained from [`yp_alloc`] or returned by [`yp_score`].
///
/// # Safety
///
/// `ptr` must have come from this module and `len` must be the length it was
/// allocated with. For a buffer returned by [`yp_score`], that length is the
/// four-byte header plus the payload it declares.
#[no_mangle]
pub unsafe extern "C" fn yp_free(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    if let Ok(layout) = Layout::from_size_align(len, 1) {
        dealloc(ptr, layout);
    }
}

/// Copy `text` into a freshly allocated, length-prefixed buffer.
///
/// The first four bytes are the payload length, little-endian, so JavaScript
/// can read the result without a second call.
fn into_buffer(text: &str) -> *mut u8 {
    let bytes = text.as_bytes();
    let total = 4 + bytes.len();
    let Ok(layout) = Layout::from_size_align(total, 1) else {
        return std::ptr::null_mut();
    };
    unsafe {
        let out = alloc(layout);
        if out.is_null() {
            return out;
        }
        std::ptr::copy_nonoverlapping((bytes.len() as u32).to_le_bytes().as_ptr(), out, 4);
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), out.add(4), bytes.len());
        out
    }
}

/// # Safety
///
/// `ptr` must point to `len` initialised bytes, or be null with `len` zero.
unsafe fn as_str<'a>(ptr: *const u8, len: usize) -> Option<&'a str> {
    if ptr.is_null() || len == 0 {
        return None;
    }
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).ok()
}

/// A repository index, parsed once and queried per score.
struct WasmCorpus(yp_index::RepoIndex);

impl Corpus for WasmCorpus {
    fn lookup(&self, term: &str) -> Option<TermFacts> {
        self.0.lookup(term).map(|s| TermFacts {
            df: s.df,
            cf: s.cf,
            def: s.def,
        })
    }

    fn documents(&self) -> usize {
        self.0.files()
    }

    fn total_terms(&self) -> u64 {
        self.0.total_terms()
    }

    // Files picked or dropped on the page arrive under bare names.
    fn flat(&self) -> bool {
        self.0.is_flat()
    }

    // A folder added beside them keeps its paths.
    fn bare_file(&self, name: &str) -> bool {
        self.0.is_bare(name)
    }
}

/// Score a prompt, optionally against an index.
///
/// Returns a length-prefixed buffer holding the JSON of a `Score`, or null if
/// the input was not valid UTF-8 or the language resources failed to load.
///
/// # Safety
///
/// Both pointer/length pairs must describe initialised bytes, or be null with
/// a zero length. The returned buffer must be released with [`yp_free`].
#[no_mangle]
pub unsafe extern "C" fn yp_score(
    text_ptr: *const u8,
    text_len: usize,
    index_ptr: *const u8,
    index_len: usize,
) -> *mut u8 {
    let Some(text) = as_str(text_ptr, text_len) else {
        return std::ptr::null_mut();
    };

    let corpus = as_str(index_ptr, index_len)
        .and_then(yp_index::RepoIndex::parse_str)
        .filter(|index| !index.is_empty())
        .map(WasmCorpus);

    let parts = prompt::split(text);
    let Some(score) = yp_core::score_parts(&parts, corpus.as_ref().map(|c| c as &dyn Corpus))
    else {
        return std::ptr::null_mut();
    };

    match serde_json::to_string(&score) {
        Ok(json) => into_buffer(&json),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Score an instruction with its attachments already separated, as the page
/// does when someone opens a file rather than pasting into the box.
///
/// What was pasted into the box is cut out of the instruction the way the
/// hook cuts it, so the page and the status line give one prompt one score.
///
/// Attachments are joined by a NUL byte, which cannot appear in the text the
/// page accepts.
///
/// # Safety
///
/// As [`yp_score`].
#[no_mangle]
pub unsafe extern "C" fn yp_score_parts(
    text_ptr: *const u8,
    text_len: usize,
    attach_ptr: *const u8,
    attach_len: usize,
    index_ptr: *const u8,
    index_len: usize,
) -> *mut u8 {
    let Some(instruction) = as_str(text_ptr, text_len) else {
        return std::ptr::null_mut();
    };
    let joined = as_str(attach_ptr, attach_len).unwrap_or("");
    let attachments: Vec<&str> = if joined.is_empty() {
        Vec::new()
    } else {
        joined.split('\u{0}').filter(|a| !a.is_empty()).collect()
    };

    let corpus = as_str(index_ptr, index_len)
        .and_then(yp_index::RepoIndex::parse_str)
        .filter(|index| !index.is_empty())
        .map(WasmCorpus);

    let parts = prompt::from_parts(instruction, &attachments);
    let Some(score) = yp_core::score_parts(&parts, corpus.as_ref().map(|c| c as &dyn Corpus))
    else {
        return std::ptr::null_mut();
    };

    match serde_json::to_string(&score) {
        Ok(json) => into_buffer(&json),
        Err(_) => std::ptr::null_mut(),
    }
}

/// Build an index from files the visitor supplied.
///
/// Input is one record per file: the path, a unit-separator byte, then the
/// contents; records separated by a record-separator byte. Neither appears in
/// source text. Returns a length-prefixed index blob for [`yp_score_parts`],
/// or null if the input was not valid UTF-8 or held no files.
///
/// This is what lets the published page ground scores in *your* codebase
/// rather than in whichever repository happened to be shipped with it: the
/// files you drop on the page are the repository.
///
/// # Safety
///
/// As [`yp_score`]. The returned buffer must be released with [`yp_free`].
#[no_mangle]
pub unsafe extern "C" fn yp_index_build(ptr: *const u8, len: usize) -> *mut u8 {
    let Some(payload) = as_str(ptr, len) else {
        return std::ptr::null_mut();
    };

    let files: Vec<(String, String)> = payload
        .split('\u{1e}')
        .filter(|record| !record.is_empty())
        .map(|record| match record.split_once('\u{1f}') {
            Some((name, body)) => (name.to_string(), body.to_string()),
            None => (String::new(), record.to_string()),
        })
        .collect();

    if files.is_empty() {
        return std::ptr::null_mut();
    }

    into_buffer(yp_index::RepoIndex::from_files(&files).as_str())
}

/// The crate version, for the page to display.
///
/// # Safety
///
/// The returned buffer must be released with [`yp_free`].
#[no_mangle]
pub unsafe extern "C" fn yp_version() -> *mut u8 {
    into_buffer(env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGIN: &str = "pub enum AuthError {\n    Expired,\n    Invalid,\n}\n\n\
        pub fn verify_token(t: &Token) -> Result<Claims, AuthError> {\n    \
        let claims = t.decode().map_err(|_| AuthError::Invalid)?;\n    \
        if claims.exp < now() {\n        panic!(\"token expired\");\n    }\n    \
        Ok(claims)\n}\n\npub fn login(user: &str) -> Result<Token, AuthError> {\n    \
        let token = Token::issue(user);\n    verify_token(&token)?;\n    Ok(token)\n}\n";
    const TESTS: &str = "#[test]\nfn expired_token_is_rejected() {\n    \
        assert!(verify_token(&expired()).is_err());\n}\n";
    const PROMPT: &str = "In src/auth/login.rs, change verify_token so it returns \
        Err(AuthError::Expired) instead of panicking on an expired token. Don't change \
        the public signature of login(). tests/auth.rs must still pass.";

    /// Copy `bytes` into the module, as the page does.
    fn buffer(bytes: &[u8]) -> (*mut u8, usize) {
        unsafe {
            let ptr = yp_alloc(bytes.len());
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
            (ptr, bytes.len())
        }
    }

    /// Read a length-prefixed result and release it.
    fn take(ptr: *mut u8) -> String {
        assert!(!ptr.is_null());
        unsafe {
            let mut len = [0u8; 4];
            std::ptr::copy_nonoverlapping(ptr, len.as_mut_ptr(), 4);
            let len = u32::from_le_bytes(len) as usize;
            let text = std::slice::from_raw_parts(ptr.add(4), len).to_vec();
            yp_free(ptr, 4 + len);
            String::from_utf8(text).unwrap()
        }
    }

    /// The index the page builds from the files a visitor gave it.
    fn index_of(files: &[(&str, &str)]) -> String {
        let payload = files
            .iter()
            .map(|(name, text)| format!("{name}\u{1f}{text}"))
            .collect::<Vec<_>>()
            .join("\u{1e}");
        let (ptr, len) = buffer(payload.as_bytes());
        let blob = take(unsafe { yp_index_build(ptr, len) });
        unsafe { yp_free(ptr, len) };
        blob
    }

    /// What the page shows for what was typed into its box.
    fn page(text: &str, index: &str) -> String {
        let (tp, tl) = buffer(text.as_bytes());
        let (ip, il) = buffer(index.as_bytes());
        let json = take(unsafe { yp_score_parts(tp, tl, std::ptr::null(), 0, ip, il) });
        unsafe {
            yp_free(tp, tl);
            yp_free(ip, il);
        }
        json
    }

    /// What the hook records for the same prompt against the same index.
    fn status_line(text: &str, index: &str) -> String {
        let corpus = WasmCorpus(yp_index::RepoIndex::parse_str(index).unwrap());
        serde_json::to_string(&yp_core::score_with(text, Some(&corpus)).unwrap()).unwrap()
    }

    fn mark_on<'a>(score: &'a serde_json::Value, text: &str, word: &str) -> &'a str {
        score["marks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| {
                let (start, end) = (m["start"].as_u64().unwrap(), m["end"].as_u64().unwrap());
                &text[start as usize..end as usize] == word
            })
            .and_then(|m| m["kind"].as_str())
            .unwrap_or_else(|| panic!("no mark on {word}: {score}"))
    }

    #[test]
    fn the_page_and_the_status_line_agree() {
        let index = index_of(&[("src/auth/login.rs", LOGIN), ("tests/auth.rs", TESTS)]);
        for text in [
            PROMPT.to_string(),
            format!("{PROMPT}\n\n```rust\n{LOGIN}```"),
            format!(
                "fix this\n```\n{}```",
                "thread 'main' panicked at src/auth/login.rs:9:41\n".repeat(12)
            ),
            "fix the bug".to_string(),
        ] {
            assert_eq!(
                page(&text, &index),
                status_line(&text, &index),
                "{text:.60}"
            );
        }
    }

    #[test]
    fn a_called_or_qualified_name_is_checked_like_any_other() {
        // The made-up showcase marked its paths and MoonError but let
        // gravity() and orbit() through unmarked, and the real one never
        // counted login().
        let index = index_of(&[("src/auth/login.rs", LOGIN), ("tests/auth.rs", TESTS)]);
        let made_up = "In src/banana.rs, change gravity() so it returns Err(MoonError::Full) \
            instead of panicking on a full moon. Don't change the public signature of orbit(). \
            tests/moon.rs must still pass.";
        let score: serde_json::Value = serde_json::from_str(&page(made_up, &index)).unwrap();
        for word in ["gravity", "orbit", "MoonError", "Full"] {
            assert_eq!(mark_on(&score, made_up, word), "name:missing", "{word}");
        }

        let score: serde_json::Value = serde_json::from_str(&page(PROMPT, &index)).unwrap();
        assert_eq!(mark_on(&score, PROMPT, "login"), "name:unique");
        // The Expired in the only AuthError, not any word "expired".
        assert_eq!(mark_on(&score, PROMPT, "Expired"), "name:unique");
    }

    #[test]
    fn a_path_names_a_file_the_page_was_given_without_one() {
        // A browser hands over a picked file as `login.rs`.
        let index = index_of(&[("login.rs", LOGIN), ("auth.rs", TESTS)]);
        let score: serde_json::Value = serde_json::from_str(&page(PROMPT, &index)).unwrap();
        assert_eq!(mark_on(&score, PROMPT, "src/auth/login.rs"), "name:unique");
        assert_eq!(mark_on(&score, PROMPT, "tests/auth.rs"), "name:unique");
        assert_eq!(mark_on(&score, PROMPT, "login"), "name:unique");

        // A picked file beside a dropped folder: only the file that came
        // without its path is found by its name.
        let index = index_of(&[("login.rs", LOGIN), ("tests/auth.rs", TESTS)]);
        let score: serde_json::Value = serde_json::from_str(&page(PROMPT, &index)).unwrap();
        assert_eq!(mark_on(&score, PROMPT, "src/auth/login.rs"), "name:unique");
        assert_eq!(mark_on(&score, PROMPT, "tests/auth.rs"), "name:unique");
        let text = PROMPT.replace("tests/auth.rs", "src/auth.rs");
        let score: serde_json::Value = serde_json::from_str(&page(&text, &index)).unwrap();
        assert_eq!(mark_on(&score, &text, "src/auth.rs"), "name:missing");

        // Given with their paths, a path the repository lacks is missing.
        let index = index_of(&[("src/auth/login.rs", LOGIN), ("tests/auth.rs", TESTS)]);
        let text = PROMPT.replace("src/auth/login.rs", "src/auth/banana.rs");
        let score: serde_json::Value = serde_json::from_str(&page(&text, &index)).unwrap();
        assert_eq!(mark_on(&score, &text, "src/auth/banana.rs"), "name:missing");
    }
}
