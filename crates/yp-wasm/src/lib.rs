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

/// The crate version, for the page to display.
///
/// # Safety
///
/// The returned buffer must be released with [`yp_free`].
#[no_mangle]
pub unsafe extern "C" fn yp_version() -> *mut u8 {
    into_buffer(env!("CARGO_PKG_VERSION"))
}
