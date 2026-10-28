//! Offline facts about games, from Wikidata (CC0): release year, developer, publisher, genre, series,
//! game mode, number of players, review scores, the English Wikipedia article's name. Built by
//! `scripts/build-gamedb.py` into `assets/gamedb.bin` and compiled into the binary.
//!
//! The file is a short text header and one zlib stream per system, so opening a game card only
//! unpacks the games of its system:
//!
//! ```text
//! PGDB1
//! gb 0 19724
//! nes 19724 41210
//!
//! <zlib><zlib>...
//! ```
//!
//! Each stream holds one game per line, tab-separated in the order of [`FIELDS`]; lists inside a field
//! are separated by `|`, a review score is `<reviewer>=<score>`.
//!
//! Curator (docs/CURATOR.md, layer A) added seven columns at the end: the work a game is a version of,
//! game mechanics, input methods, languages (ISO 639-1), links to remakes and adaptations, and the games
//! it follows and is followed by. Older files simply lack them (the fields stay empty), and older readers
//! ignore them.
//!
//! ROM files are matched to entries by name ([`SystemDb::find`]), with the rules of `covers.rs`
//! (regions and tags dropped, scene release numbers, "Title, The") and a few of its own for the way
//! Wikidata spells titles ("Title: Subtitle", accents, "&" for "and", "III" for "3").

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::rc::Rc;

/// The columns of a line, as the build script writes them.
pub const FIELDS: [&str; 21] = [
    "qid",
    "label",
    "year",
    "developers",
    "publishers",
    "genres",
    "series",
    "modes",
    "players",
    "scores",
    "aliases",
    "enwiki",
    "sitelinks",
    "redump",
    "work",
    "mechanics",
    "inputs",
    "languages",
    "related",
    "follows",
    "followed",
];

/// The data file shipped with pocket-ui.
pub static EMBEDDED: &[u8] = include_bytes!("../assets/gamedb.bin");

const MAGIC: &str = "PGDB1";
/// No system unpacks to more than this; a damaged length can't make us allocate the card full.
const MAX_SECTION: usize = 32 << 20;

/// One game as Wikidata knows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameInfo {
    pub qid: String,
    pub label: String,
    pub year: Option<u16>,
    pub developers: Vec<String>,
    pub publishers: Vec<String>,
    pub genres: Vec<String>,
    pub series: Vec<String>,
    pub modes: Vec<String>,
    /// "1", "1-4".
    pub players: Option<String>,
    /// (reviewer, score as Wikidata has it: "92/100", "8.5/10", "90%").
    pub scores: Vec<(String, String)>,
    pub aliases: Vec<String>,
    /// Title of the English Wikipedia article, for the description fetched on demand.
    pub enwiki: Option<String>,
    /// Number of Wikipedia (and other) sites with an article: how well known the game is.
    pub sitelinks: u32,
    pub redump: Vec<String>,
    /// QID of the work this entry is a version of, when that is another item (an edition item for one
    /// system, a port filed separately); empty when the entry is its own work. Use [`GameInfo::work`].
    pub work_of: Option<String>,
    /// Game mechanics (P4151): "multiple endings", "permadeath" ... (sparse in Wikidata).
    pub mechanics: Vec<String>,
    /// Input methods (P479): "gamepad", "touchscreen", "Wii Remote", "light gun" ...
    pub inputs: Vec<String>,
    /// Languages of the work (P407), ISO 639-1 codes ("en", "ja"), a name where Wikidata has no code.
    pub languages: Vec<String>,
    /// Remakes and adaptations: (how, QID of the other item). These are separate works, never versions.
    pub related: Vec<(Relation, String)>,
    /// QIDs of the games this one follows (P155) and is followed by (P156) in its series.
    pub follows: Vec<String>,
    pub followed_by: Vec<String>,
}

/// How a game is linked to another item that is not a version of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Relation {
    /// This game is a remake or remaster of the other.
    RemakeOf,
    /// The other item is a remake or remaster of this game.
    RemadeAs,
    /// This game is based on the other one but a new work (a fan game, a spin-off built from it).
    BasedOn,
    /// The other item is based on this game.
    SourceOf,
}

impl Relation {
    fn parse(s: &str) -> Option<Relation> {
        Some(match s {
            "remake-of" => Relation::RemakeOf,
            "remade-as" => Relation::RemadeAs,
            "based-on" => Relation::BasedOn,
            "source-of" => Relation::SourceOf,
            _ => return None,
        })
    }
}

impl GameInfo {
    /// The work this game is a version of: versions of one work share it across systems (Chrono Trigger
    /// on the SNES, PlayStation and DS, including the DS edition item, all give Q761815); a remake has its
    /// own. Falls back to the game's own QID.
    pub fn work(&self) -> &str {
        self.work_of.as_deref().unwrap_or(&self.qid)
    }
}

/// The games of one system, with the keys names are matched by.
#[derive(Debug, Default)]
pub struct SystemDb {
    pub games: Vec<GameInfo>,
    /// Whole title (and each alias) -> games.
    full: HashMap<String, Vec<u32>>,
    /// Title before its subtitle ("Crash Bandicoot 2" of "Crash Bandicoot 2: Cortex Strikes Back") -> games.
    main: HashMap<String, Vec<u32>>,
    /// Words of the title, for the last, loosest try.
    words: Vec<BTreeSet<String>>,
    /// How Wikipedia article titles name this system ("Tetris (Game Boy video game)").
    names: &'static [&'static str],
}

/// The data file: its header read, systems unpacked when first asked for.
pub struct GameDb {
    bytes: Cow<'static, [u8]>,
    /// (system, start, length) of each stream in `bytes`.
    sections: Vec<(String, usize, usize)>,
    loaded: HashMap<String, Rc<SystemDb>>,
}

/// ROCKNIX system ids that are another id in the file.
pub(crate) fn canonical(system: &str) -> &str {
    match system {
        "genesis" => "megadrive",
        "famicom" => "nes",
        "sfc" => "snes",
        "tg16" | "supergrafx" => "pcengine",
        "tg16cd" => "pcenginecd",
        "megacd" => "segacd",
        "sg1000" => "sg-1000",
        "n3ds" => "3ds",
        other => other,
    }
}

/// How English Wikipedia names a system in its article titles.
fn system_names(system: &str) -> &'static [&'static str] {
    match system {
        "gb" => &["Game Boy video game"],
        "gbc" => &["Game Boy Color"],
        "gba" => &["Game Boy Advance"],
        "nes" => &["NES", "Nintendo Entertainment System", "Famicom"],
        "fds" => &["Famicom Disk System"],
        "snes" => &["SNES", "Super NES", "Super Nintendo"],
        "n64" => &["Nintendo 64", "N64"],
        "nds" => &["Nintendo DS"],
        "gamecube" => &["GameCube"],
        "wii" => &["Wii video game"],
        "3ds" => &["3DS"],
        "megadrive" => &["Genesis", "Mega Drive"],
        "mastersystem" => &["Master System"],
        "gamegear" => &["Game Gear"],
        "segacd" => &["Sega CD"],
        "sega32x" => &["32X"],
        "saturn" => &["Saturn"],
        "dreamcast" => &["Dreamcast"],
        "psx" => &["PlayStation video game", "PS1"],
        "ps2" => &["PlayStation 2"],
        "psp" => &["PlayStation Portable", "PSP"],
        "pcengine" => &["TurboGrafx", "PC Engine"],
        "atari2600" => &["Atari 2600"],
        "atarilynx" => &["Lynx"],
        _ => &[],
    }
}

/// Where else a ROM of `system` may be filed: Game Boy games in the Game Boy Color folder and so on.
pub(crate) fn related_systems(system: &str) -> &'static [&'static str] {
    match system {
        "gb" => &["gbc"],
        "gbc" => &["gb"],
        "nes" => &["fds"],
        "fds" => &["nes"],
        "ngp" => &["ngpc"],
        "ngpc" => &["ngp"],
        "wonderswan" => &["wonderswancolor"],
        "wonderswancolor" => &["wonderswan"],
        _ => &[],
    }
}

impl GameDb {
    /// Reads the header; the streams stay packed until [`GameDb::system`].
    pub fn parse(bytes: Cow<'static, [u8]>) -> Result<GameDb, String> {
        let end = bytes.windows(2).position(|w| w == b"\n\n").ok_or("no header")?;
        let header = std::str::from_utf8(&bytes[..end]).map_err(|_| "header is not text")?;
        let mut lines = header.lines();
        if lines.next() != Some(MAGIC) {
            return Err(format!("not a {MAGIC} file"));
        }
        let body = end + 2;
        let mut sections = Vec::new();
        for line in lines {
            let mut parts = line.split(' ');
            let (Some(sys), Some(off), Some(len), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
                return Err(format!("bad header line {line:?}"));
            };
            let (off, len): (usize, usize) = (off.parse().map_err(|_| "bad offset")?, len.parse().map_err(|_| "bad length")?);
            let start = body.checked_add(off).ok_or("bad offset")?;
            if start.checked_add(len).is_none_or(|e| e > bytes.len()) {
                return Err(format!("{sys} runs past the end of the file"));
            }
            sections.push((sys.to_string(), start, len));
        }
        Ok(GameDb {
            bytes,
            sections,
            loaded: HashMap::new(),
        })
    }

    /// The file compiled into pocket-ui.
    pub fn embedded() -> GameDb {
        Self::parse(Cow::Borrowed(EMBEDDED)).unwrap_or_else(|e| {
            eprintln!("gamedb: {e}");
            GameDb {
                bytes: Cow::Borrowed(&[]),
                sections: Vec::new(),
                loaded: HashMap::new(),
            }
        })
    }

    /// System ids in the file.
    pub fn systems(&self) -> Vec<&str> {
        self.sections.iter().map(|s| s.0.as_str()).collect()
    }

    /// The games of `system` (ROCKNIX's id; "genesis" finds "megadrive"), unpacked once.
    pub fn system(&mut self, system: &str) -> Option<Rc<SystemDb>> {
        let id = canonical(system);
        if let Some(db) = self.loaded.get(id) {
            return Some(db.clone());
        }
        let &(_, start, len) = self.sections.iter().find(|s| s.0 == id)?;
        let text = match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&self.bytes[start..start + len], MAX_SECTION) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("gamedb: {id} is damaged ({:?})", e.status);
                return None;
            }
        };
        let db = Rc::new(SystemDb::from_text(&String::from_utf8_lossy(&text)).for_system(id));
        self.loaded.insert(id.to_string(), db.clone());
        Some(db)
    }

    /// The entry for a ROM of `system` named `stem`, looking into related systems (Game Boy and Game Boy
    /// Color, ...) when the system itself has none.
    pub fn lookup(&mut self, system: &str, stem: &str) -> Option<GameInfo> {
        let id = canonical(system);
        std::iter::once(id)
            .chain(related_systems(id).iter().copied())
            .find_map(|s| self.system(s)?.find(stem).cloned())
    }

    /// The entry `qid` among the games of `system` and its related systems.
    pub fn get(&mut self, system: &str, qid: &str) -> Option<GameInfo> {
        let id = canonical(system);
        std::iter::once(id)
            .chain(related_systems(id).iter().copied())
            .find_map(|s| self.system(s)?.games.iter().find(|g| g.qid == qid).cloned())
    }

    /// The entry for a ROM, with the player's choice (from library.json) before the automatic match.
    pub fn resolve(&mut self, system: &str, stem: &str, choice: Option<&Choice>) -> Option<GameInfo> {
        match choice {
            Some(Choice::NoEntry) => None,
            // An entry a newer data file dropped: the automatic match rather than nothing.
            Some(Choice::Entry(qid)) => self.get(system, qid).or_else(|| self.lookup(system, stem)),
            None => self.lookup(system, stem),
        }
    }

    /// [`resolve`](Self::resolve) for a ROM its checksum named (`rom_id`, the No-Intro name): that name first, the
    /// file's own after it.
    pub fn resolve_rom(&mut self, system: &str, stem: &str, identified: Option<&str>, choice: Option<&Choice>) -> Option<GameInfo> {
        match choice {
            Some(Choice::NoEntry) => None,
            Some(Choice::Entry(qid)) => self.get(system, qid).or_else(|| self.lookup_rom(system, stem, identified)),
            None => self.lookup_rom(system, stem, identified),
        }
    }

    /// [`lookup`](Self::lookup) under the checksum's name first, then the file's.
    pub fn lookup_rom(&mut self, system: &str, stem: &str, identified: Option<&str>) -> Option<GameInfo> {
        identified.and_then(|n| self.lookup(system, n)).or_else(|| self.lookup(system, stem))
    }

    /// The `n` entries of `system` (and its related systems) whose names are closest to the ROM's, closest first.
    pub fn candidates(&mut self, system: &str, stem: &str, n: usize) -> Vec<GameInfo> {
        let wanted = bigrams(&rom_words(stem).join(" "));
        let id = canonical(system);
        let mut scored: Vec<(f32, GameInfo)> = Vec::new();
        for s in std::iter::once(id).chain(related_systems(id).iter().copied()) {
            let Some(db) = self.system(s) else { continue };
            for g in &db.games {
                if scored.iter().any(|(_, x)| x.qid == g.qid) {
                    continue;
                }
                let score = std::iter::once(&g.label)
                    .chain(&g.aliases)
                    .map(|name| dice(&wanted, &bigrams(&words(name).join(" "))))
                    .fold(0.0, f32::max);
                scored.push((score, g.clone()));
            }
        }
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.sitelinks.cmp(&a.1.sitelinks)));
        scored.into_iter().take(n).map(|(_, g)| g).collect()
    }
}

/// The player's answer to "Wrong game? Choose another", kept per ROM in library.json.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Choice {
    /// This Wikidata entry (its QID).
    Entry(String),
    /// "None of these": no facts for this game.
    NoEntry,
}

fn list(field: &str) -> Vec<String> {
    field.split('|').filter(|s| !s.is_empty()).map(str::to_string).collect()
}

fn some(field: &str) -> Option<String> {
    (!field.is_empty()).then(|| field.to_string())
}

/// "é" -> "e" and the like, for the letters game titles use.
fn fold(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => 'a',
        'ç' => 'c',
        'è' | 'é' | 'ê' | 'ë' | 'ē' => 'e',
        'ì' | 'í' | 'î' | 'ï' | 'ī' => 'i',
        'ñ' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => 'o',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => 'u',
        'ý' | 'ÿ' => 'y',
        other => other,
    }
}

/// The words of a title for matching: lowercase without accents, (…) and […] dropped, "&" is "and",
/// apostrophes join ("Link's" -> "links"), Roman numbers are numbers, a leading article goes.
fn words(name: &str) -> Vec<String> {
    let mut plain = String::new();
    let mut depth = 0;
    for ch in name.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth > 0 => {}
            '&' => plain.push_str(" and "),
            '\'' | '’' | '.' => {}
            _ => plain.extend(ch.to_lowercase().map(fold)),
        }
    }
    let mut out: Vec<String> = plain
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let n = ["ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x"].iter().position(|r| *r == w);
            n.map_or_else(|| w.to_string(), |n| (n + 2).to_string())
        })
        .collect();
    if out.len() > 1 && ["the", "a", "an"].contains(&out[0].as_str()) {
        out.remove(0);
    }
    out
}

/// The words of a ROM file's name as the matching sees them (release number, tags and ", The" dealt with).
fn rom_words(stem: &str) -> Vec<String> {
    let stem = crate::library::strip_inner_ext(stem);
    words(&crate::library::display_name(crate::covers::strip_release_number(stem)))
}

/// Letter pairs of a name, for how alike two names are.
fn bigrams(s: &str) -> Vec<(char, char)> {
    let chars: Vec<char> = s.chars().collect();
    let mut b: Vec<(char, char)> = chars.windows(2).map(|w| (w[0], w[1])).collect();
    b.sort_unstable();
    b
}

/// Dice coefficient of two sorted bigram lists: 1.0 for the same name, 0.0 for nothing in common.
fn dice(a: &[(char, char)], b: &[(char, char)]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (mut i, mut j, mut common) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Equal => {
                common += 1;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    2.0 * common as f32 / (a.len() + b.len()) as f32
}

/// Words that say nothing about which game it is: "Pokemon - Red Version" is "Pokémon Red".
fn filler(word: &str) -> bool {
    ["version", "edition"].contains(&word)
}

fn key(words: &[String]) -> String {
    words.concat()
}

/// The key without filler words.
fn lean_key(words: &[String]) -> String {
    words.iter().filter(|w| !filler(w)).map(String::as_str).collect()
}

/// A Wikidata title split at its subtitle: "Mario Kart: Double Dash" -> ["Mario Kart", "Double Dash"].
fn title_parts(title: &str) -> Vec<&str> {
    title
        .split([':', '–'])
        .flat_map(|p| p.split(" - "))
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

impl SystemDb {
    /// Parses the text of one system.
    pub fn from_text(text: &str) -> SystemDb {
        let mut db = SystemDb::default();
        for line in text.lines() {
            let mut f: Vec<&str> = line.split('\t').collect();
            if f.len() < 2 || f[1].is_empty() {
                continue;
            }
            f.resize(FIELDS.len().max(f.len()), "");
            let scores = list(f[9])
                .iter()
                .filter_map(|s| s.split_once('='))
                .map(|(by, v)| (by.to_string(), v.to_string()))
                .collect();
            db.games.push(GameInfo {
                qid: f[0].to_string(),
                label: f[1].to_string(),
                year: f[2].parse().ok(),
                developers: list(f[3]),
                publishers: list(f[4]),
                genres: list(f[5]),
                series: list(f[6]),
                modes: list(f[7]),
                players: some(f[8]),
                scores,
                aliases: list(f[10]),
                enwiki: some(f[11]),
                sitelinks: f[12].parse().unwrap_or(0),
                redump: list(f[13]),
                work_of: some(f[14]),
                mechanics: list(f[15]),
                inputs: list(f[16]),
                languages: list(f[17]),
                related: list(f[18])
                    .iter()
                    .filter_map(|r| r.split_once(':'))
                    .filter_map(|(how, q)| Some((Relation::parse(how)?, q.to_string())))
                    .collect(),
                follows: list(f[19]),
                followed_by: list(f[20]),
            });
        }
        for (i, g) in db.games.iter().enumerate() {
            let i = i as u32;
            for name in std::iter::once(&g.label).chain(&g.aliases) {
                let w = words(name);
                if w.is_empty() {
                    continue;
                }
                for k in [key(&w), lean_key(&w)] {
                    let e = db.full.entry(k).or_default();
                    if !e.contains(&i) {
                        e.push(i);
                    }
                }
                let parts = title_parts(name);
                if parts.len() > 1 {
                    let mk = key(&words(parts[0]));
                    let e = db.main.entry(mk).or_default();
                    if !e.contains(&i) {
                        e.push(i);
                    }
                }
            }
            db.words.push(words(&g.label).into_iter().filter(|w| !filler(w)).collect());
        }
        db
    }

    /// The games are this system's: same-named entries are told apart by their article's title.
    pub fn for_system(mut self, system: &str) -> Self {
        self.names = system_names(canonical(system));
        self
    }

    /// Of several entries, the one whose Wikipedia article is about this system's version ("Tetris (Game
    /// Boy video game)" before the 1985 original that lists the Game Boy too), else the best known.
    fn best(&self, ids: &[u32]) -> Option<&GameInfo> {
        let about_this = |g: &GameInfo| g.enwiki.as_deref().is_some_and(|a| self.names.iter().any(|n| a.contains(n)));
        ids.iter().map(|&i| &self.games[i as usize]).max_by_key(|g| (about_this(g), g.sitelinks))
    }

    /// The single game behind `ids`, or the best known when one clearly is (twice the sitelinks).
    fn one(&self, ids: &[u32]) -> Option<&GameInfo> {
        let mut games: Vec<&GameInfo> = ids.iter().map(|&i| &self.games[i as usize]).collect();
        games.sort_by_key(|g| std::cmp::Reverse(g.sitelinks));
        games.dedup_by_key(|g| g.qid.clone());
        match games.as_slice() {
            [only] => Some(only),
            [first, second, ..] if first.sitelinks >= 2 * second.sitelinks.max(1) => Some(first),
            _ => None,
        }
    }

    /// The entry for a ROM file name without its extension. Tries, in order: the whole title (or an
    /// alias), the title without "Version"/"Edition", the ROM's first one or two parts ("Pokemon -
    /// Yellow Version - Special Pikachu Edition"), a ROM without the subtitle Wikidata has, and finally
    /// the same words with one more in the ROM's name ("Donkey Kong 1 - Country"), never a sequel number.
    pub fn find(&self, stem: &str) -> Option<&GameInfo> {
        let stem = crate::library::strip_inner_ext(stem);
        let name = crate::library::display_name(crate::covers::strip_release_number(stem));
        let segments: Vec<&str> = name.split(" - ").collect();
        let w = words(&name);
        if w.is_empty() {
            return None;
        }
        for k in [key(&w), lean_key(&w)] {
            if let Some(ids) = self.full.get(&k) {
                return self.best(ids);
            }
        }
        // The first two parts, then the first: "Pokemon - Yellow Version - ...", "Batman - The Video Game".
        for n in (1..segments.len()).rev().take(2) {
            let part = words(&segments[..n].join(" "));
            if part.len() < 2 && n == 1 && part.first().is_none_or(|p| p.len() < 5) {
                continue;
            }
            if let Some(g) = self.full.get(&lean_key(&part)).and_then(|ids| self.one(ids)) {
                return Some(g);
            }
        }
        // "Crash Bandicoot 2" for "Crash Bandicoot 2: Cortex Strikes Back"; only a numbered title, since a
        // bare series name ("Bomberman") is not "Bomberman: Panic Bomber".
        if w.iter().any(|x| x.bytes().all(|b| b.is_ascii_digit())) {
            if let Some(g) = self.main.get(&key(&w)).and_then(|ids| self.one(ids)) {
                return Some(g);
            }
        }
        // One word more or less than an entry; the word may not be a number of a sequel.
        let have: BTreeSet<String> = w.into_iter().filter(|x| !filler(x)).collect();
        if have.len() < 2 {
            return None;
        }
        let close: Vec<u32> = self
            .words
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.len() >= 2 && {
                    let (small, big) = if e.len() <= have.len() { (*e, &have) } else { (&have, *e) };
                    big.len() - small.len() == 1 && small.is_subset(big) && !big.difference(small).any(|x| crate::covers::sequel_word(x))
                }
            })
            .map(|(i, _)| i as u32)
            .collect();
        self.one(&close)
    }
}

/// Writes a data file like the build script does (for tests and tools).
pub fn encode(sections: &[(&str, &str)]) -> Vec<u8> {
    let blobs: Vec<Vec<u8>> = sections
        .iter()
        .map(|(_, text)| miniz_oxide::deflate::compress_to_vec_zlib(text.as_bytes(), 9))
        .collect();
    let mut header = String::from(MAGIC);
    let mut off = 0;
    for ((sys, _), blob) in sections.iter().zip(&blobs) {
        header.push_str(&format!("\n{sys} {off} {}", blob.len()));
        off += blob.len();
    }
    header.push_str("\n\n");
    let mut out = header.into_bytes();
    blobs.iter().for_each(|b| out.extend_from_slice(b));
    out
}

/// A game in the user's library, for [`related`].
#[derive(Clone, Debug, PartialEq)]
pub struct Owned<'a> {
    pub system: &'a str,
    pub path: &'a Path,
    pub name: &'a str,
    pub info: Option<&'a GameInfo>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Related<'a> {
    pub by_developer: Option<(String, Vec<Owned<'a>>)>,
    pub in_series: Option<(String, Vec<Owned<'a>>)>,
}

/// Owned games other than `this` whose `field` holds `value`, oldest first, one file per game.
fn owned_with<'a>(this: &GameInfo, this_path: &Path, library: &[Owned<'a>], has: impl Fn(&GameInfo) -> bool) -> Vec<Owned<'a>> {
    let mut out: Vec<Owned<'a>> = library
        .iter()
        .filter(|o| o.path != this_path && o.info.is_some_and(|i| i.qid != this.qid && has(i)))
        .cloned()
        .collect();
    out.sort_by_key(|o| (o.info.and_then(|i| i.year), o.name.to_lowercase()));
    // A game kept as .cue and .bin, or in two systems, is listed once.
    let mut seen = BTreeSet::new();
    out.retain(|o| seen.insert(o.info.map(|i| i.qid.clone())));
    out
}

/// Games of the library that share something with `this`: (developer, games) and (series, games).
/// Only games the user has, never a catalogue; `this` itself is left out.
pub fn related<'a>(this: &GameInfo, this_path: &Path, library: &[Owned<'a>]) -> Related<'a> {
    let first = |values: &[String], field: fn(&GameInfo) -> &[String], skip: &BTreeSet<&str>| {
        values.iter().find_map(|v| {
            let games = owned_with(this, this_path, library, |i| field(i).contains(v) && !skip.contains(i.qid.as_str()));
            (!games.is_empty()).then(|| (v.clone(), games))
        })
    };
    let in_series = first(&this.series, |i| &i.series, &BTreeSet::new());
    // The series is the closer link: a game listed there is not listed again under the developer.
    let listed: BTreeSet<&str> = in_series
        .iter()
        .flat_map(|(_, g)| g.iter().filter_map(|o| o.info.map(|i| i.qid.as_str())))
        .collect();
    Related {
        by_developer: first(&this.developers, |i| &i.developers, &listed),
        in_series,
    }
}

/// "Metacritic 92", "Famitsu 35/40", "GameRankings 90%": a score as the card shows it.
pub fn score_text(by: &str, score: &str) -> String {
    let score = score.strip_suffix("/100").unwrap_or(score);
    format!("{by} {score}")
}

/// The scores worth showing, best known reviewers first, at most `max`.
pub fn top_scores(info: &GameInfo, max: usize) -> Vec<String> {
    let order = ["Metacritic", "GameRankings", "OpenCritic", "Famitsu"];
    let mut s: Vec<&(String, String)> = info.scores.iter().collect();
    s.sort_by_key(|(by, _)| order.iter().position(|o| o == by).unwrap_or(order.len()));
    s.into_iter().take(max).map(|(by, v)| score_text(by, v)).collect()
}

/// "1 player", "1–4 players", or from the game modes "Single-player", "Multiplayer".
pub fn players_text(info: &GameInfo) -> Option<String> {
    if let Some(p) = &info.players {
        return Some(match p.split_once('-') {
            Some((a, b)) => format!("{a}–{b} players"),
            None if p == "1" => "1 player".to_string(),
            None => format!("{p} players"),
        });
    }
    // In this order, whatever order the file has them in.
    let known = [
        ("single-player video game", "Single-player"),
        ("cooperative video game", "Co-op"),
        ("multiplayer video game", "Multiplayer"),
    ];
    let modes: Vec<&str> = known
        .iter()
        .filter(|(m, _)| info.modes.iter().any(|x| x == m))
        .map(|(_, shown)| *shown)
        .collect();
    (!modes.is_empty()).then(|| modes.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use std::path::Path;

    const GB: &str = "Q596392\tSuper Mario Land 2: 6 Golden Coins\t1992\tNintendo Research & Development 1\tNintendo\t2D platform game\tSuper Mario Land\tsingle-player video game\t1\tMetacritic=80/100\tSML2|Super Mario Land 2\tSuper Mario Land 2: 6 Golden Coins\t30\t\n\
Q3519191\tTetris\t1989\tNintendo Research & Development 1\tNintendo\tpuzzle video game\t\tmultiplayer video game|single-player video game\t1-2\tGen4=10/10\tGame Boy Tetris\tTetris (Game Boy video game)\t12\t\n";
    const NES: &str =
        "Q11168\tSuper Mario Bros.\t1985\tNintendo EAD\tNintendo\tplatform game\tSuper Mario\tsingle-player video game\t1-2\t\t\tSuper Mario Bros.\t90\t\n";

    fn db() -> GameDb {
        GameDb::parse(Cow::Owned(encode(&[("gb", GB), ("nes", NES)]))).expect("a file written by encode reads back")
    }

    #[test]
    fn the_loader_reads_every_field_of_a_line() {
        let mut db = db();
        assert_eq!(db.systems(), ["gb", "nes"]);
        let gb = db.system("gb").expect("gb section");
        assert_eq!(gb.games.len(), 2);
        let sml2 = gb.games.iter().find(|g| g.qid == "Q596392").unwrap();
        assert_eq!(sml2.label, "Super Mario Land 2: 6 Golden Coins");
        assert_eq!(sml2.year, Some(1992));
        assert_eq!(sml2.developers, ["Nintendo Research & Development 1"]);
        assert_eq!(sml2.publishers, ["Nintendo"]);
        assert_eq!(sml2.genres, ["2D platform game"]);
        assert_eq!(sml2.series, ["Super Mario Land"]);
        assert_eq!(sml2.players.as_deref(), Some("1"));
        assert_eq!(sml2.scores, [("Metacritic".to_string(), "80/100".to_string())]);
        assert_eq!(sml2.aliases, ["SML2", "Super Mario Land 2"]);
        assert_eq!(sml2.enwiki.as_deref(), Some("Super Mario Land 2: 6 Golden Coins"));
        assert_eq!(sml2.sitelinks, 30);
        let tetris = gb.games.iter().find(|g| g.label == "Tetris").unwrap();
        assert_eq!(tetris.modes, ["multiplayer video game", "single-player video game"]);
        assert!(tetris.series.is_empty() && tetris.redump.is_empty());
        assert_eq!(db.system("nes").unwrap().games[0].label, "Super Mario Bros.");
        assert!(db.system("psx").is_none(), "a system the file doesn't have");
    }

    #[test]
    fn the_loader_knows_rocknix_aliases_and_unpacks_a_system_once() {
        let mut db = GameDb::parse(Cow::Owned(encode(&[("megadrive", NES)]))).unwrap();
        let a = db.system("genesis").expect("genesis is the Mega Drive");
        let b = db.system("megadrive").unwrap();
        assert!(std::rc::Rc::ptr_eq(&a, &b), "the same unpacked list");
    }

    #[test]
    fn a_damaged_file_is_an_error_or_an_empty_system_never_a_panic() {
        assert!(GameDb::parse(Cow::Borrowed(b"")).is_err());
        assert!(GameDb::parse(Cow::Borrowed(b"PGDB9\n\n")).is_err(), "unknown version");
        assert!(GameDb::parse(Cow::Borrowed(b"PGDB1\ngb 0 99999\n\nxx")).is_err(), "section past the end");
        // A stream that isn't zlib: the system is missing, the rest still works.
        let mut good = encode(&[("gb", GB)]);
        let at = good.len() - 10;
        good[at..].fill(0xff);
        let mut db = GameDb::parse(Cow::Owned(good)).unwrap();
        assert!(db.system("gb").is_none());
        // Short lines are padded, extra columns ignored.
        let long = format!("Q2\tLong\t2000{}\textra\n", "\t".repeat(FIELDS.len() - 3));
        let s = SystemDb::from_text(&format!("Q1\tShort\n\n{long}"));
        assert_eq!(s.games.len(), 2);
        assert_eq!(s.games[1].year, Some(2000));
        assert_eq!(s.games[0].work(), "Q1", "a short line is its own work");
        assert!(s.games[1].followed_by.is_empty() && s.games[1].work_of.is_none());
    }

    #[test]
    fn the_curator_columns_read_back_and_old_lines_still_load() {
        // A line as the build script writes it since 2026-10-23: 14 old columns, then work, mechanics,
        // inputs, languages, related, follows, followed by.
        let new = "Q507828\tChrono Trigger\t2008\tSquare Enix\t\trole-playing video game\tChrono\t\t1\t\t\t\t2\t\t\
Q761815\tmultiple endings\tgamepad|touchscreen\ten|ja\tremade-as:Q9|based-on:Q8|nonsense:Q7|broken\t\tQ1927695\n";
        let old = "Q761815\tChrono Trigger\t1995\tSquare\t\trole-playing video game\tChrono\t\t1\t\t\tChrono Trigger\t38\t\n";
        let s = SystemDb::from_text(&format!("{new}{old}"));
        let ds = &s.games[0];
        assert_eq!(ds.work(), "Q761815", "the DS edition item is a version of the SNES game's work");
        assert_eq!(ds.mechanics, ["multiple endings"]);
        assert_eq!(ds.inputs, ["gamepad", "touchscreen"]);
        assert_eq!(ds.languages, ["en", "ja"]);
        assert_eq!(
            ds.related,
            [(Relation::RemadeAs, "Q9".to_string()), (Relation::BasedOn, "Q8".to_string())],
            "unknown kinds and broken entries are left out"
        );
        assert!(ds.follows.is_empty());
        assert_eq!(ds.followed_by, ["Q1927695"]);
        let snes = &s.games[1];
        assert_eq!(snes.work(), "Q761815", "an old line is its own work");
        assert!(snes.mechanics.is_empty() && snes.related.is_empty() && snes.languages.is_empty());
    }

    #[test]
    fn the_shipped_file_groups_versions_by_work() {
        let mut db = GameDb::embedded();
        let work = |db: &mut GameDb, system: &str, qid: &str| db.get(system, qid).map(|g| g.work().to_string());
        // Chrono Trigger: the same item on the SNES, PlayStation, DS, Wii and PSP, and a DS edition item.
        for system in ["snes", "psx", "nds", "wii", "psp"] {
            assert_eq!(work(&mut db, system, "Q761815").as_deref(), Some("Q761815"), "{system}");
        }
        assert_eq!(work(&mut db, "nds", "Q507828").as_deref(), Some("Q761815"), "the DS edition item");
        // A fan game built on it is a work of its own.
        let echoes = db.system("snes").unwrap().games.iter().find(|g| g.label.contains("Crimson Echoes")).cloned();
        if let Some(e) = echoes {
            assert_ne!(e.work(), "Q761815", "Crimson Echoes is not a version of Chrono Trigger");
        }
        // A remake points at its original and stays separate.
        let oot3d = db.system("3ds").unwrap().games.iter().find(|g| g.label == "The Legend of Zelda: Ocarina of Time 3D").cloned();
        let oot3d = oot3d.expect("Ocarina of Time 3D on the 3DS");
        assert_ne!(oot3d.work(), "Q213911");
        assert!(oot3d.related.contains(&(Relation::RemakeOf, "Q213911".to_string())), "{:?}", oot3d.related);
        // A film tie-in is not a version of the film.
        let ts = db.lookup("megadrive", "Toy Story (USA)").expect("Toy Story");
        assert_eq!(ts.work(), ts.qid, "the game is its own work, not the film's");
        // Series order.
        let ct = db.get("snes", "Q761815").unwrap();
        assert!(ct.followed_by.iter().any(|q| q == "Q1927695"), "followed by Radical Dreamers: {:?}", ct.followed_by);
    }

    #[test]
    fn the_shipped_file_loads_and_knows_the_well_known_games() {
        let mut db = GameDb::embedded();
        for system in ["gb", "gbc", "gba", "nes", "snes", "n64", "megadrive", "psx", "ps2", "psp", "nds"] {
            let s = db.system(system).unwrap_or_else(|| panic!("{system} missing from assets/gamedb.bin"));
            assert!(s.games.len() > 100, "{system}: only {} games", s.games.len());
        }
        let sml2 = db.lookup("gb", "Super Mario Land 2 - 6 Golden Coins (USA, Europe) (Rev 2)").expect("SML2");
        assert_eq!(sml2.qid, "Q596392");
        assert_eq!(sml2.year, Some(1992));
        assert!(sml2.developers.iter().any(|d| d.contains("Nintendo")), "{:?}", sml2.developers);
        assert!(sml2.series.iter().any(|s| s == "Super Mario Land"), "{:?}", sml2.series);
        // The Game Boy's own Tetris, not the 1985 original (which lists the Game Boy too and is better known).
        assert_eq!(db.lookup("gb", "Tetris (World) (Rev 1)").map(|g| g.qid).as_deref(), Some("Q3519191"));
        // No automatic match for the PSP "Bomberman": the choices offered are Bomberman games.
        let offered: Vec<String> = db.candidates("psp", "Bomberman (US)", 5).into_iter().map(|g| g.label).collect();
        println!("Bomberman (US) on the PSP: {offered:?}");
        assert!(offered.iter().take(3).all(|l| l.contains("Bomberman")), "{offered:?}");
    }

    /// Lines of a `system/file` list as (system, stem).
    fn names(list: &str) -> Vec<(&str, &str)> {
        list.lines()
            .filter_map(|l| l.split_once('/'))
            .map(|(sys, file)| (sys, file.rsplit_once('.').map_or(file, |(stem, _)| stem)))
            .collect()
    }

    #[test]
    fn real_rom_names_match_the_right_games() {
        let mut db = GameDb::embedded();
        let list = include_str!("testdata/roms-real.txt");
        let expect: &[(&str, Option<&str>)] = &[
            (
                "Super Mario Land 2 - 6 Golden Coins (USA, Europe) (Rev 2)",
                Some("Super Mario Land 2: 6 Golden Coins"),
            ),
            ("Donkey Kong 1 - Country", Some("Donkey Kong Country")),
            ("Contra III - The Alien Wars", Some("Contra III: The Alien Wars")),
            ("Crash Bandicoot 2", Some("Crash Bandicoot 2: Cortex Strikes Back")),
            ("Mario Kart 64 (USA)", Some("Mario Kart 64")),
            ("Harvest Moon 64 (USA)", Some("Harvest Moon 64")),
            ("New Super Mario Bros. (USA, Australia)", Some("New Super Mario Bros.")),
            ("Ico (USA)", Some("Ico")),
            ("Burnout 3 - Takedown (USA)", Some("Burnout 3: Takedown")),
            ("Mario Kart - Double Dash!! (USA).nkit", Some("Mario Kart: Double Dash")),
            ("Sonic Adventure 2 (Europe)[!]", Some("Sonic Adventure 2")),
            ("0902 - MegaMan & Bass (U)(Rev A)", Some("Mega Man & Bass")),
            ("Final Fantasy VI Advance [Restored]", Some("Final Fantasy VI")),
            // Test ROMs, memory cards: nothing, rather than a wrong game.
            ("a2600-test", None),
            ("gg-test", None),
            ("sms-test", None),
            ("Mcd001", None),
            ("Mcd002", None),
        ];
        let mut found = 0;
        let all = names(list);
        for (sys, stem) in &all {
            let hit = db.lookup(sys, stem);
            println!(
                "{sys:12} {stem:60} -> {}",
                hit.as_ref().map_or("-".to_string(), |g| format!("{} ({})", g.label, g.qid))
            );
            found += hit.is_some() as usize;
            if let Some((_, want)) = expect.iter().find(|(s, _)| s == stem) {
                assert_eq!(hit.map(|g| g.label).as_deref(), *want, "{sys}/{stem}");
            }
        }
        println!("real list: {found} of {} file names matched", all.len());
    }

    #[test]
    fn well_known_no_intro_names_match_nine_in_ten() {
        let mut db = GameDb::embedded();
        let all = names(include_str!("testdata/nointro-well-known.txt"));
        let mut missed = Vec::new();
        for (sys, stem) in &all {
            match db.lookup(sys, stem) {
                Some(g) => println!("{sys:12} {stem:70} -> {} ({})", g.label, g.qid),
                None => missed.push(format!("{sys}/{stem}")),
            }
        }
        let rate = 100.0 * (all.len() - missed.len()) as f32 / all.len() as f32;
        println!(
            "well-known list: {} of {} matched ({rate:.1} %); missed:\n  {}",
            all.len() - missed.len(),
            all.len(),
            missed.join("\n  ")
        );
        assert!(all.len() >= 200);
        assert!(rate >= 90.0, "{rate:.1} %");
    }

    #[test]
    fn same_named_games_and_subtitles_are_told_apart() {
        let s = SystemDb::from_text(
            "Q1\tPokémon Red and Blue\t1996\t\t\t\tPokémon\t\t\t\tPokémon Red|Pokémon Blue|Pokemon Red Version\t\t80\t\n\
Q2\tPokémon Yellow\t1998\t\t\t\tPokémon\t\t\t\t\t\t60\t\n\
Q3\tThe Legend of Zelda: Link's Awakening\t1993\t\t\t\t\t\t\t\t\t\t70\t\n\
Q4\tTetris\t1989\t\t\t\t\t\t\t\t\t\t12\t\n\
Q5\tTetris\t1989\t\t\t\t\t\t\t\t\t\t3\t\n\
Q6\tCrash Bandicoot 2: Cortex Strikes Back\t1997\t\t\t\t\t\t\t\t\t\t40\t\n\
Q7\tCrash Bandicoot\t1996\t\t\t\t\t\t\t\t\t\t50\t\n\
Q8\tFinal Fantasy III\t1990\t\t\t\t\t\t\t\t\t\t50\t\n\
Q9\tBomberman: Panic Bomber\t1995\t\t\t\t\t\t\t\t\t\t9\t\n",
        );
        let q = |stem: &str| s.find(stem).map(|g| g.qid.as_str());
        assert_eq!(q("Pokemon - Red Version (USA, Europe) (SGB Enhanced)"), Some("Q1"), "alias, accent folded");
        assert_eq!(q("Pokemon - Yellow Version - Special Pikachu Edition (USA, Europe)"), Some("Q2"));
        assert_eq!(
            q("Legend of Zelda, The - Link's Awakening (USA, Europe) (Rev 2)"),
            Some("Q3"),
            "the article moves"
        );
        assert_eq!(q("Tetris (World) (Rev 1)"), Some("Q4"), "the better known of two");
        assert_eq!(q("Crash Bandicoot 2"), Some("Q6"), "subtitle left off");
        assert_eq!(q("Crash Bandicoot 3"), None, "never a different number");
        assert_eq!(q("Final Fantasy II"), None);
        assert_eq!(q("Final Fantasy 3"), Some("Q8"), "roman and arabic numbers are the same");
        assert_eq!(q("Mcd001"), None);
        // A series name alone is not one of its games: only a numbered title may leave its subtitle off.
        assert_eq!(q("Bomberman (US)"), None);
        // Among same-named games the one whose article is about this system's version wins, before fame.
        let gb = SystemDb::from_text(
            "Q71910\tTetris\t1985\t\t\t\t\t\t\t\t\tTetris\t120\t\nQ3519191\tTetris\t1989\t\t\t\t\t\t\t\t\tTetris (Game Boy video game)\t12\t\n",
        )
        .for_system("gb");
        assert_eq!(gb.find("Tetris (World) (Rev 1)").map(|g| g.qid.as_str()), Some("Q3519191"));
    }

    const KONG: &str = "Q518987\tDonkey Kong Country\t1994\tRare\t\t\tDonkey Kong\t\t\t\t\tDonkey Kong Country\t60\t\n\
Q1072051\tDonkey Kong Country 2: Diddy's Kong Quest\t1995\tRare\t\t\tDonkey Kong\t\t\t\t\t\t40\t\n\
Q1072052\tDonkey Kong Country 3: Dixie Kong's Double Trouble!\t1996\tRare\t\t\tDonkey Kong\t\t\t\t\t\t35\t\n\
Q2564685\tDonkey Kong Jr.\t1982\t\t\t\t\t\t\t\t\t\t30\t\n\
Q1061560\tSuper Mario Kart\t1992\t\t\t\t\t\t\t\t\t\t70\t\n\
Q1800173\tSecret of Mana\t1993\t\t\t\t\t\t\t\tSeiken Densetsu 2\t\t50\t\n";

    fn kong_db() -> GameDb {
        GameDb::parse(Cow::Owned(encode(&[("snes", KONG), ("gb", GB)]))).unwrap()
    }

    #[test]
    fn candidates_are_the_closest_names_of_the_system_closest_first() {
        let mut db = kong_db();
        let c: Vec<String> = db.candidates("snes", "Donkey Kong 1 - Country", 4).into_iter().map(|g| g.label).collect();
        assert_eq!(c.len(), 4);
        assert_eq!(c[0], "Donkey Kong Country");
        assert!(
            c[1..].iter().all(|l| l.starts_with("Donkey Kong")),
            "the Donkey Kong games before the rest: {c:?}"
        );
        // An alias counts as a name: a Japanese title finds the game.
        assert_eq!(db.candidates("snes", "Seiken Densetsu 2 (Japan)", 1)[0].label, "Secret of Mana");
        // Only this system's games (and its related systems'): no Game Boy Tetris for a Super Nintendo ROM.
        assert!(db.candidates("snes", "Tetris", 10).iter().all(|g| g.label != "Tetris"));
        assert_eq!(db.candidates("snes", "Tetris", 10).len(), 6, "short lists stay whole");
    }

    #[test]
    fn the_players_choice_beats_the_automatic_match() {
        let mut db = kong_db();
        let stem = "Donkey Kong 1 - Country";
        assert_eq!(db.resolve("snes", stem, None).map(|g| g.qid).as_deref(), Some("Q518987"), "automatic");
        let pick = Choice::Entry("Q1072051".into());
        assert_eq!(db.resolve("snes", stem, Some(&pick)).map(|g| g.qid).as_deref(), Some("Q1072051"));
        assert_eq!(db.resolve("snes", stem, Some(&Choice::NoEntry)), None, "None of these: no facts");
        // A choice the data file no longer has: back to the automatic match rather than nothing.
        let gone = Choice::Entry("Q999999999".into());
        assert_eq!(db.resolve("snes", stem, Some(&gone)).map(|g| g.qid).as_deref(), Some("Q518987"));
        assert_eq!(db.get("snes", "Q1061560").map(|g| g.label).as_deref(), Some("Super Mario Kart"));
        assert_eq!(db.get("snes", "Q596392"), None, "a Game Boy entry is not a Super Nintendo one");
    }

    #[test]
    fn players_read_as_a_person_would_say_them() {
        let with = |players: Option<&str>, modes: &[&str]| GameInfo {
            players: players.map(str::to_string),
            modes: modes.iter().map(|m| m.to_string()).collect(),
            ..Default::default()
        };
        assert_eq!(players_text(&with(Some("1-4"), &[])).as_deref(), Some("1–4 players"));
        assert_eq!(players_text(&with(Some("1"), &[])).as_deref(), Some("1 player"));
        // The file lists modes alphabetically; single-player comes first on the card.
        let modes = with(None, &["multiplayer video game", "single-player video game"]);
        assert_eq!(players_text(&modes).as_deref(), Some("Single-player, Multiplayer"));
        assert_eq!(players_text(&with(None, &["massively multiplayer online game"])), None);
    }

    #[test]
    fn more_by_and_series_list_only_the_users_own_games() {
        let text = "Q1\tMetroid II: Return of Samus\t1991\tNintendo R&D1\t\t\tMetroid\t\t\t\t\t\t1\t\n\
Q2\tKid Icarus: Of Myths and Monsters\t1991\tNintendo R&D1|TOSE\t\t\tKid Icarus\t\t\t\t\t\t1\t\n\
Q3\tSuper Metroid\t1994\tNintendo R&D1|Intelligent Systems\t\t\tMetroid\t\t\t\t\t\t1\t\n\
Q4\tMetroid Fusion\t2002\tNintendo R&D1\t\t\tMetroid\t\t\t\t\t\t1\t\n\
Q5\tTetris\t1989\tNintendo R&D1\t\t\t\t\t\t\t\t\t1\t\n";
        let s = SystemDb::from_text(text);
        let g = |q: &str| s.games.iter().find(|g| g.qid == q).unwrap();
        let (m2, kid, sm, other) = (
            Path::new("/r/gb/m2.gb"),
            Path::new("/r/gb/kid.gb"),
            Path::new("/r/snes/sm.sfc"),
            Path::new("/r/gb/x.gb"),
        );
        // The library: Metroid II, Kid Icarus, Super Metroid and one game without an entry. Metroid Fusion
        // and Tetris are in the database but not owned.
        let library = [
            Owned {
                system: "gb",
                path: m2,
                name: "Metroid II",
                info: Some(g("Q1")),
            },
            Owned {
                system: "gb",
                path: kid,
                name: "Kid Icarus",
                info: Some(g("Q2")),
            },
            Owned {
                system: "snes",
                path: sm,
                name: "Super Metroid",
                info: Some(g("Q3")),
            },
            Owned {
                system: "gb",
                path: other,
                name: "Homebrew",
                info: None,
            },
        ];
        let r = related(g("Q1"), m2, &library);
        let (dev, by) = r.by_developer.expect("more by");
        assert_eq!(dev, "Nintendo R&D1");
        let names: Vec<_> = by.iter().map(|o| o.name).collect();
        assert_eq!(
            names,
            ["Kid Icarus"],
            "owned games only, not Metroid Fusion or Tetris, not itself, not Super Metroid twice"
        );
        let (series, games) = r.in_series.expect("series");
        assert_eq!(series, "Metroid");
        assert_eq!(games.iter().map(|o| o.name).collect::<Vec<_>>(), ["Super Metroid"]);
        // Nothing else by the developer or in the series that the user owns: no section.
        let lonely = [Owned {
            system: "gb",
            path: m2,
            name: "Metroid II",
            info: Some(g("Q1")),
        }];
        assert_eq!(related(g("Q1"), m2, &lonely), Related::default());
    }
}
