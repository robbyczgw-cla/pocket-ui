//! Box art from the libretro thumbnail collection (thumbnails.libretro.com): free, no account.
//! Games are matched by the name their checksum gives (`rom_id`), or the name the serial on a PlayStation disc gives
//! (`disc_serial`), else by file name (No-Intro style names match best), and cached on the SD card in
//! `<data>/covers/<system>/<rom stem>.png`. Network calls use the system's curl and block.

use std::path::{Path, PathBuf};
use crate::platform::http::{curl_listing, curl_with};

const BASE: &str = "https://thumbnails.libretro.com";

/// libretro playlist name for a ROCKNIX system id.
pub fn playlist(system: &str) -> Option<&'static str> {
    Some(match system {
        "gb" => "Nintendo - Game Boy",
        "gbc" => "Nintendo - Game Boy Color",
        "gba" => "Nintendo - Game Boy Advance",
        "nes" | "famicom" => "Nintendo - Nintendo Entertainment System",
        "fds" => "Nintendo - Family Computer Disk System",
        "snes" | "sfc" => "Nintendo - Super Nintendo Entertainment System",
        "n64" => "Nintendo - Nintendo 64",
        "nds" => "Nintendo - Nintendo DS",
        "virtualboy" => "Nintendo - Virtual Boy",
        "megadrive" | "genesis" => "Sega - Mega Drive - Genesis",
        "mastersystem" => "Sega - Master System - Mark III",
        "gamegear" => "Sega - Game Gear",
        "segacd" | "megacd" => "Sega - Mega-CD - Sega CD",
        "sega32x" => "Sega - 32X",
        "saturn" => "Sega - Saturn",
        "dreamcast" => "Sega - Dreamcast",
        "psx" => "Sony - PlayStation",
        "psp" => "Sony - PlayStation Portable",
        "pcengine" | "tg16" => "NEC - PC Engine - TurboGrafx 16",
        "pcenginecd" | "tg16cd" => "NEC - PC Engine CD - TurboGrafx-CD",
        "ngp" => "SNK - Neo Geo Pocket",
        "ngpc" => "SNK - Neo Geo Pocket Color",
        "atarilynx" => "Atari - Lynx",
        "atari2600" => "Atari - 2600",
        "wonderswan" => "Bandai - WonderSwan",
        "wonderswancolor" => "Bandai - WonderSwan Color",
        "gamecube" => "Nintendo - GameCube",
        "ps2" => "Sony - PlayStation 2",
        "sg-1000" | "sg1000" => "Sega - SG-1000",
        _ => return None,
    })
}

pub fn cover_path(data_dir: &Path, system: &str, rom: &Path) -> PathBuf {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
    data_dir.join("covers").join(system).join(format!("{stem}.png"))
}

/// Lowercase letters and digits only, with (…) and […] groups removed: "Tetris (World) (Rev 1)" → "tetris".
pub fn normalize(name: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for ch in name.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 && ch.is_alphanumeric() => out.extend(ch.to_lowercase()),
            _ => {}
        }
    }
    out
}

/// Lower is better: clean dumps first, then the usual region order, then the plainest name
/// ("(USA)" before "(USA) (Virtual Console)").
pub(crate) fn rank(name: &str) -> u32 {
    let regions = ["(World", "(USA", "(Europe", "(Japan"];
    let region = regions.iter().position(|r| name.contains(r)).unwrap_or(regions.len()) as u32;
    let dirty = if name.contains('[') { 10 } else { 0 };
    // Re-releases come with their own box art; the original box is the one people know.
    let rereleases = ["Collection)", "(Virtual Console", "(Switch Online", "(Classic Mini", "(Evercade"];
    let region = if rereleases.iter().any(|r| name.contains(r)) { region + 5 } else { region };
    let extras = name.matches('(').count().min(9) as u32;
    (region + dirty) * 10 + extras
}

/// "0902 - MegaMan & Bass (U)" → "MegaMan & Bass (U)": scene sets number their releases.
pub(crate) fn strip_release_number(stem: &str) -> &str {
    let digits = stem.bytes().take_while(u8::is_ascii_digit).count();
    match stem[digits..].trim_start().strip_prefix('-') {
        Some(rest) if (3..=4).contains(&digits) => rest.trim_start(),
        _ => stem,
    }
}

/// The words of a name, lowercase, without (…) and […] groups: "Donkey Kong 1 - Country" → donkey kong 1 country.
fn words(name: &str) -> std::collections::BTreeSet<String> {
    let mut plain = String::new();
    let mut depth = 0;
    for ch in name.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => plain.extend(ch.to_lowercase()),
            _ => {}
        }
    }
    plain.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// The cleanest entry when `found` is one game (in one or more regions); `None` when it is several games.
/// Several games count as one when a single one ranks best: "Cortex Strikes Back (USA)" over the
/// Japanese "Cortex no Gyakushuu! (Japan)", but not "Red Version (USA)" over "Blue Version (USA)".
fn one_game<'a>(found: Vec<&'a String>) -> Option<&'a str> {
    let mut best: Vec<(u32, String, &'a String)> = Vec::new();
    for n in found {
        let (r, game) = (rank(n), normalize(n));
        match best.iter_mut().find(|(_, g, _)| *g == game) {
            Some(entry) if r < entry.0 => *entry = (r, game, n),
            Some(_) => {}
            None => best.push((r, game, n)),
        }
    }
    best.sort_by_key(|(r, _, _)| *r);
    match best.as_slice() {
        [only] => Some(only.2),
        [first, second, ..] if first.0 / 10 < second.0 / 10 => Some(first.2),
        _ => None,
    }
}

/// "2", "III": a word that makes the name a different game of a series.
pub(crate) fn sequel_word(word: &str) -> bool {
    (word.bytes().all(|b| b.is_ascii_digit()) && word != "1") || ["ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x"].contains(&word)
}

/// Picks the box art for a ROM stem: the exact name if present, else the best same-title entry. Names
/// that don't follow No-Intro get three looser tries, each only taken when it points at a single game:
/// without a scene release number, the title without its subtitle, and one stray word too many.
pub fn best_match<'a>(stem: &str, index: &'a [String]) -> Option<&'a str> {
    let stem = crate::library::strip_inner_ext(stem);
    // libretro replaces these characters with '_' in file names.
    let wanted: String = stem.chars().map(|c| if "&*/:`<>?\\|".contains(c) { '_' } else { c }).collect();
    if let Some(exact) = index.iter().find(|n| **n == wanted) {
        return Some(exact);
    }
    let stem = strip_release_number(stem);
    let key = normalize(stem);
    if key.is_empty() {
        return None;
    }
    if let Some(same) = index.iter().filter(|n| normalize(n) == key).min_by_key(|n| rank(n)) {
        return Some(same);
    }
    // "Crash Bandicoot 2" for "Crash Bandicoot 2 - Cortex Strikes Back (USA)".
    let main_title = |n: &str| normalize(n.split(" - ").next().unwrap_or(n));
    if let Some(titled) = one_game(index.iter().filter(|n| main_title(n) == key).collect()) {
        return Some(titled);
    }
    // "Donkey Kong 1 - Country" for "Donkey Kong Country (USA)": every word of the entry is in the name,
    // and the name has at most one word more.
    let have = words(stem);
    if have.len() < 3 {
        return None;
    }
    one_game(
        index
            .iter()
            .filter(|n| {
                let w = words(n);
                w.len() >= 2 && w.len() <= have.len() && have.len() - w.len() <= 1 && w.is_subset(&have) && !have.difference(&w).any(|x| sequel_word(x))
            })
            .collect(),
    )
}

/// The entry for a game: under the name its checksum gave (`rom_id`) when the collection has it, else by its file
/// name as [`best_match`] finds it.
pub fn pick<'a>(stem: &str, identified: Option<&str>, index: &'a [String]) -> Option<&'a str> {
    identified.and_then(|name| best_match(name, index)).or_else(|| best_match(stem, index))
}

pub(crate) fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// File names (without .png) listed in the playlist's Named_Boxarts directory.
pub fn parse_index(html: &str) -> Vec<String> {
    html.split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .filter(|href| href.ends_with(".png"))
        .map(|href| percent_decode(href.trim_end_matches(".png")))
        .collect()
}

/// A text file from the web (a game list of libretro-database, `rom_id`), asked for compressed; None offline.
pub fn fetch_text(url: &str) -> Option<String> {
    curl_listing(url).map(|b| String::from_utf8_lossy(&b).into_owned())
}

pub fn fetch_index(playlist: &str) -> Vec<String> {
    fetch_index_in(playlist, "Named_Boxarts")
}

/// The names listed in one of the playlist's directories (`Named_Boxarts`, `Named_Snaps`, `Named_Titles`).
pub fn fetch_index_in(playlist: &str, remote_dir: &str) -> Vec<String> {
    let url = format!("{BASE}/{}/{remote_dir}/", percent_encode(playlist));
    curl_listing(&url).map(|b| parse_index(&String::from_utf8_lossy(&b))).unwrap_or_default()
}

/// Downloads the box art for one game to `dest`; true when a cover is now on disk.
pub fn download(playlist: &str, name: &str, dest: &Path) -> bool {
    download_from(playlist, "Named_Boxarts", name, dest, &[], &|_| true)
}

/// Downloads one picture of the collection to `dest` (written as `.part` first, moved into place only when
/// `accept` likes it). `extra` are more curl options (a size limit). True when the file is on disk.
pub fn download_from(playlist: &str, remote_dir: &str, name: &str, dest: &Path, extra: &[&str], accept: &dyn Fn(&Path) -> bool) -> bool {
    if let Some(dir) = dest.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let url = format!("{BASE}/{}/{remote_dir}/{}.png", percent_encode(playlist), percent_encode(name));
    let tmp = dest.with_extension("part");
    let ok = curl_with(&url, Some(&tmp), extra).is_some() && accept(&tmp) && std::fs::rename(&tmp, dest).is_ok();
    let _ = std::fs::remove_file(&tmp);
    ok
}

/// One scrape at a time: the automatic one (src/auto_covers.rs) and Extras > Cover art write the same files.
static SCRAPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Held while a scrape runs; dropping it lets the next one start.
pub struct Claim(());

impl Drop for Claim {
    fn drop(&mut self) {
        SCRAPING.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// The right to scrape now, or None while another scrape runs.
pub fn claim() -> Option<Claim> {
    use std::sync::atomic::Ordering;
    SCRAPING.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).ok().map(|_| Claim(()))
}

/// A scrape is running.
pub fn busy() -> bool {
    SCRAPING.load(std::sync::atomic::Ordering::Acquire)
}

/// The games a `.missing` list names, by ROM stem.
pub fn listed_missing(data_dir: &Path, system: &str) -> std::collections::HashSet<String> {
    let file = data_dir.join("covers").join(system).join(".missing");
    std::fs::read_to_string(file).unwrap_or_default().lines().map(str::to_string).collect()
}

/// What `scrape_until` got through.
pub struct Scraped {
    pub found: usize,
    /// The games it looked up (found or not); the rest were left for later.
    pub looked_up: Vec<PathBuf>,
}

/// Downloads missing box art for `games` (system id, ROM path). Games without a match are noted in
/// `covers/<system>/.missing` and skipped next time. `progress(done, total, found)` runs after each game.
pub fn scrape(data_dir: &Path, games: &[(String, PathBuf)], progress: impl Fn(usize, usize, usize)) -> usize {
    scrape_until(data_dir, games, progress, || false).found
}

/// The games `scrape_until` would look up: a known system, no cover on the card.
pub fn todo<'a>(data_dir: &Path, games: &'a [(String, PathBuf)]) -> Vec<&'a (String, PathBuf)> {
    games.iter().filter(|(sys, rom)| playlist(sys).is_some() && !cover_path(data_dir, sys, rom).exists()).collect()
}

/// `scrape`, checking `stop` before each game (a game was started): it ends after the cover it is on.
pub fn scrape_until(data_dir: &Path, games: &[(String, PathBuf)], progress: impl Fn(usize, usize, usize), stop: impl Fn() -> bool) -> Scraped {
    let todo = todo(data_dir, games);
    let total = todo.len();
    let (mut done, mut found) = (0, 0);
    let mut looked_up = Vec::new();
    let mut index: Option<(String, Vec<String>)> = None;
    // The proper names the games' checksums gave (`rom_id`), worked out before the download.
    let names = crate::rom_id::known(data_dir, games);
    for (sys, rom) in todo {
        if stop() {
            break;
        }
        looked_up.push(rom.clone());
        let list = playlist(sys).unwrap_or_default();
        let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let missing_file = data_dir.join("covers").join(sys).join(".missing");
        let missing = std::fs::read_to_string(&missing_file).unwrap_or_default();
        if !missing.lines().any(|l| l == stem) {
            if index.as_ref().is_none_or(|(p, _)| p != list) {
                index = Some((list.to_string(), fetch_index(list)));
            }
            let list_names = &index.as_ref().unwrap().1;
            match pick(stem, names.get(rom).map(String::as_str), list_names) {
                Some(name) if download(list, name, &cover_path(data_dir, sys, rom)) => found += 1,
                // An empty index means the catalog didn't load; don't mark the game as missing then.
                _ if !list_names.is_empty() => {
                    let _ = crate::fsutil::write_atomic(&missing_file, format!("{missing}{stem}\n").as_bytes());
                }
                _ => {}
            }
        }
        done += 1;
        progress(done, total, found);
    }
    Scraped { found, looked_up }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn scene_release_numbers_and_short_regions_still_match() {
        let idx = index(&["Mega Man & Bass (Japan)", "Mega Man & Bass (USA)", "Mega Man Zero (USA)"]);
        assert_eq!(best_match("0902 - MegaMan & Bass (U)(Venom)", &idx), Some("Mega Man & Bass (USA)"));
        assert_eq!(best_match("1234 - Mega Man Zero (U)", &idx), Some("Mega Man Zero (USA)"));
        // The plain release before a re-release of it.
        let idx = index(&["Mega Man _ Bass (USA) (Virtual Console)", "Mega Man _ Bass (USA)"]);
        assert_eq!(best_match("0902 - MegaMan & Bass (U)(Venom)", &idx), Some("Mega Man _ Bass (USA)"));
        // The original box before a later collection's.
        let idx = index(&["Contra III - The Alien Wars (World) (Contra Anniversary Collection)", "Contra III - The Alien Wars (USA)"]);
        assert_eq!(best_match("Contra III - The Alien Wars", &idx), Some("Contra III - The Alien Wars (USA)"));
    }

    #[test]
    fn a_title_without_its_subtitle_finds_the_one_game_that_has_it() {
        let idx = index(&[
            "Crash Bandicoot (USA)",
            "Crash Bandicoot 2 - Cortex Strikes Back (Europe, Australia)",
            "Crash Bandicoot 2 - Cortex Strikes Back (USA)",
            "Crash Bandicoot 2 - Cortex no Gyakushuu! (Japan)",
            "Crash Bandicoot - Warped (USA)",
            "Pokemon - Red Version (USA, Europe)",
            "Pokemon - Blue Version (USA, Europe)",
        ]);
        assert_eq!(best_match("Crash Bandicoot 2", &idx), Some("Crash Bandicoot 2 - Cortex Strikes Back (USA)"));
        // Two different games share the title: no guess.
        assert_eq!(best_match("Pokemon", &idx), None);
    }

    #[test]
    fn one_stray_word_in_a_hand_made_name_is_forgiven() {
        let idx = index(&["Donkey Kong Country (USA)", "Donkey Kong Country 2 - Diddy's Kong Quest (USA)", "Donkey Kong (Japan)"]);
        assert_eq!(best_match("Donkey Kong 1 - Country", &idx), Some("Donkey Kong Country (USA)"));
        // Too far from anything: nothing.
        assert_eq!(best_match("Kong", &idx), None);
        // A sequel number is never the stray word.
        assert_eq!(best_match("Donkey Kong Country 3", &index(&["Donkey Kong Country (USA)"])), None);
        assert_eq!(best_match("Crash Bandicoot 2", &index(&["Crash Bandicoot (USA)"])), None);
        assert_eq!(best_match("Donkey Kong Country Returns Deluxe", &idx), None);
    }

    #[test]
    fn the_checksums_name_comes_first_and_the_file_name_after_it() {
        let idx = index(&["Mega Man & Bass (USA)", "Mega Man Zero (USA)", "Tetris (World) (Rev 1)"]);
        // A file name nothing matches, recognised by its checksum.
        assert_eq!(pick("mmbs-final-v2", Some("Mega Man & Bass (USA)"), &idx), Some("Mega Man & Bass (USA)"));
        // A name the collection lacks (a newer list): the file name still finds it.
        assert_eq!(pick("Tetris", Some("Tetris (World) (Rev 2)"), &idx), Some("Tetris (World) (Rev 1)"));
        // Not recognised: as before, by file name.
        assert_eq!(pick("1234 - Mega Man Zero (U)", None, &idx), Some("Mega Man Zero (USA)"));
        assert_eq!(pick("Unknown", None, &idx), None);
    }

    #[test]
    fn ps2_and_sg1000_have_covers() {
        assert_eq!(playlist("ps2"), Some("Sony - PlayStation 2"));
        assert_eq!(playlist("sg-1000"), Some("Sega - SG-1000"));
    }

    #[test]
    fn prefers_exact_then_clean_world_release() {
        let idx = index(&["Tetris (Japan)", "Tetris (World) (Rev 1)", "Tetris (World) [b]", "Tetris 2 (USA)"]);
        assert_eq!(best_match("Tetris (World) (Rev 1)", &idx), Some("Tetris (World) (Rev 1)"));
        assert_eq!(best_match("Tetris", &idx), Some("Tetris (World) (Rev 1)"));
        assert_eq!(best_match("tetris (j)", &idx), Some("Tetris (World) (Rev 1)"));
        assert_eq!(best_match("Unknown Game", &idx), None);
    }

    #[test]
    fn handles_libretro_character_rules_and_html() {
        let idx = index(&["Legend of Zelda, The - Link_s Awakening DX (USA, Europe)"]);
        assert!(best_match("Legend of Zelda, The - Link's Awakening DX (USA, Europe)", &idx).is_some());
        let html = r#"<a href="Tetris%20%28World%29.png">x</a><a href="../">up</a>"#;
        assert_eq!(parse_index(html), vec!["Tetris (World)".to_string()]);
        assert_eq!(normalize("Pokémon - Gelbe Edition (Germany) [S]"), "pokémongelbeedition");
    }
}
