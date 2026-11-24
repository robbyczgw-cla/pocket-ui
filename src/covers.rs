//! Box art from the libretro thumbnail collection (thumbnails.libretro.com): free, no account.
//! Games are matched by the name their checksum gives (`rom_id`), or the name the serial on a PlayStation disc gives
//! (`disc_serial`), else by file name (No-Intro style names match best), and cached on the SD card in
//! `<data>/covers/<system>/<rom stem>.png`. Network calls use the system's curl and block.

use std::path::{Path, PathBuf};
use crate::platform::http::{curl_listing, curl_with};

const BASE: &str = "https://thumbnails.libretro.com";
const GITHUB_API: &str = "https://api.github.com/repos/libretro-thumbnails";
const GITHUB_RAW: &str = "https://raw.githubusercontent.com/libretro-thumbnails";

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
        "psvita" => "Sony - PlayStation Vita",
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
        "xbox" => "Microsoft - Xbox",
        "3ds" | "n3ds" => "Nintendo - Nintendo 3DS",
        "wii" => "Nintendo - Wii",
        "wiiu" => "Nintendo - Wii U",
        "n64dd" => "Nintendo - Nintendo 64DD",
        "pokemini" => "Nintendo - Pokemon Mini",
        "ps3" => "Sony - PlayStation 3",
        "neogeo" => "SNK - Neo Geo",
        "neocd" => "SNK - Neo Geo CD",
        "supergrafx" => "NEC - PC Engine SuperGrafx",
        "amiga" => "Commodore - Amiga",
        "amigacd32" => "Commodore - CD32",
        "c64" => "Commodore - 64",
        "vic20" => "Commodore - VIC-20",
        "msx" => "Microsoft - MSX",
        "msx2" => "Microsoft - MSX2",
        "3do" => "The 3DO Company - 3DO",
        "atarijaguar" | "jaguar" => "Atari - Jaguar",
        "atari5200" => "Atari - 5200",
        "atari7800" => "Atari - 7800",
        "atari800" => "Atari - 8-bit Family",
        "atarist" => "Atari - ST",
        "colecovision" => "Coleco - ColecoVision",
        "intellivision" => "Mattel - Intellivision",
        "vectrex" => "GCE - Vectrex",
        "odyssey2" => "Magnavox - Odyssey2",
        "videopac" => "Philips - Videopac+",
        "channelf" => "Fairchild - Channel F",
        "supervision" => "Watara - Supervision",
        "cdi" => "Philips - CD-i",
        "zxspectrum" => "Sinclair - ZX Spectrum",
        "dos" => "DOS",
        "scummvm" => "ScummVM",
        "tic-80" | "tic80" => "TIC-80",
        _ => return None,
    })
}

pub mod custom;

pub fn cover_path(data_dir: &Path, system: &str, rom: &Path) -> PathBuf {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
    data_dir.join("covers").join(system).join(format!("{stem}.png"))
}

/// Box art to display. Downloaders continue to use the automatic cover's path.
pub fn active_cover_path(data_dir: &Path, system: &str, rom: &Path) -> PathBuf {
    custom::existing(data_dir, system, rom).unwrap_or_else(|| cover_path(data_dir, system, rom))
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

/// The words of a name in order, lowercase, without (…) and […] groups.
fn word_list(name: &str) -> Vec<String> {
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

/// A collection's names (a playlist's box art, one name per picture) with what [`best_match`] compares worked out once:
/// every game's look-up compared its name with each entry's normalised name, title and words, made again for every
/// entry each time (3,000–5,000 entries, up to four passes, for each game of the card: millions of new strings).
#[derive(Clone, Debug, Default)]
pub struct Index {
    names: Vec<String>,
    keys: Vec<Key>,
}

#[derive(Clone, Debug)]
struct Key {
    /// `normalize(name)`
    plain: String,
    /// `normalize` of the title before the first " - "
    title: String,
    /// `word_list(name)`
    list: Vec<String>,
    /// `words(name)`
    words: std::collections::BTreeSet<String>,
}

impl Index {
    pub fn new(names: Vec<String>) -> Index {
        let keys = names
            .iter()
            .map(|n| {
                let list = word_list(n);
                Key { plain: normalize(n), title: normalize(n.split(" - ").next().unwrap_or(n)), words: list.iter().cloned().collect(), list }
            })
            .collect();
        Index { names, keys }
    }

    /// The names, in the collection's order.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    fn entries(&self) -> impl Iterator<Item = (&String, &Key)> {
        self.names.iter().zip(&self.keys)
    }
}

impl From<Vec<String>> for Index {
    fn from(names: Vec<String>) -> Index {
        Index::new(names)
    }
}

/// Picks the box art for a ROM stem: the exact name if present, else the best same-title entry. Names
/// that don't follow No-Intro get three looser tries, each only taken when it points at a single game:
/// without a scene release number, the title without its subtitle, and one stray word too many.
pub fn best_match<'a>(stem: &str, index: &'a Index) -> Option<&'a str> {
    let stem = crate::library::strip_inner_ext(stem);
    // libretro replaces these characters with '_' in file names.
    let wanted: String = stem.chars().map(|c| if "&*/:`<>?\\|".contains(c) { '_' } else { c }).collect();
    if let Some(exact) = index.names.iter().find(|n| **n == wanted) {
        return Some(exact);
    }
    let stem = strip_release_number(stem);
    let key = normalize(stem);
    if key.is_empty() {
        return None;
    }
    if let Some((same, _)) = index.entries().filter(|(_, k)| k.plain == key).min_by_key(|(n, _)| rank(n)) {
        return Some(same);
    }
    // "Crash Bandicoot 2" for "Crash Bandicoot 2 - Cortex Strikes Back (USA)".
    if let Some(titled) = one_game(index.entries().filter(|(_, k)| k.title == key).map(|(n, _)| n).collect()) {
        return Some(titled);
    }
    // "Top Spin" for "Top Spin Tennis (USA)": the name is the start of one game's title (two words at least,
    // one word more at most, not a sequel).
    let start: Vec<String> = word_list(stem);
    if start.len() >= 2 {
        let longer: Vec<&String> = index
            .entries()
            .filter(|(_, k)| k.list.len() == start.len() + 1 && k.list.starts_with(&start) && !sequel_word(&k.list[start.len()]))
            .map(|(n, _)| n)
            .collect();
        if let Some(only) = one_game(longer.clone()).filter(|_| longer.iter().all(|n| normalize(n) == normalize(longer[0]))) {
            return Some(only);
        }
    }
    // "Donkey Kong 1 - Country" for "Donkey Kong Country (USA)": every word of the entry is in the name,
    // and the name has at most one word more.
    let have = words(stem);
    if have.len() < 3 {
        return None;
    }
    one_game(
        index
            .entries()
            .filter(|(_, k)| {
                let w = &k.words;
                w.len() >= 2 && w.len() <= have.len() && have.len() - w.len() <= 1 && w.is_subset(&have) && !have.difference(w).any(|x| sequel_word(x))
            })
            .map(|(n, _)| n)
            .collect(),
    )
}

/// The entry for a game: under the name its checksum gave (`rom_id`) when the collection has it, else by its file
/// name as [`best_match`] finds it.
pub fn pick<'a>(stem: &str, identified: Option<&str>, index: &'a Index) -> Option<&'a str> {
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
    #[cfg(not(target_os = "android"))]
    let cancel = crate::update::Cancel::new();
    #[cfg(not(target_os = "android"))]
    let _optional = crate::optional_data::lock(&cancel);
    index_from(playlist, remote_dir, |url| (!crate::background_io::paused()).then(|| curl_listing(url)).flatten())
}

fn repository(playlist: &str) -> String {
    // Spaces become `_`, what a repository name can't have is left out ("Philips - Videopac+": `Philips_-_Videopac`).
    playlist.chars().filter_map(|ch| match ch {
        'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => Some(ch),
        ' ' => Some('_'),
        _ => None,
    }).collect()
}

fn index_from(playlist: &str, remote_dir: &str, fetch: impl Fn(&str) -> Option<Vec<u8>>) -> Vec<String> {
    let url = format!("{BASE}/{}/{remote_dir}/", percent_encode(playlist));
    let cdn = fetch(&url).map(|b| parse_index(&String::from_utf8_lossy(&b))).unwrap_or_default();
    if !cdn.is_empty() {
        return cdn;
    }
    // Only the one directory: the whole repository's tree is 12-20 MB for PlayStation or NES, and GitHub answers 500
    // for PlayStation 2's (20.11.2026); `master:Named_Boxarts` is about 450 KB gzipped for each.
    let url = format!("{GITHUB_API}/{}/git/trees/master:{remote_dir}", repository(playlist));
    fetch(&url).map(|b| github_index(&b)).unwrap_or_default()
}

fn github_index(bytes: &[u8]) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct Tree {
        truncated: bool,
        tree: Vec<Entry>,
    }
    #[derive(serde::Deserialize)]
    struct Entry {
        path: String,
        #[serde(rename = "type")]
        kind: String,
    }
    let Ok(tree) = serde_json::from_slice::<Tree>(bytes) else { return Vec::new() };
    if tree.truncated {
        return Vec::new();
    }
    tree.tree.into_iter().filter_map(|entry| {
        let name = entry.path.strip_suffix(".png")?;
        (entry.kind == "blob" && !name.contains('/')).then(|| name.to_string())
    }).collect()
}

/// A git symlink comes from raw.githubusercontent.com as its target's relative path, not as the picture (a sixth of
/// the SNES box art are such links): the target's address in the same repository and ref as `url`, or None for
/// anything else (another server, a path out of the repository, a picture).
fn symlink_target(url: &str, bytes: &[u8]) -> Option<String> {
    let (repo, rest) = url.strip_prefix(GITHUB_RAW)?.strip_prefix('/')?.split_once('/')?;
    let (git_ref, path) = rest.split_once('/')?;
    if bytes.len() > 512 || bytes.starts_with(b"\x89PNG") {
        return None;
    }
    let target = std::str::from_utf8(bytes).ok()?.trim();
    if !target.ends_with(".png") || target.starts_with('/') || target.contains(['\n', '\\']) {
        return None;
    }
    let mut dirs: Vec<String> = path.split('/').map(str::to_string).collect();
    dirs.pop()?;
    for seg in target.split('/') {
        match seg {
            // Never above the repository's root.
            ".." => {
                dirs.pop()?;
            }
            "." | "" => {}
            seg => dirs.push(percent_encode(seg)),
        }
    }
    Some(format!("{GITHUB_RAW}/{repo}/{git_ref}/{}", dirs.join("/")))
}

/// One request's response into `tmp`, binary (Android's transport gives text without a file).
fn fetch_to(url: &str, tmp: &Path, extra: &[&str]) -> bool {
    // curl has exited before the card is written: a whole batch must not dirty the Brick's card during Wi-Fi input.
    #[cfg(feature = "brick")]
    let ok = curl_with(url, None, extra).is_some_and(|bytes| !crate::background_io::paused() && crate::fsutil::write_atomic_bulk(tmp, &bytes).is_ok());
    #[cfg(not(feature = "brick"))]
    let ok = curl_with(url, Some(tmp), extra).is_some();
    ok
}

/// One picture into `tmp`, following a git symlink of GitHub's once (`symlink_target`).
fn fetch_picture_to(url: &str, tmp: &Path, extra: &[&str]) -> bool {
    if !fetch_to(url, tmp, extra) {
        return false;
    }
    match symlink_in(url, tmp) {
        Some(target) => !crate::background_io::paused() && fetch_to(&target, tmp, extra),
        None => true,
    }
}

/// The symlink a download in `file` is (`symlink_target`): None when the file is missing, larger than a link's text
/// (a picture), empty, cut short, not UTF-8 or not a picture's name.
fn symlink_in(url: &str, file: &Path) -> Option<String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    crate::background_io::open(file).ok()?.take(513).read_to_end(&mut bytes).ok()?;
    symlink_target(url, &bytes)
}

fn picture_from(playlist: &str, remote_dir: &str, name: &str, fetch: impl Fn(&str) -> bool) -> bool {
    let file = format!("{remote_dir}/{}.png", percent_encode(name));
    let url = format!("{BASE}/{}/{file}", percent_encode(playlist));
    fetch(&url) || fetch(&format!("{GITHUB_RAW}/{}/master/{file}", repository(playlist)))
}

/// Checks a listed picture before trusting the absence of other entries in its directory. `scratch` holds the
/// download while it is checked and is removed after.
pub(crate) fn picture_available(playlist: &str, remote_dir: &str, name: &str, extra: &[&str], scratch: &Path) -> bool {
    #[cfg(not(target_os = "android"))]
    let cancel = crate::update::Cancel::new();
    #[cfg(not(target_os = "android"))]
    let _optional = crate::optional_data::lock(&cancel);
    if let Some(dir) = scratch.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    picture_from(playlist, remote_dir, name, |url| {
        let ok = !crate::background_io::paused() && fetch_picture_to(url, scratch, extra) && crate::image::png_size(scratch).is_some();
        let _ = std::fs::remove_file(scratch);
        ok
    })
}


/// Downloads one picture of the collection to `dest` (written as `.part` first, moved into place only when
/// `accept` likes it). `extra` are more curl options (a size limit). True when the file is on disk.
pub fn download_from(playlist: &str, remote_dir: &str, name: &str, dest: &Path, extra: &[&str], accept: &dyn Fn(&Path) -> bool) -> bool {
    #[cfg(not(target_os = "android"))]
    let cancel = crate::update::Cancel::new();
    #[cfg(not(target_os = "android"))]
    let _optional = crate::optional_data::lock(&cancel);
    if crate::background_io::paused() { return false; }
    if let Some(dir) = dest.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = dest.with_extension("part");
    picture_from(playlist, remote_dir, name, |url| {
        if crate::background_io::paused() { return false; }
        // A game that started meanwhile gets the CPU and the card: the picture is asked for again later.
        let fetched = fetch_picture_to(url, &tmp, extra);
        let ok = fetched && !crate::background_io::paused() && accept(&tmp) && std::fs::rename(&tmp, dest).is_ok();
        let _ = std::fs::remove_file(&tmp);
        ok
    })
}

/// A game is starting or running (`set_game_on`, from the launch until the game's exit is handled). Every background
/// download and lookup (covers, pictures, descriptions, the checksums) stops at once, also the first start's own
/// run, which had no game to wait for: a real Brick right after the first start lost sound for 4 chunks at the game's
/// start while the last cover of a run was still being fetched, checked and written to its `sync`-mounted card.
#[cfg(test)]
pub(crate) static READER_TEST: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn set_game_on(on: bool) {
    crate::background_io::set_game_on(on);
}

pub fn game_on() -> bool {
    crate::background_io::game_on()
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
    crate::background_io::read_to_string(file).unwrap_or_default().lines().map(str::to_string).collect()
}

/// What `scrape_until` got through.
pub struct Scraped {
    pub found: usize,
    /// The games it looked up (found or not); the rest were left for later.
    pub looked_up: Vec<PathBuf>,
    /// It gave up because the server didn't answer `GIVE_UP_AFTER` times in a row, not because games have no art.
    pub unreachable: bool,
}

/// Failed requests in a row after which a run stops: a server that is down would otherwise cost a minute per game.
pub const GIVE_UP_AFTER: usize = 3;

/// Downloads missing box art for `games` (system id, ROM path). Games without a match are noted in
/// `covers/<system>/.missing` and skipped next time. `progress(done, total, found)` runs after each game.
pub fn scrape(data_dir: &Path, games: &[(String, PathBuf)], progress: impl Fn(usize, usize, usize)) -> usize {
    scrape_until(data_dir, games, progress, || false).found
}

/// The games `scrape_until` would look up: a known system, no cover on the card.
pub fn todo<'a>(data_dir: &Path, games: &'a [(String, PathBuf)]) -> Vec<&'a (String, PathBuf)> {
    // An empty file gets no picture (a cover would claim the game is fine, `library_cache::is_empty_file`).
    games.iter().take_while(|_| !crate::background_io::cancelled())
        .filter(|(sys, rom)| playlist(sys).is_some() && !crate::background_io::exists(cover_path(data_dir, sys, rom))
            && !crate::fsutil::is_empty_file(rom))
        .collect()
}

/// `scrape`, checking `stop` before each game (a game was started): it ends after the cover it is on.
pub fn scrape_until(data_dir: &Path, games: &[(String, PathBuf)], progress: impl Fn(usize, usize, usize), stop: impl Fn() -> bool) -> Scraped {
    scrape_from(&crate::game_pictures::Libretro, data_dir, games, false, progress, stop)
}

/// `scrape_until` against any `Remote` (replaced in the tests). Only the collection's listing lacking a game marks it
/// as without a cover; a server that doesn't answer, an error page or a broken file leaves it for the next run.
pub(crate) fn scrape_from(
    remote: &dyn crate::game_pictures::Remote,
    data_dir: &Path,
    games: &[(String, PathBuf)],
    retry_missing: bool,
    progress: impl Fn(usize, usize, usize),
    stop: impl Fn() -> bool,
) -> Scraped {
    use crate::game_pictures::{Catalog, Kind, Lookup};
    let todo = todo(data_dir, games);
    let total = todo.len();
    let (mut done, mut found, mut failures) = (0, 0, 0);
    let mut looked_up = Vec::new();
    let mut unreachable = false;
    let mut index = None::<crate::game_pictures::Catalog>;
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
        let missing = crate::background_io::read_to_string(&missing_file).unwrap_or_default();
        if stop() { break; }
        if retry_missing || !missing.lines().any(|l| l == stem) {
            let catalog = Catalog::for_playlist(&mut index, remote, list, Kind::Box);
            match catalog.lookup(remote, stem, names.get(rom).map(String::as_str), &cover_path(data_dir, sys, rom)) {
                Lookup::Got => {
                    failures = 0;
                    found += 1;
                }
                Lookup::Failed | Lookup::Unusable => failures += 1,
                Lookup::Missing => {
                    failures = 0;
                    if !stop() && !missing.lines().any(|line| line == stem) {
                        let _ = crate::fsutil::write_atomic(&missing_file, format!("{missing}{stem}\n").as_bytes());
                    }
                }
            }
        }
        done += 1;
        progress(done, total, found);
        if failures >= GIVE_UP_AFTER {
            unreachable = true;
            break;
        }
    }
    Scraped { found, looked_up, unreachable }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(names: &[&str]) -> Index {
        Index::new(names.iter().map(|s| s.to_string()).collect())
    }

    /// The cover job looks every game of the card up in a collection of thousands of names. Each look-up normalised
    /// every entry again, up to four times (new strings and word sets each time): millions of allocations for a card's
    /// worth of games. With the index made once it is a comparison per entry. Thread CPU.
    #[test]
    fn looking_up_many_games_in_a_big_collection_makes_its_keys_once() {
        let names: Vec<String> = (0..4000).map(|i| format!("Some Game Title {i} - The Subtitle (USA) (Rev {})", i % 3)).collect();
        let idx = Index::new(names);
        let cpu = || {
            let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
            assert_eq!(unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) }, 0);
            std::time::Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
        };
        let t = cpu();
        // No exact name: every look-up goes through the looser tries; the last ones find nothing.
        let found = (0..400).filter(|i| best_match(&format!("Some Game Title {} - The Subtitle (E)", i * 7), &idx).is_some()).count();
        let none = (0..100).filter(|i| best_match(&format!("Unknown Thing Number {i} Deluxe Edition"), &idx).is_none()).count();
        let took = cpu() - t;
        assert_eq!((found, none), (400, 100));
        assert!(took < std::time::Duration::from_millis(2500), "500 look-ups in 4,000 names took {took:?} of CPU");
    }

    #[test]
    fn github_catalog_keeps_the_exact_psp_disc_name_when_the_cdn_directory_is_down() {
        let playlist = playlist("psp").unwrap();
        let name = "Grand Theft Auto - Chinatown Wars (Europe) (En,Fr,De,Es,It)";
        let tree = serde_json::json!({
            "truncated": false,
            "tree": [
                {"path": format!("{name}.png"), "type": "blob"},
                {"path": "notes.txt", "type": "blob"},
                {"path": "folder.png", "type": "tree"},
            ]
        }).to_string();
        let listing = index_from(playlist, "Named_Boxarts", |url| {
            if url == "https://api.github.com/repos/libretro-thumbnails/Sony_-_PlayStation_Portable/git/trees/master:Named_Boxarts" {
                Some(tree.as_bytes().to_vec())
            } else {
                assert_eq!(url, "https://thumbnails.libretro.com/Sony%20-%20PlayStation%20Portable/Named_Boxarts/");
                None
            }
        });
        let chd = format!("{name}.chd");
        let stem = Path::new(&chd).file_stem().unwrap().to_str().unwrap();
        assert_eq!(listing, [name]);
        assert_eq!(best_match(stem, &Index::new(listing)), Some(name));
        assert!(github_index(tree.replace("false", "true").as_bytes()).is_empty());
        assert_eq!(repository("Philips - Videopac+"), "Philips_-_Videopac");
    }

    /// 599 of the 3785 SNES box art files on GitHub are git symlinks: raw.githubusercontent.com answers with the
    /// target's name, which must be followed, not taken for a broken picture (three in a row ended the run).
    fn png_bytes() -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        b.extend(64u32.to_be_bytes());
        b.extend(64u32.to_be_bytes());
        b
    }

    #[test]
    fn a_github_symlink_is_followed_to_the_picture_beside_it() {
        // The download on the card: missing, empty, cut short, flipped, not UTF-8 or too big is no symlink.
        let d = crate::testutil::TestDir::new("symlink-file");
        let file = d.join("B.part");
        let link = "https://raw.githubusercontent.com/libretro-thumbnails/R/master/Named_Boxarts/B.png";
        assert_eq!(symlink_in(link, &file), None, "missing");
        for (case, bytes) in [
            ("empty", b"".to_vec()),
            ("truncated", b"A (US".to_vec()),
            ("flipped", b"gnp.)ASU( A".to_vec()),
            ("encoding", b"A \xff\xfe (USA).png".to_vec()),
            ("a picture", png_bytes()),
            ("too big", format!("{}.png", "A".repeat(600)).into_bytes()),
        ] {
            std::fs::write(&file, &bytes).unwrap();
            assert_eq!(symlink_in(link, &file), None, "{case}");
        }
        std::fs::write(&file, b"A (USA).png").unwrap();
        assert_eq!(symlink_in(link, &file).unwrap(), "https://raw.githubusercontent.com/libretro-thumbnails/R/master/Named_Boxarts/A%20%28USA%29.png");

        let url = "https://raw.githubusercontent.com/libretro-thumbnails/R/master/Named_Boxarts/B%20%28Europe%29.png";
        let base = "https://raw.githubusercontent.com/libretro-thumbnails/R/master";
        assert_eq!(symlink_target(url, b"A (USA).png\n").unwrap(), format!("{base}/Named_Boxarts/A%20%28USA%29.png"));
        assert_eq!(symlink_target(url, b"../Named_Snaps/A.png").unwrap(), format!("{base}/Named_Snaps/A.png"));
        assert_eq!(symlink_target(url, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"), None);
        assert_eq!(symlink_target(url, b"<html>login</html>"), None);
        // Only inside the same repository and ref of GitHub's raw files: an answer can't send the download elsewhere.
        assert_eq!(symlink_target(url, b"../../other/master/A.png"), None, "out of the repository");
        assert_eq!(symlink_target(url, b"../../A.png"), None);
        assert_eq!(symlink_target(url, b"//evil.example/A.png"), None);
        assert_eq!(symlink_target(url, b"x/@evil.example:80/A.png").unwrap(), format!("{base}/Named_Boxarts/x/%40evil.example%3A80/A.png"));
        assert_eq!(symlink_target("https://thumbnails.libretro.com/S/Named_Boxarts/B.png", b"A.png"), None, "the libretro server has no symlinks");
        assert_eq!(symlink_target("https://raw.githubusercontent.com/libretro-thumbnailsX/R/master/N/B.png", b"A.png"), None);
    }

    #[test]
    fn the_github_mirror_is_tried_for_every_kind_when_the_cdn_file_fails() {
        let playlist = playlist("psp").unwrap();
        let name = "Grand Theft Auto - Chinatown Wars (Europe) (En,Fr,De,Es,It)";
        for kind in [crate::game_pictures::Kind::Box, crate::game_pictures::Kind::Snap, crate::game_pictures::Kind::Title] {
            let file = format!("{}/Grand%20Theft%20Auto%20-%20Chinatown%20Wars%20%28Europe%29%20%28En%2CFr%2CDe%2CEs%2CIt%29.png", kind.remote());
            let primary = format!("https://thumbnails.libretro.com/Sony%20-%20PlayStation%20Portable/{file}");
            let mirror = format!("https://raw.githubusercontent.com/libretro-thumbnails/Sony_-_PlayStation_Portable/master/{file}");
            let urls = std::cell::RefCell::new(Vec::new());
            assert!(picture_from(playlist, kind.remote(), name, |url| {
                urls.borrow_mut().push(url.to_string());
                url == mirror
            }));
            assert_eq!(*urls.borrow(), [primary.clone(), mirror]);
            assert!(picture_from(playlist, kind.remote(), name, |url| {
                assert_eq!(url, primary);
                true
            }));
            assert!(!picture_from(playlist, kind.remote(), name, |_| false));
        }
    }

    #[test]
    fn xbox_and_the_other_disc_and_computer_systems_have_a_playlist() {
        assert_eq!(playlist("xbox"), Some("Microsoft - Xbox"));
        assert_eq!(playlist("3ds"), Some("Nintendo - Nintendo 3DS"));
        assert_eq!(playlist("wii"), Some("Nintendo - Wii"));
        assert_eq!(playlist("neogeo"), Some("SNK - Neo Geo"));
        assert_eq!(playlist("zxspectrum"), Some("Sinclair - ZX Spectrum"));
        assert_eq!(playlist("pico-8"), None);
    }

    #[test]
    fn xbox_games_by_their_file_names_find_the_box() {
        // Names as in the libretro "Microsoft - Xbox" collection; the files are .iso and (Redump) .xiso.iso.
        let idx = index(&[
            "Jade Empire (USA)",
            "Jade Empire - Limited Edition (USA)",
            "Tom Clancy's - Splinter Cell (USA)",
            "Tom Clancy's - Splinter Cell - Chaos Theory (USA)",
            "Tom Clancy's - Splinter Cell - Double Agent (USA)",
            "Top Spin Tennis (USA)",
        ]);
        let stem = |file: &str| Path::new(file).file_stem().unwrap().to_str().unwrap().to_string();
        let find = |file: &str| best_match(&stem(file), &idx);
        assert_eq!(find("Jade Empire (USA, Europe) (En,Es,It).iso"), Some("Jade Empire (USA)"));
        assert_eq!(find("Tom Clancy's Splinter Cell - Chaos Theory (USA, Europe) (En,Fr,De,Es,It).xiso.iso"), Some("Tom Clancy's - Splinter Cell - Chaos Theory (USA)"));
        assert_eq!(find("Top Spin (USA) (En,Fr,De).xiso.iso"), Some("Top Spin Tennis (USA)"));
        // The cover is saved under the stem with .xiso, as for every game (`cover_path`).
        let data = Path::new("/d");
        assert_eq!(cover_path(data, "xbox", Path::new("/r/xbox/Top Spin (USA).xiso.iso")), Path::new("/d/covers/xbox/Top Spin (USA).xiso.png"));
    }

    #[test]
    fn a_title_that_starts_another_game_s_name_needs_that_to_be_the_only_one() {
        let idx = index(&["Top Spin Tennis (USA)", "Top Spin 2 (USA)", "Mega Man Zero (USA)", "Mega Man Legends (USA)"]);
        assert_eq!(best_match("Top Spin", &idx), Some("Top Spin Tennis (USA)"));
        assert_eq!(best_match("Mega Man", &idx), None);
        assert_eq!(best_match("Top", &idx), None);
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

    /// New-user test 2026-10-30: a misspelled file name must never end up with another game's box. No cover (and a
    /// clean title card) is better than a wrong one.
    #[test]
    fn an_empty_file_is_not_looked_up_for_a_cover() {
        let dir = crate::testutil::TestDir::new("covers-empty");
        let (empty, full) = (dir.join("Tetris (World).gb"), dir.join("Pokemon Red.gb"));
        std::fs::write(&empty, b"").unwrap();
        std::fs::write(&full, b"x").unwrap();
        let games = vec![("gb".to_string(), empty), ("gb".to_string(), full.clone())];
        let todo: Vec<_> = todo(&dir.join("data"), &games).into_iter().map(|g| g.1.clone()).collect();
        assert_eq!(todo, [full]);
    }

    #[test]
    fn a_misspelled_name_gets_no_cover_rather_than_another_games() {
        let idx = index(&["Advance Wars (USA)", "Metroid Fusion (USA)", "Super Mario Land 2 - 6 Golden Coins (World)", "Mario Land (World)"]);
        assert_eq!(best_match("Metroid - Fusoin (Europe)", &idx), None);
        assert_eq!(best_match("Supar Mario Land 2", &idx), None);
        assert_eq!(best_match("Totally Unknown Game", &idx), None);
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

    /// A made-up collection for the run: what its listing and its downloads answer.
    struct Server {
        listing: Vec<String>,
        answer: crate::game_pictures::Fetched,
        asked: std::cell::Cell<usize>,
    }

    impl crate::game_pictures::Remote for Server {
        fn index(&self, _: &str, _: crate::game_pictures::Kind) -> Vec<String> {
            self.listing.clone()
        }
        fn available(&self, _: &str, _: crate::game_pictures::Kind, _: &str, _: &Path) -> bool {
            self.answer == crate::game_pictures::Fetched::Got
        }
        fn download(&self, _: &str, _: crate::game_pictures::Kind, _: &str, dest: &Path) -> crate::game_pictures::Fetched {
            self.asked.set(self.asked.get() + 1);
            if self.answer == crate::game_pictures::Fetched::Got {
                std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
                std::fs::write(dest, b"png").unwrap();
            }
            self.answer
        }
    }

    fn run(listing: &[&str], answer: crate::game_pictures::Fetched, stems: &[&str]) -> (crate::testutil::TestDir, Server, Scraped) {
        let d = crate::testutil::TestDir::new("cover-retry");
        let games: Vec<(String, PathBuf)> = stems.iter().map(|s| ("gba".to_string(), PathBuf::from(format!("/storage/roms/gba/{s}.gba")))).collect();
        let server = Server { listing: index(listing).names().to_vec(), answer, asked: Default::default() };
        let out = scrape_from(&server, &d, &games, false, |_, _, _| {}, || false);
        (d, server, out)
    }

    fn missing_list(d: &Path) -> String {
        std::fs::read_to_string(d.join("covers/gba/.missing")).unwrap_or_default()
    }

    #[test]
    fn a_server_that_does_not_answer_marks_nothing_as_missing_and_the_run_gives_up() {
        use crate::game_pictures::Fetched::Failed;
        // Unreachable: the listing is empty. Timeout / 500 / partial file: the listing loads, the download fails.
        let (d, server, out) = run(&[], Failed, &["A", "B", "C", "D", "E", "F"]);
        assert_eq!(missing_list(&d), "", "offline is not 'no cover'");
        assert!(out.unreachable && out.looked_up.len() == GIVE_UP_AFTER, "no retry storm: {} looked up", out.looked_up.len());
        assert_eq!(server.asked.get(), 0);
        let (d, server, out) = run(&["A (USA)", "B (USA)", "C (USA)", "D (USA)", "E (USA)"], Failed, &["A (USA)", "B (USA)", "C (USA)", "D (USA)", "E (USA)"]);
        assert_eq!(missing_list(&d), "", "a 500 or a timeout is not 'no cover'");
        assert!(out.unreachable && out.found == 0);
        assert_eq!(server.asked.get(), GIVE_UP_AFTER, "three tries, then it stops asking");
    }

    #[test]
    fn a_listing_without_reachable_files_does_not_rule_out_unlisted_games() {
        let (d, _, out) = run(
            &["Known Game (USA)"],
            crate::game_pictures::Fetched::Failed,
            &["Grand Theft Auto - Chinatown Wars (Europe) (En,Fr,De,Es,It)"],
        );
        assert_eq!(missing_list(&d), "", "the next start must try again");
        assert_eq!(out.found, 0);
    }

    #[test]
    fn a_broken_file_is_asked_for_again_later_not_remembered() {
        let (d, _, out) = run(&["A (USA)"], crate::game_pictures::Fetched::Unusable, &["A (USA)"]);
        assert_eq!(missing_list(&d), "");
        assert!(!out.unreachable, "one bad file is not an outage");
    }

    #[test]
    fn only_a_game_the_loaded_listing_lacks_is_remembered_as_without_a_cover() {
        let (d, _, out) = run(&["A (USA)"], crate::game_pictures::Fetched::Got, &["A (USA)", "Zzzz Qqqq Xxxx (USA)"]);
        assert_eq!(out.found, 1);
        assert_eq!(missing_list(&d), "Zzzz Qqqq Xxxx (USA)\n", "the collection's 404 is remembered");
        assert!(!out.unreachable);
    }
}
