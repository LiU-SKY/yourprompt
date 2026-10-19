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
//! `src/auth/login.rs` they can outweigh the prose around them.
//!
//! Quotations are set aside next, because the language of a quotation is not
//! the language of the prompt: "ru.json has 'Сохранить' twice" is an English
//! request, and was reported as Russian. Only when there is no prose outside
//! the quotes is what they hold read instead.
//!
//! What is left is judged in this order:
//!
//! - **Hangul anywhere** means Korean, and **one English word** from a list
//!   of common ones means English or a mix with it. Both are read, so both
//!   are tasks.
//! - **Script.** Kana is Japanese; Han without kana is Chinese; Cyrillic,
//!   Greek, Arabic, Hebrew, Thai and Devanagari each name a language. The
//!   script has to outweigh the Latin prose around it.
//! - **Function words**, for Latin script. Every language has a few dozen
//!   short words that hold its sentences together and are not English --
//!   "que", "und", "het", "için" -- and a handful of verbs every request
//!   opens with: "arregla", "behebe", "napraw". Two different ones from the
//!   same language, in a prompt of at least four words, decide it. From
//!   three words on, so does stronger evidence: two that no list of locale
//!   codes could hold, "behebe den Login-Fehler", or one of those and a word
//!   with a letter English does not use, "napraw błąd w logowaniu".
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

/// Latin words in a prompt long enough to be named by two function words,
/// one of them telling.
const MIN_WORDS: usize = 4;

/// Below this many Latin words a prompt is too short to tell a language by:
/// "sí, continúa" stays a task. From here to [`MIN_WORDS`] a language takes
/// stronger evidence than a sentence needs, so "behebe den Login-Fehler" is
/// named and "arregla el bug" stays a task.
const MIN_SHORT_WORDS: usize = 3;

/// Distinct function words of one language needed to name it.
const MIN_FUNCTION_WORDS: usize = 2;

/// Lowercase words that sit inside names: "Ursula von der Leyen", "Vincent
/// van Gogh", "José de la Cruz". Each also spells a function word, and "Seed
/// users: Ursula von der Leyen, Mark van der Berg" was reported as German.
/// A run of them between two capitalised words is part of a name, unless
/// the prose holds other words of the same language: German capitalises its
/// nouns, so "Die Größe von Dateien ist falsch" has "von" between two
/// capitals as well, and read as a name it left the prompt graded. Sorted,
/// for binary search.
const NAME_PARTICLES: &[&str] = &[
    "da", "de", "den", "der", "di", "dos", "la", "van", "von", "zu",
];

/// English words that settle the question: a prompt containing one is
/// English, or a mix the lexicons can still read.
///
/// Function words and the commonest request verbs, minus every word that is
/// also a word in one of the languages below. "a", "in", "no", "do", "to",
/// "of", "was", "will" and "also" each mean something else somewhere, so
/// they are evidence of nothing. "has" is Spanish too, "¿por qué has
/// borrado el test?"; "for" is Danish and Norwegian, "i stedet for"; "are"
/// is Romanian for "has", and "add hozzá" is Hungarian for "add it". Each
/// of them made a prompt in those languages read as English.
///
/// "english", "translate" and "summarize" are here because a request about
/// text in another language says in English what to do with it. Sorted,
/// for binary search.
const ENGLISH: &[&str] = &[
    "about",
    "after",
    "again",
    "all",
    "already",
    "always",
    "and",
    "another",
    "any",
    "anything",
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
    "english",
    "every",
    "explain",
    "few",
    "find",
    "fix",
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
    "instead",
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
    "summarise",
    "summarize",
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
    "tldr",
    "too",
    "translate",
    "translation",
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
/// none of them an English word, and the imperatives a request in each one
/// opens with.
///
/// Words that read as English are left out even where they are the
/// commonest word of their language: Italian "per", German "die", Dutch
/// "van" and "op", Polish "to" and "ten", Turkish "ve" (what "I've" leaves
/// behind), Danish "at" and "for", Hungarian "add", "is" and "van".
/// Spanish "del" is a Python keyword, but it takes a second Spanish word to
/// count, and leaving it out let "arregla el error del login" be graded.
/// An elided article or pronoun is listed with its apostrophe, "l'" and
/// "dell'", and is read off the front of the word it is joined to.
///
/// The verbs are what a short request has instead of function words:
/// "corrija o erro de login" holds one function word and its verb. The
/// lists may share words with each other; the language with the most
/// distinct matches wins. Each list is sorted, for binary search.
const FUNCTION_WORDS: &[(&str, &[&str])] = &[
    (
        "Czech",
        &[
            "aby", "bude", "být", "jak", "jen", "ještě", "když", "která", "které", "který", "musí",
            "místo", "napiš", "nebo", "není", "odstraň", "oprav", "pokud", "pouze", "proč", "při",
            "přidej", "tak", "také", "tento", "toto", "uprav", "už", "změň", "že",
        ],
    ),
    (
        "Danish",
        &[
            "alle", "den", "denne", "det", "dette", "efter", "eller", "fejl", "fejlen", "fjern",
            "fra", "har", "hvad", "hvordan", "hvorfor", "ikke", "jeg", "kan", "med", "når", "og",
            "også", "opdater", "på", "rette", "skal", "skriv", "som", "så", "tilføj", "være",
            "ændr",
        ],
    ),
    (
        "Dutch",
        &[
            "aan",
            "als",
            "bij",
            "dat",
            "de",
            "deze",
            "dit",
            "een",
            "en",
            "geen",
            "graag",
            "herstel",
            "het",
            "kan",
            "maak",
            "maar",
            "moet",
            "moeten",
            "naar",
            "niet",
            "nog",
            "ook",
            "repareer",
            "schrijf",
            "te",
            "verwijder",
            "voeg",
            "voor",
            "wat",
            "wel",
            "wijzig",
            "worden",
            "wordt",
            "zal",
            "zijn",
            "zodat",
        ],
    ),
    (
        "Finnish",
        &[
            "ei",
            "ennen",
            "että",
            "ja",
            "jos",
            "jotta",
            "kaikki",
            "kanssa",
            "kirjoita",
            "korjaa",
            "kun",
            "lisää",
            "miksi",
            "miten",
            "mutta",
            "muuta",
            "myös",
            "niin",
            "ole",
            "olla",
            "poista",
            "päivitä",
            "sen",
            "siitä",
            "sitä",
            "tai",
            "tämä",
            "tässä",
            "vielä",
        ],
    ),
    (
        "French",
        &[
            "ajoute", "au", "avec", "c'est", "ce", "cette", "corrige", "crée", "d'", "dans", "de",
            "des", "du", "elle", "en", "enlève", "est", "et", "fais", "il", "j'", "je", "l'", "la",
            "le", "les", "m'", "mais", "modifie", "n'", "ne", "nous", "ou", "où", "pas", "qu'",
            "que", "qui", "remplace", "répare", "s'", "se", "sont", "supprime", "sur", "un", "une",
            "vous", "vérifie", "à", "ça", "écris", "être",
        ],
    ),
    (
        "German",
        &[
            "aktualisiere",
            "auch",
            "auf",
            "aus",
            "behebe",
            "bei",
            "beim",
            "bitte",
            "damit",
            "dann",
            "das",
            "dass",
            "dem",
            "den",
            "der",
            "des",
            "diese",
            "dieser",
            "dieses",
            "durch",
            "ein",
            "eine",
            "einem",
            "einen",
            "einer",
            "entferne",
            "ersetze",
            "erstelle",
            "es",
            "füge",
            "für",
            "haben",
            "ich",
            "im",
            "ins",
            "ist",
            "kann",
            "korrigiere",
            "können",
            "lösche",
            "mach",
            "mache",
            "mit",
            "muss",
            "müssen",
            "nach",
            "nicht",
            "noch",
            "nur",
            "oder",
            "prüfe",
            "repariere",
            "schreibe",
            "sich",
            "sie",
            "sind",
            "soll",
            "sollen",
            "sollte",
            "und",
            "vom",
            "von",
            "warum",
            "wenn",
            "werden",
            "wie",
            "wir",
            "wird",
            "zu",
            "zum",
            "zur",
            "ändere",
            "über",
        ],
    ),
    (
        "Hungarian",
        &[
            "amely",
            "ami",
            "amit",
            "azt",
            "csak",
            "egy",
            "előtt",
            "esetén",
            "ezt",
            "frissítsd",
            "helyett",
            "hogy",
            "javítsd",
            "kell",
            "lesz",
            "mert",
            "már",
            "még",
            "módosítsd",
            "nem",
            "nincs",
            "nélkül",
            "töröld",
            "után",
            "vagy",
            "változtasd",
            "és",
            "írd",
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
            "buat",
            "buatkan",
            "dalam",
            "dan",
            "dari",
            "dengan",
            "di",
            "hapus",
            "harus",
            "ini",
            "itu",
            "jika",
            "juga",
            "kalau",
            "kamu",
            "karena",
            "ke",
            "kenapa",
            "oleh",
            "pada",
            "perbaiki",
            "perbarui",
            "saya",
            "sudah",
            "tambahkan",
            "tidak",
            "tolong",
            "tulis",
            "ubah",
            "untuk",
            "yang",
        ],
    ),
    (
        "Italian",
        &[
            "aggiorna", "aggiungi", "al", "all'", "alla", "anche", "che", "con", "correggi",
            "cosa", "d'", "da", "dal", "dall'", "degli", "dei", "del", "dell'", "della", "delle",
            "dello", "deve", "di", "fai", "gli", "il", "l'", "la", "le", "lo", "ma", "metti", "mi",
            "modifica", "nei", "nel", "nell'", "nella", "perché", "più", "puoi", "quando",
            "questa", "questo", "rimuovi", "scrivi", "se", "si", "sistema", "sono", "sul", "sull'",
            "togli", "tra", "un", "un'", "una", "è",
        ],
    ),
    (
        "Norwegian",
        &[
            "alle", "den", "denne", "det", "dette", "eller", "endre", "etter", "feil", "feilen",
            "fiks", "fjern", "fra", "har", "hva", "hvordan", "hvorfor", "ikke", "jeg", "kan",
            "legg", "med", "når", "og", "også", "oppdater", "på", "skal", "skriv", "slik", "som",
            "så", "være",
        ],
    ),
    (
        "Polish",
        &[
            "aby", "być", "czy", "dla", "dodaj", "gdy", "jak", "jest", "jeszcze", "jeśli", "już",
            "która", "które", "który", "lub", "może", "musi", "muszą", "na", "napisz", "napraw",
            "nie", "od", "oraz", "popraw", "proszę", "przez", "się", "sprawdź", "tak", "tego",
            "tylko", "tym", "usuń", "utwórz", "za", "zamiast", "zmień", "że", "żeby",
        ],
    ),
    (
        "Portuguese",
        &[
            "adiciona", "adicione", "altere", "ao", "aos", "apague", "arrume", "atualize", "com",
            "como", "conserte", "corrija", "crie", "da", "das", "de", "em", "escreva", "esta",
            "este", "está", "faça", "isso", "isto", "mais", "mas", "mude", "na", "nas", "nos",
            "não", "para", "pela", "pelo", "pode", "por", "que", "remova", "se", "sem", "também",
            "troque", "uma", "você", "é",
        ],
    ),
    (
        "Romanian",
        &[
            "aceasta",
            "această",
            "acest",
            "acum",
            "adaugă",
            "astfel",
            "când",
            "că",
            "dacă",
            "dar",
            "doar",
            "este",
            "fără",
            "mai",
            "modifică",
            "nu",
            "pe",
            "pentru",
            "repară",
            "sau",
            "schimbă",
            "scrie",
            "să",
            "trebuie",
            "unde",
            "în",
            "încât",
            "şi",
            "şterge",
            "și",
            "șterge",
        ],
    ),
    (
        "Spanish",
        &[
            "actualiza",
            "agrega",
            "al",
            "arregla",
            "añade",
            "borra",
            "cambia",
            "como",
            "con",
            "corrige",
            "crea",
            "cuando",
            "de",
            "debe",
            "deben",
            "del",
            "donde",
            "el",
            "elimina",
            "en",
            "es",
            "escribe",
            "esta",
            "este",
            "esto",
            "está",
            "están",
            "haz",
            "la",
            "las",
            "lo",
            "los",
            "más",
            "para",
            "pero",
            "por",
            "porque",
            "puede",
            "puedes",
            "que",
            "quita",
            "qué",
            "revisa",
            "se",
            "sube",
            "también",
            "un",
            "una",
        ],
    ),
    (
        "Swedish",
        &[
            "att",
            "bort",
            "den",
            "det",
            "detta",
            "där",
            "efter",
            "eller",
            "ett",
            "fel",
            "felet",
            "fixa",
            "från",
            "för",
            "har",
            "hur",
            "inte",
            "jag",
            "kan",
            "lägg",
            "med",
            "måste",
            "när",
            "och",
            "också",
            "på",
            "rätta",
            "ska",
            "skriv",
            "som",
            "så",
            "uppdatera",
            "vad",
            "varför",
            "ändra",
            "är",
        ],
    ),
    (
        "Turkish",
        &[
            "ama",
            "ancak",
            "bana",
            "bir",
            "bu",
            "cok",
            "daha",
            "degil",
            "değil",
            "değiştir",
            "düzelt",
            "ekle",
            "eğer",
            "gibi",
            "güncelle",
            "hiç",
            "icin",
            "ile",
            "ise",
            "için",
            "kadar",
            "kaldır",
            "ki",
            "lutfen",
            "lütfen",
            "mi",
            "misin",
            "mı",
            "mısın",
            "nasıl",
            "ne",
            "neden",
            "olan",
            "olarak",
            "oluştur",
            "sil",
            "sonra",
            "tüm",
            "veya",
            "yani",
            "yaz",
            "yerine",
            "yok",
            "çok",
            "önce",
            "şey",
            "şimdi",
            "şu",
        ],
    ),
    (
        "Vietnamese",
        &[
            "bạn", "cho", "các", "có", "của", "giúp", "hãy", "khi", "không", "là", "làm", "lỗi",
            "một", "như", "những", "này", "nếu", "sẽ", "sửa", "theo", "thêm", "thì", "trong",
            "tôi", "tạo", "từ", "viết", "và", "về", "với", "xoá", "xóa", "đang", "đã", "được",
            "để", "đổi",
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

/// A quotation mark that is also an apostrophe.
fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}')
}

/// The marks that close a quotation opened by `c`, if `c` opens one:
/// '…', "…", ‘…’, “…”, „…“, «…», »…«, ‹…›, 「…」 and 『…』.
fn closing_marks(c: char) -> Option<&'static [char]> {
    Some(match c {
        '\'' => &['\''],
        '"' => &['"'],
        '\u{2018}' => &['\u{2019}'],
        '\u{201C}' => &['\u{201D}'],
        '\u{201E}' => &['\u{201C}', '\u{201D}'],
        '\u{AB}' => &['\u{BB}'],
        '\u{BB}' => &['\u{AB}'],
        '\u{2039}' => &['\u{203A}'],
        '\u{300C}' => &['\u{300D}'],
        '\u{300E}' => &['\u{300F}'],
        _ => return None,
    })
}

/// Where a quotation whose text starts at `rest` ends: the byte just past
/// its closing mark, or, when it never closes, the end of its line. A
/// quotation closes on the line it opens on, and an apostrophe with a
/// letter after it -- "'L'application plante'" -- is inside a word, not the
/// close.
fn quotation_end(rest: &str, marks: &[char]) -> Result<usize, usize> {
    let mut chars = rest.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '\n' {
            return Err(i);
        }
        let in_word = is_apostrophe(c) && chars.peek().is_some_and(|&(_, n)| n.is_alphanumeric());
        if marks.contains(&c) && !in_word {
            return Ok(i + c.len_utf8());
        }
    }
    Err(rest.len())
}

/// The quotations in a stretch of prose, as byte ranges that include their
/// marks. An apostrophe after a letter or digit opens nothing: it is part
/// of "don't", "l'erreur", "dell'app" or "users'".
fn quotations(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    // A mark that found no close finds none later on the same line either,
    // so where that line ends is remembered rather than searched again: a
    // pasted line of a hundred thousand "«" would otherwise take quadratic
    // time.
    let mut unclosed: Vec<(char, usize)> = Vec::new();
    let mut i = 0;
    let mut prev: Option<char> = None;
    while let Some(c) = text[i..].chars().next() {
        let start = i;
        i += c.len_utf8();
        let after_word = prev.is_some_and(char::is_alphanumeric);
        prev = Some(c);
        let Some(marks) = closing_marks(c) else {
            continue;
        };
        if (c == '\'' && after_word) || unclosed.iter().any(|&(m, end)| m == c && start < end) {
            continue;
        }
        match quotation_end(&text[i..], marks) {
            Ok(end) => {
                i += end;
                out.push((start, i));
                prev = text[..i].chars().next_back();
            }
            Err(line_end) => {
                unclosed.retain(|&(m, _)| m != c);
                unclosed.push((c, i + line_end));
            }
        }
    }
    out
}

/// An elided article or pronoun split off the front of its word: "l'" from
/// "l'erreur", "dell'" from "dell'app". English elides at the back --
/// "don't", "it's" -- so an English word stays whole.
fn split_elision(word: &str) -> (Option<&str>, &str) {
    let Some(at) = word.find(is_apostrophe) else {
        return (None, word);
    };
    let end = at + word[at..].chars().next().map_or(1, char::len_utf8);
    let (head, rest) = word.split_at(end);
    let key = head.to_lowercase().replace('\u{2019}', "'");
    let listed = FUNCTION_WORDS
        .iter()
        .any(|(_, list)| list.binary_search(&key.as_str()).is_ok());
    if rest.is_empty() || !listed {
        return (None, word);
    }
    (Some(head), rest)
}

/// A function word no list of locale codes could hold. Two letters is where
/// function words and ISO 639-1 codes meet: "en es de fr" is a list of
/// locales, not a Spanish sentence.
fn is_telling(word: &str) -> bool {
    word.chars().count() > 2 || !word.bytes().all(|b| b.is_ascii_lowercase())
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
    /// Written with a capital, wherever it stands.
    capital: bool,
    /// Capitalised where no sentence starts, or written in capitals: a name
    /// or an acronym -- "Los Angeles", "La Liga", "MIT" -- rather than the
    /// function word it happens to spell.
    name_like: bool,
}

/// What is left of a prompt once its code, and usually its quotations, are
/// set aside.
#[derive(Default)]
struct Prose {
    letters: [usize; SCRIPTS],
    words: Vec<Word>,
    /// A quotation was set aside unread.
    quoted: bool,
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
        // Its semicolon makes it read as code, but only English writes it:
        // "TL;DR: Ce module gère l'authentification" asks in English.
        if core.eq_ignore_ascii_case("tl;dr") {
            self.readable = true;
            return ends_sentence;
        }
        if core.is_empty() || is_code(core) {
            return ends_sentence;
        }
        let mut first = starts_sentence;
        for word in core.split(|c: char| !(c.is_alphabetic() || is_joiner(c) || is_mark(c))) {
            let word = word.trim_matches(is_joiner);
            if word.is_empty() {
                continue;
            }
            let (elided, word) = split_elision(word);
            for part in elided.into_iter().chain([word]) {
                if !self.read_word(part, first) {
                    return ends_sentence;
                }
                first = false;
            }
        }
        ends_sentence
    }

    /// Count one word of prose, or stop at an English one: false once the
    /// prompt is known to be readable.
    fn read_word(&mut self, word: &str, first: bool) -> bool {
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
            return false;
        }
        self.words.push(Word {
            text,
            capital,
            name_like: shouted || (capital && !first),
        });
        true
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

    /// Read text with no code in it, setting its quotations aside when
    /// `skip_quotes` says to.
    fn read_prose(&mut self, text: &str, sentence: &mut bool, skip_quotes: bool) {
        if !skip_quotes {
            return self.read_plain(text, sentence);
        }
        let mut cursor = 0;
        for (start, end) in quotations(text) {
            self.read_plain(&text[cursor..start], sentence);
            if self.readable {
                return;
            }
            self.quoted = true;
            // A quotation sits inside a sentence, as code does.
            *sentence = false;
            cursor = end;
        }
        self.read_plain(&text[cursor..], sentence);
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

    /// Which words are particles inside a name: a run of [`NAME_PARTICLES`]
    /// between two capitalised words, "von der" in "Ursula von der Leyen".
    fn particles(&self) -> Vec<bool> {
        let words = &self.words;
        let particle = |i: usize| {
            words.get(i).is_some_and(|w: &Word| {
                !w.capital && NAME_PARTICLES.binary_search(&w.text.as_str()).is_ok()
            })
        };
        let mut out = vec![false; words.len()];
        let mut i = 1;
        while i < words.len() {
            if !(words[i - 1].capital && particle(i)) {
                i += 1;
                continue;
            }
            let mut end = i + 1;
            while particle(end) {
                end += 1;
            }
            if words.get(end).is_some_and(|w| w.capital) {
                out[i..end].fill(true);
            }
            i = end;
        }
        out
    }

    /// The Latin-script language whose function words this prose uses most.
    fn function_word_language(&self) -> Option<Foreign> {
        if self.words.len() < MIN_SHORT_WORDS {
            return None;
        }
        let particles = self.particles();
        // Lowercase words with a letter English does not use: "błąd" or
        // "modülü", but not "Müller", which is a name.
        let accented: Vec<&str> = self
            .words
            .iter()
            .filter(|w| !w.capital && !w.text.is_ascii())
            .map(|w| w.text.as_str())
            .collect();
        let mut best: Option<(&'static str, usize)> = None;
        let mut tied = false;
        for (language, list) in FUNCTION_WORDS {
            let mut found: Vec<&str> = Vec::new();
            let mut in_names: Vec<&str> = Vec::new();
            for (word, &particle) in self.words.iter().zip(&particles) {
                if word.name_like || list.binary_search(&word.text.as_str()).is_err() {
                    continue;
                }
                if particle {
                    in_names.push(&word.text);
                } else {
                    found.push(&word.text);
                }
            }
            // A particle is a name's only while nothing else in the prose is
            // this language.
            if !found.is_empty() {
                found.append(&mut in_names);
            }
            found.sort_unstable();
            found.dedup();
            let telling = found.iter().filter(|w| is_telling(w)).count();
            // The accent has to be a second piece of evidence, not the
            // function word again: "über cool feature" stays a task.
            let accent = accented.iter().any(|w| !found.contains(w));
            if !decides(self.words.len(), found.len(), telling, accent) {
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

/// Whether the function words found for one language name it, in prose of
/// `words` words: `found` distinct ones, `telling` of them telling, and
/// `accent` when another word has a letter English does not use.
///
/// A sentence needs two, one of them telling. Stronger evidence decides a
/// prompt of any length, three words included: two telling ones, "behebe
/// den Login-Fehler", or one and an accented word, "napraw błąd w
/// logowaniu". Fewer than three words never get here.
fn decides(words: usize, found: usize, telling: usize, accent: bool) -> bool {
    let sentence = words >= MIN_WORDS && found >= MIN_FUNCTION_WORDS && telling >= 1;
    sentence || telling >= MIN_FUNCTION_WORDS || (telling >= 1 && accent)
}

/// What is left of a prompt once its code and quotations are set aside.
/// When nothing at all is left outside the quotes, what they hold is read
/// instead: "'Сохранить'" on its own is Russian.
fn read(text: &str) -> Prose {
    let outside = read_with(text, true);
    if outside.quoted && !outside.readable && outside.letters.iter().all(|&n| n == 0) {
        return read_with(text, false);
    }
    outside
}

fn read_with(text: &str, skip_quotes: bool) -> Prose {
    let mut prose = Prose::default();
    let mut sentence = true;
    let mut cursor = 0;
    for region in code_regions(text) {
        prose.read_prose(&text[cursor..region.start], &mut sentence, skip_quotes);
        if prose.readable {
            return prose;
        }
        // Code sits inside a sentence, so what follows it continues one.
        sentence = false;
        cursor = region.end;
    }
    prose.read_prose(&text[cursor..], &mut sentence, skip_quotes);
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
            // Each of these was graded, between 102.5 and 187.0, an F.
            (
                "fixa buggen i src/auth/login.rs så att verify_token returnerar \
                 ett fel när token har gått ut",
                "Swedish",
            ),
            (
                "ret fejlen i src/auth/login.rs så verify_token returnerer en fejl \
                 når tokenet er udløbet",
                "Danish",
            ),
            (
                "fiks feilen i src/auth/login.rs slik at verify_token returnerer en \
                 feil når tokenet har utløpt",
                "Norwegian",
            ),
            (
                "korjaa virhe tiedostossa src/auth/login.rs niin että verify_token \
                 palauttaa virheen kun tunnus vanhenee",
                "Finnish",
            ),
            (
                "oprav chybu v src/auth/login.rs, aby verify_token vracela chybu, \
                 když token vyprší",
                "Czech",
            ),
            (
                "javítsd a hibát a src/auth/login.rs fájlban, hogy a verify_token \
                 hibát adjon vissza lejárt token esetén",
                "Hungarian",
            ),
            (
                "repară eroarea din src/auth/login.rs astfel încât verify_token să \
                 returneze o eroare când tokenul a expirat",
                "Romanian",
            ),
        ] {
            assert_eq!(language(text), Some(Some(name)), "{text}");
        }
    }

    #[test]
    fn a_short_request_is_named_on_stronger_evidence() {
        for (text, name) in [
            // An elided article is a function word no locale list holds.
            ("corrige l'erreur de connexion", "French"),
            ("correggi l'errore di login", "Italian"),
            ("arregla el error del login", "Spanish"),
            ("corrija o erro de login", "Portuguese"),
            // Three words: two telling function words.
            ("behebe den Login-Fehler", "German"),
            // Or one, and another word English would not spell.
            ("napraw błąd w logowaniu", "Polish"),
            ("napraw błąd logowania", "Polish"),
            ("ödeme modülüne test ekle", "Turkish"),
        ] {
            assert_eq!(language(text), Some(Some(name)), "{text}");
        }
        for text in [
            "arregla el bug",
            "hola que tal",
            // The accent is the function word's own, so there is one piece
            // of evidence, not two.
            "über cool feature",
            // A capital is a name before it is an accent.
            "Müller und Söhne invoice template",
            "perbaiki error login",
        ] {
            assert_eq!(detect(text), None, "{text}");
        }
    }

    #[test]
    fn a_quotation_is_set_aside() {
        // The prose around the quotes is English, so the prompt is. Read
        // whole, each of these was named as the language it quotes.
        for text in [
            "ru.json has 'Сохранить' twice",
            "build fails: 'Das System kann den angegebenen Pfad nicht finden'",
            "Customer wrote: 'La aplicación se cierra cuando subo una foto'",
            "Ticket 4521: 'L'application plante quand je télécharge une photo'",
            "French string \"Vous devez être connecté pour accéder à cette page\" \
             is truncated on mobile",
            "string “Paramètres de l’utilisateur” truncated",
            "string «Paramètres de l'utilisateur» truncated",
            "string ‘Paramètres de l’utilisateur’ truncated",
            "label 「設定を保存できませんでした」",
        ] {
            assert_eq!(detect(text), None, "{text}");
        }
        // And around these it is not.
        assert_eq!(
            language("Cambia el texto 'Save the changes' por 'Guardar' en el formulario"),
            Some(Some("Spanish"))
        );
        // With nothing outside the quotes, the quotation is the prompt.
        assert_eq!(language("'Сохранить изменения'"), Some(Some("Russian")));
    }

    #[test]
    fn an_apostrophe_in_a_word_opens_no_quotation() {
        let quoted = |text: &str| -> Vec<String> {
            quotations(text)
                .into_iter()
                .map(|(start, end)| text[start..end].to_string())
                .collect()
        };
        assert!(quoted("don't ship it's l'erreur dell'app or the users' table").is_empty());
        assert_eq!(quoted("don't touch 'Mot de passe' yet"), ["'Mot de passe'"]);
        assert_eq!(
            quoted("'L'application plante' and 'x'"),
            ["'L'application plante'", "'x'"]
        );
        // A quotation closes on its own line, or it is not one.
        assert!(quoted("the '90s theme\nand 'more").is_empty());
    }

    #[test]
    fn a_line_of_unclosed_quotes_is_searched_once() {
        // Searching to the end of the line again for each mark would take
        // minutes here, not milliseconds.
        let line = "« „ ".repeat(100_000);
        assert!(quotations(&line).is_empty());
        assert_eq!(detect(&line), None);
    }

    #[test]
    fn a_particle_inside_a_name_is_part_of_it() {
        for text in [
            "Seed users: Ursula von der Leyen, Mark van der Berg",
            "Fixtures: Ursula von der Leyen, Karl-Theodor zu Guttenberg",
            "Seed: Ursula von der Leyen, Mark van der Berg, José de la Cruz",
        ] {
            assert_eq!(detect(text), None, "{text}");
        }
        // German capitalises its nouns, so a particle between two capitals
        // can be German grammar, and the other German words say it is. Read
        // as names, these were graded.
        for text in [
            "Die Länge der Liste ist falsch",
            "Die Größe von Dateien ist falsch",
            "Fehler beim Laden von Bildern",
            "Änderung von Passwort funktioniert nicht",
            "Behebe den Login-Fehler",
        ] {
            assert_eq!(language(text), Some(Some("German")), "{text}");
        }
    }

    #[test]
    fn a_word_that_is_also_another_language_settles_nothing() {
        // "has", "for", "are" and "add" would each settle many a terse
        // English prompt, but on the English list each makes one of these
        // read as English too.
        for (text, name) in [
            ("¿Por qué has borrado el test de login?", "Spanish"),
            (
                "Ret verify_token i src/auth/login.rs, så den returnerer \
                 Err(AuthError::Expired) i stedet for at gå i panik, når tokenet er \
                 udløbet. Ændr ikke den offentlige signatur for login().",
                "Danish",
            ),
            (
                "Funcția verify_token din src/auth/login.rs are o eroare când tokenul \
                 a expirat, repară-o astfel încât să returneze Err(AuthError::Expired).",
                "Romanian",
            ),
            (
                "add hozzá a teszteket a tests/auth.rs fájlhoz, hogy a verify_token \
                 hibát adjon vissza lejárt token esetén",
                "Hungarian",
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
            // A request about text in another language says in English what
            // to do with it. "TL;DR" reads as code, and only English writes
            // it.
            "TL;DR: Ce module gère l'authentification des utilisateurs et la gestion \
             des sessions",
            "Summarize: Dieses Modul kümmert sich um die Anmeldung der Benutzer",
            "translate to English: Ce module gère l'authentification des utilisateurs",
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
