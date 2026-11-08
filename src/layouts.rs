//! Other systems' SD cards: the folder names they give a system and the places they keep box art (docs/OTHER-LAYOUTS.md).
//! Written for this project from the layouts' public documentation (names and folder conventions only). Everything here
//! is read-only: a card that comes from another handheld is looked at, never changed.
//!
//! * [`system_ids`]: which of our systems a folder name means: `Game Boy Advance (GBA)` (a tag in brackets), `GBA` or `Imgs`
//!   style upper case, `gba`, `Sega Genesis`, `Nintendo Game Boy Advance`. Used by `library` to read such a folder as the
//!   system it is, without renaming it.
//! * [`art_dirs`]: the picture folders beside the games (`.media`, `Imgs`, `images`, `covers`, `media/covers` …) listed once, so
//!   asking for a game's picture later is a lookup, not a `stat` per candidate file.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// What a picture shows. Lower is better as a shelf cover: the box first, then a mix of screenshot and logo, then a screenshot.
/// The box art download leaves a game alone when it has a picture up to [`Kind::Likely`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Box,
    /// A gamelist's lone `<image>`: what the scraper was asked for, most often the box. A box that is known as one comes first.
    Likely,
    Mix,
    Shot,
}

// ---- folder names -------------------------------------------------------------------------------------------------

/// Tags in brackets (`Game Boy Advance (GBA)`: MinUI and the systems built on it) and the short upper-case folder names
/// of Anbernic's own firmware, with the ROCKNIX ids they mean, the first one this device has wins. Upper case.
const TAGS: &[(&str, &[&str])] = &[
    ("GB", &["gb"]),
    ("GBC", &["gbc"]),
    ("GBA", &["gba"]),
    ("MGBA", &["gba"]),
    ("SGB", &["gb", "sgb"]),
    ("FC", &["nes", "famicom"]),
    ("NES", &["nes", "famicom"]),
    ("FDS", &["fds"]),
    ("SFC", &["snes", "sfc"]),
    ("SUPA", &["snes", "sfc"]),
    ("SNES", &["snes", "sfc"]),
    ("MD", &["megadrive", "genesis"]),
    ("SMS", &["mastersystem"]),
    ("MS", &["mastersystem"]),
    ("GG", &["gamegear"]),
    ("SG1000", &["sg-1000", "sg1000"]),
    ("SEGACD", &["segacd", "megacd"]),
    ("32X", &["sega32x"]),
    ("PS", &["psx"]),
    ("PS1", &["psx"]),
    ("PSP", &["psp"]),
    ("PCE", &["pcengine", "tg16"]),
    ("PCECD", &["pcenginecd", "tg16cd"]),
    ("VB", &["virtualboy"]),
    ("NGP", &["ngp"]),
    ("NGPC", &["ngpc", "ngp"]),
    ("LYNX", &["atarilynx"]),
    ("A2600", &["atari2600"]),
    ("A5200", &["atari5200"]),
    ("A7800", &["atari7800"]),
    ("COLECO", &["colecovision"]),
    ("MSX", &["msx"]),
    ("PKM", &["pokemini"]),
    ("C64", &["c64"]),
    ("PUAE", &["amiga"]),
    ("N64", &["n64"]),
    ("NDS", &["nds"]),
    ("DC", &["dreamcast"]),
    ("WS", &["wonderswan"]),
    ("WSC", &["wonderswancolor"]),
    ("FBN", &["arcade", "fbneo"]),
    ("FBNEO", &["fbneo", "arcade"]),
    ("MAME", &["mame", "arcade"]),
    ("ARCADE", &["arcade", "mame", "fbneo"]),
    ("NEOGEO", &["neogeo"]),
];

/// Names people and other systems give a folder (lowercase letters and digits only) and the ROCKNIX id they mean. A guess is
/// made only for an id this device has. Batocera/ES-DE ids (`wswan`, `genesis`), the long names of other cards
/// (`nintendogameboyadvance`, `segamegadrivegenesis`) and the usual spellings.
const ALIASES: &[(&str, &str)] = &[
    ("segagenesis", "megadrive"),
    ("genesis", "megadrive"),
    ("md", "megadrive"),
    ("segamegadrive", "megadrive"),
    ("megadrivegenesis", "megadrive"),
    ("segamegadrivegenesis", "megadrive"),
    ("ps1", "psx"),
    ("ps", "psx"),
    ("playstation", "psx"),
    ("playstation1", "psx"),
    ("sonyplaystation", "psx"),
    ("ps2", "ps2"),
    ("playstation2", "ps2"),
    ("snes", "snes"),
    ("supernintendo", "snes"),
    ("supernes", "snes"),
    ("supernintendoentertainmentsystem", "snes"),
    ("nintendosnessfc", "snes"),
    ("superfamicom", "sfc"),
    ("nes", "nes"),
    ("nintendo", "nes"),
    ("nintendoentertainmentsystem", "nes"),
    ("nintendonesfamicom", "nes"),
    ("famicom", "famicom"),
    ("gameboy", "gb"),
    ("nintendogameboy", "gb"),
    ("gameboycolor", "gbc"),
    ("gameboycolour", "gbc"),
    ("nintendogameboycolor", "gbc"),
    ("gameboyadvance", "gba"),
    ("nintendogameboyadvance", "gba"),
    ("gba", "gba"),
    ("n64", "n64"),
    ("nintendo64", "n64"),
    ("nds", "nds"),
    ("ds", "nds"),
    ("nintendods", "nds"),
    ("gamecube", "gamecube"),
    ("gc", "gamecube"),
    ("ngc", "gamecube"),
    ("nintendogamecube", "gamecube"),
    ("dreamcast", "dreamcast"),
    ("dc", "dreamcast"),
    ("segadreamcast", "dreamcast"),
    ("saturn", "saturn"),
    ("segasaturn", "saturn"),
    ("segacd", "segacd"),
    ("megacd", "segacd"),
    ("megacdsegacd", "segacd"),
    ("segamegacdsegacd", "segacd"),
    ("sega32x", "sega32x"),
    ("32x", "sega32x"),
    ("mastersystem", "mastersystem"),
    ("segamastersystem", "mastersystem"),
    ("sms", "mastersystem"),
    ("gamegear", "gamegear"),
    ("segagamegear", "gamegear"),
    ("gg", "gamegear"),
    ("sg1000", "sg-1000"),
    ("segasg1000", "sg-1000"),
    ("pcengine", "pcengine"),
    ("necpcengine", "pcengine"),
    ("turbografx16", "pcengine"),
    ("turbografx", "pcengine"),
    ("tg16", "pcengine"),
    ("pcenginecd", "pcenginecd"),
    ("necpcenginecd", "pcenginecd"),
    ("turbografxcd", "pcenginecd"),
    ("psp", "psp"),
    ("playstationportable", "psp"),
    ("psvita", "psvita"),
    ("playstationvita", "psvita"),
    ("arcade", "arcade"),
    ("mame", "mame"),
    ("fbneo", "fbneo"),
    ("fba", "fbneo"),
    ("neogeo", "neogeo"),
    ("snkneogeo", "neogeo"),
    ("atari2600", "atari2600"),
    ("2600", "atari2600"),
    ("atari5200", "atari5200"),
    ("atari7800", "atari7800"),
    ("lynx", "atarilynx"),
    ("atarilynx", "atarilynx"),
    ("wonderswan", "wonderswan"),
    ("wswan", "wonderswan"),
    ("bandaiwonderswan", "wonderswan"),
    ("wonderswancolor", "wonderswancolor"),
    ("wswanc", "wonderswancolor"),
    ("neogeopocket", "ngp"),
    ("snkneogeopocket", "ngp"),
    ("neogeopocketcolor", "ngpc"),
    ("snkneogeopocketcolor", "ngpc"),
    ("virtualboy", "virtualboy"),
    ("nintendovirtualboy", "virtualboy"),
    ("fds", "fds"),
    ("famicomdisksystem", "fds"),
    ("nintendofds", "fds"),
    ("pokemonmini", "pokemini"),
    ("nintendopokemonmini", "pokemini"),
    ("colecovision", "colecovision"),
    ("amiga", "amiga"),
    ("amiga500", "amiga"),
    ("commodoreamiga", "amiga"),
    ("c64", "c64"),
    ("commodore64", "c64"),
    ("msx1", "msx"),
    ("microsoftmsx", "msx"),
    ("dos", "pc"),
    ("msdos", "pc"),
];

/// The part of a folder name in brackets at its end: "Game Boy Advance (GBA)" → ("Game Boy Advance", Some("GBA")).
pub fn split_tag(name: &str) -> (&str, Option<&str>) {
    let name = name.trim();
    if let Some(open) = name.rfind(" (").filter(|_| name.ends_with(')')) {
        let tag = name[open + 2..name.len() - 1].trim();
        let head = name[..open].trim();
        if !tag.is_empty() && !head.is_empty() && !tag.contains('(') {
            return (head, Some(tag));
        }
    }
    (name, None)
}

fn plain(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// The ROCKNIX ids a folder called `name` may mean, best first: its tag in brackets, then its name as people write it. Empty
/// when nothing is known by that name (an unknown folder stays unread). Only the name is looked at, never the folder.
pub fn system_ids(name: &str) -> Vec<&'static str> {
    let (head, tag) = split_tag(name);
    let mut out: Vec<&'static str> = Vec::new();
    let mut add = |ids: &[&'static str]| {
        for id in ids {
            if !out.contains(id) {
                out.push(id);
            }
        }
    };
    if let Some(tag) = tag {
        let upper = tag.to_uppercase();
        if let Some((_, ids)) = TAGS.iter().find(|(t, _)| *t == upper) {
            add(ids);
        }
    }
    // The whole name first ("Super Nintendo" as written), then the name without its tag.
    for key in [plain(name), plain(head)] {
        if let Some((_, id)) = ALIASES.iter().find(|(alias, _)| *alias == key) {
            add(&[id]);
        }
    }
    // A short upper-case folder of Anbernic's own firmware ("GBA", "FC", "MS"): the same table as the tags.
    if tag.is_none() && name.chars().all(|c| c.is_ascii_alphanumeric()) && name.chars().any(|c| c.is_ascii_alphabetic()) && !name.chars().any(|c| c.is_ascii_lowercase()) {
        if let Some((_, ids)) = TAGS.iter().find(|(t, _)| *t == name) {
            add(ids);
        }
    }
    out
}

// ---- picture folders ----------------------------------------------------------------------------------------------

/// Extensions of pictures pocket-ui reads.
fn picture(file: &str) -> Option<&str> {
    let (stem, ext) = file.rsplit_once('.')?;
    ["png", "jpg", "jpeg"].iter().any(|e| ext.eq_ignore_ascii_case(e)).then_some(stem)
}

#[derive(Clone, Copy, PartialEq)]
enum Folder {
    /// The picture is named like the game and shows the box (NextUI `.media`, Anbernic `Imgs`, scrapers' `covers`).
    Box,
    Mix,
    Shot,
    /// EmulationStation's `images`: the kind is in a suffix of the file name (`-thumb`, `-image`).
    Suffixed,
    /// `media/` of Skyscraper and Recalbox: one more level of the folders above.
    Wrapper,
}

fn folder_kind(name: &str) -> Option<Folder> {
    Some(match name.to_lowercase().as_str() {
        ".media" | "imgs" | "covers" | "cover" | "boxart" | "boxarts" | "box2d" | "boxes" | "box" | "3dboxes" | "box3d" => Folder::Box,
        "miximages" | "mixes" | "mix" => Folder::Mix,
        "screenshots" | "screenshot" | "snaps" | "snap" | "titlescreens" | "preview" => Folder::Shot,
        "images" | "image" | "img" => Folder::Suffixed,
        "media" => Folder::Wrapper,
        _ => return None,
    })
}

/// A file of `images/`: the game name and what the picture shows. `None`: a picture that is no cover (marquee, logo, wheel,
/// fan art, cartridge, video preview).
fn suffixed(stem: &str) -> Option<(&str, Kind)> {
    let lower = stem.to_lowercase();
    for (suffix, kind) in [("-thumb", Some(Kind::Box)), ("-boxart", Some(Kind::Box)), ("-box", Some(Kind::Box)), ("-cover", Some(Kind::Box)), ("-mix", Some(Kind::Mix)), ("-image", Some(Kind::Shot)), ("-screenshot", Some(Kind::Shot)), ("-titleshot", Some(Kind::Shot))] {
        if lower.ends_with(suffix) {
            return kind.map(|k| (&stem[..stem.len() - suffix.len()], k));
        }
    }
    if ["-marquee", "-wheel", "-logo", "-fanart", "-cartridge", "-cart", "-video", "-map", "-manual", "-bezel", "-back"].iter().any(|s| lower.ends_with(s)) {
        return None;
    }
    // A bare name in `images/` is a picture somebody put there for the game.
    Some((stem, Kind::Mix))
}

/// One picture folder, listed: lowercase game name → (file name, what it shows).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtDir {
    pub dir: PathBuf,
    files: HashMap<String, (String, Kind)>,
}

impl ArtDir {
    fn read(dir: &Path, folder: Folder) -> ArtDir {
        let mut files: HashMap<String, (String, Kind)> = HashMap::new();
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(stem) = picture(name) else { continue };
            let (game, kind) = match folder {
                Folder::Box => (stem, Kind::Box),
                Folder::Mix => (stem, Kind::Mix),
                Folder::Shot => (stem, Kind::Shot),
                Folder::Suffixed => match suffixed(stem) {
                    Some(found) => found,
                    None => continue,
                },
                Folder::Wrapper => continue,
            };
            let key = game.to_lowercase();
            if files.get(&key).is_none_or(|(_, k)| kind < *k) {
                files.insert(key, (name.to_string(), kind));
            }
        }
        ArtDir { dir: dir.to_path_buf(), files }
    }

    /// The picture for the game called `stem` (any case) and what it shows.
    pub fn find(&self, stem: &str) -> Option<(PathBuf, Kind)> {
        self.files.get(&stem.to_lowercase()).map(|(name, kind)| (self.dir.join(name), *kind))
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

/// The picture folders directly in a system's ROM folder (and one level of `media/`), listed. A few `readdir`s; nothing else
/// on the card is opened.
pub fn art_dirs(rom_dir: &Path) -> Vec<ArtDir> {
    let mut out = Vec::new();
    let take = |dir: &Path, folder: Folder, out: &mut Vec<ArtDir>| {
        let listed = ArtDir::read(dir, folder);
        if !listed.is_empty() {
            out.push(listed);
        }
    };
    let mut wrappers = Vec::new();
    for entry in std::fs::read_dir(rom_dir).into_iter().flatten().flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        // A link to a folder counts (a shared pictures folder); a link to a file does not.
        let is_dir = kind.is_dir() || (kind.is_symlink() && entry.path().is_dir());
        if !is_dir {
            continue;
        }
        match folder_kind(name) {
            Some(Folder::Wrapper) => wrappers.push(entry.path()),
            Some(folder) => take(&entry.path(), folder, &mut out),
            None => {}
        }
    }
    for wrapper in wrappers {
        for entry in std::fs::read_dir(&wrapper).into_iter().flatten().flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some(folder) = folder_kind(name).filter(|f| *f != Folder::Wrapper) {
                if entry.path().is_dir() {
                    take(&entry.path(), folder, &mut out);
                }
            }
        }
    }
    // The same order on every read, whatever order the card lists its folders in: the box folders first.
    out.sort_by(|a, b| a.dir.cmp(&b.dir));
    out
}

/// The catalogue of muOS (`MUOS/info/catalogue/<System name>/box|preview/<game>.png`) for the system ids `ids`: the folders
/// whose name means one of them. `catalogue` is the `catalogue` folder itself.
pub fn catalogue_dirs(catalogue: &Path, ids: &[&str]) -> Vec<ArtDir> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(catalogue).into_iter().flatten().flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !system_ids(name).iter().any(|id| ids.contains(id)) {
            continue;
        }
        for (sub, folder) in [("box", Folder::Box), ("preview", Folder::Shot)] {
            let dir = entry.path().join(sub);
            if dir.is_dir() {
                let listed = ArtDir::read(&dir, folder);
                if !listed.is_empty() {
                    out.push(listed);
                }
            }
        }
    }
    out.sort_by(|a, b| a.dir.cmp(&b.dir));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_in_brackets_and_the_usual_spellings_find_the_system() {
        assert_eq!(system_ids("Game Boy Advance (GBA)"), ["gba"]);
        assert_eq!(system_ids("Super Nintendo Entertainment System (SFC)"), ["snes", "sfc"]);
        assert_eq!(system_ids("Sega Genesis (MD)"), ["megadrive", "genesis"]);
        assert_eq!(system_ids("Game Boy Advance (MGBA)"), ["gba"]);
        assert_eq!(system_ids("GBA"), ["gba"]);
        assert_eq!(system_ids("FC"), ["nes", "famicom"]);
        assert_eq!(system_ids("Nintendo Game Boy Advance"), ["gba"]);
        assert_eq!(system_ids("Sega Mega Drive - Genesis"), ["megadrive"]);
        assert_eq!(system_ids("wswan"), ["wonderswan"]);
        assert!(system_ids("Holiday photos").is_empty());
        assert!(system_ids("images").is_empty());
    }

    #[test]
    fn a_name_in_brackets_that_is_no_tag_is_not_cut() {
        assert_eq!(split_tag("Game Boy Advance (GBA)"), ("Game Boy Advance", Some("GBA")));
        assert_eq!(split_tag("Roms (old) backup"), ("Roms (old) backup", None));
        assert_eq!(split_tag("(GBA)"), ("(GBA)", None));
    }

    #[test]
    fn images_folders_keep_the_box_apart_from_screenshots_and_logos() {
        assert_eq!(suffixed("Sonic-thumb"), Some(("Sonic", Kind::Box)));
        assert_eq!(suffixed("Sonic-image"), Some(("Sonic", Kind::Shot)));
        assert_eq!(suffixed("Sonic-marquee"), None);
        assert_eq!(suffixed("Sonic-wheel"), None);
        assert_eq!(suffixed("Sonic"), Some(("Sonic", Kind::Mix)));
    }
}
