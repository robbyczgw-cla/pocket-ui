//! The files behind "the case knows where you are" (`case_knows` reads the bytes): which battery save belongs to a game,
//! which ROM it is, a thread that answers in the background, and the line in the interface's language.
//!
//! **On demand.** Nothing is read until the shelf, a box or a start asks for a game ([`Reader::request`]), and then on a
//! thread of the lowest CPU and I/O priority that waits while a game runs. The painter never waits: it shows the line
//! once the answer is there, and nothing until then.
//!
//! **Cached by file time.** The ROM's checksum comes from the identify cache (`rom_id`, `identify/checksums.tsv`) when
//! it has the file at its size and time, else this thread works it out once, only for a file of a size one of the
//! releases has (`case_knows::worth_checking`). Those answers and what the reads of a several-file save remember
//! (`case_knows::Memory`) are kept in `<data>/identify/case-knows.tsv`. The save is read again only when its size or
//! time changed: a few KiB, under a millisecond.
//!
//! **Which save.** The newest of: pocket-core's own folder on the TrimUI Brick (`POCKET_CORE_SAVE_DIR/<system>`), the
//! player's `saves/<key>/` in Android's handheld view (beside `states/<key>/`, `slots::base`), and `<game>.srm` or
//! `<game>.sav` (DeSmuME's `<game>.dsv`) beside the game (RetroArch, pocket-core and standalone emulators on ROCKNIX).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::case_knows::{self, Memory, Progress, Release};
use crate::case_knows_art::{self, Art, Places};

/// Bigger battery saves than this are none of the games read (DS flash is 512 KiB; DeSmuME's `.dsv` adds a footer, a
/// GBA emulator may add a clock).
const MAX_SAVE: u64 = 1 << 20;

/// The cache's file, beside the identify cache.
fn cache_path(data_dir: &Path) -> PathBuf {
    data_dir.join("identify").join("case-knows.tsv")
}

/// Where each ROM's pictures are (`case_knows_art::Places`): `size  mtime  places|-  path`.
fn art_path(data_dir: &Path) -> PathBuf {
    data_dir.join("identify").join("case-knows-art.tsv")
}

/// Where the save of `rom` may be, in the order they are looked at.
pub fn save_places(system: &str, rom: &Path) -> Vec<PathBuf> {
    let Some(stem) = rom.file_stem().and_then(|s| s.to_str()) else { return Vec::new() };
    let mut out = Vec::new();
    #[cfg(feature = "brick")]
    if let Some(dir) = std::env::var_os("POCKET_CORE_SAVE_DIR") {
        out.push(PathBuf::from(dir).join(system).join(format!("{stem}.srm")));
    }
    // Android's handheld view: `<player>/states/<key>/<stem>` has its battery save in `<player>/saves/<key>/`.
    let base = crate::slots::base(system, stem);
    if let (Some(key), Some(states)) = (base.parent(), base.parent().and_then(Path::parent)) {
        if states.file_name().is_some_and(|n| n == "states") {
            if let (Some(player), Some(key)) = (states.parent(), key.file_name()) {
                out.push(player.join("saves").join(key).join(format!("{stem}.srm")));
            }
        }
    }
    if let Some(dir) = rom.parent() {
        for ext in ["srm", "sav", "eep", "dsv", "SRM", "SAV", "DSV"] {
            out.push(dir.join(format!("{stem}.{ext}")));
        }
    }
    out
}

/// The newest save of `rom` that is there, with its size and time.
pub fn save_of(system: &str, rom: &Path) -> Option<(PathBuf, (u64, u64))> {
    save_places(system, rom).into_iter().filter_map(|p| Some((stamp(&p)?, p))).max_by_key(|(at, _)| at.1).map(|(at, p)| (p, at))
}

fn stamp(path: &Path) -> Option<(u64, u64)> {
    let m = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
    let mtime = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    Some((m.len(), mtime))
}

/// What is known of one ROM: its stamp, the release (None: not one read), what the save's reads remember.
#[derive(Clone, Debug, Default, PartialEq)]
struct Known {
    rom: (u64, u64),
    release: Option<u32>,
    memory: Memory,
}

/// The thread's state: answers by ROM and the last read of each save.
#[derive(Default)]
pub struct Reading {
    data_dir: PathBuf,
    known: HashMap<PathBuf, Known>,
    /// The identify cache, loaded at the first ROM it may know.
    identify: Option<crate::rom_id::Cache>,
    /// The last answer per ROM and the save's stamp it was read at.
    read: HashMap<PathBuf, ((PathBuf, (u64, u64)), Option<Progress>)>,
    dirty: bool,
    /// Where each ROM's pictures are, by its stamp, as the cache line has it (`Places::line`, `GbPlaces::line`, or `zelda <sheet> <palette>`);
    /// None: looked, none there.
    places: HashMap<PathBuf, ((u64, u64), Option<String>)>,
    places_dirty: bool,
}

impl Reading {
    pub fn new(data_dir: &Path) -> Reading {
        let text = std::fs::read_to_string(cache_path(data_dir)).unwrap_or_default();
        let known = text
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.splitn(6, '\t').collect();
                let [size, mtime, crc, current, prints, path] = f[..] else { return None };
                let release = u32::from_str_radix(crc, 16).ok();
                let current = current.parse().ok();
                let prints = prints.split(',').filter(|p| !p.is_empty()).filter_map(|p| u32::from_str_radix(p, 16).ok()).collect();
                Some((PathBuf::from(path), Known { rom: (size.parse().ok()?, mtime.parse().ok()?), release, memory: Memory { prints, current } }))
            })
            .collect();
        let text = std::fs::read_to_string(art_path(data_dir)).unwrap_or_default();
        let places = text
            .lines()
            .filter_map(|l| {
                let f: Vec<&str> = l.splitn(4, '\t').collect();
                let [size, mtime, places, path] = f[..] else { return None };
                Some((PathBuf::from(path), ((size.parse().ok()?, mtime.parse().ok()?), (places != "-").then(|| places.to_string()))))
            })
            .collect();
        Reading { data_dir: data_dir.to_path_buf(), known, places, ..Default::default() }
    }

    /// Writes the caches when something changed (`fsutil::write_atomic`).
    pub fn keep(&mut self) {
        if self.places_dirty {
            let mut lines: Vec<String> = self
                .places
                .iter()
                .map(|(p, (at, pl))| format!("{}\t{}\t{}\t{}\n", at.0, at.1, pl.as_deref().unwrap_or("-"), p.display()))
                .collect();
            lines.sort();
            let path = art_path(&self.data_dir);
            if path.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && crate::fsutil::write_atomic(&path, lines.concat().as_bytes()).is_ok() {
                self.places_dirty = false;
            }
        }
        if !self.dirty {
            return;
        }
        let mut lines: Vec<String> = self
            .known
            .iter()
            .map(|(p, k)| {
                let prints: Vec<String> = k.memory.prints.iter().map(|x| format!("{x:x}")).collect();
                let crc = k.release.map_or("-".to_string(), |c| format!("{c:08X}"));
                let current = k.memory.current.map_or("-".to_string(), |c| c.to_string());
                format!("{}\t{}\t{}\t{}\t{}\t{}\n", k.rom.0, k.rom.1, crc, current, prints.join(","), p.display())
            })
            .collect();
        lines.sort();
        let path = cache_path(&self.data_dir);
        if path.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && crate::fsutil::write_atomic(&path, lines.concat().as_bytes()).is_ok() {
            self.dirty = false;
        }
    }

    /// The release `rom` is, by its checksum: the identify cache's, else worked out here (and kept).
    fn release(&mut self, system: &str, rom: &Path) -> Option<&'static Release> {
        let at = stamp(rom)?;
        if let Some(k) = self.known.get(rom).filter(|k| k.rom == at) {
            return k.release.and_then(|crc| case_knows::release(crc, None));
        }
        // A DS game by the game code in its header: 512 bytes, never the whole ROM (`case_knows::ds_key`).
        if rom.extension().is_some_and(|e| e.eq_ignore_ascii_case("nds")) {
            let mut header = vec![0u8; 0x200];
            let read = std::fs::File::open(rom).and_then(|mut f| std::io::Read::read_exact(&mut f, &mut header));
            let release = read.ok().and_then(|_| case_knows::ds_key(&header)).and_then(|k| case_knows::release(k, None));
            self.known.insert(rom.to_path_buf(), Known { rom: at, release: release.map(|r| r.crc), memory: Memory::default() });
            self.dirty = true;
            return release;
        }
        // The identify cache's sum first (it has read the file already); else read it through here, only when its size
        // is one of a release's.
        let zip = rom.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip"));
        let identify = self.identify.get_or_insert_with(|| crate::rom_id::Cache::load(&self.data_dir));
        let sums = match identify.sums_of(rom, at) {
            Some(sums) => sums,
            None if case_knows::worth_checking(system, at.0, zip) => crate::rom_id::checksums(system, rom).ok()?,
            None => None,
        };
        let release = sums.and_then(|s| case_knows::release(s.crc, s.alt));
        self.known.insert(rom.to_path_buf(), Known { rom: at, release: release.map(|r| r.crc), memory: Memory::default() });
        self.dirty = true;
        release
    }

    /// Whether answering for `rom` reads no more than its save and a few hundred bytes of it: the ROM and where its
    /// pictures are (if it has any) are known at its size and time.
    pub fn cheap(&self, rom: &Path) -> bool {
        let at = stamp(rom);
        let known = self.known.get(rom).filter(|k| Some(k.rom) == at);
        let pictures = known.and_then(|k| k.release).and_then(|c| case_knows::release(c, None)).is_some_and(|r| case_knows_art::has_pictures(r.kind));
        known.is_some() && (!pictures || self.places.get(rom).is_some_and(|(s, _)| Some(*s) == at))
    }

    /// The pictures for `progress` of `rom` from the player's own ROM (`case_knows_art`): the places found once (the ROM
    /// read through) and kept; None for a game without pictures or a ROM where they aren't found.
    pub fn art(&mut self, rom: &Path, progress: &Progress) -> Option<Art> {
        let at = stamp(rom)?;
        let release = self.known.get(rom).filter(|k| k.rom == at).and_then(|k| k.release).and_then(|c| case_knows::release(c, None))?;
        if !case_knows_art::has_pictures(release.kind) {
            return None;
        }
        // A SNES file may start with a 512-byte copier header; the places are counted without it.
        let header = if release.kind == case_knows::Kind::LinkToThePast && at.0 % 1024 == 512 { 512u64 } else { 0 };
        let places = match self.places.get(rom).filter(|(s, _)| *s == at) {
            Some((_, p)) => p.clone(),
            None => {
                let found = if case_knows_art::is_ds(release.kind) {
                    // A DS ROM is 64 to 512 MiB: only its header, file tables and the parts that hold the icons are read.
                    case_knows_art::find_nds(rom, release.kind).map(|p| p.line())
                } else {
                    let bytes = std::fs::read(rom).ok()?;
                    let bytes = &bytes[header as usize..];
                    match release.kind {
                        case_knows::Kind::LinkToThePast => case_knows_art::find_zelda(bytes).map(|(s, p)| format!("zelda {s:x} {p:x}")),
                        case_knows::Kind::GoldSilver | case_knows::Kind::Crystal => case_knows_art::find_gb(bytes).map(|p| p.line()),
                        kind => case_knows_art::find(bytes, kind).map(|p| p.line()),
                    }
                };
                self.places.insert(rom.to_path_buf(), (at, found.clone()));
                self.places_dirty = true;
                found
            }
        }?;
        let art = match progress {
            Progress::Pokemon { art: team, .. } if places.starts_with("nds ") => Some(case_knows_art::nds_art(rom, &case_knows_art::NdsPlaces::parse(&places)?, team)),
            Progress::Pokemon { art: team, .. } if places.starts_with("gb ") => Some(case_knows_art::gb_art(rom, &case_knows_art::GbPlaces::parse(&places)?, team)),
            Progress::Pokemon { art: team, .. } => Some(case_knows_art::art(rom, &Places::parse(&places)?, team)),
            Progress::Quest { .. } => {
                let f: Vec<u32> = places.strip_prefix("zelda ")?.split(' ').filter_map(|x| u32::from_str_radix(x, 16).ok()).collect();
                let [sheet, palette] = f[..] else { return None };
                case_knows_art::zelda_art(rom, (sheet, palette), header, progress)
            }
            _ => None,
        };
        art.filter(|a| !a.team.is_empty() || !a.badges.is_empty())
    }

    /// Where the player is in `rom`, from its battery save; None when it isn't a game read, has no save, or the save
    /// says nothing for sure.
    pub fn progress(&mut self, system: &str, rom: &Path) -> Option<Progress> {
        // Only what was checked against the games themselves (`Release::verified`).
        let release = self.release(system, rom).filter(|r| r.verified())?;
        let (save, at) = save_of(system, rom)?;
        if let Some((seen, answer)) = self.read.get(rom) {
            if *seen == (save.clone(), at) {
                return answer.clone();
            }
        }
        let bytes = if at.0 <= MAX_SAVE { std::fs::read(&save).ok()? } else { Vec::new() };
        let before = self.known.get(rom).map(|k| k.memory.clone());
        let (answer, memory) = case_knows::read(release, &bytes, before.as_ref());
        if let Some(k) = self.known.get_mut(rom) {
            if k.memory != memory {
                k.memory = memory;
                self.dirty = true;
            }
        }
        self.read.insert(rom.to_path_buf(), ((save, at), answer.clone()));
        answer
    }
}

/// One game asked about: (system id, ROM).
pub type Ask = (String, PathBuf);

/// The thread, started at the first request.
pub struct Reader {
    tx: mpsc::Sender<Ask>,
}

impl Reader {
    /// `done` runs on the thread for every answer. Requests made while a game runs wait until it is over.
    pub fn start(data_dir: PathBuf, done: impl Fn(Ask, Option<Progress>, Option<Art>) + Send + 'static) -> Reader {
        let (tx, rx) = mpsc::channel::<Ask>();
        let _ = std::thread::Builder::new().name("case-knows".into()).spawn(move || {
            #[cfg(target_os = "linux")]
            crate::web::lower_priority();
            let mut reading = Reading::new(&data_dir);
            let mut waiting: Vec<Ask> = Vec::new();
            loop {
                if waiting.is_empty() {
                    reading.keep();
                    let Ok(ask) = rx.recv() else { break };
                    waiting.push(ask);
                }
                for ask in rx.try_iter() {
                    waiting.retain(|old| old != &ask);
                    waiting.push(ask);
                }
                // The newest question first: the game the player looks at now. While a game runs only a save is read
                // (a few KiB, for "Where was I?"), never a ROM through for its checksum: that waits for the game's end.
                let ask = waiting.pop().unwrap();
                if crate::covers::game_on() && !reading.cheap(&ask.1) {
                    waiting.insert(0, ask);
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    continue;
                }
                let answer = reading.progress(&ask.0, &ask.1);
                let art = answer.as_ref().and_then(|p| reading.art(&ask.1, p));
                done(ask, answer, art);
            }
        });
        Reader { tx }
    }

    pub fn request(&self, system: &str, rom: &Path) {
        let _ = self.tx.send((system.to_string(), rom.to_path_buf()));
    }
}

/// The line in the interface's language: each part looked up in the table ("8 badges" "8 Orden" "8 medallas"), joined
/// with " · ". A Pokémon's name is the game's own and stays as it is.
pub fn line(progress: &Progress) -> String {
    progress.parts().iter().map(|p| crate::i18n::tr(p)).collect::<Vec<_>>().join(" · ")
}

/// The line on one line of at most `w` pixels: as many of its parts as fit, from the first, never a part cut short
/// ("8 badges · Charizard Lv. 45" where "· game clock 31:12" has no room); only a first part wider than `w` alone is
/// shortened.
pub fn fit(text: &mut crate::text::Text, face: crate::text::Face, px: u32, line: &str, w: i32) -> String {
    let mut out = String::new();
    for part in line.split(" · ") {
        let longer = if out.is_empty() { part.to_string() } else { format!("{out} · {part}") };
        if text.width_plain(face, px, &longer) > w {
            break;
        }
        out = longer;
    }
    if out.is_empty() { text.fit_name(face, px, line.split(" · ").next().unwrap_or(line), w) } else { out }
}

/// The line broken into lines of at most `w` pixels between its parts, never inside one ("8 badges · Charizard Lv. 45"
/// over "game clock 31:12"); a part wider than `w` alone is shortened. At most two lines; the rest is left out.
pub fn lines(text: &mut crate::text::Text, face: crate::text::Face, px: u32, line: &str, w: i32) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for part in line.split(" · ") {
        let longer = if cur.is_empty() { part.to_string() } else { format!("{cur} · {part}") };
        if cur.is_empty() || text.width_plain(face, px, &longer) <= w {
            cur = longer;
        } else {
            out.push(std::mem::replace(&mut cur, part.to_string()));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.truncate(2);
    out.into_iter().map(|l| text.fit_name(face, px, &l, w)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::case_knows::tests as saves;
    use crate::case_knows::Kind;

    fn dir(name: &str) -> crate::testutil::TestDir {
        crate::testutil::TestDir::new(name)
    }

    /// A ROM file that is the release `crc` names for the reader: the identify cache says so, as it would after its
    /// first run (no real ROM in the repository).
    fn rom_known_as(data: &Path, rom: &Path, system: &str, crc: u32) {
        std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
        std::fs::write(rom, b"rom").unwrap();
        let at = stamp(rom).unwrap();
        std::fs::create_dir_all(data.join("identify")).unwrap();
        let line = format!("{system}\t{}\t{}\t{crc:08X}\t-\t{}\n", at.0, at.1, rom.display());
        let path = data.join("identify/checksums.tsv");
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        text.push_str(&line);
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn the_save_beside_the_game_is_read_and_read_again_only_when_it_changes() {
        let d = dir("case-knows-beside");
        let (data, rom) = (d.join("data"), d.join("roms/gba/Pokemon Emerald.gba"));
        rom_known_as(&data, &rom, "gba", 0x1F1C08FB);
        let save = rom.with_extension("srm");
        std::fs::write(&save, saves::gen3_save(Kind::Emerald, &[("TORCHIC", 12, false)], 1, (2, 5), 4)).unwrap();
        let mut r = Reading::new(&data);
        assert_eq!(r.progress("gba", &rom).map(|p| p.parts().join(" · ")).as_deref(), Some("1 badge · Torchic Lv. 12 · game clock 2:05"));
        // Played on: the game saved again. The next look reads the new save.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&save, saves::gen3_save(Kind::Emerald, &[("COMBUSKEN", 22, false)], 3, (6, 40), 9)).unwrap();
        assert_eq!(r.progress("gba", &rom).map(|p| p.parts().join(" · ")).as_deref(), Some("3 badges · Combusken Lv. 22 · game clock 6:40"));
    }

    /// A DS game: known by its header's game code (the ROM is never read through), its DeSmuME `.dsv` beside it read.
    #[test]
    fn a_ds_game_is_known_by_its_header_and_its_dsv_is_read() {
        let d = dir("case-knows-ds");
        let data = d.join("data");
        let rom = d.join("roms/nds/Pokemon - Platinum Version (USA) (Rev 1).nds");
        std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
        let header = |code: &[u8; 4]| {
            let mut h = vec![0u8; 0x1000];
            h[0x0C..0x10].copy_from_slice(code);
            let sum = h[..0x15E].iter().fold(0xFFFFu16, |mut c, &b| {
                c ^= b as u16;
                for _ in 0..8 {
                    c = if c & 1 != 0 { (c >> 1) ^ 0xA001 } else { c >> 1 };
                }
                c
            });
            h[0x15E..0x160].copy_from_slice(&sum.to_le_bytes());
            h
        };
        std::fs::write(&rom, header(b"CPUE")).unwrap();
        let team: &[(&str, u8, bool)] = &[("PIPLUP", 16, false)];
        let mut save = saves::gen4_save(Kind::Platinum, [Some(((2, 1), team, (0b111, 0), (12, 34))), None]);
        save.extend_from_slice(b"|<--Snip above here to create a raw sav by excluding this DeSmuME savedata footer:");
        std::fs::write(rom.with_extension("dsv"), &save).unwrap();
        let mut r = Reading::new(&data);
        assert_eq!(r.progress("nds", &rom).map(|p| p.parts().join(" · ")).as_deref(), Some("3 badges · Piplup Lv. 16 · game clock 12:34"));
        // A Japanese Platinum (and a hack made from one): another code, nothing.
        let jp = d.join("roms/nds/Pokemon Platinum (J).nds");
        std::fs::write(&jp, header(b"CPUJ")).unwrap();
        std::fs::write(jp.with_extension("sav"), &save).unwrap();
        assert_eq!(r.progress("nds", &jp), None);
    }

    #[test]
    fn a_hack_or_a_game_without_a_save_shows_nothing() {
        let d = dir("case-knows-hack");
        let data = d.join("data");
        // A FireRed hack: same size, its own checksum, a save that would read fine as FireRed's.
        let hack = d.join("roms/gba/Pokemon Unbound.gba");
        rom_known_as(&data, &hack, "gba", 0x0BADF00D);
        std::fs::write(hack.with_extension("srm"), saves::gen3_save(Kind::FireRedLeafGreen, &[("CHARIZARD", 50, false)], 8, (1, 1), 1)).unwrap();
        // Emerald, never saved in the game.
        let fresh = d.join("roms/gba/Emerald.gba");
        rom_known_as(&data, &fresh, "gba", 0x1F1C08FB);
        let mut r = Reading::new(&data);
        assert_eq!(r.progress("gba", &hack), None);
        assert_eq!(r.progress("gba", &fresh), None);
        // A game that isn't one of the releases (another system, another size) is never read through.
        let other = d.join("roms/nes/Zelda.nes");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, vec![0u8; 1 << 20]).unwrap();
        std::fs::write(other.with_extension("srm"), saves::zelda_save([Some((1, 0, 5, 1)), None, None])).unwrap();
        assert_eq!(r.progress("nes", &other), None);
    }

    #[test]
    fn what_a_several_file_save_remembers_survives_a_restart() {
        let d = dir("case-knows-memory");
        let (data, rom) = (d.join("data"), d.join("roms/snes/Zelda - A Link to the Past.sfc"));
        rom_known_as(&data, &rom, "snes", 0x777AAC2F);
        let save = rom.with_extension("srm");
        let quest = |hearts| Progress::Quest { pendants: 1, crystals: 0, hearts, dark_world: false };
        std::fs::write(&save, saves::zelda_save([Some((1, 0, 9, 1)), None, None])).unwrap();
        let mut r = Reading::new(&data);
        assert_eq!(r.progress("snes", &rom), Some(quest(9)));
        r.keep();
        // pocket-ui restarts; the player saved a second file meanwhile.
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&save, saves::zelda_save([Some((1, 0, 9, 1)), Some((1, 0, 4, 1)), None])).unwrap();
        let mut r = Reading::new(&data);
        assert_eq!(r.progress("snes", &rom), Some(quest(4)));
    }

    #[test]
    fn the_newest_save_wins_and_a_brick_or_android_folder_counts() {
        let d = dir("case-knows-places");
        let rom = d.join("roms/gb/Pokemon Red.gb");
        std::fs::create_dir_all(rom.parent().unwrap()).unwrap();
        std::fs::write(&rom, b"rom").unwrap();
        std::fs::write(rom.with_extension("sav"), b"old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(rom.with_extension("srm"), b"new").unwrap();
        assert_eq!(save_of("gb", &rom).map(|s| s.0), Some(rom.with_extension("srm")));
        let places = save_places("gb", &rom);
        assert!(places.contains(&rom.with_extension("srm")) && places.contains(&rom.with_extension("sav")));
    }

    /// The game's own clock and pocket-ui's time played are told apart: a game played elsewhere (its save says so) is
    /// never "Played: Not yet" under "game clock 31:12".
    #[test]
    fn a_game_played_elsewhere_is_never_called_not_played_yet() {
        let mut d = crate::game_card_model::CardData::default();
        assert_eq!(d.not_played(), "Not yet");
        let p = Progress::Pokemon { badges: 8, lead: None, hours: 31, minutes: 12, art: Default::default() };
        d.progress = Some(p.parts().join(" · "));
        assert_eq!(d.not_played(), "Not here yet");
        assert!(p.parts().iter().any(|x| x == "game clock 31:12"), "the clock says whose it is");
    }

    /// A narrow place (the info page on 640 x 480) leaves the last parts out instead of cutting one with "…".
    #[test]
    fn a_narrow_place_leaves_parts_out_instead_of_cutting_one() {
        use crate::text::{Face, Text};
        let mut t = Text::new();
        let line = "8 badges · Charizard Lv. 45 · game clock 31:12";
        let w = t.width_plain(Face::Body, 28, "8 badges · Charizard Lv. 45") + 2;
        assert_eq!(fit(&mut t, Face::Body, 28, line, w), "8 badges · Charizard Lv. 45");
        assert_eq!(fit(&mut t, Face::Body, 28, line, 10_000), line);
    }

    #[test]
    fn the_line_speaks_the_interface_language() {
        let p = Progress::Pokemon { badges: 8, lead: Some(("Glurak".into(), 45)), hours: 31, minutes: 12, art: Default::default() };
        let de = crate::i18n::Lang::De.table().unwrap();
        let es = crate::i18n::Lang::Es.table().unwrap();
        let parts = p.parts();
        let say = |t: &crate::i18n::Table| parts.iter().map(|s| t.translate(s).map_or(s.clone(), |x| x.into_owned())).collect::<Vec<_>>().join(" · ");
        assert_eq!(say(de), "8 Orden · Glurak Lv. 45 · Spieluhr 31:12");
        assert_eq!(say(es), "8 medallas · Glurak Nv. 45 · reloj del juego 31:12");
        for (p, de_line, es_line) in [
            (Progress::Items { percent: 62, hours: 4, minutes: 5 }, "62 % Items · Spieluhr 4:05", "62 % de objetos · reloj del juego 4:05"),
            (Progress::Exits { done: 43, of: 96 }, "43 von 96 Ausgängen", "43 de 96 salidas"),
            (Progress::Quest { pendants: 2, crystals: 0, hearts: 9, dark_world: false }, "2 von 3 Amuletten · 9 Herzen", "2 de 3 colgantes · 9 corazones"),
            (Progress::Quest { pendants: 3, crystals: 5, hearts: 15, dark_world: true }, "5 von 7 Kristallen · 15 Herzen", "5 de 7 cristales · 15 corazones"),
            (Progress::Pokemon { badges: 0, lead: None, hours: 0, minutes: 9, art: Default::default() }, "Noch keine Orden · Spieluhr 0:09", "Aún sin medallas · reloj del juego 0:09"),
            (Progress::Pokemon { badges: 1, lead: None, hours: 1, minutes: 0, art: Default::default() }, "1 Orden · Spieluhr 1:00", "1 medalla · reloj del juego 1:00"),
            (
                Progress::Chapter { chapter: Some("Chapter 7x".into()), lord: Some("Hector"), mode: Some("Hard mode"), hours: 14, minutes: 2 },
                "Kapitel 7x · Hector · Modus Schwer · Spieluhr 14:02",
                "Capítulo 7x · Hector · Modo difícil · reloj del juego 14:02",
            ),
            (Progress::Complete { percent: 62, of: 102, clock: Some((4, 5)) }, "62 % geschafft · Spieluhr 4:05", "62 % completado · reloj del juego 4:05"),
            (Progress::Shards { done: 5 }, "5 von 8 Spiegelscherben", "5 de 8 fragmentos del espejo"),
            (Progress::Chapter { chapter: Some("Prologue".into()), lord: None, mode: Some("Easy mode"), hours: 0, minutes: 9 }, "Prolog · Modus Leicht · Spieluhr 0:09", "Prólogo · Modo fácil · reloj del juego 0:09"),
        ] {
            let parts = p.parts();
            let say = |t: &crate::i18n::Table| parts.iter().map(|s| t.translate(s).map_or(s.clone(), |x| x.into_owned())).collect::<Vec<_>>().join(" · ");
            assert_eq!(say(de), de_line);
            assert_eq!(say(es), es_line);
        }
        let pt = crate::i18n::Lang::Pt.table().unwrap();
        for (p, pt_line) in [
            (Progress::Chapter { chapter: Some("Chapter 12".into()), lord: Some("Eirika"), mode: Some("Difficult mode"), hours: 9, minutes: 3 }, "Capítulo 12 · Eirika · Modo difícil · relógio do jogo 9:03"),
            (Progress::Complete { percent: 101, of: 101, clock: None }, "101 % concluído"),
            (Progress::Shards { done: 8 }, "8 de 8 fragmentos do espelho"),
        ] {
            let parts = p.parts();
            assert_eq!(parts.iter().map(|s| pt.translate(s).map_or(s.clone(), |x| x.into_owned())).collect::<Vec<_>>().join(" · "), pt_line);
        }
    }
}
