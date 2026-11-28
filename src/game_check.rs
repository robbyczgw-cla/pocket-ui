//! Settings → Games → Game check: "will my games start?", in plain words. Goes through what is on the card on a thread of
//! its own (nice 10, stoppable) and says what is wrong and what to do; nothing here writes a file, downloads or deletes.
//!
//! The checks the app already has stay the truth: what a system needs (`systems_grid::details`, `hostsys`), discs missing
//! a file (`library::look_at_folder`), folders of no system (`library::stray_folders`), zips (`archive::refusal_of`). New
//! here are [`KNOWN_BIOS`], the one table of known-good BIOS files per system (size and CRC-32 from libretro-database's
//! System.dat; hashed only for those files, never for games), and the look at files the shelf doesn't list. Findings are
//! kept per [`Group`], at most [`MAX_PER_GROUP`] each, so a card full of trouble costs no more memory than a small one.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::library::SysInfo;
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
use crate::systems_grid::Bios;

/// Findings kept per group; the rest are counted (`Report::more`).
pub const MAX_PER_GROUP: usize = 200;
/// A BIOS file larger than this is not hashed.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
const MAX_BIOS_BYTES: u64 = 128 << 20;
/// Directory entries looked at per folder of a system.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
const MAX_ENTRIES: usize = 50_000;

/// What the green result says when the look at files stopped at `MAX_ENTRIES` (`Report::capped`).
pub const CAPPED_SUMMARY: &str = "First 50,000 files checked";

/// What a finding is about; the page's first list, in this order.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Group {
    /// Android: a folder the player chose that can't be read any more (`document_check`).
    Access,
    Bios,
    Discs,
    Empty,
    Zips,
    Twice,
    Place,
    Stronger,
    /// Android: a game goes to an emulator app that isn't installed (`document_check`).
    Emulator,
    /// Android: nothing on the phone can start it yet.
    NotHere,
    Tips,
}

impl Group {
    pub const ALL: [Group; 11] = [Group::Access, Group::Bios, Group::Discs, Group::Empty, Group::Zips, Group::Twice, Group::Place, Group::Stronger, Group::Emulator, Group::NotHere, Group::Tips];

    pub fn name(self) -> &'static str {
        match self {
            Group::Access => "Folders that can't be read",
            Group::Bios => "BIOS files",
            Group::Discs => "Discs missing a file",
            Group::Empty => "Empty files",
            Group::Zips => "Zips that won't start",
            Group::Twice => "Same game twice",
            Group::Place => "Wrong folder or file type",
            Group::Stronger => "Needs a stronger handheld",
            Group::Emulator => "Emulator apps to install",
            Group::NotHere => "Can't start in this app yet",
            Group::Tips => "Tips",
        }
    }
}

/// A one-button fix that is always safe to take back.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Fix {
    /// Hide this copy from the shelf (`library_meta`; Settings › System › Storage shows it again).
    Hide(PathBuf),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Finding {
    pub group: Group,
    /// The row: the game, the system or the file.
    pub name: String,
    /// The row's value: the system's short name.
    pub system: String,
    /// What is wrong, one sentence.
    pub what: String,
    /// What to do; empty when `what` says it already.
    pub todo: String,
    /// Where it is, as the player sees it ("roms/snes/Chrono Trigger.gba").
    pub place: Option<String>,
    pub fix: Option<Fix>,
}

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Report {
    /// Sorted by group, then name.
    pub findings: Vec<Finding>,
    /// Games on the shelf.
    pub total: usize,
    /// Of those, the ones nothing stands in the way of.
    pub ready: usize,
    /// Findings beyond `MAX_PER_GROUP`, per group.
    pub more: Vec<(Group, usize)>,
    /// A folder had more than `MAX_ENTRIES` entries, so the look at files stopped there: "All good" would claim too much.
    pub capped: bool,
}

impl Report {
    /// The groups with something in them, with their counts, in order.
    pub fn groups(&self) -> Vec<(Group, usize)> {
        Group::ALL
            .iter()
            .map(|g| (*g, self.findings.iter().filter(|f| f.group == *g).count() + self.more.iter().find(|(m, _)| m == g).map_or(0, |(_, n)| *n)))
            .filter(|(_, n)| *n > 0)
            .collect()
    }

    pub fn in_group(&self, g: Group) -> Vec<&Finding> {
        self.findings.iter().filter(|f| f.group == g).collect()
    }

    /// Nothing but tips: the green result.
    pub fn all_good(&self) -> bool {
        self.findings.iter().all(|f| f.group == Group::Tips)
    }

    pub fn ready_label(&self) -> String {
        let (ready, total) = (self.ready, self.total);
        if total == 1 { format!("{ready} of 1 game ready") } else { format!("{ready} of {total} games ready") }
    }

    /// Include what the system list cost: the whole list given up (the shelf runs on the built-in one) and the systems the
    /// shelf had to leave out, with the entry's place in the original file.
    #[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
    pub fn note_system_list(&mut self, path: &Path) {
        use crate::library::SystemListFallback as Why;
        let status = crate::library::system_list_status(path);
        if status.is_clean() { return; }
        let old = std::mem::take(self);
        let mut found = Findings::default();
        for f in old.findings { found.add(f); }
        if let Some(why) = &status.fallback {
            let what = match why {
                Why::Missing => "The system list is missing, so pocket-ui uses its built-in list.",
                Why::Unreadable(_) => "The system list can't be read, so pocket-ui uses its built-in list.",
                Why::NoSystems => "The system list has no system in it, so pocket-ui uses its built-in list.",
                Why::AllBroken => "Every entry of the system list is damaged, so pocket-ui uses its built-in list.",
            }.to_string();
            found.add(Finding {
                group: Group::Access, name: "es_systems.cfg".into(), system: String::new(),
                what,
                todo: "Games in their standard folders show anyway. To use your own list, fix this file or put a good one back, then scan your games again.".into(),
                place: Some(path.display().to_string()), fix: None,
            });
        }
        for issue in status.skipped {
            found.add(Finding {
                group: Group::Access, name: issue.name, system: String::new(),
                what: "One system was left out because its entry can't be read.".into(),
                todo: "Check this entry in es_systems.cfg, then scan your games again.".into(),
                place: Some(format!("{}:{}", path.display(), issue.line)), fix: None,
            });
        }
        *self = found.report(old.total, old.ready);
        self.capped |= old.capped;
        for (group, n) in old.more {
            if let Some((_, count)) = self.more.iter_mut().find(|(g, _)| *g == group) { *count += n; }
            else { self.more.push((group, n)); }
        }
    }

    /// The Settings row's value.
    pub fn summary(&self) -> String {
        let n: usize = self.groups().iter().filter(|(g, _)| *g != Group::Tips).map(|(_, n)| n).sum();
        match n {
            0 if self.capped => CAPPED_SUMMARY.to_string(),
            0 => "All good".to_string(),
            1 => "1 thing to fix".to_string(),
            n => format!("{n} things to fix"),
        }
    }
}

/// A BIOS file pocket-ui knows, as libretro-database's System.dat lists it.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
struct Known {
    name: &'static str,
    size: u64,
    crc: u32,
}

/// One line per system: its known-good BIOS files, and for a system that plays without one the sentence the tip starts
/// with. A system without a line has its files named by `hostsys`, or none to check by hash (NAOMI's zips differ by packer).
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
struct SystemBios {
    systems: &'static [&'static str],
    files: &'static [Known],
    tip: Option<&'static str>,
}

/// The known file with this size and checksum.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
fn known_as(files: &[Known], size: u64, crc: u32) -> Option<&Known> {
    files.iter().find(|k| k.size == size && k.crc == crc)
}

#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
const fn k(name: &'static str, size: u64, crc: u32) -> Known {
    Known { name, size, crc }
}

#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
const KNOWN_BIOS: &[SystemBios] = &[
    SystemBios {
        systems: &["psx"],
        files: &[
            k("scph5501.bin", 524288, 0x8D8CB7E4),
            k("scph5500.bin", 524288, 0xFF3EEB8C),
            k("scph5502.bin", 524288, 0xD786F0B9),
            k("scph1001.bin", 524288, 0x37157331),
            k("scph7001.bin", 524288, 0x502224B6),
            k("scph101.bin", 524288, 0x171BDCEC),
        ],
        tip: Some("PlayStation games start without a BIOS, and a few run better with one."),
    },
    SystemBios {
        systems: &["segacd", "megacd"],
        files: &[k("bios_CD_U.bin", 131072, 0xC6D10268), k("bios_CD_E.bin", 131072, 0x529AC15A), k("bios_CD_J.bin", 131072, 0x9D2DA8F2)],
        tip: None,
    },
    SystemBios {
        systems: &["pcenginecd", "tg16cd"],
        files: &[k("syscard3.pce", 262144, 0x6D9A73EF), k("syscard2.pce", 262144, 0x283B74E0), k("syscard1.pce", 262144, 0x3F9F95A4), k("gexpress.pce", 32768, 0x51A12D90)],
        tip: None,
    },
    SystemBios {
        systems: &["saturn"],
        files: &[k("saturn_bios.bin", 524288, 0x2ABA43C2), k("sega_101.bin", 524288, 0x224B752C), k("mpr-17933.bin", 524288, 0x4AFCF0FA)],
        tip: Some("Saturn games start without a BIOS, and play in pocket-ui's own player with one."),
    },
    SystemBios { systems: &["fds"], files: &[k("disksys.rom", 8192, 0x5E607DCF)], tip: None },
];

/// The systems ROCKNIX's H700 image has no emulator for (docs/user/getting-started.md, "What works, what is hidden"), by
/// the plain folder names people give them.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
const NOT_ON_H700: &[(&str, &str)] = &[
    ("ps2", "PlayStation 2"),
    ("playstation2", "PlayStation 2"),
    ("gamecube", "GameCube"),
    ("gc", "GameCube"),
    ("ngc", "GameCube"),
    ("wii", "Wii"),
    ("3ds", "Nintendo 3DS"),
    ("nintendo3ds", "Nintendo 3DS"),
];

/// Extensions several systems take: they say nothing about which system a file is for.
const SAY_NOTHING: &[&str] = &["zip", "7z", "bin", "iso", "img", "rom", "cue", "chd", "m3u", "gdi", "pbp", "cso", "dsk", "sh", "squashfs", "exe", "bat", "com", "txt"];
/// Packed files nothing opens: unpack first.
const PACKED: &[&str] = &["rar", "tar", "gz", "tgz", "bz2", "xz", "ace", "arj", "cab"];
/// What a copy or a download leaves when it stopped.
const UNFINISHED: &[&str] = &["part", "crdownload", "tmp", "download", "partial", "opdownload"];
/// Never a game, on top of what zips call clutter (`archive::is_clutter`), saves and states: what emulators and scrapers
/// keep beside games. Also `nsp` and `xci`: no system of ours takes Switch files, so the check says nothing about them,
/// not even a typo (`wrong_file` would offer `.ngp` and `.cci`).
const NOT_GAMES: &[&str] = &["nsp", "xci", "cfg", "conf", "json", "opt", "cht", "mp4", "mkv", "webp", "svg", "db", "sub", "ccd", "sbi", "wav", "mp3", "ogg", "flac", "ape", "ips", "bps", "ups", "xdelta", "pal", "mdf", "dat", "sh", "state", "auto", "nvm", "nv", "hi"];
/// Subfolders of a system's folder that belong to its emulator or to a scraper, not to its games.
pub(crate) const DATA_FOLDERS: &[&str] = &["data", "azahar", "citra", "psp", "save", "saves", "savestates", "states", "system", "media", "images", "manuals", "videos", "covers", "snap", "snaps", "boxart", "downloaded_media", "cheats", "shaders"];

/// The seam between the checks and the place games are kept: a folder on the handheld (`Disk`), a document provider's
/// listing on Android (`document_check`).
pub trait Files {
    /// The size of a file; None when it is no file or its size isn't known.
    fn size(&self, path: &Path) -> Option<u64>;
    /// `archive::refusal_of`.
    fn zip_refusal(&self, zip: &Path, wanted: &[&str], only_cut_off: bool) -> Option<String>;
    /// Where it is, as the player sees it.
    fn shown(&self, path: &Path) -> String;
    /// A game's bytes, open for reading with seeking; None when they can't be read.
    fn open(&self, path: &Path) -> Option<Box<dyn crate::document_check::ReadSeek>>;
    /// Whether the game has saves or save states (`web::saves::files_of`): a version with them is never offered for hiding.
    fn has_saves(&self, _system: &str, _path: &Path) -> bool {
        false
    }
    /// The names of the files in a zip, from its directory; empty for any other file.
    fn inside(&self, _path: &Path) -> Vec<String> {
        Vec::new()
    }
}

/// What the check knows of the games beyond their files, for [`twins`]: the names the checksums or serials gave
/// (`rom_id::known`), the checksums the card has (`rom_id::cached_sums`; neither reads a game) and the games the player
/// has something of (play time, a favourite, a rating or note: `library_meta`, `favorites`, `recents`). Empty: a version
/// is found by its file name alone, and only saves beside it or its states count as the player's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Versions {
    pub names: HashMap<PathBuf, String>,
    pub sums: HashMap<PathBuf, (u32, Option<u32>)>,
    pub played: HashSet<PathBuf>,
    /// The systems' own ROM folders (`check` adds the catalog's): the files right in any of them are one folder for
    /// [`twins`], so a shelf that shows two systems' folders as one (Genesis and Mega Drive) finds a game in both.
    pub roots: HashSet<PathBuf>,
}

/// Disc images and their kin: they may begin with a zero megabyte and still run (Sonic Adventure 2's `.cdi`), so the zeros
/// look says nothing about them.
const DISC_EXTENSIONS: &[&str] = &[
    "cdi", "iso", "bin", "img", "cue", "gdi", "chd", "cso", "ciso", "rvz", "wbfs", "gcm", "gcz", "mdf", "nrg", "pbp", "wux", "wud", "xiso", "ecm", "mds", "toc", "m3u", "dax", "jso", "zso", "3ds", "cia",
];
const EDGE: u64 = 64 << 10;
const SPOT: u64 = 4 << 10;
const SPOTS: u64 = 8;

/// Whether a cartridge game's bytes are zeros all through, as a copy that stopped early leaves them (six GBA games on
/// a real card, 2026-10-22). Cheap on a big shelf: the first 4 KiB of a real game are never all zeros, so nearly every
/// file costs one small read; only a file that starts with zeros has its first and last 64 KiB and eight spots read, and
/// is called empty only when all of that is zeros. Disc images are not looked at (`DISC_EXTENSIONS`).
fn zeros_inside(files: &dyn Files, path: &Path, len: u64) -> bool {
    if len == 0 || DISC_EXTENSIONS.contains(&extension(path).as_str()) || extension(path) == "zip" || extension(path) == "7z" {
        return false;
    }
    let Some(mut file) = files.open(path) else { return false };
    let mut buf = vec![0u8; EDGE as usize];
    let mut all_zero = |at: u64, n: u64| -> bool {
        let n = n.min(len.saturating_sub(at)) as usize;
        let chunk = &mut buf[..n];
        file.seek(SeekFrom::Start(at)).is_ok() && file.read_exact(chunk).is_ok() && chunk.iter().all(|b| *b == 0)
    };
    if !all_zero(0, SPOT) {
        return false;
    }
    if !all_zero(0, EDGE) || !all_zero(len.saturating_sub(EDGE), EDGE) {
        return false;
    }
    (1..=SPOTS).all(|i| all_zero(len / (SPOTS + 1) * i, SPOT))
}

/// The first `MAX_ENTRIES` entries of a folder; `capped` is set when there were more.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
fn read_capped(dir: &Path, capped: &mut bool) -> Vec<std::fs::DirEntry> {
    let mut it = std::fs::read_dir(dir).into_iter().flatten().flatten();
    let first: Vec<_> = it.by_ref().take(MAX_ENTRIES).collect();
    if it.next().is_some() {
        *capped = true;
    }
    first
}

/// The sentences that name the place games are copied to, and what else differs between the two.
pub struct Words {
    pub copy_again: &'static str,
    pub empty_todo: &'static str,
    pub twin_todo: &'static str,
    /// A copy can be hidden from the shelf (`Fix::Hide`).
    pub twin_hides: bool,
    pub short: fn(&SysInfo) -> String,
    /// The systems a file type is most likely for, in order (`owner`).
    pub popular: &'static [&'static str],
}

/// Findings per group, at most `MAX_PER_GROUP` kept, the rest counted.
#[derive(Default)]
pub struct Findings {
    list: Vec<Finding>,
    counts: HashMap<Group, usize>,
    /// A folder was longer than `MAX_ENTRIES` (`read_capped`).
    pub capped: bool,
}

impl Findings {
    pub fn add(&mut self, f: Finding) {
        let n = self.counts.entry(f.group).or_default();
        *n += 1;
        if *n <= MAX_PER_GROUP {
            self.list.push(f);
        }
    }

    /// The report, sorted by group, then name.
    pub fn report(mut self, total: usize, ready: usize) -> Report {
        let order = |g: Group| Group::ALL.iter().position(|a| *a == g).unwrap_or(0);
        self.list.sort_by_cached_key(|f| (order(f.group), f.name.to_lowercase()));
        let more = Group::ALL.iter().filter_map(|g| self.counts.get(g).filter(|n| **n > MAX_PER_GROUP).map(|n| (*g, n - MAX_PER_GROUP))).collect();
        Report { findings: self.list, total, ready, more, capped: self.capped }
    }
}

/// Between two files: a stop is taken at once, and now and then the storage gets a breath, so the shelf's reads go first.
pub fn breath(stop: &AtomicBool, n: usize) -> Option<()> {
    if stop.load(Ordering::Relaxed) {
        return None;
    }
    if n.is_multiple_of(64) {
        std::thread::sleep(Duration::from_millis(1));
    }
    Some(())
}

/// The handheld's card.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
struct Disk<'a> {
    roms: &'a Path,
}

#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
impl Files for Disk<'_> {
    fn size(&self, path: &Path) -> Option<u64> {
        std::fs::metadata(path).ok().filter(|m| m.is_file()).map(|m| m.len())
    }

    fn zip_refusal(&self, zip: &Path, wanted: &[&str], only_cut_off: bool) -> Option<String> {
        crate::archive::refusal_of(zip, wanted, only_cut_off)
    }

    /// "roms/snes/Game.sfc" for what is under the roms folder, else the path itself.
    fn shown(&self, p: &Path) -> String {
        let base = self.roms.parent().unwrap_or(self.roms);
        p.strip_prefix(base).unwrap_or(p).to_string_lossy().into_owned()
    }

    fn open(&self, path: &Path) -> Option<Box<dyn crate::document_check::ReadSeek>> {
        std::fs::File::open(path).ok().map(|f| Box::new(f) as Box<dyn crate::document_check::ReadSeek>)
    }

    fn has_saves(&self, system: &str, path: &Path) -> bool {
        let roots = crate::web::files::Roots { roms: self.roms.to_path_buf(), ..Default::default() };
        !crate::web::saves::files_of(&roots, system, path, false).is_empty()
    }

    fn inside(&self, path: &Path) -> Vec<String> {
        if extension(path) != "zip" {
            return Vec::new();
        }
        crate::archive::listing(path, |_| false).ok().flatten().map_or(Vec::new(), |l| l.entries.into_iter().map(|(name, _)| name).collect())
    }
}

#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
const HANDHELD: Words = Words {
    copy_again: "Copy the game to the SD card again.",
    empty_todo: "A copy or a download stopped early. Copy the game to the SD card again.",
    twin_todo: "Hide this copy from the shelf. The file stays on the card.",
    twin_hides: true,
    short: crate::systems_grid::short_name,
    popular: &crate::systems_grid::POPULAR,
};

/// What the check is given: the App's lists, so what counts as a game is what the shelf says.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
pub struct Input {
    pub catalog: Vec<SysInfo>,
    /// The games on the shelf per system id, hidden ones left out.
    pub shelf: Vec<(String, Vec<PathBuf>)>,
    /// `<roms>/bios`.
    pub bios_dir: PathBuf,
    pub roms: PathBuf,
    /// An H700 handheld or the RG DS Plus (`device::Model::small_chip`): a stray folder of a system it has no emulator
    /// for needs a stronger handheld.
    pub h700: bool,
    /// Names, checksums and the player's games, to find one game in several versions ([`Versions`]).
    pub versions: Versions,
}

/// Collects the findings and how far the work has come.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
struct Run<'a> {
    input: &'a Input,
    /// `input.versions` with the catalog's folders as `roots`.
    versions: Versions,
    disk: Disk<'a>,
    stop: &'a AtomicBool,
    progress: &'a mut dyn FnMut(usize, usize, &str),
    done: usize,
    total: usize,
    found: Findings,
    blocked_systems: HashSet<String>,
    blocked_games: HashSet<PathBuf>,
}

/// Goes through the card. None: stopped.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
pub fn check(input: &Input, stop: &AtomicBool, progress: &mut dyn FnMut(usize, usize, &str)) -> Option<Report> {
    let total = input.shelf.iter().map(|(_, g)| g.len()).sum();
    let mut versions = input.versions.clone();
    versions.roots.extend(input.catalog.iter().map(|s| s.path.clone()));
    let mut run = Run { input, versions, disk: Disk { roms: &input.roms }, stop, progress, done: 0, total, found: Findings::default(), blocked_systems: HashSet::new(), blocked_games: HashSet::new() };
    run.bios()?;
    run.games()?;
    run.folders()?;
    run.stray()?;
    let blocked_by_system: usize = input.shelf.iter().filter(|(id, _)| run.blocked_systems.contains(id)).map(|(_, g)| g.len()).sum();
    let blocked_alone = input.shelf.iter().filter(|(id, _)| !run.blocked_systems.contains(id)).flat_map(|(_, g)| g).filter(|p| run.blocked_games.contains(*p)).count();
    Some(run.found.report(total, total - blocked_by_system - blocked_alone))
}

#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
impl Run<'_> {
    fn add(&mut self, f: Finding) {
        self.found.add(f);
    }

    fn step(&mut self, n: usize) -> Option<()> {
        breath(self.stop, n)
    }

    fn info(&self, id: &str) -> Option<&SysInfo> {
        self.input.catalog.iter().find(|s| s.id == id)
    }

    fn shown(&self, p: &Path) -> String {
        self.disk.shown(p)
    }

    fn system_name(&self, id: &str) -> String {
        self.info(id).map_or_else(|| id.to_string(), crate::systems_grid::short_name)
    }

    // ---- BIOS --------------------------------------------------------------------------------------------------

    fn bios(&mut self) -> Option<()> {
        let ids: Vec<String> = self.input.shelf.iter().filter(|(_, g)| !g.is_empty()).map(|(id, _)| id.clone()).collect();
        for id in ids {
            self.step(0)?;
            let Some(info) = self.info(&id).cloned() else { continue };
            (self.progress)(self.done, self.total, &info.name);
            let line = KNOWN_BIOS.iter().find(|l| l.systems.contains(&id.as_str()));
            if let Some(line) = line {
                self.hashes(&id, line)?;
            }
            let wanted: Vec<&'static str> = crate::hostsys::find(&id).map_or(Vec::new(), |s| s.bios.iter().flat_map(|b| b.files.iter().copied()).collect());
            match crate::systems_grid::details(&info, &self.input.bios_dir).1 {
                Bios::Needed { missing: Some(message), .. } => {
                    let optional = crate::hostsys::find(&id).is_some_and(|s| s.default_without_bios);
                    let message = match line.and_then(|l| l.tip).filter(|_| optional) {
                        Some(tip) => format!("{tip} Put {} into roms/bios.", wanted.first().unwrap_or(&"")),
                        None => message,
                    };
                    self.absent(&id, &wanted, message, optional)?;
                }
                _ => {
                    // No file it can't do without. A tip when one helps and none is in the folder.
                    if let Some(tip) = line.and_then(|l| l.tip) {
                        let files: Vec<&str> = line.map_or(Vec::new(), |l| l.files.iter().map(|k| k.name).collect());
                        if !files.iter().any(|f| self.input.bios_dir.join(f).is_file()) {
                            self.absent(&id, &files, format!("{tip} Put {} into roms/bios.", files[0]), true)?;
                        }
                    }
                }
            }
        }
        Some(())
    }

    /// The known files of a system's line that are in the BIOS folder: empty, or not the file the name promises.
    fn hashes(&mut self, id: &str, line: &SystemBios) -> Option<()> {
        for known in line.files {
            let path = self.input.bios_dir.join(known.name);
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            let system = self.system_name(id);
            let place = Some(self.shown(&path));
            if meta.len() == 0 {
                self.add(Finding {
                    group: Group::Bios,
                    name: known.name.to_string(),
                    system,
                    what: "This BIOS file is empty.".into(),
                    todo: format!("Copy {} into roms/bios again.", known.name),
                    place,
                    fix: None,
                });
                if line.tip.is_none() {
                    self.blocked_systems.insert(id.to_string());
                }
                continue;
            }
            if meta.len() > MAX_BIOS_BYTES {
                continue;
            }
            let good = meta.len() == known.size && crc_of(&path, self.stop)? == Some(known.crc);
            if !good {
                self.add(Finding {
                    group: Group::Bios,
                    name: known.name.to_string(),
                    system,
                    what: "Wrong BIOS: its checksum differs from the file with this name.".into(),
                    todo: format!("Games may not start or may crash. Copy your own console's {} again.", known.name),
                    place,
                    fix: None,
                });
            }
        }
        Some(())
    }

    /// A BIOS file is not where it should be: elsewhere under another folder or spelling, else simply missing.
    fn absent(&mut self, id: &str, wanted: &[&str], message: String, optional: bool) -> Option<()> {
        let system = self.system_name(id);
        let group = if optional { Group::Tips } else { Group::Bios };
        let found = self.elsewhere(id, wanted)?;
        if let Some((path, target)) = found {
            let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let target_name = target.rsplit('/').next().unwrap_or(&target).to_string();
            let same_folder = path.parent() == self.input.bios_dir.join(&target).parent();
            let (what, todo) = if same_folder {
                (format!("{system}: the BIOS file's name is not quite right."), format!("Rename {file} to {target_name}."))
            } else {
                let folder = Path::new("roms/bios").join(Path::new(&target).parent().unwrap_or(Path::new(""))).to_string_lossy().trim_end_matches('/').to_string();
                (format!("{system}: the BIOS file is in the wrong folder."), format!("Move {file} to {folder}."))
            };
            let place = Some(self.shown(&path));
            self.add(Finding { group, name: file, system, what, todo, place, fix: None });
        } else {
            let what = if optional { format!("{system}: no BIOS.") } else { format!("{system}: BIOS missing.") };
            let todo = if optional { message } else { format!("{}. BIOS files come from your own console. {}", message.trim_end_matches('.'), bios_copy_way()) };
            self.add(Finding { group, name: system.clone(), system, what, todo, place: None, fix: None });
        }
        if !optional {
            self.blocked_systems.insert(id.to_string());
        }
        Some(())
    }

    /// One of `wanted` where it isn't read: the BIOS folder under another spelling or in a subfolder, `roms` itself, the
    /// system's folder. Then, for a name of the table: a file of the BIOS folder with its checksum ("scph5501 (1).bin").
    /// The file and the name it should have, as `wanted` writes it.
    fn elsewhere(&mut self, id: &str, wanted: &[&str]) -> Option<Option<(PathBuf, String)>> {
        let bios = self.input.bios_dir.clone();
        let mut dirs = vec![bios.clone()];
        dirs.extend(std::fs::read_dir(&bios).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir() && !is_litter(p)));
        dirs.push(self.input.roms.clone());
        if let Some(info) = self.info(id) {
            dirs.push(info.path.clone());
        }
        for dir in &dirs {
            for path in std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()) {
                self.step(1)?;
                let Some(file) = path.file_name().and_then(|n| n.to_str()) else { continue };
                if crate::library::computer_litter(file) {
                    continue;
                }
                let hit = wanted.iter().find(|w| w.rsplit('/').next().is_some_and(|base| base.eq_ignore_ascii_case(file)));
                if let Some(w) = hit {
                    if path.is_file() && path != bios.join(w) {
                        return Some(Some((path, w.to_string())));
                    }
                }
            }
        }
        let files = KNOWN_BIOS.iter().find(|l| l.systems.contains(&id)).map_or(&[][..], |l| l.files);
        for path in std::fs::read_dir(&bios).into_iter().flatten().flatten().map(|e| e.path()) {
            self.step(1)?;
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if is_litter(&path) || !meta.is_file() || !files.iter().any(|k| k.size == meta.len()) {
                continue;
            }
            if let Some(k) = crc_of(&path, self.stop)?.and_then(|crc| known_as(files, meta.len(), crc)) {
                return Some(Some((path, k.name.to_string())));
            }
        }
        Some(None)
    }

    // ---- what is on the shelf ------------------------------------------------------------------------------------

    fn games(&mut self) -> Option<()> {
        let shelf = &self.input.shelf;
        for (id, games) in shelf {
            let Some(info) = self.info(id).cloned() else { continue };
            (self.progress)(self.done, self.total, &info.name);
            let (stop, progress, done, total) = (self.stop, &mut self.progress, &mut self.done, self.total);
            let mut between = |i: usize| {
                breath(stop, i)?;
                *done += 1;
                if i.is_multiple_of(128) {
                    progress(*done, total, &info.name);
                }
                Some(())
            };
            check_system(&mut self.found, &self.disk, &HANDHELD, &info, games, &self.versions, &mut self.blocked_games, &mut between)?;
        }
        Some(())
    }

    // ---- what the shelf doesn't list -----------------------------------------------------------------------------

    /// Discs missing a file, and files in a system's folder that it can't take.
    fn folders(&mut self) -> Option<()> {
        let catalog = self.input.catalog.clone();
        let mut seen: HashSet<PathBuf> = HashSet::new();
        for info in &catalog {
            if !seen.insert(info.path.clone()) {
                continue;
            }
            self.step(0)?;
            if !info.path.is_dir() {
                continue;
            }
            // Every system of the folder counts for what it takes.
            let mut exts: Vec<String> = catalog.iter().filter(|s| s.path == info.path).flat_map(|s| s.extensions.iter().cloned()).collect();
            exts.extend(crate::hostsys::shelf_extensions(&info.id).iter().map(|e| e.to_string()));
            let (_, broken) = crate::library::look_at_folder(&info.id, &info.path, &exts);
            for disc in broken {
                let name = name_of(&disc.sheet);
                let sheet = disc.sheet.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let place = Some(self.shown(&disc.sheet));
                self.add(Finding {
                    group: Group::Discs,
                    name,
                    system: crate::systems_grid::short_name(info),
                    what: format!("Disc: {sheet} is there, its {} is missing.", disc.missing),
                    todo: format!("Put {} next to it, with exactly this name.", disc.missing),
                    place,
                    fix: None,
                });
            }
            if crate::hostsys::folders_are_games(&info.id) || exts.is_empty() || info.id == "ports" {
                continue;
            }
            self.loose_files(info, &catalog, &exts)?;
        }
        // Loose in roms itself: every system's files are welcome here, and none is read.
        let roms = self.input.roms.clone();
        let all: Vec<String> = catalog.iter().flat_map(|s| s.extensions.iter().cloned()).collect();
        let mut n = 0;
        for path in std::fs::read_dir(&roms).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_file()) {
            n += 1;
            self.step(n)?;
            let ext = extension(&path);
            if n > MAX_ENTRIES {
                self.found.capped = true;
            }
            if n > MAX_ENTRIES || !all.contains(&ext) || not_a_game_file(&path, &ext) || crate::library::computer_litter(&file_of(&path)) {
                continue;
            }
            let todo = match owner(&catalog, &ext, HANDHELD.popular) {
                Some(info) => format!("Move it to {}.", info.folder),
                None => "Games go into the folder of their system, for example roms/snes.".to_string(),
            };
            let place = Some(self.shown(&path));
            self.add(Finding { group: Group::Place, name: file_of(&path), system: String::new(), what: "This game is not in a system's folder, so nothing reads it.".into(), todo, place, fix: None });
        }
        Some(())
    }

    /// The system's folder and one level of subfolders, and the folders two levels down that nothing reads.
    fn loose_files(&mut self, info: &SysInfo, catalog: &[SysInfo], exts: &[String]) -> Option<()> {
        let mut subs = Vec::new();
        self.look_at_files(info, catalog, exts, &info.path, &mut subs)?;
        for sub in subs {
            let mut deeper = Vec::new();
            self.look_at_files(info, catalog, exts, &sub, &mut deeper)?;
            for dir in deeper {
                let holds = read_capped(&dir, &mut self.found.capped).into_iter().map(|e| e.path()).find(|p| p.is_file() && exts.contains(&extension(p)) && !matches!(extension(p).as_str(), "zip" | "7z") && !crate::library::computer_litter(&file_of(p)));
                if let Some(file) = holds {
                    let place = Some(self.shown(&file));
                    self.add(Finding {
                        group: Group::Place,
                        name: name_of(&file),
                        system: crate::systems_grid::short_name(info),
                        what: "This game is too deep in its folders: only one level of subfolders is read.".into(),
                        todo: format!("Move it up, to {} or a folder right in it.", info.folder),
                        place,
                        fix: None,
                    });
                }
            }
        }
        Some(())
    }

    fn look_at_files(&mut self, info: &SysInfo, catalog: &[SysInfo], exts: &[String], dir: &Path, subs: &mut Vec<PathBuf>) -> Option<()> {
        for (n, entry) in read_capped(dir, &mut self.found.capped).into_iter().enumerate() {
            self.step(n)?;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if crate::library::computer_litter(&name) {
                continue;
            }
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                if !name.starts_with('_') && !DATA_FOLDERS.contains(&name.to_lowercase().as_str()) {
                    subs.push(path);
                }
                continue;
            }
            let ext = extension(&path);
            if exts.contains(&ext) {
                continue;
            }
            if let Some((what, todo)) = wrong_file(&self.disk, &HANDHELD, info, catalog, exts, &path, &ext) {
                let place = Some(self.shown(&path));
                self.add(Finding { group: Group::Place, name: file_of(&path), system: crate::systems_grid::short_name(info), what, todo, place, fix: None });
            }
        }
        Some(())
    }

    /// Folders in `roms` that are no system's: a typo, or a system this handheld can't play.
    fn stray(&mut self) -> Option<()> {
        self.step(0)?;
        let strays = crate::library::stray_folders(&self.input.roms, &self.input.catalog);
        for s in strays {
            let plain: String = s.name.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
            let place = Some(format!("roms/{}", s.name));
            let weak = NOT_ON_H700.iter().find(|(f, _)| *f == plain).filter(|_| self.input.h700);
            if let Some((_, system)) = weak {
                self.add(Finding {
                    group: Group::Stronger,
                    name: system.to_string(),
                    system: String::new(),
                    what: format!("{system} needs a stronger handheld."),
                    todo: "This handheld has no emulator for it, so these games don't show on the shelf. They are safe on the card.".into(),
                    place,
                    fix: None,
                });
                continue;
            }
            let (what, todo) = (format!("Folder without a system: {}.", s.name), "Nothing reads this folder. Add games shows the name each system's folder has.".to_string());
            self.add(Finding { group: Group::Place, name: s.name.clone(), system: String::new(), what, todo, place, fix: None });
        }
        Some(())
    }
}

/// One system's games on the shelf: empty files, zips that won't start, the same game twice. `between` comes before each
/// game; None from it stops the check.
#[allow(clippy::too_many_arguments)]
pub fn check_system(found: &mut Findings, files: &dyn Files, words: &Words, info: &SysInfo, games: &[PathBuf], versions: &Versions, blocked: &mut HashSet<PathBuf>, between: &mut dyn FnMut(usize) -> Option<()>) -> Option<()> {
    let id = info.id.as_str();
    let wanted: Vec<&str> = info.extensions.iter().map(String::as_str).filter(|e| !matches!(*e, "zip" | "7z")).collect();
    let cartridge = crate::rom_id::dat_name(id).is_some();
    let mut sizes: Vec<(&PathBuf, u64)> = Vec::with_capacity(games.len());
    for (i, path) in games.iter().enumerate() {
        between(i)?;
        let Some(len) = files.size(path) else { continue };
        sizes.push((path, len));
        let system = (words.short)(info);
        let place = Some(files.shown(path));
        if len == 0 && !empty_on_purpose(id, path) {
            found.add(Finding {
                group: Group::Empty,
                name: name_of(path),
                system,
                what: "This file is empty.".into(),
                todo: words.empty_todo.into(),
                place,
                fix: None,
            });
            blocked.insert(path.clone());
            continue;
        }
        if cartridge && zeros_inside(files, path, len) {
            found.add(Finding {
                group: Group::Empty,
                name: name_of(path),
                system,
                what: "This file is empty inside.".into(),
                todo: words.copy_again.into(),
                place,
                fix: None,
            });
            blocked.insert(path.clone());
            continue;
        }
        // A zip or a 7z: the archive's refusal (`archive::refusal_in`, `seven_zip::refusal_in`), from its directory alone.
        if crate::archive::archive_type(path).is_some() {
            if let Some(why) = files.zip_refusal(path, &wanted, !cartridge) {
                // The same sentence the shelf would say, with its advice in it.
                found.add(Finding { group: Group::Zips, name: name_of(path), system, what: why, todo: String::new(), place, fix: None });
                blocked.insert(path.clone());
            }
        }
    }
    twins(found, files, words, info, &sizes, versions);
    Some(())
}

/// The same game twice on a system's shelf, in one folder (games of one name in folders of their own are games of their
/// own: a "Kids" or "Hacks" folder is the player's choice; the folders of systems one shelf shows together, `roots`,
/// are one):
/// * a copy: the same name but for "(1)" or "- Copy", as the same size or as two file types;
/// * one checksum under two names (`rom_id`'s cache: a zip's from its directory, a loose cartridge's read once);
/// * the same game in another version: another region, revision, dump or file type, "Legend of Zelda, The" for "The
///   Legend of Zelda", "0457-Chrono Trigger (US)" for "Chrono Trigger (USA)", by the name its checksum gave or by its
///   file name ([`version_title`]). A hack, a fan translation, a beta or a Japanese release is a game of its own.
///
/// The one that stays is the one the player has something of (saves, states, play time, a favourite), then the one
/// without a copy mark, Europe or World before USA before the rest ([`region_rank`]), the newest revision, a dump the
/// checksum list knows, the shortest path. A version the player has something of is never offered for hiding: what
/// they played is never parted from its game. Nothing is hashed here.
fn twins(found: &mut Findings, files: &dyn Files, words: &Words, info: &SysInfo, sizes: &[(&PathBuf, u64)], versions: &Versions) {
    let stem_of = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let copy_marked = |p: &Path| {
        let stem = stem_of(p);
        without_copy_mark(&stem).len() != stem.len()
    };
    // The folder a game is compared in: its own, or for a game right in a system's folder that of every system on this
    // shelf (Genesis and Mega Drive on one shelf are one folder; "Kids" stays a folder of its own).
    let folder = |p: &Path| -> Option<PathBuf> {
        let dir = p.parent()?;
        Some(if versions.roots.contains(dir) { PathBuf::new() } else { dir.to_path_buf() })
    };
    let n = sizes.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    fn join(parent: &mut [usize], a: usize, b: usize) {
        let (ra, rb) = (root(parent, a), root(parent, b));
        if ra != rb {
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    // Copies: per folder and name without the copy mark; the one without a mark (then the shortest path) is the
    // original, and a copy of the same type but another size is something else.
    let mut by_name: HashMap<(Option<PathBuf>, String), Vec<usize>> = HashMap::new();
    for (i, (path, _)) in sizes.iter().enumerate() {
        by_name.entry((folder(path), without_copy_mark(&stem_of(path)).to_lowercase())).or_default().push(i);
    }
    for group in by_name.values().filter(|g| g.len() > 1) {
        let mut group = group.clone();
        group.sort_by_cached_key(|&i| (copy_marked(sizes[i].0), sizes[i].0.as_os_str().len(), sizes[i].0.clone()));
        let (first, first_len) = sizes[group[0]];
        for &i in &group[1..] {
            let (path, len) = sizes[i];
            let same_type = path.extension().map(|e| e.to_ascii_lowercase()) == first.extension().map(|e| e.to_ascii_lowercase());
            if !(same_type && len != first_len) {
                join(&mut parent, group[0], i);
            }
        }
    }
    // One checksum, and one title: the file's and the one its checksum gave. A copy goes by the rule above alone, and a
    // zip whose game is a hack or translation inside is no version of anything.
    #[derive(Hash, PartialEq, Eq)]
    enum Key {
        Sum(u32),
        Title(String),
    }
    let by_titles = titles_tell(&info.id);
    let titles = |i: usize, look_inside: bool| -> Vec<String> {
        let path = sizes[i].0;
        let inner_edition = || files.inside(path).iter().any(|f| version_title(&stem_of(Path::new(f))).is_none());
        if !by_titles || copy_marked(path) || (look_inside && inner_edition()) {
            return Vec::new();
        }
        let mut out: Vec<String> = version_title(&stem_of(path)).into_iter().collect();
        out.extend(versions.names.get(path).and_then(|n| version_title(n)));
        out
    };
    let mut by_title: HashMap<(Option<PathBuf>, String), Vec<usize>> = HashMap::new();
    for (i, (path, _)) in sizes.iter().enumerate() {
        for t in titles(i, false) {
            by_title.entry((folder(path), t)).or_default().push(i);
        }
    }
    // Only the zips that would join another game have their directory read.
    let suspects: HashSet<usize> = by_title.values().filter(|g| g.len() > 1).flatten().copied().collect();
    let mut by_key: HashMap<(Option<PathBuf>, Key), usize> = HashMap::new();
    for (i, (path, _)) in sizes.iter().enumerate() {
        let path: &Path = path;
        let mut keys: Vec<Key> = Vec::new();
        if let Some((crc, alt)) = versions.sums.get(path) {
            keys.push(Key::Sum(*crc));
            keys.extend(alt.map(Key::Sum));
        }
        keys.extend(titles(i, suspects.contains(&i)).into_iter().map(Key::Title));
        for key in keys {
            match by_key.entry((folder(path), key)) {
                std::collections::hash_map::Entry::Occupied(e) => join(&mut parent, *e.get(), i),
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(i);
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = root(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let mut groups: Vec<Vec<usize>> = groups.into_values().filter(|g| g.len() > 1).collect();
    groups.sort_by_key(|g| sizes[g[0]].0.clone());
    for group in groups {
        let played = |i: usize| versions.played.contains(sizes[i].0) || files.has_saves(&info.id, sizes[i].0);
        let mut ranked: Vec<(bool, usize)> = group.iter().map(|&i| (played(i), i)).collect();
        ranked.sort_by_cached_key(|&(played, i)| {
            let path = sizes[i].0;
            let named = versions.names.get(path);
            let name = named.cloned().unwrap_or_else(|| stem_of(path));
            (!played, copy_marked(path), region_rank(&name), std::cmp::Reverse(revision(&name)), named.is_none(), path.as_os_str().len(), path.to_path_buf())
        });
        let first = sizes[ranked[0].1].0;
        for &(played, i) in &ranked[1..] {
            if played {
                continue;
            }
            let path = sizes[i].0;
            let (kept, place) = (files.shown(first), Some(files.shown(path)));
            found.add(Finding {
                group: Group::Twice,
                what: format!("Same game twice: {kept} is the same game."),
                todo: words.twin_todo.into(),
                name: file_of(path),
                system: (words.short)(info),
                place,
                fix: words.twin_hides.then(|| Fix::Hide(path.clone())),
            });
        }
    }
}

/// The tags of a name: what stands in round or square brackets, without them.
fn tags_of(name: &str) -> Vec<String> {
    let (mut out, mut tag, mut depth) = (Vec::new(), String::new(), 0);
    for ch in name.chars() {
        match ch {
            '(' | '[' => {
                if depth > 0 {
                    tag.push(ch);
                }
                depth += 1;
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    out.push(std::mem::take(&mut tag).trim().to_string());
                } else {
                    tag.push(ch);
                }
            }
            _ if depth > 0 => tag.push(ch),
            _ => {}
        }
    }
    out
}

/// A tag that makes a file a game of its own rather than a version: a hack, a patch, a fan translation ("T+Eng",
/// "[T-Ger]"), a restoration, a beta or demo, a randomizer, or a named mod with its own version number ("Emerald Rogue
/// v2.2.0", "Retro Track Grand Prix v2.1"). GoodTools' "[h1]" (hack) and "[t1]" (trainer) count as well.
fn own_edition(tag: &str) -> bool {
    const MARKS: &[&str] = &["hack", "patch", "restor", "translat", "t+", "t-", "beta", "proto", "demo", "sample", "kiosk", "randomiz", "kaizo", "remaster", "remix", "redux", "fan "];
    let t = tag.to_lowercase();
    if MARKS.iter().any(|m| t.contains(m)) {
        return true;
    }
    let mut c = t.chars();
    if matches!(c.next(), Some('h' | 't')) && c.next().is_some_and(|d| d.is_ascii_digit()) {
        return true;
    }
    let words: Vec<&str> = t.split_whitespace().collect();
    words.len() > 1 && words.iter().any(|w| w.strip_prefix('v').is_some_and(|v| v.starts_with(|d: char| d.is_ascii_digit())))
}

/// The regions of a name's tags, lowercased: "(USA, Europe)" → usa, europe; GoodTools' "(UE)" counts too.
fn regions_of(name: &str) -> Vec<String> {
    const REGIONS: &[&str] = &[
        "world", "europe", "usa", "japan", "australia", "germany", "france", "spain", "italy", "uk", "netherlands", "sweden", "scandinavia", "brazil", "canada", "korea", "china", "taiwan", "asia",
        "hong kong", "w", "e", "u", "j", "ue", "eu", "us", "ju", "jue", "g", "f", "s", "i", "k", "a", "b",
    ];
    tags_of(name)
        .iter()
        .flat_map(|t| {
            let parts: Vec<String> = t.split(',').map(|p| p.trim().to_lowercase()).collect();
            if parts.iter().all(|p| REGIONS.contains(&p.as_str())) { parts } else { Vec::new() }
        })
        .collect()
}

/// Which version stays when nothing else decides: 0 Europe or the whole world, 1 USA, 2 another region, 3 none named.
fn region_rank(name: &str) -> u8 {
    let rank = |r: &str| match r {
        "world" | "europe" | "w" | "e" | "ue" | "eu" | "jue" | "germany" | "g" | "uk" | "france" | "f" | "spain" | "s" | "italy" | "i" | "netherlands" | "sweden" | "scandinavia" => 0,
        "usa" | "u" | "us" | "ju" | "canada" | "australia" | "a" => 1,
        _ => 2,
    };
    regions_of(name).iter().map(|r| rank(r)).min().unwrap_or(3)
}

/// A name's revision: "(Rev 2)" 2000, "(Rev A)" 1000, "(V1.1)" 1001, "(v1.02)" 1002; none 0.
fn revision(name: &str) -> u32 {
    tags_of(name)
        .iter()
        .filter_map(|t| {
            let t = t.to_lowercase();
            if let Some(r) = t.strip_prefix("rev ") {
                let r = r.trim();
                return r.parse::<u32>().ok().or_else(|| (r.len() == 1).then(|| r.bytes().next().map(|b| u32::from(b.saturating_sub(b'a')) + 1)).flatten()).map(|n| n * 1000);
            }
            let v = t.strip_prefix('v')?;
            let (major, minor) = v.split_once('.').unwrap_or((v, "0"));
            Some(major.parse::<u32>().ok()? * 1000 + minor.parse::<u32>().ok()?)
        })
        .max()
        .unwrap_or(0)
}

/// The title two versions of a game share: the name without tags, release number, version and article, "&" read as
/// "and", Roman numbers as digits, accents, spaces and punctuation left out ("Legend of Zelda, The - A Link to the Past
/// (USA)" and "Zelda - A Link to the Past" are not one; "Street Fighter II Turbo" and "0925-Street Fighter 2 Turbo (US)"
/// are). None for a hack, translation or the like ([`own_edition`]). A Japanese release gets its own title: a fan
/// translation of it often has no tag at all, and is played for being another game.
pub(crate) fn version_title(stem: &str) -> Option<String> {
    let tags = tags_of(stem);
    if tags.iter().any(|t| own_edition(t)) {
        return None;
    }
    // "(Disc 2)", "(Side B)": a part of the game, not a version of it.
    let part: String = tags
        .iter()
        .map(|t| t.to_lowercase())
        .filter(|t| ["disc", "disk", "cd", "side", "tape", "part"].iter().any(|w| t.strip_prefix(w).is_some_and(|r| r.starts_with([' ', '-']) || r.starts_with(|c: char| c.is_ascii_digit()))))
        .map(|t| t.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .collect();
    let regions = regions_of(stem);
    let asian = !regions.is_empty() && regions.iter().all(|r| matches!(r.as_str(), "japan" | "j" | "korea" | "k" | "china" | "taiwan" | "asia" | "hong kong"));
    let fold = |c: char| match c {
        'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
        'è' | 'é' | 'ê' | 'ë' => 'e',
        'ì' | 'í' | 'î' | 'ï' => 'i',
        'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
        'ù' | 'ú' | 'û' | 'ü' => 'u',
        'ñ' => 'n',
        'ç' => 'c',
        c => c,
    };
    let plain: String = crate::library::display_name(stem).to_lowercase().replace('&', " and ").chars().map(fold).collect();
    let words: Vec<&str> = plain.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let roman = |w: &str| match w {
        "ii" => "2", "iii" => "3", "iv" => "4", "v" => "5", "vi" => "6", "vii" => "7", "viii" => "8", "ix" => "9",
        w => w,
    }
    .to_string();
    let skip = usize::from(words.first().is_some_and(|w| matches!(*w, "the" | "a" | "an")) && words.len() > 1);
    let title: String = words[skip..].iter().map(|w| roman(w)).collect();
    (!title.is_empty()).then(|| format!("{title}{}{part}", if asian { "#asia" } else { "" }))
}

/// Systems whose files are named after what they are, so a title says which game: not arcade sets ("sf2ce.zip" is a
/// set, its clones share its ROMs), folders per game, ports, carts and the like (`hostsys::folders_are_games`).
fn titles_tell(system: &str) -> bool {
    const NOT: &[&str] = &[
        "arcade", "mame", "fbn", "fbneo", "cps1", "cps2", "cps3", "neogeo", "neocd", "naomi", "atomiswave", "daphne", "model2", "model3", "ports", "pico-8", "scummvm", "j2me", "easyrpg", "doom",
        "tic-80", "lowres-nx", "wasm4", "openbor", "solarus", "heroic", "steam", "windows", "moonlight", "dos", "pc",
    ];
    #[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
    let folders = crate::hostsys::folders_are_games(system);
    #[cfg(all(target_os = "android", not(feature = "handheld")))]
    let folders = false;
    !NOT.contains(&system) && !folders
}

/// CRC-32 of a file, chunk by chunk. Outer None: stopped; inner None: can't be read.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
fn crc_of(path: &Path, stop: &AtomicBool) -> Option<Option<u32>> {
    let Ok(mut f) = std::fs::File::open(path) else { return Some(None) };
    let mut hasher = crc32fast::Hasher::new();
    let mut buf = vec![0; 64 << 10];
    loop {
        if stop.load(Ordering::Relaxed) {
            return None;
        }
        match f.read(&mut buf) {
            Ok(0) => return Some(Some(hasher.finalize())),
            Ok(n) => hasher.update(&buf[..n]),
            Err(_) => return Some(None),
        }
    }
}

pub(crate) fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default()
}

/// The name a finding's row shows.
pub(crate) fn name_of(path: &Path) -> String {
    crate::library::shelf_name(&path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
}

/// ROCKNIX's own PICO-8 entry "Splore" is an empty picture: the name starts the game browser, and an empty file is right.
fn empty_on_purpose(system: &str, path: &Path) -> bool {
    system == "pico-8" && path.file_stem().is_some_and(|s| s.eq_ignore_ascii_case("splore"))
}

/// The file's whole name, for a row about the name itself; painted as it is, in every language.
pub(crate) fn file_of(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    crate::i18n::learn_names([&name]);
    name
}

/// A game's file name without what a copy adds: "Game (1)", "Game - Copy", "Game - Kopie", "Game copy 2". A number of
/// four digits or one glued to the name is no copy's: old scene sets end names with their own ("(US)(4858)").
pub fn without_copy_mark(stem: &str) -> &str {
    let mut s = stem.trim_end();
    if let Some(open) = s.strip_suffix(')').and_then(|b| b.rfind('(')) {
        let number = &s[open + 1..s.len() - 1];
        if !number.is_empty() && number.len() <= 3 && number.chars().all(|c| c.is_ascii_digit()) && s[..open].ends_with(' ') {
            s = s[..open].trim_end();
        }
    } else if let Some((body, last)) = s.rsplit_once(' ') {
        if !last.is_empty() && last.chars().all(|c| c.is_ascii_digit()) && ["copy", "kopie"].iter().any(|m| body.to_lowercase().ends_with(m)) {
            s = body;
        }
    }
    let lower = s.to_lowercase();
    for mark in [" - copy", " - kopie", " copy", " kopie"] {
        if lower.ends_with(mark) {
            return s[..s.len() - mark.len()].trim_end();
        }
    }
    s
}

/// The catalog's system a file type is for, when the type says so: the popular ones first.
fn owner<'a>(catalog: &'a [SysInfo], ext: &str, popular: &[&str]) -> Option<&'a SysInfo> {
    if SAY_NOTHING.contains(&ext) {
        return None;
    }
    let takes = |s: &&SysInfo| s.extensions.iter().any(|e| e == ext);
    popular.iter().find_map(|id| catalog.iter().filter(takes).find(|s| s.id == *id)).or_else(|| catalog.iter().find(takes))
}

pub(crate) fn not_a_game_file(path: &Path, ext: &str) -> bool {
    crate::archive::is_clutter(ext) || NOT_GAMES.contains(&ext) || crate::game_files::is_save(path)
}

/// One typing slip: one letter wrong, one too many, one too few or two swapped.
fn one_slip(a: &str, b: &str) -> bool {
    let (x, y): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    match x.len() as i64 - y.len() as i64 {
        0 => {
            let diff: Vec<usize> = (0..x.len()).filter(|i| x[*i] != y[*i]).collect();
            diff.len() == 1 || (diff.len() == 2 && diff[1] == diff[0] + 1 && x[diff[0]] == y[diff[1]] && x[diff[1]] == y[diff[0]])
        }
        1 | -1 => {
            let (long, short) = if x.len() > y.len() { (&x, &y) } else { (&y, &x) };
            let at = (0..short.len()).find(|i| long[*i] != short[*i]).unwrap_or(short.len());
            long[at + 1..] == short[at..]
        }
        _ => false,
    }
}

/// Why a file in a system's folder is no game of that system, and what to do; None when it says nothing (a picture, a
/// save, a file type nobody uses for games). Common cases: another system's game, a typo, a second extension, a
/// packed or unfinished file.
pub(crate) fn wrong_file(files: &dyn Files, words: &Words, info: &SysInfo, catalog: &[SysInfo], exts: &[String], path: &Path, ext: &str) -> Option<(String, String)> {
    let file = path.file_name()?.to_string_lossy().into_owned();
    let takes = || exts.iter().filter(|e| !matches!(e.as_str(), "7z")).map(|e| format!(".{e}")).take(6).collect::<Vec<_>>().join(" ");
    let inner = path.file_stem().map(|s| extension(Path::new(s))).unwrap_or_default();
    if UNFINISHED.contains(&ext) {
        return Some(("This file was not copied to the end.".to_string(), words.copy_again.to_string()));
    }
    // `Game.nds.sav` is how other emulators name a game's save: renaming it would lose it, whatever its first type says.
    if crate::game_files::is_save(path) {
        return None;
    }
    if !inner.is_empty() && exts.contains(&inner) && !game_tag_ext(&inner) {
        let right = path.file_stem()?.to_string_lossy().into_owned();
        return Some((format!("{file} has two file types, so it isn't read."), format!("Rename it to {right}.")));
    }
    if PACKED.contains(&ext) {
        return Some((format!("{} can't open .{ext} files.", (words.short)(info)), "Unpack it on a computer and copy the game file.".to_string()));
    }
    if not_a_game_file(path, ext) {
        return None;
    }
    if ext.is_empty() {
        let big = files.size(path).is_some_and(|len| len >= 32 << 10);
        return big.then(|| (format!("{file} has no file type, so it isn't read."), format!("{} takes {}.", (words.short)(info), takes())));
    }
    if let Some(other) = owner(catalog, ext, words.popular).filter(|o| o.path != info.path) {
        return Some((format!("This is a {} file, in the {} folder.", (words.short)(other), (words.short)(info)), format!("Move it to {}.", other.folder)));
    }
    let close = exts.iter().find(|e| e.len() >= 2 && ext.len() >= 2 && one_slip(ext, e));
    if let Some(e) = close {
        let right = path.with_extension(e);
        let right = right.file_name()?.to_string_lossy().into_owned();
        return Some((format!("Typo in the file type: .{ext} isn't read."), format!("Rename it to {right}.")));
    }
    if catalog.iter().any(|s| s.extensions.iter().any(|e| e == ext)) {
        return Some((format!("{} doesn't take .{ext} files, so this one isn't read.", (words.short)(info)), format!("It takes {}.", takes())));
    }
    None
}

/// A second extension that is a normal part of a name ("Game.v1.zip" is not a typo): only real game types count, and
/// short version-like tails don't.
fn game_tag_ext(ext: &str) -> bool {
    ext.chars().all(|c| c.is_ascii_digit()) || ext.len() < 2
}

#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
fn is_litter(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()).is_some_and(crate::library::computer_litter)
}

/// Where to copy a missing BIOS file on this model: over the file share where there is Wi-Fi (`bios` is on the share),
/// into `roms/bios` on a second card where the model has a slot.
#[cfg(not(all(target_os = "android", not(feature = "handheld"))))]
fn bios_copy_way() -> &'static str {
    let d = crate::device::current();
    match (d.has_wifi(), d.has_second_card_slot()) {
        (true, true) => "Copy them into the bios folder over file sharing (Settings › Games › Add games). Or copy them into roms/bios on a second SD card in your computer.",
        (true, false) => "Copy them into the bios folder over file sharing (Settings › Games › Add games).",
        (false, true) => "Copy them into roms/bios on a second SD card in your computer.",
        (false, false) => "Copy them into roms/bios.",
    }
}

/// A report with made-up findings for `--snapshot game-check` and the pictures in the manual.
pub fn demo(all_good: bool) -> Report {
    if all_good {
        return Report {
            total: 214,
            ready: 214,
            findings: vec![Finding {
                group: Group::Tips,
                name: "PlayStation".into(),
                system: "PlayStation".into(),
                what: "PlayStation: no BIOS.".into(),
                todo: "PlayStation games start without a BIOS, and a few run better with one. Put scph5501.bin into roms/bios.".into(),
                place: None,
                fix: None,
            }],
            more: Vec::new(),
            capped: false,
        };
    }
    let f = |group, name: &str, system: &str, what: &str, todo: &str, place: Option<&str>, fix| Finding {
        group,
        name: name.to_string(),
        system: system.to_string(),
        what: what.to_string(),
        todo: todo.to_string(),
        place: place.map(str::to_string),
        fix,
    };
    Report {
        total: 214,
        ready: 186,
        more: Vec::new(),
        capped: false,
        findings: vec![
            f(Group::Bios, "Sega CD", "Sega CD", "Sega CD: BIOS missing.", "Sega CD needs a BIOS file in roms/bios: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin. BIOS files come from your own console. Copy them into the bios folder over file sharing (Settings › Games › Add games).", None, None),
            f(Group::Bios, "scph5501.bin", "PlayStation", "Wrong BIOS: its checksum differs from the file with this name.", "Games may not start or may crash. Copy your own console's scph5501.bin again.", Some("roms/bios/scph5501.bin"), None),
            f(Group::Discs, "Final Fantasy VII", "PlayStation", "Disc: Final Fantasy VII.cue is there, its Final Fantasy VII.bin is missing.", "Put Final Fantasy VII.bin next to it, with exactly this name.", Some("roms/psx/Final Fantasy VII.cue"), None),
            f(Group::Empty, "Pokémon Blau", "Game Boy", "This file is empty.", "A copy or a download stopped early. Copy the game to the SD card again.", Some("roms/gb/Pokémon Blau.gb"), None),
            f(Group::Zips, "Super Mario World", "Super Nintendo", "There is no game file in Super Mario World.zip. Check that it is the right file.", "", Some("roms/snes/Super Mario World.zip"), None),
            f(Group::Twice, "Metroid Fusion (1).gba", "Game Boy Advance", "Same game twice: roms/gba/Metroid Fusion.gba is the same game.", "Hide this copy from the shelf. The file stays on the card.", Some("roms/gba/Metroid Fusion (1).gba"), Some(Fix::Hide(PathBuf::from("/storage/roms/gba/Metroid Fusion (1).gba")))),
            f(Group::Place, "Chrono Trigger.gba", "Super Nintendo", "This is a Game Boy Advance file, in the Super Nintendo folder.", "Move it to roms/gba.", Some("roms/snes/Chrono Trigger.gba"), None),
            f(Group::Place, "Sega Genesis", "", "Folder without a system: Sega Genesis.", "Its games go in roms/megadrive. Rename the folder or move them.", Some("roms/Sega Genesis"), None),
            f(Group::Tips, "PlayStation", "PlayStation", "PlayStation: no BIOS.", "PlayStation games start without a BIOS, and a few run better with one. Put scph5501.bin into roms/bios.", None, None),
        ],
    }
}

#[cfg(test)]
mod tests {
// Written as a player makes these mistakes: a real folder tree per test, the catalog the way es_systems.cfg gives it.

    use super::*;
    use crate::testutil::TestDir;

    struct Card {
        dir: TestDir,
        catalog: Vec<SysInfo>,
    }

    fn sys(roms: &Path, id: &str, name: &str, exts: &[&str]) -> SysInfo {
        SysInfo { id: id.into(), name: name.into(), folder: format!("roms/{id}"), path: roms.join(id), extensions: exts.iter().map(|e| e.to_string()).collect() }
    }

    impl Card {
        fn new(tag: &str) -> Card {
            let dir = TestDir::new(&format!("game-check-{tag}"));
            let roms = dir.join("roms");
            let catalog = vec![
                sys(&roms, "gba", "Game Boy Advance", &["gba", "zip", "7z"]),
                sys(&roms, "snes", "Super Nintendo", &["smc", "sfc", "zip", "7z"]),
                sys(&roms, "psx", "PlayStation", &["cue", "chd", "m3u", "pbp", "bin", "iso", "zip"]),
                sys(&roms, "segacd", "Sega CD", &["cue", "chd"]),
                sys(&roms, "saturn", "Saturn", &["cue", "chd"]),
                sys(&roms, "megadrive", "Mega Drive", &["md", "gen", "zip"]),
                sys(&roms, "mame", "Arcade", &["zip", "7z"]),
            ];
            for s in &catalog {
                std::fs::create_dir_all(&s.path).unwrap();
            }
            std::fs::create_dir_all(roms.join("bios")).unwrap();
            Card { dir, catalog }
        }

        fn roms(&self) -> PathBuf {
            self.dir.join("roms")
        }

        fn put(&self, rel: &str, bytes: &[u8]) -> PathBuf {
            let p = self.roms().join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, bytes).unwrap();
            p
        }

        /// What the shelf lists: asks `library` itself, as the App's list does.
        fn input(&self, h700: bool) -> Input {
            let shelf = self
                .catalog
                .iter()
                .map(|s| (s.id.clone(), scan(s)))
                .filter(|(_, g)| !g.is_empty())
                .collect();
            Input { catalog: self.catalog.clone(), shelf, bios_dir: self.roms().join("bios"), roms: self.roms(), h700, versions: Versions::default() }
        }

        fn report(&self) -> Report {
            check(&self.input(false), &AtomicBool::new(false), &mut |_, _, _| {}).unwrap()
        }
    }

    fn scan(s: &SysInfo) -> Vec<PathBuf> {
        // The folder through the shelf's own reader: a game is what has a file type the system takes, and no missing track.
        let mut out = Vec::new();
        for e in std::fs::read_dir(&s.path).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_file() && s.extensions.contains(&extension(&p)) && !crate::library::computer_litter(&file_of(&p)) {
                out.push(p);
            }
        }
        out.sort();
        out
    }

    fn only(r: &Report, g: Group) -> Vec<&Finding> {
        r.in_group(g)
    }

    /// A Mac's `._` twins and hidden folders, and Windows' folders, on a drive a computer has had: nothing to report, and no game to count.
    #[test]
    fn a_macs_and_windows_litter_is_never_a_finding_or_a_game() {
        let c = Card::new("litter");
        c.put("gba/Metroid Fusion (USA).gba", b"game");
        for f in [
            "gba/._Metroid Fusion (USA).gba",
            "gba/.DS_Store",
            "gba/.Spotlight-V100/Store-V2/Stray.gba",
            "gba/.Trashes/501/Deleted.gba",
            "gba/.fseventsd/fseventsd-uuid",
            "gba/System Volume Information/Indexed.gba",
            "gba/System Volume Information/IndexerVolumeGuid",
            "gba/$RECYCLE.BIN/S-1-5-21/$R123.gba",
            "snes/._Chrono.sfc",
            "psx/._Disc.cue",
            "._Loose.gba",
            "._gba",
            ".DS_Store",
            ".Spotlight-V100/Store-V2/Stray.gba",
            ".Trashes/501/Deleted.gba",
            ".fseventsd/fseventsd-uuid",
            "System Volume Information/IndexerVolumeGuid",
            "System Volume Information/Indexed.gba",
            "$RECYCLE.BIN/S-1-5-21/$R123.gba",
        ] {
            c.put(f, b"x");
        }
        let r = c.report();
        assert_eq!(r.findings, Vec::new());
        assert_eq!((r.total, r.ready), (1, 1));
    }

    /// Release review 0.1.12: a folder longer than the look at files reads is not "All good".
    #[test]
    fn a_folder_past_the_cap_is_not_all_good() {
        let c = Card::new("capped");
        c.put("snes/Chrono Trigger (USA).sfc", b"game");
        let r = c.report();
        assert!(!r.capped);
        assert_eq!(r.summary(), "All good");
        let notes = c.roms().join("snes/notes");
        std::fs::create_dir_all(&notes).unwrap();
        // The cap counts directory entries, not distinct inodes or file contents. Link the
        // same ignored note: 100,002 allocated files consumed the shared test filesystem's
        // free inodes/blocks and made unrelated upload tests report a full card. Keep the
        // real production limit and both real oversized directories without that footprint.
        // One seed per directory also stays below ext4's per-inode hardlink limit.
        let seed = c.roms().join("note-seed.txt");
        let other_seed = c.roms().join("other-note-seed.txt");
        std::fs::write(&seed, b"x").unwrap();
        std::fs::write(&other_seed, b"x").unwrap();
        for i in 0..=MAX_ENTRIES {
            std::fs::hard_link(&seed, notes.join(format!("n{i}.txt"))).unwrap();
            std::fs::hard_link(&other_seed, c.roms().join(format!("snes/n{i}.txt"))).unwrap();
        }
        std::fs::remove_file(seed).unwrap();
        std::fs::remove_file(other_seed).unwrap();
        let r = c.report();
        assert!(r.capped);
        assert_eq!(r.summary(), CAPPED_SUMMARY);
        assert_eq!(CAPPED_SUMMARY, "First 50,000 files checked");
    }

    #[test]
    fn a_clean_card_is_all_good_and_counts_its_games() {
        let c = Card::new("clean");
        c.put("gba/Metroid Fusion (USA).gba", b"game");
        c.put("snes/Chrono Trigger (USA).sfc", b"game");
        c.put("gba/Metroid Fusion (USA).srm", b"save");
        c.put("gba/gamelist.xml", b"<x/>");
        c.put("gba/images/cover.png", b"pic");
        let r = c.report();
        assert_eq!(r.findings, Vec::new());
        assert!(r.all_good());
        assert_eq!((r.total, r.ready), (2, 2));
        assert_eq!(r.summary(), "All good");
    }

    /// GitHub issue #4: a player's own cheat files (beside the game, in a cheats folder, RetroArch's `cheats/<system>/`) are
    /// kept with the games; the check says nothing about them, not even "unknown file".
    #[test]
    fn cheat_files_are_no_unknown_games() {
        let c = Card::new("cht-files");
        c.put("gba/Metroid Fusion (USA).gba", b"game");
        c.put("gba/Metroid Fusion (USA).cht", b"cheat0_desc = \"x\"\ncheat0_code = \"AAAA\"\n");
        c.put("gba/METROID FUSION (USA).CHT", b"x");
        c.put("gba/cheats/Metroid Fusion (USA).cht", b"x");
        c.put("cheats/gba/Metroid Fusion (USA).cht", b"x");
        c.put("cheats/mGBA/Metroid Fusion (USA).cht", b"x");
        let r = c.report();
        assert_eq!(r.findings, Vec::new());
        assert_eq!((r.total, r.ready), (1, 1));
    }

    #[test]
    fn an_empty_card_has_nothing_to_say_and_no_panic() {
        let c = Card::new("none");
        let r = c.report();
        assert_eq!((r.total, r.ready, r.findings.len()), (0, 0, 0));
        // No folders at all.
        let i = Input { catalog: Vec::new(), shelf: Vec::new(), bios_dir: c.dir.join("nope/bios"), roms: c.dir.join("nope"), h700: false, versions: Versions::default() };
        assert_eq!(check(&i, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap().findings, Vec::new());
    }

    #[test]
    fn a_save_beside_its_game_is_no_fault_whatever_its_name_holds() {
        let c = Card::new("save-two-types");
        c.put("nds/Game.nds", &[7u8; 40000]);
        c.put("nds/Game.nds.sav", b"save");
        c.put("gba/Other.gba", &[7u8; 40000]);
        c.put("gba/Other.gba.srm", b"save");
        c.put("gba/Other.gba.part", b"half");
        c.put("gba/Third.gba.txt", b"game");
        let r = c.report();
        let place = only(&r, Group::Place);
        assert!(!place.iter().any(|f| f.place.as_deref().is_some_and(|p| p.contains(".sav") || p.contains(".srm"))), "{place:?}");
        // What the check is for stays: a game with a second type, and a copy that stopped.
        assert!(place.iter().any(|f| f.todo == "Rename it to Third.gba."), "{place:?}");
        assert!(place.iter().any(|f| f.place.as_deref().is_some_and(|p| p.ends_with("Other.gba.part"))), "{place:?}");
    }

    #[test]
    fn a_game_with_the_wrong_extension_says_what_to_do() {
        let c = Card::new("wrongext");
        c.put("snes/Chrono Trigger.gba", b"game");
        c.put("snes/Super Mario World.sfcc", b"game");
        c.put("snes/Secret of Mana.sfc.txt", b"game");
        c.put("snes/Star Fox.rar", b"game");
        c.put("snes/F-Zero.md", b"game");
        c.put("snes/Zelda.sfc.part", b"game");
        c.put("snes/Kirby.iso", b"game");
        c.put("snes/Readme.txt", b"hi");
        c.put("snes/cover.png", b"pic");
        let r = c.report();
        let place = only(&r, Group::Place);
        let say = |file: &str| place.iter().find(|f| f.place.as_deref().is_some_and(|p| p.ends_with(file))).unwrap_or_else(|| panic!("{file} not reported: {place:?}"));
        assert_eq!(say("Chrono Trigger.gba").todo, "Move it to roms/gba.");
        assert!(say("Chrono Trigger.gba").what.contains("Game Boy Advance"));
        assert_eq!(say("Super Mario World.sfcc").todo, "Rename it to Super Mario World.sfc.");
        assert_eq!(say("Secret of Mana.sfc.txt").todo, "Rename it to Secret of Mana.sfc.");
        assert!(say("Star Fox.rar").what.contains(".rar"));
        assert_eq!(say("F-Zero.md").todo, "Move it to roms/megadrive.", "a Mega Drive file in the SNES folder");
        assert!(say("Zelda.sfc.part").what.contains("not copied to the end"));
        assert!(say("Kirby.iso").todo.contains(".sfc"));
        assert_eq!(place.len(), 7, "neither the readme nor the picture: {place:?}");
    }

    #[test]
    fn a_bios_in_the_wrong_folder_or_spelling_is_found() {
        let c = Card::new("biosplace");
        c.put("segacd/Sonic CD.cue", b"FILE \"Sonic CD.bin\" BINARY\n");
        c.put("segacd/Sonic CD.bin", b"x");
        c.put("segacd/bios_CD_E.bin", &[1; 16]);
        let r = c.report();
        let bios = only(&r, Group::Bios);
        assert_eq!(bios.len(), 1, "{bios:?}");
        assert!(bios[0].what.contains("wrong folder"), "{bios:?}");
        assert_eq!(bios[0].todo, "Move bios_CD_E.bin to roms/bios.");
        assert_eq!(r.ready, 0, "not read there, so the games can't start");

        let c = Card::new("biosspell");
        c.put("segacd/Sonic CD.cue", b"FILE \"Sonic CD.bin\" BINARY\n");
        c.put("segacd/Sonic CD.bin", b"x");
        c.put("bios/BIOS_CD_E.BIN", &[1; 16]);
        let r = c.report();
        let bios = only(&r, Group::Bios);
        assert_eq!(bios[0].todo, "Rename BIOS_CD_E.BIN to bios_CD_E.bin.", "ext4 tells the two apart");

        let c = Card::new("biosnone");
        c.put("segacd/Sonic CD.cue", b"FILE \"Sonic CD.bin\" BINARY\n");
        c.put("segacd/Sonic CD.bin", b"x");
        let r = c.report();
        let bios = only(&r, Group::Bios);
        assert_eq!(bios[0].what, "Sega CD: BIOS missing.");
        assert!(bios[0].todo.starts_with("Sega CD needs a BIOS file in roms/bios: bios_CD_U.bin, bios_CD_E.bin or bios_CD_J.bin."), "{}", bios[0].todo);
        assert_eq!(bios[0].fix, None, "never fetched for the player");
    }

    #[test]
    fn computer_litter_in_the_bios_folder_is_no_bios_and_no_alarm() {
        // A Mac copying over the share leaves ._ files next to the BIOS files.
        let c = Card::new("bioslitter");
        c.put("segacd/Sonic CD.cue", b"FILE \"Sonic CD.bin\" BINARY\n");
        c.put("segacd/Sonic CD.bin", b"x");
        c.put("bios/._bios_CD_E.bin", &[1; 4096]);
        c.put("bios/.DS_Store", &[1; 131072]);
        let r = c.report();
        let bios = only(&r, Group::Bios);
        assert_eq!(bios.len(), 1, "{bios:?}");
        assert_eq!(bios[0].what, "Sega CD: BIOS missing.", "the ._ file is not the BIOS, nor a misnamed one");
        assert!(bios[0].todo.contains("over file sharing") || bios[0].todo.contains("roms/bios."), "{}", bios[0].todo);

        let c = Card::new("bioslitter2");
        c.put("segacd/Sonic CD.cue", b"FILE \"Sonic CD.bin\" BINARY\n");
        c.put("segacd/Sonic CD.bin", b"x");
        c.put("bios/bios_CD_E.bin", &[1; 131072]);
        c.put("bios/._bios_CD_E.bin", &[1; 4096]);
        let r = c.report();
        assert!(only(&r, Group::Bios).iter().all(|f| !f.name.starts_with('.')), "no finding about the litter: {:?}", only(&r, Group::Bios));
        assert!(r.findings.iter().all(|f| !f.name.starts_with('.') && !f.place.as_deref().is_some_and(|p| p.contains("/.")) ), "{:?}", r.findings);
    }

    #[test]
    fn a_bios_is_judged_by_its_checksum() {
        let c = Card::new("bioshash");
        c.put("psx/Crash.chd", b"x");
        c.put("bios/scph5501.bin", &[0; 524288]);
        let r = c.report();
        let bios = only(&r, Group::Bios);
        assert!(bios[0].what.starts_with("Wrong BIOS"), "{bios:?}");
        assert_eq!(r.ready, 1, "it may still start: a warning, not a block");

        let c = Card::new("biosempty");
        c.put("segacd/Sonic.chd", b"x");
        c.put("bios/bios_CD_E.bin", b"");
        let r = c.report();
        assert_eq!(only(&r, Group::Bios)[0].what, "This BIOS file is empty.");
        assert_eq!(r.ready, 0);
    }

    #[test]
    fn a_system_that_plays_without_a_bios_gets_a_tip_and_stays_green() {
        let c = Card::new("tips");
        c.put("psx/Crash.chd", b"x");
        c.put("saturn/Nights.chd", b"x");
        let r = c.report();
        assert!(r.all_good(), "{:?}", r.findings);
        assert_eq!(r.ready, 2);
        let tips = only(&r, Group::Tips);
        assert_eq!(tips.len(), 2);
        assert!(tips.iter().any(|t| t.todo.contains("Put scph5501.bin into roms/bios.")));
        assert!(tips.iter().any(|t| t.todo.contains("saturn_bios.bin")));
        assert_eq!(r.summary(), "All good");
    }

    #[test]
    fn a_disc_missing_its_track_and_umlauts_in_every_name() {
        let c = Card::new("disc");
        c.put("psx/Müller Köln.cue", b"FILE \"M\xfcller K\xf6ln.bin\" BINARY\n");
        c.put("psx/Müller Köln.bin", b"x");
        c.put("psx/Mädchen.cue", "FILE \"Mädchen.bin\" BINARY\n".as_bytes());
        let r = c.report();
        let discs = only(&r, Group::Discs);
        assert_eq!(discs.len(), 1, "the Latin-1 sheet finds its file: {discs:?}");
        assert_eq!(discs[0].what, "Disc: Mädchen.cue is there, its Mädchen.bin is missing.");
        assert_eq!(discs[0].place.as_deref(), Some("roms/psx/Mädchen.cue"));
    }

    #[test]
    fn a_name_that_is_not_utf8_is_no_panic() {
        use std::os::unix::ffi::OsStrExt;
        let c = Card::new("notutf8");
        let p = c.roms().join("snes").join(std::ffi::OsStr::from_bytes(b"M\xfcller.gba"));
        std::fs::write(&p, b"game").unwrap();
        let r = c.report();
        assert_eq!(only(&r, Group::Place).len(), 1);
    }

    #[test]
    fn empty_files_and_zips_that_hold_no_game() {
        let c = Card::new("empty");
        c.put("gba/Pokémon Blau.gba", b"");
        let readme = crate::archive::tests::zip_of(&[("readme.txt", b"hi")]);
        c.put("gba/Nur Text.zip", &readme);
        c.put("gba/Two.zip", &crate::archive::tests::zip_of(&[("a.gba", b"a"), ("b.gba", b"b")]));
        c.put("gba/Good.zip", &crate::archive::tests::zip_of(&[("g.gba", b"g")]));
        c.put("mame/sf2.zip", &crate::archive::tests::zip_of(&[("sf2.01", b"a"), ("sf2.02", b"b"), ("readme.txt", b"c")]));
        c.put("mame/cut.zip", b"PK\x03\x04 stopped");
        let r = c.report();
        assert_eq!(only(&r, Group::Empty)[0].what, "This file is empty.");
        let zips = only(&r, Group::Zips);
        assert!(zips.iter().any(|f| f.what == "There is no game file in Nur Text.zip. Check that it is the right file."), "{zips:?}");
        assert!(zips.iter().any(|f| f.what.contains("Two.zip holds 2 game files")), "{zips:?}");
        assert!(zips.iter().any(|f| f.place.as_deref().is_some_and(|p| p.ends_with("mame/cut.zip"))), "a cut-off zip counts everywhere: {zips:?}");
        assert!(!zips.iter().any(|f| f.place.as_deref().is_some_and(|p| p.contains("sf2"))), "an arcade set is many files: {zips:?}");
        assert_eq!(r.ready, 2, "Good.zip and the arcade set");
    }

    #[test]
    fn a_game_of_zeros_is_empty_inside_but_a_disc_that_starts_with_zeros_is_not() {
        let c = Card::new("zeros");
        let real = |zeros_first: usize, len: usize| {
            let mut v = vec![0u8; len];
            for (i, b) in v.iter_mut().enumerate().skip(zeros_first) {
                *b = (i % 251) as u8 + 1;
            }
            v
        };
        c.put("gba/Blank.gba", &vec![0u8; 4096]);
        c.put("gba/Big Blank.gba", &vec![0u8; 3 << 20]);
        c.put("gba/Good.gba", &real(0, 1 << 20));
        c.put("gba/Zero Head.gba", &real(100 << 10, 1 << 20));
        let r = c.report();
        let empty = only(&r, Group::Empty);
        let names: Vec<&str> = empty.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        // Sonic Adventure 2's .cdi has a zero first megabyte and runs: disc images are not looked at at all.
        let roms = c.roms();
        let disk = Disk { roms: &roms };
        let sa2 = c.put("dreamcast/Sonic Adventure 2.cdi", &real(1 << 20, 4 << 20));
        let zero_cdi = c.put("dreamcast/Zero.cdi", &vec![0u8; 2 << 20]);
        assert!(!zeros_inside(&disk, &sa2, 4 << 20));
        assert!(!zeros_inside(&disk, &zero_cdi, 2 << 20), "a disc format is skipped, whatever it holds");
        assert!(zeros_inside(&disk, &roms.join("gba/Big Blank.gba"), 3 << 20));
        assert!(names.iter().any(|n| n.contains("Blank")) && names.iter().any(|n| n.contains("Big Blank")), "{names:?}");
        assert!(empty.iter().all(|f| f.what == "This file is empty inside."), "{empty:?}");
        assert_eq!(r.ready, 2, "a real game and one starting with zeros but not zero all through");
    }

    #[test]
    fn the_same_game_twice_and_two_regions_are_one_game_in_two_versions() {
        let c = Card::new("twice");
        c.put("gba/Metroid Fusion.gba", b"same");
        let copy = c.put("gba/Metroid Fusion (1).gba", b"same");
        c.put("gba/Advance Wars.gba", b"same");
        c.put("gba/Advance Wars - Kopie.gba", b"different size");
        c.put("gba/Tobu (Europe).gba", b"same");
        c.put("gba/Tobu (World).gba", b"same");
        c.put("snes/Star Fox.sfc", b"game");
        c.put("snes/Star Fox.zip", &crate::archive::tests::zip_of(&[("Star Fox.sfc", b"game")]));
        // Different games in folders of their own, the file named alike and as big as the other: no twins.
        c.put("gba/Hack A/game.gba", b"same");
        c.put("gba/Hack B/game.gba", b"same");
        c.put("gba/Hack C/game.gbc", b"x");
        let r = c.report();
        let twins = only(&r, Group::Twice);
        assert_eq!(twins.len(), 3, "{twins:?}");
        let mario = twins.iter().find(|f| f.place.as_deref().is_some_and(|p| p.contains("Metroid"))).unwrap();
        assert_eq!(mario.fix, Some(Fix::Hide(copy)), "the copy goes, not the original");
        assert_eq!(mario.place.as_deref(), Some("roms/gba/Metroid Fusion (1).gba"));
        // Two regions of one game are two versions of it: the one for the whole world stays.
        let tobu = twins.iter().find(|f| f.place.as_deref().is_some_and(|p| p.contains("Tobu"))).unwrap();
        assert_eq!(tobu.place.as_deref(), Some("roms/gba/Tobu (Europe).gba"), "{tobu:?}");
        assert_eq!(tobu.what, "Same game twice: roms/gba/Tobu (World).gba is the same game.");
        assert_eq!(r.ready, r.total, "a second copy stops nothing");
    }

    /// The doubles of a real card (pocket, 25.11.2026: 182 top games copied beside a set already there) that Game check
    /// let through because only "(1)" and "- Copy" counted: other regions and revisions, the old set's release numbers,
    /// "Title, The", "&" for "and", Roman numbers, a zip beside the loose file. Each pair is one game; the copy that
    /// stays is the one played, else Europe before USA, else the newest revision.
    #[test]
    fn versions_of_one_game_are_found_and_the_best_stays() {
        let c = Card::new("versions");
        let pairs: [(&str, &str); 10] = [
            ("snes/0457-Chrono Trigger (US).smc", "snes/Chrono Trigger (Europe).sfc"),
            ("snes/Legend of Zelda, The - A Link to the Past (USA).smc", "snes/The Legend of Zelda - A Link to the Past (Europe).sfc"),
            ("snes/Teenage Mutant Ninja Turtles IV - Turtles in Time (USA).smc", "snes/1013-Teenage Mutant Ninja Turtles 4 - Turtles in Time (E)(4858).smc"),
            ("gba/Mario and Luigi - Superstar Saga (USA).gba", "gba/Mario & Luigi - Superstar Saga (Europe) (En,Fr,De,Es,It).7z"),
            ("gba/Pokemon - Fire Red Version (USA).7z", "gba/Pokemon - FireRed Version (USA, Europe) (Rev 1).gba"),
            ("gba/Metroid - Zero Mission (U) [!].gba", "gba/Metroid - Zero Mission (Europe) (En,Fr,De,Es,It).7z"),
            ("snes/Mega Man X 3.smc", "snes/Mega Man X3 (E).zip"),
            ("snes/Super Mariokart (U).smc", "snes/Super Mario Kart (Europe).sfc"),
            ("snes/Rock n' Roll Racing (USA).sfc", "snes/Rock 'N' Roll Racing (E).smc"),
            ("gba/Star Fox (USA) (Rev 1).gba", "gba/Star Fox (USA) (Rev 2).gba"),
        ];
        for (i, (goes, stays)) in pairs.iter().enumerate() {
            let body = if goes.ends_with(".zip") { crate::archive::tests::zip_of(&[("x.sfc", format!("game {i}").as_bytes())]) } else { format!("go {i}").into_bytes() };
            c.put(goes, &body);
            let body = if stays.ends_with(".zip") { crate::archive::tests::zip_of(&[("x.sfc", format!("game {i}!").as_bytes())]) } else { format!("stay {i}").into_bytes() };
            c.put(stays, &body);
        }
        let r = c.report();
        let twins = only(&r, Group::Twice);
        let mut offered: Vec<&str> = twins.iter().filter_map(|f| f.place.as_deref()).collect();
        offered.sort();
        // The first of each pair goes (Mega Man X 3 names no region, its zip is Europe's).
        let mut want: Vec<String> = pairs.iter().map(|(goes, _)| format!("roms/{goes}")).collect();
        want.sort();
        assert_eq!(offered, want, "{twins:#?}");
        for f in &twins {
            assert!(f.what.starts_with("Same game twice: roms/"), "{f:?}");
            assert!(matches!(&f.fix, Some(Fix::Hide(p)) if f.place.as_deref().is_some_and(|s| p.ends_with(s.trim_start_matches("roms/")))), "{f:?}");
        }
    }

    /// What is never called the same game: hacks, fan translations (also when only the file inside the zip says so),
    /// restorations, betas, a mod with its own version, Japanese releases, two parts of one game, and copies in a folder of
    /// their own. Different games stay different.
    #[test]
    fn hacks_translations_japanese_releases_and_parts_are_games_of_their_own() {
        let c = Card::new("editions");
        for f in [
            "gba/Final Fantasy VI Advance (Europe).gba",
            "gba/Final Fantasy VI Advance [Restored].gba",
            "gba/Pokemon (Emerald Rogue v2.2.0 EX).gba",
            "gba/Pokemon (Order and Chaos Remastered v06252024).gba",
            "gba/Pokemon - Ruby Version (USA).gba",
            "gba/Pokemon - Ruby Version (USA) [T+Ger].gba",
            "gba/Pokemon - Sapphire Version (USA).gba",
            "gba/Kirby (Beta).gba",
            "gba/Kirby (USA).gba",
            "snes/Tales of Phantasia (Japan).sfc",
            "snes/Tales of Phantasia.sfc",
            "snes/Demon's Crest (USA).sfc",
            "psx/Final Fantasy VII (Europe) (Disc 1).chd",
            "psx/Final Fantasy VII (Europe) (Disc 2).chd",
            "gba/Kids/Kirby (USA).gba",
        ] {
            c.put(f, f.as_bytes());
        }
        c.put("snes/Demon's Crest.zip", &crate::archive::tests::zip_of(&[("Demon's Crest (U) [T+Por1.1_Hexagon].smc", b"translated")]));
        let r = c.report();
        assert!(only(&r, Group::Twice).is_empty(), "{:#?}", only(&r, Group::Twice));
    }

    /// The player's progress never parts from its game: a version with a save beside it, a state, play time or a star stays,
    /// even when another version would rank higher, and when two versions were both played neither is offered.
    #[test]
    fn a_played_version_stays_and_two_played_versions_are_both_kept() {
        let c = Card::new("played");
        let usa = c.put("gba/Advance Wars (USA).gba", b"usa");
        c.put("gba/Advance Wars (USA).srm", b"save");
        let europe = c.put("gba/Advance Wars (Europe).gba", b"europe");
        let gs_eu = c.put("gba/Golden Sun (Europe).gba", b"eu");
        let gs_us = c.put("gba/Golden Sun (USA).gba", b"us");
        let k_eu = c.put("gba/Kirby (Europe).gba", b"eu");
        c.put("gba/Kirby (Europe).sav", b"save");
        let k_us = c.put("gba/Kirby (USA).gba", b"us");
        let mut input = c.input(false);
        input.versions.played.insert(gs_us.clone());
        input.versions.played.insert(k_us.clone());
        let r = check(&input, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap();
        let twins = only(&r, Group::Twice);
        assert_eq!(twins.len(), 2, "{twins:#?}");
        let aw = twins.iter().find(|f| f.name.contains("Advance Wars")).unwrap();
        assert_eq!(aw.fix, Some(Fix::Hide(europe)), "the USA copy has the save: {aw:?}");
        assert!(aw.what.contains("Advance Wars (USA).gba"), "{aw:?}");
        let gs = twins.iter().find(|f| f.name.contains("Golden Sun")).unwrap();
        assert_eq!(gs.fix, Some(Fix::Hide(gs_eu)), "play time keeps the USA copy: {gs:?}");
        assert!(!twins.iter().any(|f| f.fix == Some(Fix::Hide(k_eu.clone())) || f.fix == Some(Fix::Hide(k_us.clone()))), "both Kirbys were played");
        assert!(!twins.iter().any(|f| f.fix == Some(Fix::Hide(usa.clone()))));
    }

    /// One checksum under two names, and two names the checksum list gives one title (`Versions`, from `rom_id`'s cache):
    /// "Secrets of Mana 1" is Secret of Mana. A checksum alone joins nothing across folders.
    #[test]
    fn a_checksum_or_its_list_name_finds_one_game_under_two_file_names() {
        let c = Card::new("sums");
        let a = c.put("snes/0863-Secret of Mana (US).smc", b"a");
        let b = c.put("snes/Secrets of Mana 1.smc", b"b");
        let x = c.put("gba/Pokemom Gaia Version.gba", b"x");
        let y = c.put("gba/pokemon-gaia.gba", b"y");
        let z = c.put("gba/Kids/Gaia.gba", b"z");
        let mut input = c.input(false);
        // The Kids folder's copy is on the shelf too (`library::scan` reads one level of folders).
        input.shelf.iter_mut().find(|(id, _)| id == "gba").unwrap().1.push(z.clone());
        input.versions.names.insert(a.clone(), "Secret of Mana (USA)".into());
        input.versions.names.insert(b.clone(), "Secret of Mana (USA)".into());
        for p in [&x, &y, &z] {
            input.versions.sums.insert(p.to_path_buf(), (0xB946_5C5A, None));
        }
        let r = check(&input, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap();
        let mut hidden: Vec<PathBuf> = only(&r, Group::Twice).iter().filter_map(|f| f.fix.as_ref().map(|Fix::Hide(p)| p.clone())).collect();
        hidden.sort();
        assert_eq!(hidden.len(), 2, "{hidden:?}");
        assert!(hidden.contains(&b) || hidden.contains(&a), "{hidden:?}");
        assert!(hidden.contains(&x) || hidden.contains(&y), "{hidden:?}");
        assert!(!hidden.contains(&z), "the Kids folder's copy is the player's own choice");
    }

    /// One shelf for two systems' folders (Genesis and Mega Drive, `region_pairs`): Sonic in both is one game twice, as in
    /// one folder; a copy in a folder of its own beside them stays the player's choice.
    #[test]
    fn a_game_in_two_systems_folders_of_one_shelf_is_the_same_game_twice() {
        let c = Card::new("pairs");
        let mut input = c.input(false);
        let genesis = c.roms().join("genesis");
        std::fs::create_dir_all(&genesis).unwrap();
        input.catalog.push(sys(&c.roms(), "genesis", "Genesis", &["md", "gen", "zip"]));
        let us = c.put("genesis/Sonic the Hedgehog (USA).md", b"us");
        let eu = c.put("megadrive/Sonic the Hedgehog (Europe).md", b"eu");
        let kids = c.put("megadrive/Kids/Sonic the Hedgehog (Europe).md", b"eu");
        input.shelf.push(("megadrive".into(), vec![us.clone(), eu.clone(), kids.clone()]));
        let r = check(&input, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap();
        let hidden: Vec<&Fix> = only(&r, Group::Twice).iter().filter_map(|f| f.fix.as_ref()).collect();
        assert_eq!(hidden, [&Fix::Hide(us)], "the USA one in genesis goes, Europe stays, Kids is its own");
    }

    /// The shelves `library` really joins (`region_pairs`: Genesis into Mega Drive, Super Famicom into Super Nintendo), read
    /// from an es_systems.cfg as the App reads them: a game in both systems' folders is the same game twice, the played
    /// version stays wherever it is, and a Kids folder stays the player's own.
    #[test]
    fn region_pair_shelves_find_a_game_in_both_systems_folders() {
        let c = Card::new("region-pairs");
        let roms = c.roms();
        let mut cfg = String::from("<systemList>");
        for (id, exts) in [("megadrive", ".md .gen .zip"), ("genesis", ".md .gen .zip"), ("snes", ".sfc .smc .zip"), ("sfc", ".sfc .smc .zip")] {
            std::fs::create_dir_all(roms.join(id)).unwrap();
            cfg += &format!("<system><name>{id}</name><fullname>{id}</fullname><path>{}</path><extension>{exts}</extension></system>", roms.join(id).display());
        }
        cfg += "</systemList>";
        let es = c.dir.join("es_systems.cfg");
        std::fs::write(&es, cfg).unwrap();
        c.put("megadrive/Sonic the Hedgehog (Europe).md", b"eu");
        let sonic_us = c.put("genesis/Sonic the Hedgehog (USA).md", b"us");
        c.put("genesis/Kids/Sonic the Hedgehog (USA).md", b"us");
        c.put("snes/Super Metroid (Europe).sfc", b"eu");
        let metroid_us = c.put("sfc/Super Metroid (Japan, USA).sfc", b"us");
        let chrono_eu = c.put("snes/Chrono Trigger (Europe).sfc", b"eu");
        c.put("sfc/Chrono Trigger (USA).sfc", b"us");
        c.put("sfc/Chrono Trigger (USA).srm", b"save");
        let systems = crate::library::load(&es).unwrap();
        let names: Vec<&str> = systems.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names.len(), 2, "two shelves: {names:?}");
        let shelf = systems.iter().map(|s| (s.name.clone(), s.games.iter().map(|g| g.path.clone()).collect())).collect();
        let input = Input { catalog: crate::library::all_systems(&es).unwrap(), shelf, bios_dir: roms.join("bios"), roms: roms.clone(), h700: false, versions: Versions::default() };
        let r = check(&input, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap();
        let mut hidden: Vec<PathBuf> = only(&r, Group::Twice).iter().filter_map(|f| f.fix.as_ref().map(|Fix::Hide(p)| p.clone())).collect();
        hidden.sort();
        let mut want = vec![sonic_us, metroid_us, chrono_eu];
        want.sort();
        assert_eq!(hidden, want, "{:#?}", only(&r, Group::Twice));
    }

    #[test]
    fn version_titles_regions_and_revisions_read_like_a_player_would() {
        for (a, b) in [
            ("Street Fighter II Turbo", "0925-Street Fighter 2 Turbo (US)"),
            ("Legend of Zelda, The - The Minish Cap (U)", "The Legend of Zelda - The Minish Cap (Europe) (En,Fr,De,Es,It)"),
            ("Pokémon - Emerald Version (U)", "Pokemon - Emerald Version (USA, Europe)"),
            ("Super Ghouls'n Ghosts (USA)", "0031 Super Ghouls 'N Ghosts"),
            ("Mario Kart - Double Dash!! (USA).nkit", "Mario Kart Double Dash (Europe)"),
        ] {
            assert_eq!(version_title(a), version_title(b), "{a} / {b}");
            assert!(version_title(a).is_some());
        }
        for (a, b) in [("Zelda - A Link to the Past", "Legend of Zelda, The - A Link to the Past"), ("Tetris (Japan)", "Tetris (World)"), ("Pokemon - Red Version", "Pokemon - Blue Version")] {
            assert_ne!(version_title(a), version_title(b), "{a} / {b}");
        }
        for edition in ["Mario Kart Double Dash (Retro Track Grand Prix v2.1)", "Pokemom Gaia Version (Already Patched)", "Earthbound (T+Ger)", "Zelda [h1]", "Metroid (Beta)"] {
            assert_eq!(version_title(edition), None, "{edition}");
        }
        assert!(region_rank("Game (USA, Europe)") < region_rank("Game (USA)"));
        assert!(region_rank("Game (USA)") < region_rank("Game (Japan)"));
        assert!(region_rank("Game (Japan)") < region_rank("Game"));
        assert_eq!(region_rank("Game (UE) [!]"), 0);
        assert!(revision("Game (USA) (Rev 2)") > revision("Game (USA) (Rev 1)"));
        assert!(revision("Game (U) (V1.1)") > revision("Game (U)"));
        assert!(revision("Game (Rev B)") > revision("Game (Rev A)"));
    }

    #[test]
    fn rocknixs_empty_splore_entry_is_no_fault() {
        assert!(empty_on_purpose("pico-8", Path::new("/r/pico-8/Splore.png")));
        assert!(!empty_on_purpose("pico-8", Path::new("/r/pico-8/Celeste.png")));
        assert!(!empty_on_purpose("gba", Path::new("/r/gba/Splore.gba")));
    }

    #[test]
    fn copy_marks_come_off_and_real_names_stay() {
        for (with, without) in [
            ("Game (1)", "Game"),
            ("Game - Copy", "Game"),
            ("Game - Kopie", "Game"),
            ("Game copy 2", "Game"),
            ("Game (USA)", "Game (USA)"),
            ("Street Fighter 2", "Street Fighter 2"),
            ("Pokémon - Kopie", "Pokémon"),
            ("Game (12)", "Game"),
            ("Teenage Mutant Ninja Turtles 4 (US)(4858)", "Teenage Mutant Ninja Turtles 4 (US)(4858)"),
            ("Game (2048)", "Game (2048)"),
        ] {
            assert_eq!(without_copy_mark(with), without, "{with}");
        }
    }

    #[test]
    fn wrong_folders_and_games_too_deep() {
        let c = Card::new("folders");
        c.put("Retro Stuff/Sonic.md", b"x");
        c.put("GBA/Metroid.gba", b"x");
        c.put("Loose Game.sfc", b"x");
        c.put("snes/Collection/Set 1/Star Fox.sfc", b"x");
        c.put("snes/Collection/readme.txt", b"x");
        c.put("psx/data/x/ignored.bin", b"x");
        c.put("bios/x.bin", b"x");
        let r = c.report();
        let place = only(&r, Group::Place);
        let by = |n: &str| place.iter().find(|f| f.place.as_deref().is_some_and(|p| p.contains(n))).unwrap_or_else(|| panic!("{n}: {place:?}"));
        assert_eq!(by("Retro Stuff").todo, "Nothing reads this folder. Add games shows the name each system's folder has.");
        assert_eq!(by("Loose Game").todo, "Move it to roms/snes.");
        assert!(by("Set 1").what.contains("too deep"));
        // "GBA" is read as the Game Boy Advance (docs/OTHER-LAYOUTS.md), so it is no finding; the emulator's data folder says nothing.
        assert_eq!(place.len(), 3, "{place:?}");
    }

    #[test]
    fn a_stronger_handheld_is_said_only_where_we_know_it() {
        let c = Card::new("stronger");
        c.put("ps2/God of War.iso", b"x");
        c.put("3DS/Zelda.3ds", b"x");
        let catalog = c.catalog.iter().cloned().map(|mut s| {
            // A file type a system of the catalog takes, so the folder counts as holding games.
            if s.id == "psx" {
                s.extensions.push("3ds".to_string());
            }
            s
        });
        let i = Input { catalog: catalog.collect(), ..c.input(true) };
        let r = check(&i, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap();
        let weak = only(&r, Group::Stronger);
        assert_eq!(weak.len(), 2, "{:?}", r.findings);
        assert_eq!(weak[0].what, "Nintendo 3DS needs a stronger handheld.");
        assert_eq!(weak[1].what, "PlayStation 2 needs a stronger handheld.");
        let i = Input { h700: false, ..i };
        let r = check(&i, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap();
        assert!(only(&r, Group::Stronger).is_empty(), "the other devices say nothing of the kind");
    }

    #[test]
    fn it_stops_at_once_and_reads_no_more_than_it_keeps() {
        let c = Card::new("stop");
        for i in 0..300 {
            c.put(&format!("gba/Game {i}.gba"), b"");
        }
        let stop = AtomicBool::new(true);
        assert!(check(&c.input(false), &stop, &mut |_, _, _| {}).is_none(), "stopped gives no report");
        let r = c.report();
        assert_eq!(r.in_group(Group::Empty).len(), MAX_PER_GROUP);
        assert_eq!(r.groups(), vec![(Group::Empty, 300)], "the count stays whole");
        assert_eq!(r.ready, 0);
    }

    #[test]
    fn every_known_bios_file_is_one_hostsys_names() {
        for line in KNOWN_BIOS.iter().filter(|l| l.systems[0] != "psx") {
            let s = crate::hostsys::find(line.systems[0]).unwrap_or_else(|| panic!("{}", line.systems[0]));
            for k in line.files {
                assert!(s.bios.iter().any(|b| b.files.contains(&k.name)), "{} is not a file {} looks for", k.name, line.systems[0]);
            }
        }
        assert!(crate::hostsys::find("psx").unwrap().bios.is_empty(), "PlayStation plays without one: a tip, not a missing file");
    }

    #[test]
    fn a_file_is_known_by_size_and_checksum_together() {
        let files = [k("disksys.rom", 8192, 0x5E607DCF)];
        assert_eq!(known_as(&files, 8192, 0x5E607DCF).map(|k| k.name), Some("disksys.rom"));
        assert!(known_as(&files, 8192, 1).is_none());
        assert!(known_as(&files, 16, 0x5E607DCF).is_none());
    }

    /// How long a large card takes and what it keeps: 5000 games, all sizes of trouble; on the build machine a bound far above
    /// what it takes, so only a real slowdown fails. The same on the H700: `examples/game_check_bench.rs`.
    #[test]
    fn five_thousand_games_stay_within_bounds() {
        let c = Card::new("bench");
        for i in 0..5000 {
            let (sys, ext) = [("gba", "gba"), ("snes", "sfc"), ("megadrive", "md"), ("psx", "chd")][i % 4];
            c.put(&format!("{sys}/Spiel Nummer {i} (Europe).{ext}"), if i % 500 == 0 { b"" } else { b"x" });
        }
        let input = c.input(false);
        let t = crate::testutil::clock::CpuInstant::now();
        let r = check(&input, &AtomicBool::new(false), &mut |_, _, _| {}).unwrap();
        let took = t.elapsed();
        println!("game check, 5000 games: {took:?}");
        assert_eq!(r.total, 5000);
        assert!(took < Duration::from_secs(30), "{took:?}");
        assert!(r.findings.len() <= 8 * MAX_PER_GROUP);
    }
}
