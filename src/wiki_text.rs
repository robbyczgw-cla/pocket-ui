//! The short description on the game card: the lead of the game's English Wikipedia article, loaded
//! when the card is opened and kept in `<data>/wikipedia/`. Wikipedia text is CC BY-SA 4.0, so it is
//! never shipped with pocket-ui; the card names the article and the licence under the text.
//! Network calls use the system's curl, from a worker thread (see `app_card.rs`).

use std::path::{Path, PathBuf};

/// How pocket-ui introduces itself to Wikipedia (card and background loading alike); no contact data.
pub const USER_AGENT: &str = "pocket-ui-wiki/0.1";
/// The page summary endpoint of the Wikimedia REST API.
const API: &str = "https://en.wikipedia.org/api/rest_v1/page/summary/";
/// The description stays short: whole sentences up to about this many characters.
const MAX_CHARS: usize = 700;

pub fn cache_path(data_dir: &Path, title: &str) -> PathBuf {
    let name: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .collect();
    data_dir.join("wikipedia").join(format!("{}.txt", name.trim()))
}

/// The description loaded before, if any.
pub fn cached(data_dir: &Path, title: &str) -> Option<String> {
    std::fs::read_to_string(cache_path(data_dir, title)).ok().filter(|t| !t.trim().is_empty())
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'(' | b')' | b',' | b'\'' | b'!' => (b as char).to_string(),
            b' ' => "_".to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// The text of a summary answer, cut after the last whole sentence that fits.
pub fn extract(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    if v.get("type").and_then(|t| t.as_str()) == Some("disambiguation") {
        return None;
    }
    let text = v.get("extract")?.as_str()?.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= MAX_CHARS {
        return Some(text);
    }
    let cut: String = text.chars().take(MAX_CHARS).collect();
    match cut.rfind(". ") {
        Some(end) if end > MAX_CHARS / 3 => Some(cut[..=end].to_string()),
        _ => Some(format!("{}…", cut.trim_end())),
    }
}

/// Loads the description of `title` from Wikipedia and caches it; an error is a few words for the card.
pub fn fetch(data_dir: &Path, title: &str) -> Result<String, String> {
    let url = format!("{API}{}", percent_encode(title));
    let json = crate::platform::wiki_json(&url)?;
    let text = extract(&String::from_utf8_lossy(&json)).ok_or("no text")?;
    let _ = crate::fsutil::write_atomic(&cache_path(data_dir, title), text.as_bytes());
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_text_is_taken_and_kept_short() {
        let json = r#"{"type":"standard","title":"Tetris (Game Boy video game)","extract":"Tetris is a puzzle video game.  It was released in 1989."}"#;
        assert_eq!(extract(json).as_deref(), Some("Tetris is a puzzle video game. It was released in 1989."));
        let long = format!(r#"{{"type":"standard","extract":"{}"}}"#, "A sentence that goes on. ".repeat(60));
        let text = extract(&long).unwrap();
        assert!(text.chars().count() <= MAX_CHARS && text.ends_with('.'), "{text}");
        assert_eq!(extract(r#"{"type":"disambiguation","extract":"Tetris may refer to:"}"#), None);
        assert_eq!(extract("not json"), None);
    }

    #[test]
    fn titles_become_safe_urls_and_file_names() {
        assert_eq!(percent_encode("Super Mario Land 2: 6 Golden Coins"), "Super_Mario_Land_2%3A_6_Golden_Coins");
        assert_eq!(percent_encode("Pokémon Red and Blue"), "Pok%C3%A9mon_Red_and_Blue");
        let p = cache_path(Path::new("/d"), "Tetris (Game Boy video game)");
        assert_eq!(p, Path::new("/d/wikipedia/Tetris _Game Boy video game_.txt"));
        assert_eq!(cache_path(Path::new("/d"), "../x").file_name().unwrap(), "___x.txt");
    }
}
