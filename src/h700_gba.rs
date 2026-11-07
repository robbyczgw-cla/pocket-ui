//! H700 GBA uses bundled ARM64 gpSP, retaining mGBA for existing Continue moments.
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::thread::JoinHandle;

pub const CORES: &[&str] = &["gpsp", "mgba"];
pub fn is_gba(system: &str) -> bool { matches!(system, "gba" | "gbah" | "gbav") }

/// Beside the programs in the version directory, so OTA rollback also rolls back the core.
pub fn bundled_path() -> PathBuf {
    std::env::var_os("POCKETUI_GBA_CORE").map(PathBuf::from).unwrap_or_else(|| {
        std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_default().join("cores/gpsp_libretro.so")
    })
}
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| (crate::device::current().model.is_some_and(|m| m.is_h700())
        || std::env::var_os("POCKETUI_GBA_CORE").is_some()) && bundled_path().is_file())
}

type Owners = HashMap<(String, String), &'static str>;

#[derive(Default)]
struct OwnerCache {
    owners: Option<Owners>,
    pending: Option<JoinHandle<Owners>>,
}

impl OwnerCache {
    fn finish(&mut self, root: &Path) {
        if let Some(pending) = self.pending.take() {
            // A failed worker must not leave stale ownership after a game wrote a new moment.
            self.owners = pending.join().ok();
        }
        if self.owners.is_none() {
            self.owners = Some(scan(root));
        }
    }

    fn preload(&mut self, root: PathBuf) {
        // No older scan may publish its result after this refresh. The worker never locks this cache.
        if let Some(pending) = self.pending.take() {
            let _ = pending.join();
        }
        self.owners = None;
        self.pending = std::thread::Builder::new().name("gba-continue".into()).spawn(move || scan(&root)).ok();
    }

    fn core(&mut self, root: &Path, system: &str, stem: &str) -> &'static str {
        self.finish(root);
        self.owners.as_ref().and_then(|owners| owners.get(&(system.to_string(), stem.to_string()))).copied().unwrap_or("gpsp")
    }
}

static OWNERS: Mutex<OwnerCache> = Mutex::new(OwnerCache { owners: None, pending: None });

/// The default only: explicit game/console choices still win in emu_choice. A fast start waits for the
/// startup/post-exit worker: guessing either core could retire the other core's Continue moment.
pub fn default_core(system: &str, rom: Option<&Path>) -> &'static str {
    let Some(stem) = rom.and_then(Path::file_stem).and_then(|s| s.to_str()) else { return "gpsp" };
    OWNERS.lock().unwrap_or_else(|e| e.into_inner()).core(&crate::slots::states_dir(), system, stem)
}

/// gpSP's pinned BSON state header (savestate.c, GBA_STATE_MAGIC 0x06BAC0DE).
fn owner(len: u64, header: &[u8]) -> &'static str {
    let gpsp = len == 416 * 1024 && header.get(4..20) == Some(b"\x10info-magic\0\xde\xc0\xba\x06");
    if gpsp { "gpsp" } else { "mgba" }
}

fn file_owner(path: &Path, compressed: bool) -> Option<&'static str> {
    let file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if compressed {
        // Use the player's format and checksum validation. Damaged compressed bytes must never
        // switch an existing mGBA Continue moment to gpSP merely because its payload looks like gpSP.
        let state = crate::state_file::read(path).ok()?;
        Some(owner(state.len() as u64, &state))
    } else {
        // Raw states only need their length and BSON header, never a full state read at launch.
        let mut header = [0; 20];
        file.take(20).read_exact(&mut header).ok()?;
        Some(owner(len, &header))
    }
}

fn compressed_auto_name(name: &str) -> bool {
    name.ends_with(".pstate.auto.z") || name.ends_with(".pstate.auto.z.taken")
}

/// The game's name of a save slot's file (`slots::state_path`: `<stem>.pstate`, `<stem>.pstate1` to `.pstate7`).
fn slot_stem(name: &str) -> Option<&str> {
    if let Some(stem) = name.strip_suffix(".pstate") {
        return Some(stem);
    }
    let (stem, n) = name.rsplit_once(".pstate")?;
    (n.len() == 1 && n.parse::<usize>().is_ok_and(|n| (1..crate::slots::SLOTS).contains(&n))).then_some(stem)
}

fn scan(root: &Path) -> Owners {
    let mut found = HashMap::new();
    for system in ["gba", "gbah", "gbav"] {
        let mut files: Vec<_> = std::fs::read_dir(root.join(system)).into_iter().flatten().flatten().collect();
        // Current quit first, then taken. Within a stage the latest real capture wins, including raw/compressed.
        // As slots::auto_candidates does: compressed precedes raw when capture times tie.
        files.sort_by_key(|e| std::cmp::Reverse((
            e.metadata().and_then(|m| m.modified()).ok(),
            e.file_name().to_str().is_some_and(compressed_auto_name),
        )));
        let mut unreadable = Vec::new();
        for taken in [false, true] {
            for entry in &files {
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let suffixes = if taken { [".pstate.auto.taken", ".pstate.auto.z.taken"] } else { [".pstate.auto", ".pstate.auto.z"] };
                let Some(stem) = suffixes.iter().find_map(|suffix| name.strip_suffix(suffix)) else { continue };
                let key = (system.to_string(), stem.to_string());
                if found.contains_key(&key) { continue }
                let compressed = compressed_auto_name(name);
                match file_owner(&entry.path(), compressed) {
                    Some(core) => {
                        found.insert(key, core);
                    }
                    // As `Core::resume` does: a compressed moment that can't be read gives way to the next one, whose core
                    // goes on. With none readable behind it, the slots decide (below), else the conservative mGBA as before.
                    None if compressed => unreadable.push(key),
                    None => {
                        found.insert(key, "mgba");
                    }
                }
            }
        }
        // No Continue moment: the newest save slot decides (raw `.pstate`, `.pstate1`..`.pstate7`). A player who saved
        // in slots on mGBA (Continue off, or quit without keeping) would otherwise get gpSP, which refuses every slot
        // (an mGBA state is 397,888 bytes, gpSP takes only its 425,984: "wrong size"), review 0.3.
        for entry in &files {
            let name = entry.file_name();
            let Some(stem) = name.to_str().and_then(slot_stem) else { continue };
            let key = (system.to_string(), stem.to_string());
            if found.contains_key(&key) { continue }
            let core = file_owner(&entry.path(), false).unwrap_or("mgba");
            found.insert(key, core);
        }
        // A game whose Continue moments can't be read and that has no slot: the conservative mGBA as before.
        for key in unreadable {
            found.entry(key).or_insert("mgba");
        }
    }
    found
}

/// Warm at startup and refresh after a game exits (including Quit without keeping a moment).
pub fn preload() {
    if !enabled() { return }
    OWNERS.lock().unwrap_or_else(|e| e.into_inner()).preload(crate::slots::states_dir());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpsp_state() -> Vec<u8> {
        let mut state = vec![0; 416 * 1024];
        state[4..20].copy_from_slice(b"\x10info-magic\0\xde\xc0\xba\x06");
        state
    }

    #[test]
    fn fast_start_waits_for_gpsp_continue_instead_of_retiring_it_with_mgba() {
        let d = crate::testutil::TestDir::new("gba-fast-start");
        std::fs::create_dir_all(d.join("gba")).unwrap();
        let auto = d.join("gba/Fast.pstate.auto");
        let state = gpsp_state();
        std::fs::write(&auto, &state).unwrap();
        let mut cache = OwnerCache::default();
        cache.preload(d.to_path_buf());
        assert_eq!(cache.core(&d, "gba", "Fast"), "gpsp");
        assert_eq!(std::fs::read(auto).unwrap(), state, "Continue remains untouched");
    }

    #[test]
    fn immediate_relaunch_uses_the_new_moment_and_worker_failure_rescans() {
        let d = crate::testutil::TestDir::new("gba-fast-relaunch");
        std::fs::create_dir_all(d.join("gba")).unwrap();
        let auto = d.join("gba/Game.pstate.auto");
        std::fs::write(&auto, b"old mGBA moment").unwrap();
        let mut cache = OwnerCache::default();
        assert_eq!(cache.core(&d, "gba", "Game"), "mgba");
        std::fs::write(&auto, gpsp_state()).unwrap();
        cache.preload(d.to_path_buf());
        assert_eq!(cache.core(&d, "gba", "Game"), "gpsp", "post-exit scan replaces stale mGBA ownership before launch");
        std::fs::write(&auto, b"another mGBA session").unwrap();
        cache.preload(d.to_path_buf());
        assert_eq!(cache.core(&d, "gba", "Game"), "mgba");
        std::fs::write(&auto, gpsp_state()).unwrap();
        cache.pending = Some(std::thread::spawn(|| panic!("failed owner worker")));
        assert_eq!(cache.core(&d, "gba", "Game"), "gpsp", "worker failure must rescan instead of using stale mGBA ownership");
        std::fs::remove_file(&auto).unwrap();
        cache.preload(d.to_path_buf());
        assert_eq!(cache.core(&d, "gba", "Game"), "gpsp", "fresh starts use the lighter core");
    }
    #[test]
    fn upgrading_keeps_old_mgba_moments_and_new_gpsp_moments_separate() {
        let d = crate::testutil::TestDir::new("gba-upgrade");
        let gba = d.join("gba"); std::fs::create_dir_all(&gba).unwrap();
        let old = gba.join("Old.pstate.auto"); std::fs::write(&old, b"existing mGBA moment").unwrap();
        let state = gpsp_state();
        std::fs::write(gba.join("New.pstate.auto.taken"), &state).unwrap();
        let owners = scan(&d);
        assert_eq!(owners.get(&("gba".into(), "Old".into())), Some(&"mgba"));
        assert_eq!(owners.get(&("gba".into(), "New".into())), Some(&"gpsp"));
        assert_eq!(std::fs::read(&old).unwrap(), b"existing mGBA moment");
        std::fs::remove_file(&old).unwrap();
        assert!(!scan(&d).contains_key(&("gba".into(), "Old".into())), "a fresh start no longer pins mGBA");
    }

    /// Review 0.3: slots saved on mGBA (0.2) with no Continue moment: the game stays on mGBA, so they still load.
    /// gpSP refuses them ("wrong size"), and nothing on the shelf says why.
    #[test]
    fn mgba_save_slots_keep_a_game_without_continue_on_mgba() {
        let d = crate::testutil::TestDir::new("gba-slot-owner");
        let gba = d.join("gba");
        std::fs::create_dir_all(&gba).unwrap();
        let at = |p: &Path, secs: u64| {
            std::fs::File::options().write(true).open(p).unwrap().set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs)).unwrap();
        };
        std::fs::write(gba.join("Emerald.pstate"), b"an mGBA slot 1").unwrap();
        std::fs::write(gba.join("Emerald.pstate3"), b"an mGBA slot 4").unwrap();
        std::fs::write(gba.join("Fresh.pstate2"), gpsp_state()).unwrap();
        let owners = scan(&d);
        assert_eq!(owners.get(&("gba".into(), "Emerald".into())), Some(&"mgba"), "mGBA's slots still load");
        assert_eq!(owners.get(&("gba".into(), "Fresh".into())), Some(&"gpsp"));
        // The newest slot decides; a Continue moment still comes first.
        std::fs::write(gba.join("Both.pstate"), b"old mGBA slot").unwrap();
        std::fs::write(gba.join("Both.pstate1"), gpsp_state()).unwrap();
        at(&gba.join("Both.pstate"), 1_700_000_000);
        at(&gba.join("Both.pstate1"), 1_700_000_100);
        assert_eq!(scan(&d).get(&("gba".into(), "Both".into())), Some(&"gpsp"));
        std::fs::write(gba.join("Emerald.pstate.auto"), gpsp_state()).unwrap();
        assert_eq!(scan(&d).get(&("gba".into(), "Emerald".into())), Some(&"gpsp"), "Continue first");
        assert_eq!(slot_stem("Game.pstate8"), None, "eight slots: .pstate to .pstate7");
        assert_eq!(slot_stem("Game.pstate.auto"), None);
    }

    #[test]
    fn compressed_continue_uses_its_core_and_damaged_data_does_not_switch_cores() {
        let d = crate::testutil::TestDir::new("gba-compressed-owner");
        std::fs::create_dir_all(d.join("gba")).unwrap();
        let auto = d.join("gba/Game.pstate.auto.z");
        let key = ("gba".into(), "Game".into());
        crate::state_file::write(&auto, &gpsp_state()).unwrap();
        assert_eq!(scan(&d).get(&key), Some(&"gpsp"));
        let mut bytes = std::fs::read(&auto).unwrap();
        bytes[24] ^= 1; // Payload still inflates, but its recorded raw checksum is damaged.
        std::fs::write(&auto, &bytes).unwrap();
        assert_eq!(scan(&d).get(&key), Some(&"mgba"), "a damaged header cannot claim gpSP ownership");
        assert_eq!(std::fs::read(&auto).unwrap(), bytes, "the Continue moment is retained for recovery");
        crate::state_file::write(&auto, b"existing mGBA moment").unwrap();
        let taken = crate::slots::taken_auto_path(&auto);
        std::fs::rename(&auto, &taken).unwrap();
        assert_eq!(scan(&d).get(&key), Some(&"mgba"), "compressed crash recovery keeps mGBA too");
        crate::state_file::write(&auto, &gpsp_state()).unwrap();
        assert_eq!(scan(&d).get(&key), Some(&"gpsp"), "a new quit wins over the old taken moment");
    }

    #[test]
    fn a_damaged_newest_moment_gives_way_to_the_core_of_the_one_continue_takes() {
        // `Core::resume` passes over a compressed moment it can't read and takes the next one: its core must play it.
        let d = crate::testutil::TestDir::new("gba-damaged-newest");
        std::fs::create_dir_all(d.join("gba")).unwrap();
        let auto = d.join("gba/Game.pstate.auto.z");
        crate::state_file::write(&crate::slots::taken_auto_path(&auto), &gpsp_state()).unwrap();
        std::fs::write(&auto, b"PCKSTZ\x01\0 cut off").unwrap();
        assert_eq!(scan(&d).get(&("gba".into(), "Game".into())), Some(&"gpsp"));
        // Nothing readable behind it: the slots decide, as without a Continue moment; with none either, mGBA.
        std::fs::remove_file(crate::slots::taken_auto_path(&auto)).unwrap();
        assert_eq!(scan(&d).get(&("gba".into(), "Game".into())), Some(&"mgba"));
        std::fs::write(d.join("gba/Game.pstate2"), gpsp_state()).unwrap();
        assert_eq!(scan(&d).get(&("gba".into(), "Game".into())), Some(&"gpsp"));
    }

    #[test]
    fn continue_core_matches_player_when_capture_times_tie_and_names_contain_state_suffixes() {
        let d = crate::testutil::TestDir::new("gba-owner-tie");
        std::fs::create_dir_all(d.join("gba")).unwrap();
        let raw = d.join("gba/Tied.pstate.auto");
        let packed = d.join("gba/Tied.pstate.auto.z");
        std::fs::write(&raw, b"existing mGBA moment").unwrap();
        crate::state_file::write(&packed, &gpsp_state()).unwrap();
        let at = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        for p in [&raw, &packed] {
            std::fs::File::options().write(true).open(p).unwrap().set_modified(at).unwrap();
        }
        let selected = crate::slots::auto_candidates(&packed, false).into_iter().find(|p| p.is_file());
        assert_eq!(selected, Some(packed), "the player prefers the compressed moment on a tie");
        assert_eq!(scan(&d).get(&("gba".into(), "Tied".into())), Some(&"gpsp"), "core ownership must select that same moment");
        let stem = "Unusual.pstate.auto.z";
        let unusual = d.join(format!("gba/{stem}.pstate.auto"));
        std::fs::write(&unusual, gpsp_state()).unwrap();
        assert_eq!(scan(&d).get(&("gba".into(), stem.into())), Some(&"gpsp"), "the ROM name cannot turn a raw state into a compressed one");
    }
}
