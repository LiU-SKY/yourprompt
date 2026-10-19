//! Prose in a language the lexicons do not read.
//!
//! The lexicons cover English and Korean. Anything else was scored as if it
//! said almost nothing: a request that scores 633.9 in English scored 149.1
//! in Spanish, and one that scores 629.3 scored 155.3 in Japanese, an F both
//! times. That F is the scorer failing to read the prompt, not a judgement of
//! it, so such a prompt is reported as unsupported instead of graded.
//!
//! # How the language is told
//!
//! Code is set aside first. Fences, inline spans, paths, URLs and identifiers
//! read the same in every language, and in a prompt about `verify_token` in
//! `src/auth/login.rs` they can outweigh the prose around them. What is left
//! is judged in this order:
//!
//! - **Hangul anywhere** means Korean, and **one English word** from a list
//!   of common ones means English or a mix with it. Both are read, so both
//!   are tasks.
//! - **Script.** Kana is Japanese; Han without kana is Chinese; Cyrillic,
//!   Greek, Arabic, Hebrew, Thai and Devanagari each name a language. The
//!   script has to outweigh the Latin prose around it.
//! - **Function words**, for Latin script. Every language has a few dozen
//!   short words that hold its sentences together and are not English:
//!   "que", "und", "het", "için". Two different ones from the same language,
//!   in a prompt of at least four words, decide it.
//!
//! Every rule leans toward reading the prompt. A wrong "unsupported" hides a
//! real prompt's grade; a wrong "task" only grades a foreign prompt badly,
//! which is what happened to every one of them before.

use crate::token::{code_regions, is_hangul};

/// A prompt written in a language the lexicons do not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Foreign {
    /// The language's English name, when the evidence points at one. Some
    /// prompts are plainly not English or Korean without saying which
    /// language they are -- a script nothing here names, or a tie between
    /// two languages that share their function words.
    pub name: Option<&'static str>,
}

/// Letters of another script needed before a prompt counts as written in it.
const MIN_LETTERS: usize = 2;

/// Below this many Latin words a prompt is too short to tell a language by:
/// "arregla el bug" stays a task.
const MIN_WORDS: usize = 4;

/// Distinct function words of one language needed to name it.
const MIN_FUNCTION_WORDS: usize = 2;

/// English words that settle the question: a prompt containing one is
/// English, or a mix the lexicons can still read.
///
/// Function words and the commonest request verbs, minus every word that is
/// also a word in one of the languages below. "a", "in", "no", "do", "to",
/// "of", "was", "will" and "also" each mean something else somewhere, so
/// they are evidence of nothing. Sorted, for binary search.
const ENGLISH: &[&str] = &[
    "about",
    "add",
    "after",
    "again",
    "all",
    "already",
    "always",
    "and",
    "another",
    "any",
    "anything",
    "are",
    "aren't",
    "around",
    "because",
    "been",
    "before",
    "being",
    "both",
    "but",
    "can",
    "can't",
    "cannot",
    "cant",
    "change",
    "could",
    "couldn't",
    "create",
    "delete",
    "did",
    "didn't",
    "does",
    "doesn't",
    "doing",
    "don't",
    "done",
    "dont",
    "each",
    "every",
    "explain",
    "few",
    "find",
    "fix",
    "for",
    "from",
    "get",
    "give",
    "going",
    "had",
    "have",
    "having",
    "help",
    "here",
    "how",
    "however",
    "i'm",
    "if",
    "into",
    "isn't",
    "it",
    "it's",
    "its",
    "just",
    "keep",
    "know",
    "least",
    "less",
    "let",
    "let's",
    "like",
    "look",
    "make",
    "many",
    "might",
    "more",
    "move",
    "much",
    "must",
    "my",
    "need",
    "never",
    "not",
    "nothing",
    "now",
    "off",
    "only",
    "onto",
    "other",
    "our",
    "out",
    "please",
    "rather",
    "really",
    "remove",
    "rename",
    "replace",
    "run",
    "same",
    "say",
    "see",
    "set",
    "she",
    "should",
    "shouldn't",
    "show",
    "since",
    "some",
    "something",
    "still",
    "such",
    "sure",
    "take",
    "than",
    "that",
    "that's",
    "the",
    "their",
    "them",
    "then",
    "there",
    "there's",
    "these",
    "they",
    "this",
    "those",
    "though",
    "through",
    "too",
    "try",
    "until",
    "update",
    "upon",
    "us",
    "use",
    "very",
    "want",
    "wasn't",
    "we",
    "well",
    "were",
    "what",
    "what's",
    "when",
    "where",
    "whether",
    "which",
    "while",
    "who",
    "why",
    "with",
    "within",
    "without",
    "won't",
    "would",
    "wouldn't",
    "write",
    "yet",
    "you",
    "you're",
    "your",
];

/// The function words of the Latin-script languages most likely to turn up,
/// none of them an English word.
///
/// Words that read as English or as code are left out even where they are
/// the commonest word of their language: Spanish "del" (a Python keyword),
/// Italian "per", German "die", Dutch "van" and "op", Polish "to" and "ten",
/// Turkish "ve" (what "I've" leaves behind). The lists may share words with
/// each other; the language with the most distinct matches wins. Each list is
/// sorted, for binary search.
const FUNCTION_WORDS: &[(&str, &[&str])] = &[
    (
        "Dutch",
        &[
            "aan", "als", "bij", "dat", "de", "deze", "dit", "een", "en", "geen", "graag", "het",
            "kan", "maar", "moet", "moeten", "naar", "niet", "nog", "ook", "te", "voor", "wat",
            "wel", "worden", "wordt", "zal", "zijn", "zodat",
        ],
    ),
    (
        "French",
        &[
            "au", "avec", "c'est", "ce", "cette", "dans", "de", "des", "elle", "en", "est", "et",
            "il", "je", "la", "le", "les", "mais", "ne", "nous", "ou", "où", "pas", "que", "qui",
            "se", "sont", "sur", "un", "une", "vous", "à", "ça", "être",
        ],
    ),
    (
        "German",
        &[
            "auch", "auf", "aus", "bei", "beim", "bitte", "damit", "dann", "das", "dass", "dem",
            "den", "der", "des", "diese", "dieser", "dieses", "durch", "ein", "eine", "einem",
            "einen", "einer", "es", "für", "haben", "ich", "im", "ins", "ist", "kann", "können",
            "mit", "muss", "müssen", "nach", "nicht", "noch", "nur", "oder", "sich", "sie", "sind",
            "soll", "sollen", "sollte", "und", "vom", "von", "warum", "wenn", "werden", "wie",
            "wir", "wird", "zu", "zum", "zur", "über",
        ],
    ),
    (
        "Indonesian",
        &[
            "agar",
            "akan",
            "apa",
            "atau",
            "bagaimana",
            "belum",
            "bisa",
            "dalam",
            "dan",
            "dari",
            "dengan",
            "di",
            "harus",
            "ini",
            "itu",
            "jika",
            "juga",
            "kalau",
            "karena",
            "ke",
            "kenapa",
            "oleh",
            "pada",
            "saya",
            "sudah",
            "tidak",
            "tolong",
            "untuk",
            "yang",
        ],
    ),
    (
        "Italian",
        &[
            "alla", "anche", "che", "con", "cosa", "da", "dal", "dei", "della", "delle", "deve",
            "di", "gli", "il", "la", "le", "lo", "ma", "mi", "nel", "nella", "perché", "più",
            "puoi", "quando", "questa", "questo", "se", "si", "sono", "sul", "tra", "un", "una",
            "è",
        ],
    ),
    (
        "Polish",
        &[
            "być", "czy", "dla", "gdy", "jak", "jest", "jeszcze", "jeśli", "już", "która", "które",
            "który", "lub", "może", "musi", "muszą", "na", "nie", "od", "oraz", "proszę", "przez",
            "się", "tak", "tego", "tylko", "tym", "za", "zamiast", "że", "żeby",
        ],
    ),
    (
        "Portuguese",
        &[
            "ao", "com", "como", "da", "das", "de", "em", "esta", "este", "está", "isso", "isto",
            "mais", "mas", "na", "nas", "nos", "não", "para", "pela", "pelo", "pode", "por", "que",
            "se", "sem", "também", "uma", "você", "é",
        ],
    ),
    (
        "Spanish",
        &[
            "como", "con", "cuando", "de", "debe", "deben", "donde", "el", "en", "es", "esta",
            "este", "esto", "está", "están", "la", "las", "lo", "los", "más", "para", "pero",
            "por", "porque", "puede", "puedes", "que", "qué", "se", "también", "un", "una",
        ],
    ),
    (
        "Turkish",
        &[
            "ama", "ancak", "bana", "bir", "bu", "cok", "daha", "degil", "değil", "eğer", "gibi",
            "hiç", "icin", "ile", "ise", "için", "kadar", "ki", "lutfen", "lütfen", "mi", "mı",
            "nasıl", "ne", "neden", "olan", "olarak", "sonra", "tüm", "veya", "yani", "yerine",
            "yok", "çok", "önce", "şey", "şimdi", "şu",
        ],
    ),
    (
        "Vietnamese",
        &[
            "bạn", "các", "có", "của", "giúp", "hãy", "khi", "không", "là", "làm", "lỗi", "một",
            "như", "những", "này", "nếu", "sẽ", "sửa", "theo", "thì", "trong", "tôi", "từ", "và",
            "về", "với", "đang", "đã", "được", "để",
        ],
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Latin,
    Hangul,
    Kana,
    Han,
    Cyrillic,
    Greek,
    Arabic,
    Hebrew,
    Thai,
    Devanagari,
    Other,
}

const SCRIPTS: usize = Script::Other as usize + 1;

/// The script a letter belongs to. Only called on letters.
fn script_of(c: char) -> Script {
    if is_hangul(c) {
        return Script::Hangul;
    }
    match c as u32 {
        0x41..=0x5A
        | 0x61..=0x7A
        | 0xC0..=0x24F
        | 0x1E00..=0x1EFF
        | 0xFF21..=0xFF3A
        | 0xFF41..=0xFF5A => Script::Latin,
        0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => Script::Kana,
        0x3005..=0x3007
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xF900..=0xFAFF
        | 0x20000..=0x323AF => Script::Han,
        0x400..=0x52F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => Script::Cyrillic,
        0x370..=0x3FF | 0x1F00..=0x1FFF => Script::Greek,
        0x600..=0x6FF | 0x750..=0x77F | 0x8A0..=0x8FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            Script::Arabic
        }
        0x590..=0x5FF | 0xFB1D..=0xFB4F => Script::Hebrew,
        0xE00..=0xE7F => Script::Thai,
        0x900..=0x97F | 0xA8E0..=0xA8FF => Script::Devanagari,
        _ => Script::Other,
    }
}

/// Characters that can belong to a Latin-script word or the code around it.
fn is_latinish(c: char) -> bool {
    c.is_ascii_graphic()
        || c == '\u{2019}'
        || is_mark(c)
        || (c.is_alphabetic() && script_of(c) == Script::Latin)
}

/// A combining accent, as in a decomposed "é".
fn is_mark(c: char) -> bool {
    matches!(c as u32, 0x300..=0x36F)
}

/// Keeps a word whole: "don't", "c'est", "non-blocking".
fn is_joiner(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}' | '-')
}

/// Characters that code uses inside a word and prose does not.
const CODE_CHARS: [char; 25] = [
    '/', '\\', '_', '@', '#', '$', '%', '^', '&', '*', '=', '<', '>', '{', '}', '[', ']', '(', ')',
    '|', '`', '~', ';', ':', '+',
];

/// True for a run of Latin-script characters that is code rather than prose:
/// a path, a URL, an identifier, a call, a dotted name, a version.
fn is_code(core: &str) -> bool {
    if core.contains(CODE_CHARS) || core.contains(|c: char| c.is_ascii_digit()) {
        return true;
    }
    let bytes = core.as_bytes();
    // camelCase, and a dot with a letter or digit on either side.
    bytes
        .windows(2)
        .any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase())
        || bytes
            .windows(3)
            .any(|w| w[0].is_ascii_alphanumeric() && w[1] == b'.' && w[2].is_ascii_alphanumeric())
}

/// A Latin-script word of prose.
struct Word {
    /// Lowercased, with a typographic apostrophe made plain.
    text: String,
    /// Capitalised where no sentence starts, or written in capitals: a name
    /// or an acronym -- "Los Angeles", "La Liga", "MIT" -- rather than the
    /// function word it happens to spell.
    name_like: bool,
}

/// What is left of a prompt once its code is set aside.
#[derive(Default)]
struct Prose {
    letters: [usize; SCRIPTS],
    words: Vec<Word>,
    /// Letters that tell a language apart from the commonest one of its
    /// script: ґ є і ї for Ukrainian, ٹ ڈ ڑ ں ھ ہ ے for Urdu, پ چ ژ گ ک ی
    /// for Persian.
    ukrainian: bool,
    urdu: bool,
    persian: bool,
    /// Set at the first Hangul letter or English word. Either means the
    /// lexicons can read the prompt and nothing after it changes that, so
    /// reading stops there: an English prompt is settled by its first few
    /// words, however long it goes on.
    readable: bool,
}

impl Prose {
    fn count(&mut self, c: char) {
        let script = script_of(c);
        self.letters[script as usize] += 1;
        if script == Script::Hangul {
            self.readable = true;
        }
        match c {
            '\u{490}' | '\u{491}' | '\u{404}' | '\u{454}' | '\u{406}' | '\u{456}' | '\u{407}'
            | '\u{457}' => self.ukrainian = true,
            '\u{679}' | '\u{688}' | '\u{691}' | '\u{6BA}' | '\u{6BE}' | '\u{6C1}' | '\u{6D2}' => {
                self.urdu = true
            }
            '\u{67E}' | '\u{686}' | '\u{698}' | '\u{6AF}' | '\u{6A9}' | '\u{6CC}' => {
                self.persian = true
            }
            _ => {}
        }
    }

    /// Read a run of Latin-script characters, and say whether it ends a
    /// sentence.
    fn read_latin(&mut self, run: &str, starts_sentence: bool) -> bool {
        let ends_sentence = run
            .trim_end_matches(['"', '\'', ')', ']', '\u{2019}'])
            .ends_with(['.', '!', '?', ':']);
        let core = run.trim_matches(|c: char| c.is_ascii_punctuation() || c == '\u{2019}');
        if core.is_empty() || is_code(core) {
            return ends_sentence;
        }
        let mut first = starts_sentence;
        for word in core.split(|c: char| !(c.is_alphabetic() || is_joiner(c) || is_mark(c))) {
            let word = word.trim_matches(is_joiner);
            if word.is_empty() {
                continue;
            }
            let letters = word.chars().filter(|c| c.is_alphabetic()).count();
            self.letters[Script::Latin as usize] += letters;
            let capital = word
                .chars()
                .find(|c| c.is_alphabetic())
                .is_some_and(char::is_uppercase);
            let shouted = letters >= 2
                && word
                    .chars()
                    .filter(|c| c.is_alphabetic())
                    .all(char::is_uppercase);
            let text = word.to_lowercase().replace('\u{2019}', "'");
            if ENGLISH.binary_search(&text.as_str()).is_ok() {
                self.readable = true;
                break;
            }
            self.words.push(Word {
                text,
                name_like: shouted || (capital && !first),
            });
            first = false;
        }
        ends_sentence
    }

    /// Read text with no code in it. `sentence` says whether the next word
    /// starts a sentence, and is left saying so for whatever follows.
    fn read_plain(&mut self, text: &str, sentence: &mut bool) {
        let mut run: Option<usize> = None;
        for (i, c) in text.char_indices() {
            if self.readable {
                return;
            }
            if is_latinish(c) {
                run.get_or_insert(i);
                continue;
            }
            if let Some(start) = run.take() {
                *sentence = self.read_latin(&text[start..i], *sentence);
            }
            if matches!(c, '\n' | '\u{3002}' | '\u{FF01}' | '\u{FF1F}') {
                *sentence = true;
            }
            if c.is_alphabetic() {
                self.count(c);
            }
        }
        if let Some(start) = run {
            *sentence = self.read_latin(&text[start..], *sentence);
        }
    }

    /// The language of the dominant non-Latin script.
    fn script_language(&self) -> Option<&'static str> {
        if self.letters[Script::Kana as usize] > 0 {
            return Some("Japanese");
        }
        let script = [
            Script::Han,
            Script::Cyrillic,
            Script::Greek,
            Script::Arabic,
            Script::Hebrew,
            Script::Thai,
            Script::Devanagari,
            Script::Other,
        ]
        .into_iter()
        .max_by_key(|s| self.letters[*s as usize])?;
        match script {
            Script::Han => Some("Chinese"),
            Script::Cyrillic if self.ukrainian => Some("Ukrainian"),
            Script::Cyrillic => Some("Russian"),
            Script::Greek => Some("Greek"),
            Script::Arabic if self.urdu => Some("Urdu"),
            Script::Arabic if self.persian => Some("Persian"),
            Script::Arabic => Some("Arabic"),
            Script::Hebrew => Some("Hebrew"),
            Script::Thai => Some("Thai"),
            Script::Devanagari => Some("Hindi"),
            _ => None,
        }
    }

    /// The Latin-script language whose function words this prose uses most.
    fn function_word_language(&self) -> Option<Foreign> {
        if self.words.len() < MIN_WORDS {
            return None;
        }
        let mut best: Option<(&'static str, usize)> = None;
        let mut tied = false;
        for (language, list) in FUNCTION_WORDS {
            let mut found: Vec<&str> = self
                .words
                .iter()
                .filter(|w| !w.name_like && list.binary_search(&w.text.as_str()).is_ok())
                .map(|w| w.text.as_str())
                .collect();
            found.sort_unstable();
            found.dedup();
            // Two letters is where function words and ISO 639-1 codes meet:
            // "en es de fr" is a list of locales, not a Spanish sentence.
            let telling = found.iter().any(|w| w.chars().count() > 2 || !w.is_ascii());
            if found.len() < MIN_FUNCTION_WORDS || !telling {
                continue;
            }
            match best {
                Some((_, most)) if most > found.len() => {}
                Some((_, most)) if most == found.len() => tied = true,
                _ => {
                    best = Some((language, found.len()));
                    tied = false;
                }
            }
        }
        best.map(|(language, _)| Foreign {
            name: (!tied).then_some(language),
        })
    }
}

fn read(text: &str) -> Prose {
    let mut prose = Prose::default();
    let mut sentence = true;
    let mut cursor = 0;
    for region in code_regions(text) {
        prose.read_plain(&text[cursor..region.start], &mut sentence);
        if prose.readable {
            return prose;
        }
        // Code sits inside a sentence, so what follows it continues one.
        sentence = false;
        cursor = region.end;
    }
    prose.read_plain(&text[cursor..], &mut sentence);
    prose
}

/// The language of a prompt the lexicons cannot read, or `None` when they
/// can: English, Korean, a mix of either with something else, or too little
/// prose to tell.
pub fn detect(text: &str) -> Option<Foreign> {
    let prose = read(text);
    if prose.readable {
        return None;
    }
    let latin = prose.letters[Script::Latin as usize];
    let other: usize = prose.letters.iter().sum::<usize>() - latin;
    if other >= MIN_LETTERS && other >= latin {
        return Some(Foreign {
            name: prose.script_language(),
        });
    }
    prose.function_word_language()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn language(text: &str) -> Option<Option<&'static str>> {
        detect(text).map(|f| f.name)
    }

    #[test]
    fn every_list_is_sorted_so_the_search_works() {
        let check = |name: &str, list: &[&str]| {
            let mut sorted = list.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                list,
                sorted.as_slice(),
                "{name} is not sorted or repeats a word"
            );
        };
        check("ENGLISH", ENGLISH);
        for (language, list) in FUNCTION_WORDS {
            check(language, list);
        }
    }

    #[test]
    fn no_function_word_is_also_english() {
        // A shared word is evidence of nothing, and an English one on a
        // foreign list is how an English prompt would end up unsupported.
        for (language, list) in FUNCTION_WORDS {
            for word in *list {
                assert!(
                    ENGLISH.binary_search(word).is_err(),
                    "{language} {word:?} is on the English list"
                );
                assert!(
                    !crate::legible::is_common_word(word),
                    "{language} {word:?} is a common English word"
                );
                assert!(
                    word.chars().count() > 1 || !word.is_ascii(),
                    "{language} {word:?} is a bare letter"
                );
            }
        }
    }

    #[test]
    fn the_showcase_prompt_is_named_in_each_latin_language() {
        for (text, name) in [
            (
                "arregla el bug en src/auth/login.rs para que verify_token devuelva \
                 Err(AuthError::Expired). Los tests deben pasar.",
                "Spanish",
            ),
            (
                "corrija o bug em src/auth/login.rs para que verify_token retorne \
                 Err(AuthError::Expired). Os testes devem passar.",
                "Portuguese",
            ),
            (
                "corrige le bug dans src/auth/login.rs pour que verify_token renvoie \
                 Err(AuthError::Expired). Les tests doivent passer.",
                "French",
            ),
            (
                "behebe den Fehler in src/auth/login.rs, damit verify_token \
                 Err(AuthError::Expired) zurückgibt. Die Tests müssen bestehen.",
                "German",
            ),
            (
                "correggi il bug in src/auth/login.rs in modo che verify_token \
                 restituisca Err(AuthError::Expired). I test devono passare.",
                "Italian",
            ),
            (
                "repareer de bug in src/auth/login.rs zodat verify_token \
                 Err(AuthError::Expired) teruggeeft. De tests moeten slagen.",
                "Dutch",
            ),
            (
                "src/auth/login.rs dosyasındaki verify_token için panic yerine \
                 Err(AuthError::Expired) dönsün, testler geçmeli.",
                "Turkish",
            ),
            (
                "perbaiki bug di src/auth/login.rs agar verify_token mengembalikan \
                 Err(AuthError::Expired) dan tidak panic. Tes harus lulus.",
                "Indonesian",
            ),
            (
                "napraw błąd w src/auth/login.rs, żeby verify_token zwracał \
                 Err(AuthError::Expired) zamiast panikować. Testy muszą przechodzić.",
                "Polish",
            ),
            (
                "sửa lỗi trong src/auth/login.rs để verify_token trả về \
                 Err(AuthError::Expired) thay vì panic. Các bài test phải qua.",
                "Vietnamese",
            ),
        ] {
            assert_eq!(language(text), Some(Some(name)), "{text}");
        }
    }

    #[test]
    fn a_script_names_its_language() {
        for (text, name) in [
            (
                "src/auth/login.rs の verify_token を修正して、期限切れトークンで panic \
                 せずに Err(AuthError::Expired) を返すようにしてください。",
                Some("Japanese"),
            ),
            (
                "修复 src/auth/login.rs 中的 verify_token，让它在令牌过期时返回 \
                 Err(AuthError::Expired) 而不是 panic。",
                Some("Chinese"),
            ),
            (
                "исправь verify_token в src/auth/login.rs, чтобы он возвращал \
                 Err(AuthError::Expired) вместо паники.",
                Some("Russian"),
            ),
            (
                "виправ verify_token у src/auth/login.rs, щоб він повертав \
                 Err(AuthError::Expired) замість паніки.",
                Some("Ukrainian"),
            ),
            (
                "διόρθωσε το verify_token στο src/auth/login.rs ώστε να επιστρέφει \
                 Err(AuthError::Expired).",
                Some("Greek"),
            ),
            (
                "أصلح verify_token في src/auth/login.rs لكي يعيد Err(AuthError::Expired) \
                 بدلا من الذعر.",
                Some("Arabic"),
            ),
            (
                "\u{62A}\u{627}\u{628}\u{639} verify_token \u{631}\u{627} \u{62F}\u{631}\u{633}\u{62A} \
                 \u{6A9}\u{646} \u{62A}\u{627} Err(AuthError::Expired) \u{628}\u{631}\u{6AF}\u{631}\u{62F}\u{627}\u{646}\u{62F}.",
                Some("Persian"),
            ),
            (
                "verify_token \u{6A9}\u{648} \u{679}\u{6BE}\u{6CC}\u{6A9} \u{6A9}\u{631}\u{6CC}\u{6BA} \
                 \u{62A}\u{627}\u{6A9}\u{6C1} Err(AuthError::Expired) \u{648}\u{627}\u{67E}\u{633} \u{6A9}\u{631}\u{6D2}.",
                Some("Urdu"),
            ),
            (
                "תקן את verify_token ב-src/auth/login.rs כך שיחזיר Err(AuthError::Expired).",
                Some("Hebrew"),
            ),
            (
                "แก้ verify_token ใน src/auth/login.rs ให้คืนค่า Err(AuthError::Expired) แทนการ panic",
                Some("Thai"),
            ),
            (
                "src/auth/login.rs में verify_token को ठीक करो ताकि वह Err(AuthError::Expired) लौटाए।",
                Some("Hindi"),
            ),
            // A script nothing here names is still plainly neither English
            // nor Korean.
            ("გაასწორე verify_token ფაილში src/auth/login.rs", None),
        ] {
            assert_eq!(language(text), Some(name), "{text}");
        }
    }

    #[test]
    fn english_and_korean_are_read() {
        for text in [
            "fix the login bug",
            "In src/auth/login.rs, change verify_token so it returns Err(AuthError::Expired) \
             instead of panicking on an expired token. Don't change the public signature of \
             login(). tests/auth.rs must still pass.",
            "WHY IS THIS STILL BROKEN?? FIX IT NOW",
            "Refactor everything to be clean, scalable and production-ready.",
            "verify_token 함수가 만료된 토큰을 받으면 panic 이 나는데 이걸 고쳐줘",
            // Korean with a Japanese literal is still Korean.
            "버튼 라벨을 'ログイン' 으로 바꿔줘",
        ] {
            assert_eq!(detect(text), None, "{text}");
        }
    }

    #[test]
    fn a_mix_with_english_is_read() {
        for text in [
            "arregla el bug para que funcione, it's broken since the last deploy",
            "Set the title to 'ユーザー設定を変更しました。もう一度ログインしてください。'",
            "Change the label to «Paramètres de l'utilisateur» and keep the rest",
        ] {
            assert_eq!(detect(text), None, "{text}");
        }
    }

    #[test]
    fn short_or_code_heavy_prompts_are_left_to_be_graded() {
        for text in [
            "arregla el bug",
            "hola que tal",
            "verify_token src/auth/login.rs Err(AuthError::Expired) para el",
            "`translate this: c'est la vie, n'est-ce pas`",
            "```\n// Kommentar: das ist ein Test und nicht mehr\nfn main() {}\n```",
            "の",
        ] {
            assert_eq!(detect(text), None, "{text}");
        }
    }

    #[test]
    fn locales_names_and_acronyms_are_not_function_words() {
        for text in [
            // Two-letter locale codes collide with two-letter function words.
            "i18n: en es de fr pt",
            "locales en, es, de",
            // Capitalised mid-sentence, these are names.
            "El Capitan build fails on Los Angeles server",
            "Jean de la Fontaine quote widget",
            // And in capitals, acronyms.
            "MIT license for ES build, DE mirror",
        ] {
            assert_eq!(detect(text), None, "{text}");
        }
    }

    #[test]
    fn a_tie_between_languages_is_unsupported_but_unnamed() {
        // Spanish and Portuguese share "para", "que" and "como"; with nothing
        // else to go on, the prompt is plainly not English, and not plainly
        // either one.
        assert_eq!(language("para que como bug"), Some(None));
    }
}
