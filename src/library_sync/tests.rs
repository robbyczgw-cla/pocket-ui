//! Two (or three) devices sharing one folder, as Syncthing would leave it once both are in step: each pass reads
//! the others' files from it. Edits are made with the App's own types; the time of an edit is the file's mtime,
//! set by the test.

use super::*;
use crate::curator::lens::Lens;
use crate::play_log::{Game, Session};
use crate::testutil::TestDir;
use std::fs;
use std::time::{Duration, UNIX_EPOCH};

const ZELDA: &str = "/storage/roms/gb/Legend of Zelda, The - Link's Awakening (USA, Europe) (Rev 2).gb";
const TETRIS: &str = "/storage/roms/gb/Tetris (World) (Rev 1).gb";
/// Far after any real clock in a test run: mtimes are taken as they are.
const NOW: u64 = 4_000_000_000_000;
/// An hour in milliseconds, and the base of the made-up times (2027).
const H: u64 = 3_600_000;
const T0: u64 = 1_800_000_000_000;

fn root(name: &str) -> crate::testutil::TestDir {
    let d = TestDir::new(&format!("libsync-{name}"));
    fs::create_dir_all(&d).unwrap();
    d
}

struct Device {
    data: PathBuf,
    me: String,
    root: PathBuf,
    /// The play log (None: Play history off).
    log: Option<Arc<Log>>,
    /// Games of the library by key, where this device keeps them elsewhere.
    games: Vec<PathBuf>,
}

impl Device {
    fn new(root: &Path, me: &str) -> Device {
        let data = root.join(format!("data-{me}"));
        fs::create_dir_all(&data).unwrap();
        Device {
            data,
            me: me.to_string(),
            root: root.join("library-sync"),
            log: None,
            games: Vec::new(),
        }
    }

    fn meta(&self) -> LibraryMeta {
        LibraryMeta::load(&self.data)
    }

    fn favs(&self) -> Favorites {
        Favorites::load(&self.data)
    }

    fn prefs(&self) -> CuratorPrefs {
        CuratorPrefs::load(&self.data)
    }

    /// Sets the time a file was last written: the time of the edit made in it.
    fn at(&self, file: &str, t: u64) {
        let f = fs::File::options().write(true).open(self.data.join(file)).unwrap();
        f.set_modified(UNIX_EPOCH + Duration::from_millis(t)).unwrap();
    }

    fn edit_meta(&self, t: u64, f: impl FnOnce(&mut LibraryMeta)) {
        let mut m = self.meta();
        f(&mut m);
        self.at("library.json", t);
    }

    fn star(&self, t: u64, path: &str, name: &str) {
        let mut f = self.favs();
        f.toggle("gb", name, Path::new(path));
        self.at("favorites.json", t);
    }

    fn pass(&self) -> Outcome {
        let idx = index(self.games.iter().map(PathBuf::as_path));
        let find = |k: &str| idx.get(k).cloned().flatten();
        pass(&Setup {
            data_dir: &self.data,
            root: &self.root,
            me: &self.me,
            now: NOW,
            log: self.log.clone(),
            find: &find,
        })
    }

    /// A whole round: pass, apply, write the own folder (what the App does).
    fn sync(&self) -> (Outcome, Applied) {
        let out = self.pass();
        let applied = self.apply(&out);
        store(&self.root, &self.me, &out).unwrap();
        (out, applied)
    }

    fn apply(&self, out: &Outcome) -> Applied {
        let (mut m, mut f, mut p) = (self.meta(), self.favs(), self.prefs());
        let (applied, saved) = super::apply(&out.changes, &mut m, &mut f, &mut p);
        saved.unwrap();
        if applied.shelves {
            assert!(p.save(&self.data));
        }
        applied
    }

    fn get(&self, path: &str) -> GameMeta {
        self.meta().get(Path::new(path))
    }

    fn starred(&self, path: &str) -> bool {
        self.favs().contains(Path::new(path))
    }
}

fn pair(name: &str) -> (crate::testutil::TestDir, Device, Device) {
    let r = root(name);
    let a = Device::new(&r, "AAAAAAA-AAAAAAA");
    let b = Device::new(&r, "BBBBBBB-BBBBBBB");
    (r, a, b)
}

#[test]
fn keys_are_system_and_file_name_and_device_folders_the_start_of_the_id() {
    assert_eq!(game_key(Path::new(TETRIS)).as_deref(), Some("gb/Tetris (World) (Rev 1).gb"));
    assert_eq!(
        game_key(Path::new("/storage/roms/psx/Final Fantasy VII/FF7 (Disc 1).chd")).as_deref(),
        Some("psx/FF7 (Disc 1).chd"),
        "a subfolder keeps its system"
    );
    assert_eq!(
        game_key(Path::new("/media/usb/snes/Contra III.smc")).as_deref(),
        Some("snes/Contra III.smc"),
        "outside the roms: the folder"
    );
    assert_eq!(game_key(Path::new("/storage/roms/loose.gb")), None, "no system folder");
    assert_eq!(
        device_folder("P7T4NX2-24CSKCT-ABCDEFG-HIJKLMN-OPQRSTU-VWXYZ23-4567ABC-DEFGHIJ").as_deref(),
        Some("P7T4NX2-24CSKCT")
    );
    assert_eq!(device_folder(""), None);
    assert_eq!(device_folder("../../x-y"), None, "nothing that could leave the folder");
}

#[test]
fn both_edit_different_games_and_both_end_with_both() {
    let (_r, a, b) = pair("different");
    a.edit_meta(T0, |m| m.set_status(Path::new(ZELDA), Status::Playing).unwrap());
    b.edit_meta(T0 + H, |m| m.set_status(Path::new(TETRIS), Status::Completed).unwrap());
    b.star(T0 + H, TETRIS, "Tetris");
    let (out, _) = a.sync();
    assert!(out.changes.is_empty() && out.devices.is_empty(), "nothing from B yet");
    let (out, applied) = b.sync();
    assert_eq!(out.devices, ["AAAAAAA-AAAAAAA"]);
    assert_eq!(applied.applied, 1, "{:?}", out.changes);
    let (_, applied) = a.sync();
    assert_eq!(applied.applied, 2, "Tetris' status and star");
    for d in [&a, &b] {
        assert_eq!(d.get(ZELDA).status, Status::Playing, "{}", d.me);
        assert_eq!(d.get(TETRIS).status, Status::Completed, "{}", d.me);
        assert!(d.starred(TETRIS) && !d.starred(ZELDA), "{}", d.me);
    }
    // In step: another round changes nothing and writes nothing.
    let (out, _) = a.sync();
    assert!(out.changes.is_empty() && !out.state_changed, "{:?}", out.changes);
    let (out, _) = b.sync();
    assert!(out.changes.is_empty() && !out.state_changed, "{:?}", out.changes);
}

#[test]
fn the_same_game_edited_in_different_fields_keeps_both() {
    let (_r, a, b) = pair("fields");
    a.edit_meta(T0, |m| m.set_note(Path::new(ZELDA), "after the first boss").unwrap());
    b.edit_meta(T0 + H, |m| m.set_rating(Path::new(ZELDA), 4).unwrap());
    a.sync();
    b.sync();
    a.sync();
    for d in [&a, &b] {
        let z = d.get(ZELDA);
        assert_eq!((z.note.as_str(), z.rating), ("after the first boss", 4), "{}", d.me);
    }
}

#[test]
fn the_same_field_goes_to_the_later_edit_whichever_device_syncs_first() {
    for newer_first in [false, true] {
        let (_r, a, b) = pair(if newer_first { "same-newer-first" } else { "same-older-first" });
        // A paused at T0, B completed an hour later; neither had seen the other.
        a.edit_meta(T0, |m| m.set_status(Path::new(ZELDA), Status::Paused).unwrap());
        b.edit_meta(T0 + H, |m| m.set_status(Path::new(ZELDA), Status::Completed).unwrap());
        if newer_first {
            b.sync();
            a.sync();
            b.sync();
        } else {
            a.sync();
            b.sync();
            a.sync();
        }
        for d in [&a, &b] {
            assert_eq!(d.get(ZELDA).status, Status::Completed, "newer first: {newer_first}, {}", d.me);
        }
    }
}

#[test]
fn a_removal_beats_an_older_edit_and_loses_to_a_newer_one() {
    let (_r, a, b) = pair("delete");
    a.star(T0, ZELDA, "Link's Awakening");
    a.sync();
    b.sync();
    assert!(b.starred(ZELDA), "the star arrived");
    // B writes a note (T0+1h); A unstars (T0+2h). Both kept: the star is gone, the note stays.
    b.edit_meta(T0 + H, |m| m.set_note(Path::new(ZELDA), "water temple").unwrap());
    a.star(T0 + 2 * H, ZELDA, "Link's Awakening");
    assert!(!a.starred(ZELDA));
    b.sync();
    a.sync();
    b.sync();
    for d in [&a, &b] {
        assert!(!d.starred(ZELDA), "unstarred everywhere: {}", d.me);
        assert_eq!(d.get(ZELDA).note, "water temple", "{}", d.me);
    }
    // A note removed on A (T0+3h) and changed on B before that (T0+2.5h): removed.
    b.edit_meta(T0 + 5 * H / 2, |m| m.set_note(Path::new(ZELDA), "fire temple").unwrap());
    a.edit_meta(T0 + 3 * H, |m| m.set_note(Path::new(ZELDA), "").unwrap());
    a.sync();
    b.sync();
    a.sync();
    for d in [&a, &b] {
        assert_eq!(d.get(ZELDA).note, "", "{}", d.me);
    }
    // And starred again later on B: starred everywhere (the tombstone is older).
    b.star(T0 + 4 * H, ZELDA, "Link's Awakening");
    b.sync();
    a.sync();
    assert!(a.starred(ZELDA) && b.starred(ZELDA));
    let text = fs::read_to_string(a.root.join(&a.me).join(STATE_FILE)).unwrap();
    assert!(text.contains("\"note\"") && text.contains("null"), "the removed note is a tombstone: {text}");
}

#[test]
fn hidden_games_and_ratings_follow_and_say_the_lists_need_reading() {
    let (_r, a, b) = pair("hidden");
    a.edit_meta(T0, |m| {
        m.set_hidden(Path::new(TETRIS), true).unwrap();
        m.set_rating(Path::new(ZELDA), 9).unwrap();
    });
    a.sync();
    let (_, applied) = b.sync();
    assert!(applied.hidden && applied.library);
    assert!(b.get(TETRIS).hidden);
    assert_eq!(b.get(ZELDA).rating, RATING_MAX);
    b.edit_meta(T0 + H, |m| m.set_hidden(Path::new(TETRIS), false).unwrap());
    b.sync();
    let (_, applied) = a.sync();
    assert!(applied.hidden);
    assert!(!a.get(TETRIS).hidden);
}

#[test]
fn case_stickers_follow_and_come_off_again() {
    use crate::case_stickers::{CaseStickers, Kind};
    let (_r, a, b) = pair("case");
    let mut on = CaseStickers::default();
    on.set(Kind::Completed, true, 0);
    on.set(Kind::Goty, true, 1993);
    a.edit_meta(T0, |m| m.set_case_stickers(Path::new(ZELDA), on).unwrap());
    a.sync();
    let (_, applied) = b.sync();
    assert!(applied.library);
    assert_eq!(b.get(ZELDA).case, on);
    b.edit_meta(T0 + H, |m| m.set_case_stickers(Path::new(ZELDA), CaseStickers::default()).unwrap());
    b.sync();
    a.sync();
    assert!(a.get(ZELDA).case.is_empty());
}

#[test]
fn play_history_is_the_union_of_sessions_each_once() {
    let (_r, mut a, mut b) = pair("history");
    let session = |game: usize, start: u64, secs: u64| Session {
        game,
        start,
        tz: 3600,
        secs,
        ended: true,
        emulator: "pocket-core/gambatte".into(),
        ..Session::default()
    };
    let zelda = Game {
        path: ZELDA.into(),
        name: "Link's Awakening".into(),
        system: "gb".into(),
    };
    let tetris = Game {
        path: TETRIS.into(),
        name: "Tetris".into(),
        system: "gb".into(),
    };
    // Play history starts at 2026 times; T0 is in milliseconds.
    let s0 = 1_790_000_000;
    a.log = Some(Arc::new(play_log::build(
        vec![zelda.clone()],
        vec![session(0, s0, 1800), session(0, s0 + 86_400, 600)],
        vec![],
        vec![],
    )));
    b.log = Some(Arc::new(play_log::build(
        vec![tetris.clone(), zelda.clone()],
        vec![session(0, s0 + 3600, 900), session(1, s0 + 7200, 1200)],
        vec![],
        vec![],
    )));
    a.sync();
    let (out, _) = b.sync();
    let extra = out.extra.unwrap();
    assert_eq!(extra.sessions.len(), 2, "A's two sessions, shown on B: {extra:?}");
    assert!(extra.sessions.iter().all(|s| extra.games[s.game].path == ZELDA));
    let (out, _) = a.sync();
    let extra = out.extra.unwrap();
    assert_eq!(extra.sessions.len(), 2, "B's two, each once though B's file holds A's too: {extra:?}");
    assert_eq!(out.history.as_ref().unwrap().sessions.len(), 4);
    // The pages see the sum: four sessions, Zelda's three on one game.
    let all = play_log::combine(a.log.as_ref().unwrap(), &extra);
    assert_eq!(all.sessions.len(), 4);
    let z = all.find(ZELDA).unwrap();
    assert_eq!(all.sessions.iter().filter(|s| s.game == z).count(), 3);
    assert_eq!(all.sessions.iter().map(|s| s.secs).sum::<u64>(), 1800 + 600 + 900 + 1200);
    // A third round with nothing new writes nothing.
    let (out, _) = b.sync();
    assert!(!out.history_changed && out.extra.unwrap().sessions.len() == 2);
    // With Play history off a device reads and writes no history.
    a.log = None;
    let (out, _) = a.sync();
    assert!(out.history.is_none() && out.extra.is_none());
}

#[test]
fn a_damaged_or_strange_file_from_another_device_leaves_the_local_data_alone() {
    let (_r, a, b) = pair("corrupt");
    a.edit_meta(T0, |m| m.set_note(Path::new(ZELDA), "keep me").unwrap());
    a.star(T0, TETRIS, "Tetris");
    a.sync();
    let bdir = b.root.join(&b.me);
    fs::create_dir_all(&bdir).unwrap();
    for bad in [
        &b""[..],
        b"{\"version\":1,\"games\":{",
        b"\x00\x00\x00",
        b"[1,2,3]",
        b"{\"version\":1,\"games\":{\"gb/x.gb\":{\"f\":{\"note\":{\"v\":5}}}}}",
    ] {
        fs::write(bdir.join(STATE_FILE), bad).unwrap();
        fs::write(bdir.join(HISTORY_FILE), bad).unwrap();
        let (out, applied) = a.sync();
        assert!(out.changes.is_empty() && applied == Applied::default(), "{bad:?}: {:?}", out.changes);
        assert!(!out.problems.is_empty(), "said in the log: {bad:?}");
        assert_eq!(a.get(ZELDA).note, "keep me");
        assert!(a.starred(TETRIS));
    }
    // A folder that is no device's, a file in place of a folder: nothing read, nothing broken.
    fs::write(a.root.join("stray.txt"), "x").unwrap();
    fs::create_dir_all(a.root.join(".stfolder")).unwrap();
    let (out, _) = a.sync();
    assert!(out.changes.is_empty());
    assert_eq!(a.get(ZELDA).note, "keep me");
}

#[test]
fn what_a_newer_version_wrote_is_carried_and_never_overwritten() {
    let (_r, a, b) = pair("newer");
    a.edit_meta(T0, |m| m.set_status(Path::new(ZELDA), Status::Playing).unwrap());
    a.sync();
    // B (a newer pocket-ui) has a status this version doesn't know and a field of its own.
    let mut theirs = State {
        version: 2,
        device: b.me.clone(),
        ..State::default()
    };
    let rec = theirs.games.entry(game_key(Path::new(ZELDA)).unwrap()).or_default();
    rec.f.insert(
        "status".into(),
        Reg {
            v: Value::String("replaying".into()),
            t: T0 + H,
            d: b.me.clone(),
        },
    );
    rec.f.insert(
        "mood".into(),
        Reg {
            v: Value::String("calm".into()),
            t: T0 + H,
            d: b.me.clone(),
        },
    );
    fs::create_dir_all(b.root.join(&b.me)).unwrap();
    fs::write(b.root.join(&b.me).join(STATE_FILE), serde_json::to_vec(&theirs).unwrap()).unwrap();
    let (out, _) = a.sync();
    assert!(out.changes.is_empty(), "nothing this version can't hold: {:?}", out.changes);
    assert_eq!(a.get(ZELDA).status, Status::Playing, "kept as it was");
    // Carried on in A's own file, so a third device of the newer version gets it.
    let own: State = serde_json::from_slice(&fs::read(a.root.join(&a.me).join(STATE_FILE)).unwrap()).unwrap();
    let z = &own.games[&game_key(Path::new(ZELDA)).unwrap()];
    assert_eq!(z.f["mood"].v, "calm");
    assert_eq!(z.f["status"].v, "replaying");
    // And a second round does not take A's own older value for a new edit.
    let (out, _) = a.sync();
    assert!(!out.state_changed && out.changes.is_empty());
}

#[test]
fn an_edit_made_while_the_pass_ran_stays_and_wins_next_time() {
    let (_r, a, b) = pair("concurrent");
    b.edit_meta(T0, |m| m.set_note(Path::new(ZELDA), "from B").unwrap());
    b.sync();
    let out = a.pass();
    assert_eq!(out.changes.len(), 1);
    // The player writes a note on A before the result is applied.
    a.edit_meta(T0 + H, |m| m.set_note(Path::new(ZELDA), "typed on A").unwrap());
    let applied = a.apply(&out);
    assert_eq!((applied.applied, applied.skipped), (0, 1));
    assert_eq!(a.get(ZELDA).note, "typed on A");
    store(&a.root, &a.me, &out).unwrap();
    a.sync();
    b.sync();
    assert_eq!(b.get(ZELDA).note, "typed on A", "the later edit went over");
}

#[test]
fn a_pass_never_applied_is_simply_done_again() {
    let (_r, a, b) = pair("crash");
    b.edit_meta(T0, |m| m.set_status(Path::new(TETRIS), Status::Dropped).unwrap());
    b.sync();
    // A's pass found the change, and pocket-ui stopped before applying it (nothing stored either).
    let first = a.pass();
    assert_eq!(first.changes.len(), 1);
    // Meanwhile A's library.json was written for something else (a game's time): still B's status arrives.
    a.edit_meta(T0 + H, |m| m.record_session(Path::new(ZELDA), 600));
    let (again, applied) = a.sync();
    assert_eq!(again.changes, first.changes);
    assert_eq!(applied.applied, 1);
    assert_eq!(a.get(TETRIS).status, Status::Dropped);
}

#[test]
fn a_game_kept_in_another_folder_is_found_by_its_key() {
    let (_r, a, mut b) = pair("paths");
    let here = "/storage/roms/psx/Final Fantasy VII/FF7 (Disc 1).chd";
    let there = "/storage/roms/psx/FF7 (Disc 1).chd";
    b.games = vec![PathBuf::from(here)];
    a.star(T0, there, "Final Fantasy VII");
    a.edit_meta(T0, |m| m.set_status(Path::new(there), Status::Playing).unwrap());
    a.sync();
    b.sync();
    assert!(b.starred(here) && !b.starred(there));
    assert_eq!(b.get(here).status, Status::Playing);
    // B's own edit of it goes back to A's path.
    b.edit_meta(T0 + H, |m| m.set_note(Path::new(here), "disc 2 next").unwrap());
    b.sync();
    a.sync();
    assert_eq!(a.get(there).note, "disc 2 next");
    // Two files with one key on a device are left out there, not mixed up.
    let local = Local::of(
        &{
            let mut m = b.meta();
            m.set_note(Path::new("/storage/roms/psx/Other/FF7 (Disc 1).chd"), "another game").unwrap();
            m
        },
        &b.favs(),
        &b.prefs(),
    );
    assert!(local.ambiguous.contains("psx/FF7 (Disc 1).chd") && !local.games.contains_key("psx/FF7 (Disc 1).chd"));
}

#[test]
fn stickers_travel_by_name_and_a_rename_follows() {
    let (_r, a, b) = pair("stickers");
    a.edit_meta(T0, |m| {
        let couch = m.add_sticker("Couch co-op").unwrap();
        m.toggle_sticker(Path::new(ZELDA), couch).unwrap();
        m.set_sticker_pinned(couch, true).unwrap();
    });
    // B has stickers of its own: other ids, and ids are never compared.
    b.edit_meta(T0, |m| {
        let bed = m.add_sticker("Before bed").unwrap();
        m.toggle_sticker(Path::new(TETRIS), bed).unwrap();
    });
    a.sync();
    b.sync();
    a.sync();
    for d in [&a, &b] {
        let m = d.meta();
        let name_of = |path: &str| -> Vec<String> {
            m.get(Path::new(path))
                .stickers
                .iter()
                .filter_map(|id| m.stickers().iter().find(|s| s.id == *id))
                .map(|s| s.name.clone())
                .collect()
        };
        assert_eq!(name_of(ZELDA), ["Couch co-op"], "{}", d.me);
        assert_eq!(name_of(TETRIS), ["Before bed"], "{}", d.me);
        assert!(m.stickers().iter().find(|s| s.name == "Couch co-op").unwrap().pinned, "{}", d.me);
    }
    // Renamed on B: the old name goes, the new one is on the same games.
    b.edit_meta(T0 + H, |m| {
        let id = m.stickers().iter().find(|s| s.name == "Couch co-op").unwrap().id;
        m.rename_sticker(id, "Two players").unwrap();
    });
    b.sync();
    a.sync();
    let m = a.meta();
    let names: Vec<&str> = m.stickers().iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"Two players") && !names.contains(&"Couch co-op"), "{names:?}");
    let two = m.stickers().iter().find(|s| s.name == "Two players").unwrap().id;
    assert_eq!(m.get(Path::new(ZELDA)).stickers, [two]);
    // Deleted on A (later): gone on B too.
    a.edit_meta(T0 + 2 * H, |m| m.delete_sticker(two).unwrap());
    a.sync();
    b.sync();
    assert!(b.meta().stickers().iter().all(|s| s.name != "Two players"));
    assert!(b.get(ZELDA).stickers.is_empty());
}

#[test]
fn smart_shelves_are_made_unpinned_and_removed_on_the_other_device() {
    let (_r, a, b) = pair("shelves");
    let mut p = a.prefs();
    let mut lens = Lens::default();
    lens.picks.insert("status".into(), vec!["playing".to_string()]);
    p.add_shelf("Now playing", lens.clone());
    p.shelves[0].pinned = false;
    assert!(p.save(&a.data));
    a.at(crate::curator::prefs::FILE, T0);
    a.sync();
    let (_, applied) = b.sync();
    assert!(applied.shelves);
    let shelves = b.prefs().shelves;
    assert_eq!(shelves.len(), 1);
    assert_eq!((shelves[0].name.as_str(), shelves[0].pinned, &shelves[0].lens), ("Now playing", false, &lens));
    // Removed on B: removed on A.
    let mut p = b.prefs();
    p.shelves.clear();
    assert!(p.save(&b.data));
    b.at(crate::curator::prefs::FILE, T0 + H);
    b.sync();
    a.sync();
    assert!(a.prefs().shelves.is_empty());
}

#[test]
fn switched_off_nothing_is_read_and_edits_made_meanwhile_go_over_when_it_is_back() {
    assert!(!wanted(&crate::sync::Prefs::default()), "off at the start");
    assert!(
        !wanted(&crate::sync::Prefs {
            library: true,
            ..Default::default()
        }),
        "only with Syncthing on"
    );
    assert!(wanted(&crate::sync::Prefs {
        enabled: true,
        library: true,
        ..Default::default()
    }));
    let (_r, a, b) = pair("off");
    a.edit_meta(T0, |m| m.set_status(Path::new(ZELDA), Status::Playing).unwrap());
    a.sync();
    b.sync();
    // B is switched off (no passes) and edits; A edits another game.
    b.edit_meta(T0 + H, |m| m.set_note(Path::new(ZELDA), "made while off").unwrap());
    a.edit_meta(T0 + 2 * H, |m| m.set_status(Path::new(TETRIS), Status::Completed).unwrap());
    a.sync();
    assert_eq!(b.get(TETRIS).status, Status::NotStarted, "nothing arrives while off");
    // Back on: both ways.
    b.sync();
    a.sync();
    assert_eq!(a.get(ZELDA).note, "made while off");
    assert_eq!(b.get(TETRIS).status, Status::Completed);
}

#[test]
fn a_fresh_device_takes_everything_and_sends_nothing_back() {
    let (r, a, _b) = pair("fresh");
    a.edit_meta(T0, |m| {
        m.set_status(Path::new(ZELDA), Status::Completed).unwrap();
        m.set_note(Path::new(ZELDA), "100%").unwrap();
    });
    a.star(T0, ZELDA, "Link's Awakening");
    a.sync();
    // A re-flashed card: a new device ID, nothing on it.
    let c = Device::new(&r, "CCCCCCC-CCCCCCC");
    let (out, applied) = c.sync();
    assert_eq!(applied.applied, 3, "{:?}", out.changes);
    assert!(c.starred(ZELDA));
    assert_eq!(c.get(ZELDA).note, "100%");
    // Its own file holds A's values with A's times, no new ones of its own.
    let own: State = serde_json::from_slice(&fs::read(c.root.join(&c.me).join(STATE_FILE)).unwrap()).unwrap();
    assert!(own.games.values().flat_map(|r| r.f.values()).all(|r| r.d == a.me));
}

#[test]
fn the_syncthing_folder_is_the_one_a_pass_reads() {
    let dir = &crate::sync::Share::Library.dirs()[0];
    assert_eq!(Path::new(dir.path), super::root(Path::new(crate::sync::DATA_DIR)));
    assert_eq!(dir.id, "pocket-library");
    // Our temporary files and last good copies are never sent.
    assert!(dir.ignore.contains(&"*.tmp") && dir.ignore.contains(&"*.bak"));
}

#[test]
fn a_sticker_deleted_on_one_device_stays_where_the_other_put_it_on_later() {
    let (_r, a, b) = pair("sticker-race");
    a.edit_meta(T0, |m| {
        let id = m.add_sticker("Couch co-op").unwrap();
        m.toggle_sticker(Path::new(ZELDA), id).unwrap();
    });
    a.sync();
    b.sync();
    // A deletes it (T0+1h); B, not knowing, puts it on Tetris (T0+2h).
    a.edit_meta(T0 + H, |m| {
        let id = m.stickers()[0].id;
        m.delete_sticker(id).unwrap();
    });
    b.edit_meta(T0 + 2 * H, |m| {
        let id = m.stickers()[0].id;
        m.toggle_sticker(Path::new(TETRIS), id).unwrap();
    });
    a.sync();
    b.sync();
    a.sync();
    b.sync();
    for d in [&a, &b] {
        let m = d.meta();
        let couch = m
            .stickers()
            .iter()
            .find(|s| s.name == "Couch co-op")
            .unwrap_or_else(|| panic!("kept on {}", d.me))
            .id;
        assert_eq!(m.get(Path::new(TETRIS)).stickers, [couch], "{}", d.me);
        assert!(m.get(Path::new(ZELDA)).stickers.is_empty(), "taken off where A deleted it: {}", d.me);
    }
}
