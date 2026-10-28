//! Settings → System → Storage: what takes the room on the card, what can safely go, and a word before it is full.
//!
//! * [`measure`] walks the card once like `du` (on a thread, never the UI's; links are not followed) and sorts every
//!   file into a [`Kind`]: the games of each system, save states, saves, screenshots, pocket-ui's downloaded pictures,
//!   its logs, backups, everything else. It also notes what the clean-ups need: the automatic states with their age,
//!   the logs of earlier starts, pocket-ui's backups. The App keeps the result ([`Usage`]) and measures again when the
//!   page opens after a game, a delete or ten minutes.
//! * Four clean-ups, each only what its name says ([`Cleanup`], [`selection`], [`clean`]): the **automatic states**
//!   (Quit's resume state, Before load and Earlier's states, never a slot the player saved into) of games not played
//!   for some months;
//!   the **picture cache** (box art and game pictures, downloaded again); the **logs of earlier starts**; **old
//!   backups** (all of pocket-ui's backups but the newest). Saves and memory cards are never on any list, and
//!   [`clean`] checks every path again before it deletes it.
//! * **Low space** ([`LOW`]): under 1 GB free, a notice on the shelf once (until there is [`RECOVERED`] again) and a
//!   line on the page. Checked on a thread at start and after every game ([`check`]). Why 1 GB and not a share of
//!   the card: what fills a card in a session does not grow with the card. A PS2 state is 20–40 MB and a GameCube or
//!   Wii state 40–100 MB, and Quit writes one, Load keeps one more, so a session of a big game can take 200 MB, and an
//!   app update needs about 60 MB to download; 1 GB is several such sessions of warning. 2 % would be 10 GB on a
//!   512 GB card, a warning nobody needs yet, and 300 MB on a 16 GB card, too late for a PS2 player.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::game_files::is_save;

/// Under this many bytes free, the card counts as nearly full.
pub const LOW: u64 = 1_000_000_000;
/// A warning that was shown comes again only after the free space has been above this.
pub const RECOVERED: u64 = 1_500_000_000;
/// The choices for "not played for": months.
pub const MONTHS: [u32; 3] = [3, 6, 12];
/// What "Delete old automatic states" starts with.
pub const DEFAULT_MONTHS: u32 = 6;
/// A month, for the age of a state.
const MONTH_SECS: u64 = 30 * 24 * 3600;
/// The low-space warning's memory, in the data folder.
pub const STATE_FILE: &str = "storage.json";
/// The page measures again each time it opens, unless what it has is younger than this (a second visit right after
/// the first). Files copied in over file sharing change the sizes without pocket-ui noticing, so the old numbers are
/// shown at once with a quiet "Updating…" while the walk on its thread brings the new ones.
pub const FRESH: Duration = Duration::from_secs(10);

/// Whether opening the page starts a walk: nothing measured yet, something changed on the card since (a game, a
/// delete), or the last walk is older than [`FRESH`].
pub fn measure_on_open(last_walk: Option<Duration>, changed_since: bool) -> bool {
    changed_since || last_walk.is_none_or(|age| age >= FRESH)
}

/// What a file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Games,
    States,
    Saves,
    Screenshots,
    Pictures,
    Logs,
    Backups,
    Other,
}

impl Kind {
    pub const ALL: [Kind; 8] = [Kind::Games, Kind::States, Kind::Saves, Kind::Screenshots, Kind::Pictures, Kind::Logs, Kind::Backups, Kind::Other];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Games => "Games",
            Kind::States => "Save states",
            Kind::Saves => "Saves",
            Kind::Screenshots => "Screenshots",
            Kind::Pictures => "Box art and pictures",
            Kind::Logs => "Logs",
            Kind::Backups => "Backups",
            Kind::Other => "Everything else",
        }
    }

    fn index(self) -> usize {
        Kind::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

/// Where things are: `/storage/roms`, pocket-ui's data folder, and pocket-ui's backups.
#[derive(Clone, Debug)]
pub struct Roots {
    pub roms: PathBuf,
    pub data: PathBuf,
    pub backups: PathBuf,
}

impl Roots {
    pub fn device(data: &Path) -> Roots {
        Roots { roms: PathBuf::from("/storage/roms"), data: data.to_path_buf(), backups: PathBuf::from(crate::backup::ROOT) }
    }

    fn states(&self) -> PathBuf {
        self.roms.join("savestates")
    }
}

/// A system's ROM folder, as es_systems.cfg names it.
#[derive(Clone, Debug)]
pub struct SystemDir {
    pub name: String,
    pub path: PathBuf,
}

/// An automatic state on the card (or its picture): which game's (the state folder's system and the name before
/// the ending), how big, when it was written.
#[derive(Clone, Debug, PartialEq)]
pub struct AutoState {
    pub path: PathBuf,
    pub system: String,
    pub base: String,
    pub bytes: u64,
    /// Unix seconds.
    pub modified: u64,
}

/// A folder the clean-ups may remove whole, and its size.
#[derive(Clone, Debug, PartialEq)]
pub struct Folder {
    pub path: PathBuf,
    pub bytes: u64,
}

/// What [`measure`] found.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    bytes: [u64; 8],
    /// Games by system (full name), largest first.
    pub systems: Vec<(String, u64)>,
    pub auto_states: Vec<AutoState>,
    /// What Earlier's states (`timeline`) take, pictures included: the Save states row says it beside their caps.
    pub earlier: u64,
    /// The logs of earlier starts (`logs/boot-*`).
    pub old_logs: Vec<Folder>,
    /// pocket-ui's backups, newest first.
    pub backups: Vec<Folder>,
    /// Folders that could not be read; what is in them is not counted.
    pub unreadable: usize,
    /// Free and total bytes on the card.
    pub free: Option<(u64, u64)>,
    pub took: Duration,
}

impl Usage {
    pub fn bytes(&self, kind: Kind) -> u64 {
        self.bytes[kind.index()]
    }

    /// A made-up figure (the snapshots).
    pub fn set_bytes(&mut self, kind: Kind, n: u64) {
        self.bytes[kind.index()] = n;
    }

    fn add(&mut self, kind: Kind, n: u64) {
        self.bytes[kind.index()] += n;
    }
}

/// Free and total bytes of the file system that holds `path`; None when it can't be asked (a statvfs can wait for the
/// card: never on the UI thread).
pub fn free_space(path: &Path) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    // SAFETY: statvfs only writes into the struct we pass.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(c.as_ptr(), &mut st) } == 0).then(|| (st.f_bavail as u64 * st.f_frsize as u64, st.f_blocks as u64 * st.f_frsize as u64))
}

/// "740 KB", "312 MB", "4.2 GB" (powers of 1000, as the card's maker counts and About's free space).
pub fn size(bytes: u64) -> String {
    match bytes {
        0 => "0 MB".into(),
        1..=999_999 => format!("{} KB", bytes.div_ceil(1000)),
        1_000_000..=999_999_999 => format!("{} MB", bytes / 1_000_000),
        _ => format!("{:.1} GB", bytes as f64 / 1e9),
    }
}

/// Walks the card: `roots.roms` and the parts of `roots.data` that grow. `systems` are the ROM folders (games in a
/// folder of no system count as Everything else). Takes a few seconds on a full card: a thread.
pub fn measure(roots: &Roots, systems: &[SystemDir]) -> Usage {
    let t0 = Instant::now();
    let mut u = Usage::default();
    let mut by_system: HashMap<String, u64> = HashMap::new();
    let states = roots.states();
    let backups_parent = roots.backups.parent().map(Path::to_path_buf);
    let mut walked: Vec<PathBuf> = Vec::new();
    let mut top: Vec<PathBuf> = match fs::read_dir(&roots.roms) {
        Ok(rd) => rd.flatten().map(|e| e.path()).collect(),
        Err(_) => {
            u.unreadable += 1;
            Vec::new()
        }
    };
    // A system folder somewhere else than right under the games folder is walked as well.
    for s in systems {
        if !top.contains(&s.path) && !s.path.starts_with(&roots.roms) {
            top.push(s.path.clone());
        }
    }
    top.sort();
    for p in top {
        let Ok(m) = fs::symlink_metadata(&p) else { continue };
        if !m.is_dir() {
            if m.is_file() {
                u.add(if is_save(&p) { Kind::Saves } else { Kind::Other }, m.len());
            }
            continue;
        }
        walked.push(p.clone());
        if p == states {
            walk(&p, &mut u, &mut |u, f, bytes, modified| {
                u.add(Kind::States, bytes);
                note_auto(u, &states, f, bytes, modified);
            });
        } else if p.file_name().is_some_and(|n| n == "screenshots") {
            walk(&p, &mut u, &mut |u, _, bytes, _| u.add(Kind::Screenshots, bytes));
        } else if Some(&p) == backups_parent.as_ref() {
            walk(&p, &mut u, &mut |u, _, bytes, _| u.add(Kind::Backups, bytes));
        } else if let Some(sys) = systems.iter().find(|s| s.path == p) {
            let mut games = 0;
            walk(&p, &mut u, &mut |u, f, bytes, _| {
                if is_save(f) {
                    u.add(Kind::Saves, bytes);
                } else {
                    u.add(Kind::Games, bytes);
                    games += bytes;
                }
            });
            *by_system.entry(sys.name.clone()).or_default() += games;
        } else {
            walk(&p, &mut u, &mut |u, f, bytes, _| u.add(if is_save(f) { Kind::Saves } else { Kind::Other }, bytes));
        }
    }
    // pocket-ui's own: the pictures it downloaded and its logs (settings and lists are a few hundred KB).
    for folder in ["covers", "snaps", "titles"] {
        let mut n = 0;
        walk(&roots.data.join(folder), &mut u, &mut |_, _, bytes, _| n += bytes);
        u.add(Kind::Pictures, n);
    }
    let logs = roots.data.join(crate::logs::DIR_NAME);
    if let Ok(rd) = fs::read_dir(&logs) {
        for e in rd.flatten() {
            let (p, name) = (e.path(), e.file_name().to_string_lossy().into_owned());
            let mut n = 0;
            if p.is_dir() {
                walk(&p, &mut u, &mut |_, _, bytes, _| n += bytes);
                if name.starts_with("boot-") {
                    u.old_logs.push(Folder { path: p, bytes: n });
                }
            } else {
                n = e.metadata().map_or(0, |m| m.len());
            }
            u.add(Kind::Logs, n);
        }
    }
    u.old_logs.sort_by(|a, b| a.path.cmp(&b.path));
    // Newest first, as the Backup page lists them; each with all it holds.
    u.backups = crate::backup::list(&roots.backups)
        .into_iter()
        .map(|b| {
            let mut n = 0;
            walk(&b.dir, &mut Usage::default(), &mut |_, _, bytes, _| n += bytes);
            Folder { path: b.dir, bytes: n }
        })
        .collect();
    if !walked.iter().any(|w| roots.backups.starts_with(w)) {
        // Backups kept outside the games folder (another POCKETUI setup) still count.
        u.add(Kind::Backups, u.backups.iter().map(|b| b.bytes).sum());
    }
    let mut systems: Vec<(String, u64)> = by_system.into_iter().filter(|(_, b)| *b > 0).collect();
    systems.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    u.systems = systems;
    u.free = free_space(&roots.roms);
    u.took = t0.elapsed();
    u
}

/// Every file under `dir` (links not followed, 24 levels at most): `f(usage, path, bytes, modified)`. A folder that
/// can't be read counts in `unreadable`.
fn walk(dir: &Path, u: &mut Usage, f: &mut dyn FnMut(&mut Usage, &Path, u64, u64)) {
    let mut todo = vec![(dir.to_path_buf(), 0u32)];
    let mut first = true;
    while let Some((d, depth)) = todo.pop() {
        let rd = match fs::read_dir(&d) {
            Ok(rd) => rd,
            // A folder that isn't there (no covers yet) is nothing to count, not a problem.
            Err(e) if first && e.kind() == std::io::ErrorKind::NotFound => return,
            Err(_) => {
                u.unreadable += 1;
                continue;
            }
        };
        first = false;
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                if depth < 24 {
                    todo.push((e.path(), depth + 1));
                }
            } else if m.is_file() {
                let modified = m.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
                f(u, &e.path(), m.len(), modified);
            }
        }
    }
}

/// Endings of the states nobody saved by hand: Quit's resume state (pocket-core, RetroArch, ARMSX2, Dolphin, Azahar)
/// and Before load, longest first. Each may carry `.png` (its picture). Never a slot: `.pstate`, `.pstate3`,
/// `.state1`, `.s01`, `.01.p2s`, `.00.cst` are the player's.
const AUTO_ENDINGS: &[&str] = &[
    ".pstate.auto.taken.retry",
    ".pstate.auto.taken",
    ".pstate.auto",
    ".pstate.undo",
    ".state.auto",
    ".pocket-auto",
    ".pocket-undo",
    ".auto.cst",
    ".undo.cst",
];

/// The game an automatic state belongs to (the name before its ending), or None for any other file. Earlier's states
/// count (`timeline::base_of`): nobody saved them by hand either.
pub fn auto_state_base(name: &str) -> Option<&str> {
    if let Some(base) = crate::timeline::base_of(name) {
        return Some(base);
    }
    let core = name.strip_suffix(".png").unwrap_or(name);
    if let Some(base) = AUTO_ENDINGS.iter().find_map(|e| core.strip_suffix(e)) {
        return (!base.is_empty()).then_some(base);
    }
    // ARMSX2's own numbers for them (`states::ARMSX2_AUTO`, `ARMSX2_UNDO`).
    [crate::states::ARMSX2_AUTO, crate::states::ARMSX2_UNDO].iter().find_map(|n| core.strip_suffix(&format!(".{n:02}.p2s"))).filter(|b| !b.is_empty())
}

fn note_auto(u: &mut Usage, states: &Path, f: &Path, bytes: u64, modified: u64) {
    let Some(name) = f.file_name().and_then(|n| n.to_str()) else { return };
    let Some(base) = auto_state_base(name) else { return };
    let system = f.parent().filter(|p| p.parent() == Some(states)).and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
    let Some(system) = system else { return };
    if crate::timeline::base_of(name).is_some() {
        u.earlier += bytes;
    }
    u.auto_states.push(AutoState { path: f.to_path_buf(), system, base: base.to_string(), bytes, modified });
}

/// What a clean-up deletes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cleanup {
    /// Automatic states of games not played for this many months.
    OldStates(u32),
    Pictures,
    /// The logs of earlier starts.
    OldLogs,
    /// pocket-ui's backups but the newest.
    OldBackups,
}

/// The files and folders a clean-up would delete, and what that frees.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selection {
    pub paths: Vec<PathBuf>,
    pub bytes: u64,
    /// Games whose states go (OldStates), starts whose logs go (OldLogs), backups that go (OldBackups).
    pub count: usize,
}

/// A recently played game: its system, its file's stem and name, when it was last played (Unix seconds).
#[derive(Clone, Debug)]
pub struct Played {
    pub system: String,
    pub stem: String,
    pub file: String,
    pub last: u64,
}

/// What `cleanup` would delete, from what [`measure`] found. `played`: the recent games, so a game played within the
/// months keeps its states even if their files are older; `now`: Unix seconds.
pub fn selection(u: &Usage, cleanup: Cleanup, played: &[Played], now: u64, roots: &Roots) -> Selection {
    let mut s = Selection::default();
    match cleanup {
        Cleanup::OldStates(months) => {
            let cutoff = now.saturating_sub(u64::from(months) * MONTH_SECS);
            // A game's automatic states go together, or not at all: the newest of them says when it was played.
            let mut games: HashMap<(&str, &str), (u64, Vec<&AutoState>)> = HashMap::new();
            for a in &u.auto_states {
                let g = games.entry((a.system.as_str(), a.base.as_str())).or_default();
                g.0 = g.0.max(a.modified);
                g.1.push(a);
            }
            let mut old: Vec<_> = games
                .into_iter()
                .filter(|((system, base), (newest, _))| {
                    let recent = played.iter().any(|p| p.system == *system && (p.stem == *base || p.file == *base) && p.last >= cutoff);
                    *newest < cutoff && !recent
                })
                .collect();
            old.sort_by(|a, b| a.0.cmp(&b.0));
            s.count = old.len();
            for (_, (_, files)) in old {
                for a in files {
                    s.paths.push(a.path.clone());
                    s.bytes += a.bytes;
                }
            }
        }
        // The three folders as they are, when the measurement found pictures in them (no look at the card here).
        Cleanup::Pictures if u.bytes(Kind::Pictures) > 0 => {
            s.paths = ["covers", "snaps", "titles"].iter().map(|f| roots.data.join(f)).collect();
            s.bytes = u.bytes(Kind::Pictures);
        }
        Cleanup::Pictures => {}
        Cleanup::OldLogs => {
            for f in &u.old_logs {
                s.paths.push(f.path.clone());
                s.bytes += f.bytes;
                s.count += 1;
            }
        }
        Cleanup::OldBackups => {
            // Every backup but the newest.
            for b in u.backups.iter().skip(1) {
                s.paths.push(b.path.clone());
                s.bytes += b.bytes;
                s.count += 1;
            }
        }
    }
    s
}

/// Deletes what [`selection`] chose, after checking each path is what `cleanup` may delete: an automatic state in the
/// states folder (never a save), one of pocket-ui's picture folders, an earlier start's logs, one of its backups.
/// Anything else is skipped and said in the log. Returns the bytes freed (as measured).
pub fn clean(cleanup: Cleanup, sel: &Selection, roots: &Roots) -> Result<u64, String> {
    let states = roots.states();
    let logs = roots.data.join(crate::logs::DIR_NAME);
    let mut failed = 0;
    for p in &sel.paths {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let allowed = match cleanup {
            Cleanup::OldStates(_) => p.starts_with(&states) && auto_state_base(name).is_some() && !is_save(p) && p.is_file(),
            Cleanup::Pictures => ["covers", "snaps", "titles"].iter().any(|f| *p == roots.data.join(f)),
            Cleanup::OldLogs => p.parent() == Some(logs.as_path()) && name.starts_with("boot-") && p.is_dir(),
            Cleanup::OldBackups => p.parent() == Some(roots.backups.as_path()) && p.join(crate::backup::MANIFEST).is_file(),
        };
        if !allowed {
            eprintln!("storage: not deleted, not part of the clean-up: {}", p.display());
            continue;
        }
        let done = if p.is_dir() { fs::remove_dir_all(p) } else { fs::remove_file(p) };
        match done {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                eprintln!("storage: {} not deleted: {e}", p.display());
                failed += 1;
            }
        }
    }
    if failed > 0 {
        return Err(format!("{failed} could not be deleted. Is the SD card read-only?"));
    }
    Ok(sel.bytes)
}

/// The low-space warning's memory: whether it was shown since the card last had room.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Warned {
    #[serde(default)]
    pub low_space: bool,
}

impl Warned {
    pub fn load(data: &Path) -> Warned {
        crate::fsutil::read_json_or_backup(&data.join(STATE_FILE)).unwrap_or_default()
    }
}

/// What a look at the free space says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Check {
    /// Nothing to do.
    Fine,
    /// Nearly full and not said yet: say it (then remember it was said).
    Warn(u64),
    /// There is room again: the next time it runs low, say it again.
    Recovered,
}

/// The rule: warn once under [`LOW`], again only after the card had [`RECOVERED`].
pub fn check(free: Option<u64>, warned: Warned) -> Check {
    match free {
        Some(f) if f < LOW && !warned.low_space => Check::Warn(f),
        Some(f) if f >= RECOVERED && warned.low_space => Check::Recovered,
        _ => Check::Fine,
    }
}

/// The notice on the shelf.
pub fn low_notice(free: u64) -> String {
    format!("The SD card is nearly full: {} left. Settings › System › Storage shows what takes up space.", size(free))
}

/// Unix seconds now.
pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    struct Tree(crate::testutil::TestDir);

    impl Tree {
        fn new(tag: &str) -> Tree {
            let d = TestDir::new(&format!("storage-{tag}"));
            fs::create_dir_all(d.join("roms")).unwrap();
            Tree(d)
        }
        fn put(&self, rel: &str, bytes: usize) -> PathBuf {
            let p = self.0.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, vec![b'x'; bytes]).unwrap();
            p
        }
        fn aged(&self, rel: &str, bytes: usize, secs_ago: u64) -> PathBuf {
            let p = self.put(rel, bytes);
            let t = SystemTime::now() - Duration::from_secs(secs_ago);
            fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
            p
        }
        fn has(&self, rel: &str) -> bool {
            self.0.join(rel).exists()
        }
        fn roots(&self) -> Roots {
            Roots { roms: self.0.join("roms"), data: self.0.join("data"), backups: self.0.join("roms/backup/pocket-ui") }
        }
        fn systems(&self) -> Vec<SystemDir> {
            [("Game Boy Advance", "gba"), ("PlayStation 2", "ps2"), ("PlayStation", "psx")]
                .iter()
                .map(|(n, f)| SystemDir { name: n.to_string(), path: self.0.join("roms").join(f) })
                .collect()
        }
        fn backup(&self, name: &str, created: u64, bytes: usize) {
            self.put(&format!("roms/backup/pocket-ui/{name}/settings.json"), bytes);
            let m = crate::backup::Manifest { version: 1, created, reason: crate::backup::Reason::Manual, app: "1".into(), files: vec!["settings.json".into()] };
            fs::write(self.0.join(format!("roms/backup/pocket-ui/{name}/backup.json")), serde_json::to_vec(&m).unwrap()).unwrap();
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(self.0.join("roms/locked"), fs::Permissions::from_mode(0o755));
            }
        }
    }

    const DAY: u64 = 24 * 3600;

    /// A card with a bit of everything: the sizes add up per kind, per system, and nothing is counted twice.
    fn card(t: &Tree) {
        t.put("roms/gba/Alpha.gba", 4_000);
        t.put("roms/gba/Alpha.srm", 32);
        t.put("roms/gba/Pack/Beta.gba", 2_000);
        t.put("roms/ps2/God of War.iso", 900_000);
        t.put("roms/ps2/Mcd001.ps2", 8_000);
        t.put("roms/psx/Saga/Saga.bin", 50_000);
        t.put("roms/psx/Saga/Saga.mcr", 128);
        t.put("roms/savestates/gba/Alpha.pstate", 300);
        t.put("roms/savestates/gba/Alpha.pstate.auto", 300);
        t.put("roms/savestates/gba/Alpha.pstate.auto.png", 20);
        t.put("roms/screenshots/gba/Alpha 2026-10-24 18-05-09.png", 700);
        t.put("roms/bios/scph5501.bin", 512);
        t.put("roms/bios/GC/MemoryCard A.USA.raw", 1_000);
        t.put("roms/music/song.ogg", 60);
        t.put("roms/README.txt", 5);
        t.put("data/covers/gba/Alpha.png", 90);
        t.put("data/snaps/gba/Alpha.png", 80);
        t.put("data/titles/gba/Alpha.png", 70);
        t.put("data/logs/pocket-ui.log", 11);
        t.put("data/logs/boot-0001-2026-10-20-0800/pocket-ui.log", 100);
        t.put("data/logs/boot-0002-2026-10-21-0800/kernel.log", 200);
        t.put("data/settings.json", 3);
        t.backup("2026-10-20_0800", 1_000, 40);
        t.backup("2026-10-23_1420", 2_000, 50);
    }

    #[test]
    fn sizes_add_up_per_kind_and_per_system_on_a_made_up_card() {
        let t = Tree::new("sizes");
        card(&t);
        let u = measure(&t.roots(), &t.systems());
        assert_eq!(u.bytes(Kind::Games), 4_000 + 2_000 + 900_000 + 50_000);
        assert_eq!(u.bytes(Kind::Saves), 32 + 8_000 + 128 + 1_000, "srm, the PS2 card, the PS1 card, the GameCube card");
        assert_eq!(u.bytes(Kind::States), 620);
        assert_eq!(u.bytes(Kind::Screenshots), 700);
        assert_eq!(u.bytes(Kind::Pictures), 240);
        assert_eq!(u.bytes(Kind::Logs), 311);
        let manifests: u64 = u.backups.iter().map(|b| b.bytes).sum();
        assert_eq!(u.bytes(Kind::Backups), manifests, "each backup with its list, counted once");
        assert_eq!(u.bytes(Kind::Other), 512 + 60 + 5, "BIOS, music, a loose file");
        assert_eq!(u.systems, vec![("PlayStation 2".to_string(), 900_000), ("PlayStation".to_string(), 50_000), ("Game Boy Advance".to_string(), 6_000)], "largest first");
        assert_eq!(u.old_logs.len(), 2);
        assert_eq!(u.backups.len(), 2);
        assert!(u.backups[0].path.ends_with("2026-10-23_1420"), "newest first");
        assert_eq!(u.unreadable, 0);
        assert!(u.free.is_some_and(|(free, total)| total >= free && total > 0));
        let auto: Vec<&str> = u.auto_states.iter().map(|a| a.path.file_name().unwrap().to_str().unwrap()).collect();
        assert_eq!(auto.len(), 2, "the auto state and its picture, not the slot: {auto:?}");
    }

    #[test]
    fn an_empty_or_missing_card_measures_as_nothing_and_an_unreadable_folder_is_counted() {
        let t = Tree::new("empty");
        let u = measure(&t.roots(), &t.systems());
        assert!(Kind::ALL.iter().all(|k| u.bytes(*k) == 0));
        assert!(u.systems.is_empty() && u.auto_states.is_empty() && u.backups.is_empty() && u.unreadable == 0);

        let gone = Roots { roms: t.0.join("no-card"), ..t.roots() };
        assert_eq!(measure(&gone, &[]).unreadable, 1, "the games folder itself missing is said");

        #[cfg(unix)]
        if unsafe { libc::geteuid() } != 0 {
            use std::os::unix::fs::PermissionsExt;
            t.put("roms/locked/inner/x.bin", 10);
            t.put("roms/gba/A.gba", 7);
            fs::set_permissions(t.0.join("roms/locked"), fs::Permissions::from_mode(0o000)).unwrap();
            let u = measure(&t.roots(), &t.systems());
            assert_eq!(u.unreadable, 1);
            assert_eq!(u.bytes(Kind::Games), 7, "the rest is still counted");
        }
    }

    #[test]
    fn automatic_states_are_told_from_the_players_slots() {
        for (name, base) in [
            ("Alpha.pstate.auto", "Alpha"),
            ("Alpha.pstate.auto.png", "Alpha"),
            ("Alpha.pstate.auto.taken.retry", "Alpha"),
            ("Alpha.pstate.undo", "Alpha"),
            ("Alpha.pstate.earlier3", "Alpha"),
            ("Alpha.pstate.started1.png", "Alpha"),
            ("Alpha.state.auto", "Alpha"),
            ("SLES-12345 (ABCD1234).99.p2s", "SLES-12345 (ABCD1234)"),
            ("SLES-12345 (ABCD1234).98.p2s", "SLES-12345 (ABCD1234)"),
            ("Mario Kart.iso.pocket-auto", "Mario Kart.iso"),
            ("Zelda.3ds.undo.cst", "Zelda.3ds"),
        ] {
            assert_eq!(auto_state_base(name), Some(base), "{name}");
        }
        for name in ["Alpha.pstate", "Alpha.pstate3", "Alpha.pstate.png", "Alpha.state1", "Alpha.srm", "Mario Kart.iso.s01", "SLES-12345 (ABCD1234).01.p2s", "SLES-12345 (ABCD1234).00.p2s", "00040000001B5000.00.cst", ".pstate.auto"] {
            assert_eq!(auto_state_base(name), None, "{name}");
        }
    }

    #[test]
    fn the_cleanup_takes_only_old_automatic_states_never_slots_or_saves() {
        let t = Tree::new("cleanup");
        let year = 365 * DAY;
        // Old: Alpha's auto state, its picture, Before load; its slot and its save stay.
        t.aged("roms/savestates/gba/Alpha.pstate.auto", 300, year);
        t.aged("roms/savestates/gba/Alpha.pstate.auto.png", 20, year);
        t.aged("roms/savestates/gba/Alpha.pstate.undo", 300, year);
        // Earlier's states go with them (`timeline`).
        t.aged("roms/savestates/gba/Alpha.pstate.earlier2", 300, year);
        t.aged("roms/savestates/gba/Alpha.pstate.earlier2.png", 20, year);
        t.aged("roms/savestates/gba/Alpha.pstate", 300, year);
        t.aged("roms/savestates/gba/Alpha.pstate2", 300, year);
        t.aged("roms/gba/Alpha.srm", 32, year);
        // Old file, but played last week (the recent games say so): kept.
        t.aged("roms/savestates/gba/Beta.pstate.auto", 300, year);
        // Recent: kept.
        t.aged("roms/savestates/gba/Gamma.pstate.auto", 300, 10 * DAY);
        t.aged("roms/savestates/gba/Gamma.pstate.started1", 300, 10 * DAY);
        // A PS2 game's auto state from a year ago goes, its slot stays.
        t.aged("roms/savestates/ps2/SLES-1 (AB).99.p2s", 30_000, year);
        t.aged("roms/savestates/ps2/SLES-1 (AB).01.p2s", 30_000, year);
        // One old auto file next to a fresh one of the same game: the game was played, nothing of it goes.
        t.aged("roms/savestates/gba/Delta.pstate.undo", 300, year);
        t.aged("roms/savestates/gba/Delta.pstate.auto", 300, DAY);
        let roots = t.roots();
        let u = measure(&roots, &t.systems());
        assert_eq!(u.earlier, 300 + 20 + 300, "Earlier's states are counted on their own as well");
        let played = [Played { system: "gba".into(), stem: "Beta".into(), file: "Beta.gba".into(), last: now() - 7 * DAY }];
        let sel = selection(&u, Cleanup::OldStates(6), &played, now(), &roots);
        let mut names: Vec<String> = sel.paths.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["Alpha.pstate.auto", "Alpha.pstate.auto.png", "Alpha.pstate.earlier2", "Alpha.pstate.earlier2.png", "Alpha.pstate.undo", "SLES-1 (AB).99.p2s"]);
        assert_eq!(sel.bytes, 300 + 20 + 300 + 20 + 300 + 30_000);
        assert_eq!(sel.count, 2, "two games");
        assert_eq!(selection(&u, Cleanup::OldStates(12), &played, now() - 30 * DAY, &roots).count, 0, "12 months: none is that old yet");

        clean(Cleanup::OldStates(6), &sel, &roots).unwrap();
        for kept in ["roms/savestates/gba/Alpha.pstate", "roms/savestates/gba/Alpha.pstate2", "roms/gba/Alpha.srm", "roms/savestates/gba/Beta.pstate.auto", "roms/savestates/gba/Gamma.pstate.auto", "roms/savestates/gba/Gamma.pstate.started1", "roms/savestates/ps2/SLES-1 (AB).01.p2s", "roms/savestates/gba/Delta.pstate.undo"] {
            assert!(t.has(kept), "{kept} kept");
        }
        assert!(!t.has("roms/savestates/gba/Alpha.pstate.auto") && !t.has("roms/savestates/ps2/SLES-1 (AB).99.p2s"));
    }

    #[test]
    fn clean_refuses_anything_its_cleanup_does_not_cover() {
        let t = Tree::new("refuse");
        let roots = t.roots();
        let save = t.put("roms/savestates/gba/Alpha.srm", 32);
        let slot = t.put("roms/savestates/gba/Alpha.pstate", 32);
        let game = t.put("roms/gba/Alpha.pstate.auto", 32);
        let current_log = t.put("data/logs/pocket-ui.log", 5);
        let not_backup = t.put("roms/backup/pocket-ui/notes/x.txt", 5);
        let sneaky = Selection { paths: vec![save.clone(), slot.clone(), game.clone()], bytes: 96, count: 1 };
        clean(Cleanup::OldStates(6), &sneaky, &roots).unwrap();
        assert!(save.exists() && slot.exists() && game.exists(), "a save, a slot, a file outside the states folder");
        let sneaky = Selection { paths: vec![current_log.clone(), not_backup.parent().unwrap().to_path_buf(), t.0.join("roms")], bytes: 0, count: 0 };
        for c in [Cleanup::OldLogs, Cleanup::OldBackups, Cleanup::Pictures] {
            clean(c, &sneaky, &roots).unwrap();
        }
        assert!(current_log.exists() && not_backup.exists() && t.has("roms"), "this start's log, a folder without a backup list, the card");
    }

    #[test]
    fn logs_of_earlier_starts_and_all_backups_but_the_newest_go_and_pictures_go_whole() {
        let t = Tree::new("logs");
        card(&t);
        let roots = t.roots();
        let u = measure(&roots, &t.systems());
        let logs = selection(&u, Cleanup::OldLogs, &[], now(), &roots);
        assert_eq!((logs.count, logs.bytes), (2, 300), "two earlier starts");
        let backups = selection(&u, Cleanup::OldBackups, &[], now(), &roots);
        assert_eq!(backups.count, 1, "one backup of two");
        clean(Cleanup::OldLogs, &logs, &roots).unwrap();
        clean(Cleanup::OldBackups, &backups, &roots).unwrap();
        assert!(t.has("data/logs/pocket-ui.log"), "this start's log stays");
        assert!(!t.has("data/logs/boot-0001-2026-10-20-0800"));
        assert!(t.has("roms/backup/pocket-ui/2026-10-23_1420") && !t.has("roms/backup/pocket-ui/2026-10-20_0800"), "the newest backup stays");

        let pics = selection(&u, Cleanup::Pictures, &[], now(), &roots);
        assert_eq!(pics.bytes, 240);
        clean(Cleanup::Pictures, &pics, &roots).unwrap();
        assert!(!t.has("data/covers") && !t.has("data/snaps") && !t.has("data/titles") && t.has("data/settings.json"));
        assert!(t.has("roms/gba/Alpha.srm") && t.has("roms/ps2/Mcd001.ps2"), "saves are on no list");
    }

    #[test]
    fn the_low_space_warning_comes_once_and_again_only_after_there_was_room() {
        let w = |low_space| Warned { low_space };
        assert_eq!(check(Some(900_000_000), w(false)), Check::Warn(900_000_000));
        assert_eq!(check(Some(900_000_000), w(true)), Check::Fine, "said once");
        assert_eq!(check(Some(1_200_000_000), w(true)), Check::Fine, "a little room is not enough to say it again");
        assert_eq!(check(Some(RECOVERED), w(true)), Check::Recovered);
        assert_eq!(check(Some(5_000_000_000), w(false)), Check::Fine);
        assert_eq!(check(None, w(false)), Check::Fine, "free space unknown: nothing");
        assert_eq!(low_notice(812_000_000), "The SD card is nearly full: 812 MB left. Settings › System › Storage shows what takes up space.");
    }

    /// VDEV-ROUND-2026-10-25 #11: files copied in over file sharing showed in Storage only after 10 minutes.
    #[test]
    fn opening_the_page_counts_again_unless_it_just_did() {
        assert!(measure_on_open(None, false), "nothing measured yet");
        assert!(measure_on_open(Some(Duration::from_secs(11)), false), "a visit after the minimum, nothing changed: files may have been copied in");
        assert!(measure_on_open(Some(Duration::from_secs(5 * 60)), false), "minutes old, the case that showed old numbers");
        assert!(!measure_on_open(Some(Duration::from_secs(3)), false), "a second visit right after the first");
        assert!(measure_on_open(Some(Duration::from_secs(3)), true), "a game or a delete changed the card since");
    }

    #[test]
    fn sizes_read_like_the_cards_maker_counts() {
        assert_eq!(size(0), "0 MB");
        assert_eq!(size(1), "1 KB");
        assert_eq!(size(740_000), "740 KB");
        assert_eq!(size(312_400_000), "312 MB");
        assert_eq!(size(4_210_000_000), "4.2 GB");
    }
}
