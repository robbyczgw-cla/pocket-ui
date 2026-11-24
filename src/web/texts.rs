//! The page in the Pocket's language. The page's scripts write each text as `t("English")` (with `{n}`/`{s}`
//! placeholders, as in `assets/i18n/*.tsv`); `GET /api/texts` answers, for the language set on the Pocket, every such
//! text that has a translation, taken from the same tables the Pocket paints with (`crate::i18n`). Which texts the page
//! has is read from its scripts ([`SOURCES`]), so a new text needs only its line in the tables.
//!
//! No sign-in needed (the page asks before pairing, and the answer holds nothing private) and answered while a game
//! runs: one look-up per language, kept for as long as pocket-ui runs.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::json;

use super::assets;
use super::http::Response;
use crate::i18n::Lang;

/// The page's scripts whose `t("…")` texts are translated. A new script of the page adds its line here.
pub const SOURCES: &[&str] = &[
    assets::APP_JS,
    assets::SAVES_JS,
    assets::COVER_DIAGNOSTICS_JS,
    assets::MEDIA_UPDATE_JS,
];

/// Every text a script hands to `t(…)` as a literal: `t("Saves")`, `t('Slot {n}', …)`. A call with anything but a
/// string literal first is not a text of the table and is skipped.
pub fn keys(src: &str) -> Vec<String> {
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(at) = src[i..].find("t(") {
        let start = i + at;
        i = start + 2;
        // `t(` as a name of its own: not `set(`, `x.t(`, `$t(`.
        if start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || matches!(bytes[start - 1], b'_' | b'$' | b'.')) {
            continue;
        }
        let Some(&quote) = bytes.get(i).filter(|q| matches!(q, b'"' | b'\'')) else { continue };
        let mut text = String::new();
        let mut chars = src[i + 1..].char_indices();
        let mut end = None;
        while let Some((j, c)) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some((_, 'n')) => text.push('\n'),
                    Some((_, 't')) => text.push('\t'),
                    Some((_, other)) => text.push(other),
                    None => break,
                },
                '\n' => break,
                c if c as u32 == quote as u32 => {
                    end = Some(i + 1 + j + 1);
                    break;
                }
                c => text.push(c),
            }
        }
        if let Some(e) = end {
            i = e;
            if !text.is_empty() && !out.contains(&text) {
                out.push(text);
            }
        }
    }
    out
}

/// The translations of `keys` in `lang`: only those the table has and that differ from the English.
pub fn translations(lang: Lang, keys: &[String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Some(table) = lang.table().filter(|_| lang != Lang::En) else { return out };
    // A text with placeholders is a pattern line of the table, looked up as it is written there.
    let patterns: HashMap<String, String> = if keys.iter().any(|k| k.contains('{')) { table.entries().into_iter().collect() } else { HashMap::new() };
    for k in keys {
        let t = if k.contains('{') { patterns.get(k).cloned() } else { Some(crate::i18n::tr_in(lang, k)) };
        if let Some(t) = t.filter(|t| t != k && !t.is_empty()) {
            out.insert(k.clone(), t);
        }
    }
    out
}

/// The language's code as the page's `lang` attribute takes it ("de").
fn code(lang: Lang) -> String {
    serde_json::to_value(lang).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_else(|| "en".into())
}

/// The answer for `lang`, made once.
fn body(lang: Lang) -> Arc<Vec<u8>> {
    static MADE: OnceLock<Mutex<HashMap<Lang, Arc<Vec<u8>>>>> = OnceLock::new();
    let mut made = MADE.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    made.entry(lang)
        .or_insert_with(|| {
            let keys: Vec<String> = SOURCES.iter().flat_map(|s| keys(s)).collect();
            Arc::new(json!({ "lang": code(lang), "texts": translations(lang, &keys) }).to_string().into_bytes())
        })
        .clone()
}

/// `GET /api/texts`.
pub fn answer() -> Response {
    Response::new(200, "application/json; charset=utf-8", body(crate::i18n::current()).to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_texts_are_the_literals_handed_to_t() {
        let src = r#"
            const a = t("Saves"), b = t('Slot {n}', { n: 2 }), c = set("not this"), d = x.t("nor this");
            h("p", {}, t("Say \"hi\""), t(name), t("Saves"));
            function t(en, vars) {}
        "#;
        assert_eq!(keys(src), vec!["Saves".to_string(), "Slot {n}".into(), "Say \"hi\"".into()]);
    }

    #[test]
    fn an_unclosed_text_is_no_text() {
        assert!(keys("t(\"open\n\")").is_empty());
        assert!(keys("t(\"").is_empty());
    }

    #[test]
    fn the_page_gets_the_tables_words_and_its_patterns_as_they_are_written() {
        let keys = vec!["Two versions".to_string(), "Before restore · {s}".to_string(), "Nothing like this is in the table".to_string()];
        let de = translations(Lang::De, &keys);
        assert_eq!(de.get("Two versions").map(String::as_str), Some("Zwei Versionen"));
        assert_eq!(de.get("Before restore · {s}").map(String::as_str), Some("Vor dem Wiederherstellen · {s}"));
        assert!(!de.contains_key("Nothing like this is in the table"), "what the table lacks stays English on the page");
        assert!(translations(Lang::En, &keys).is_empty());
    }

    #[test]
    fn every_text_of_the_page_scripts_is_in_every_table() {
        // Only the texts written as t("…"): the page's older English texts are not handed to t() yet.
        let all: Vec<String> = SOURCES.iter().flat_map(|s| keys(s)).collect();
        for lang in Lang::ALL.iter().copied().filter(|l| *l != Lang::En && l.complete()) {
            let table = lang.table().expect("a table");
            let lines: std::collections::HashSet<String> = table.entries().into_iter().map(|(en, _)| en).collect();
            let missing: Vec<&String> =
                all.iter().filter(|k| !same_in_every_language(k) && if k.contains('{') { !lines.contains(*k) } else { table.translate(k).is_none() }).collect();
            assert!(missing.is_empty(), "{lang:?} lacks the page's {missing:?}");
        }
    }

    /// Texts that are the same in every language (a unit, a name).
    fn same_in_every_language(k: &str) -> bool {
        matches!(k, "ZIP")
    }
}
