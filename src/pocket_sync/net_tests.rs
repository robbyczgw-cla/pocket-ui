//! Pockets as folders on one machine, each with its service on 127.0.0.1: pairing, signatures, and the stories of
//! docs/SYNC-OWN.md over real connections, with the record and the files checked as a player would find them.

use super::*;
use crate::testutil::TestDir;

struct Pocket {
    _dir: TestDir,
    roms: PathBuf,
    store: Arc<Store>,
    svc: Arc<Service>,
    busy: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    port: u16,
}

/// A made-up key of the player for a test's Android Pocket: 16 hex letters of the game's path and system.
pub(super) fn test_key() -> super::super::KeyFn {
    Arc::new(|p: &Path, system: &str| {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (p, system).hash(&mut h);
        Some(format!("{:016x}", h.finish()))
    })
}

impl Pocket {
    fn new(name: &str) -> Pocket {
        Pocket::made(name, false)
    }

    /// Android's handheld view: games in `ROMs/`, saves in the player's `files/saves/<key>/` (`Layout::android`).
    fn android(name: &str) -> Pocket {
        Pocket::made(name, true)
    }

    fn made(name: &str, android: bool) -> Pocket {
        let dir = TestDir::new(&format!("psync-{name}"));
        let roms = dir.join(if android { "ROMs" } else { "roms" });
        std::fs::create_dir_all(&roms).unwrap();
        let data = dir.join("data/pocket-sync");
        let me = Me::load_or_new(&data, Some(name)).unwrap();
        let layout = if android {
            Layout::android(vec![roms.clone()], dir.join("files/saves"), dir.join("files/sync-backups"), test_key())
        } else {
            Layout::beside(roms.clone())
        };
        let store = Arc::new(Store::new(data, layout, me));
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let busy = Arc::new(AtomicBool::new(false));
        let b = busy.clone();
        let svc = Arc::new(Service::new(store.clone(), Box::new(move || b.load(Ordering::Relaxed)), port));
        let stop = Arc::new(AtomicBool::new(false));
        let (s2, st2) = (svc.clone(), stop.clone());
        std::thread::spawn(move || serve(listener, s2, st2));
        Pocket { _dir: dir, roms, store, svc, busy, stop, port }
    }

    /// Android: the player's `saves`.
    fn dir_saves(&self) -> PathBuf {
        self.store.layout.keyed().map(|k| k.saves.clone()).unwrap()
    }

    fn addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.port))
    }

    /// A game file, or a save where this Pocket keeps it (on Android: by its game's key).
    fn write(&self, rel: &str, bytes: &[u8]) {
        let p = self.store.layout.file(rel).unwrap_or_else(|| self.roms.join(rel));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn read(&self, rel: &str) -> Option<Vec<u8>> {
        std::fs::read(self.store.layout.file(rel).unwrap_or_else(|| self.roms.join(rel))).ok()
    }

    fn copies(&self, rel: &str) -> Vec<Vec<u8>> {
        self.store.copies(rel).into_iter().map(|(p, _)| std::fs::read(p).unwrap()).collect()
    }

    /// Shows a code, `other` pairs with it.
    fn pair_with(&self, other: &Pocket) {
        let p = Pairing::new().unwrap();
        let code = p.code.clone();
        *other.svc.pairing.lock().unwrap() = Some(p);
        let mut peers = self.svc.peers.lock().unwrap();
        pair(&self.store, &mut peers, other.addr(), &code, self.port).unwrap();
        // The other side reloads nothing: it stored this Pocket when it answered.
    }

    fn peer(&self, other: &Pocket) -> Peer {
        self.svc.peers.lock().unwrap().get(&other.store.me.id).cloned().unwrap()
    }

    fn sync(&self, other: &Pocket) -> Result<Report, SyncError> {
        let b = self.busy.clone();
        sync_with(&self.store, &self.peer(other), &move || b.load(Ordering::Relaxed))
    }

    /// The player keeps what is in the save's place now: the "Two versions" page moves the copies to the backup folder.
    fn choose_current(&self, rel: &str) {
        self.store.index();
        self.store.clear_copies(rel);
        self.store.chose(rel);
    }

    /// The player keeps the copy: it replaces the save, the old save goes to the backup folder.
    fn choose_copy(&self, rel: &str) {
        self.store.index();
        let (copy, _) = self.store.copies(rel).remove(0);
        let bytes = std::fs::read(&copy).unwrap();
        std::fs::remove_file(&copy).unwrap();
        self.store.clear_copies(rel);
        self.write(rel, &bytes);
        self.store.chose(rel);
    }
}

impl Drop for Pocket {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn paired(a: &Pocket, b: &Pocket) {
    a.pair_with(b);
}

#[test]
fn pairing_takes_the_code_once_and_counts_wrong_tries() {
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    let p = Pairing::new().unwrap();
    assert_eq!(p.code.len(), 6);
    let code = p.code.clone();
    *b.svc.pairing.lock().unwrap() = Some(p);
    let mut peers = Peers::default();
    let wrong = if code == "000000" { "000001" } else { "000000" };
    assert_eq!(pair(&a.store, &mut peers, b.addr(), wrong, a.port), Err(SyncError::Unpaired));
    assert_eq!(b.svc.pairing.lock().unwrap().as_ref().unwrap().tries_left, PAIRING_TRIES - 1);
    let peer = pair(&a.store, &mut peers, b.addr(), &code, a.port).unwrap();
    assert_eq!(peer.name, "B");
    assert!(b.svc.pairing.lock().unwrap().is_none(), "one code, one Pocket");
    assert_eq!(*b.svc.paired.lock().unwrap(), Some("A".to_string()));
    assert_eq!(pair(&a.store, &mut peers, b.addr(), &code, a.port), Err(SyncError::Unpaired));
    // Both keep the same key; B knows where A's service is.
    let there = b.svc.peers.lock().unwrap().get(&a.store.me.id).cloned().unwrap();
    assert_eq!(there.key, peer.key);
    assert_eq!(there.addr, a.addr().to_string());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(b.store.dir.join("peers.json")).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the keys are readable by root only");
    }
    // Stored and read back.
    assert_eq!(Peers::load(&a.store.dir).get(&b.store.me.id).map(|p| p.key), Some(peer.key));
}

#[test]
fn a_pocket_paired_from_this_side_can_start_a_sync_too() {
    // Found on an RG35XX Plus (18.11.2026): the Plus paired with a computer from a second process while its service
    // ran; the computer's sync to the Plus was refused (401) because the running service still knew no Pockets.
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    let p = Pairing::new().unwrap();
    let code = p.code.clone();
    *b.svc.pairing.lock().unwrap() = Some(p);
    // Pairing written to the card only, not to A's running service.
    let mut on_card = Peers::load(&a.store.dir);
    pair(&a.store, &mut on_card, b.addr(), &code, a.port).unwrap();
    assert!(a.svc.peers.lock().unwrap().0.is_empty());
    b.write("gba/G.srm", b"from b");
    let r = b.sync(&a).unwrap();
    assert_eq!(r.sent, ["gba/G.srm"]);
    assert_eq!(a.read("gba/G.srm").as_deref(), Some(&b"from b"[..]));
}

#[test]
fn an_expired_or_spent_code_pairs_nobody() {
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    let mut p = Pairing::new().unwrap();
    p.until = Instant::now() - Duration::from_secs(1);
    let code = p.code.clone();
    *b.svc.pairing.lock().unwrap() = Some(p);
    assert_eq!(pair(&a.store, &mut Peers::default(), b.addr(), &code, a.port), Err(SyncError::Unpaired));
}

#[test]
fn requests_without_the_key_are_refused_and_answers_are_checked() {
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    paired(&a, &b);
    let mut stranger = a.peer(&b);
    stranger.key = [7; 32];
    assert_eq!(sync_with(&a.store, &stranger, &|| false), Err(SyncError::Unpaired));
    // Forgotten there: refused too.
    b.svc.forget(&a.store.me.id).unwrap();
    assert_eq!(a.sync(&b), Err(SyncError::Unpaired));
}

#[test]
fn a_path_from_the_network_is_data() {
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    paired(&a, &b);
    let peer = a.peer(&b);
    let c = Conn { peer: &peer, me: &a.store.me.id };
    for rel in ["../../etc/passwd.srm", "/etc/x.srm", "gba/x.exe"] {
        let t = format!("/file?rel={}&vv=A:1&mtime=0&expect=none", pct_encode(rel));
        assert_eq!(c.call("PUT", &t, b"x").unwrap().0, 400, "{rel}");
        assert_eq!(c.call("GET", &format!("/file?rel={}", pct_encode(rel)), b"").unwrap().0, 400, "{rel}");
    }
    assert!(!b.roms.parent().unwrap().join("etc").exists());
}

#[test]
fn only_home_networks_may_connect() {
    // Built from numbers: a home network, Tailscale's range (100.64/10), loopback, link-local, IPv6 local ranges.
    let v4 = |a, b, c, d| IpAddr::from([a, b, c, d]);
    let v6 = |s: u16| IpAddr::from([s, 0, 0, 0, 0, 0, 0, 1]);
    for ok in [v4(192, 168, 1, 20), v4(10, 1, 2, 3), v4(172, 20, 0, 1), v4(100, 64, 9, 9), v4(127, 0, 0, 1), v4(169, 254, 1, 1), v6(0xfe80), v6(0xfd7a)] {
        assert!(peer_allowed(ok), "{ok}");
    }
    for no in [v4(8, 8, 8, 8), v4(100, 128, 0, 1), v4(172, 32, 0, 1), v6(0x2001)] {
        assert!(!peer_allowed(no), "{no}");
    }
}

#[test]
fn two_pockets_saves_go_both_ways_matched_by_the_game() {
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    paired(&a, &b);
    a.write("gba/Pokemon Emerald (USA).gba", b"emerald rom");
    b.write("gba/Emerald.gba", b"emerald rom");
    a.write("gba/Pokemon Emerald (USA).srm", b"emerald save 1");
    a.write("gb/Tetris.gb", b"tetris rom");
    b.write("gb/Tetris.gb", b"tetris rom");
    a.write("gb/Tetris.srm", b"tetris save");
    a.write("gb/Tetris.rtc", b"clock");
    b.write("nes/Mario.nes", b"mario rom");
    b.write("nes/Mario.srm", b"mario save");
    set_mtime(&a.roms.join("gb/Tetris.srm"), 1_794_000_000);
    let r = a.sync(&b).unwrap();
    assert_eq!((r.sent.len(), r.fetched.len(), r.asked.len()), (3, 1, 0), "{r:?}");
    assert_eq!(b.read("gba/Emerald.srm").as_deref(), Some(&b"emerald save 1"[..]), "under B's own name for the game");
    assert_eq!(b.read("gb/Tetris.rtc").as_deref(), Some(&b"clock"[..]));
    assert_eq!(a.read("nes/Mario.srm").as_deref(), Some(&b"mario save"[..]));
    let t = std::fs::metadata(b.roms.join("gb/Tetris.srm")).unwrap().modified().unwrap();
    assert_eq!(t, UNIX_EPOCH + Duration::from_secs(1_794_000_000), "the save's time is kept");
    let r = a.sync(&b).unwrap();
    assert_eq!((r.same, r.sent.len(), r.fetched.len()), (4, 0, 0), "{r:?}");
    // B plays Tetris and starts the next sync itself: A takes it, A's old save is in the backup folder.
    b.write("gb/Tetris.srm", b"tetris save 2");
    let r = b.sync(&a).unwrap();
    assert_eq!(r.sent, ["gb/Tetris.srm"]);
    assert_eq!(a.read("gb/Tetris.srm").as_deref(), Some(&b"tetris save 2"[..]));
    assert_eq!(std::fs::read(a.store.backup_dir().join("gb/Tetris.srm")).unwrap(), b"tetris save");
}

#[test]
fn both_played_one_question_nothing_overwritten_and_the_choice_goes_over() {
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    paired(&a, &b);
    a.write("gba/G.gba", b"rom");
    b.write("gba/G.gba", b"rom");
    a.write("gba/G.srm", b"v1");
    a.sync(&b).unwrap();
    a.write("gba/G.srm", b"a plays");
    b.write("gba/G.srm", b"b plays");
    let r = a.sync(&b).unwrap();
    assert_eq!(r.asked, ["gba/G.srm"]);
    assert_eq!(a.read("gba/G.srm").as_deref(), Some(&b"a plays"[..]));
    assert_eq!(b.read("gba/G.srm").as_deref(), Some(&b"b plays"[..]));
    assert_eq!(a.copies("gba/G.srm"), vec![b"b plays".to_vec()]);
    // The copy's name is the one the "Two versions" page reads.
    let (copy, _) = a.store.copies("gba/G.srm").remove(0);
    let name = copy.file_name().unwrap().to_str().unwrap();
    assert_eq!(crate::sync_conflicts::original_name(name).as_deref(), Some("G.srm"));
    // Either side starting again: no second question.
    assert_eq!(b.sync(&a).unwrap().waiting, 1);
    assert_eq!(a.sync(&b).unwrap().waiting, 1);
    assert!(b.copies("gba/G.srm").is_empty());
    // The player keeps B's version on A: it is the save everywhere, A's own goes to the backup folder.
    a.choose_copy("gba/G.srm");
    let r = a.sync(&b).unwrap();
    assert!(r.asked.is_empty() && r.waiting == 0, "{r:?}");
    assert_eq!(a.read("gba/G.srm"), b.read("gba/G.srm"));
    assert_eq!(a.read("gba/G.srm").as_deref(), Some(&b"b plays"[..]));
    assert!(a.copies("gba/G.srm").is_empty());
    // Then A plays on: it goes over without a question.
    a.write("gba/G.srm", b"a plays on");
    assert_eq!(a.sync(&b).unwrap().sent, ["gba/G.srm"]);
    assert_eq!(b.read("gba/G.srm").as_deref(), Some(&b"a plays on"[..]));
}

#[test]
fn keeping_the_own_version_replaces_the_other_there_with_a_backup() {
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    paired(&a, &b);
    a.write("gba/G.srm", b"v1");
    a.sync(&b).unwrap();
    a.write("gba/G.srm", b"a plays");
    b.write("gba/G.srm", b"b plays");
    a.sync(&b).unwrap();
    a.choose_current("gba/G.srm");
    assert_eq!(a.sync(&b).unwrap().sent, ["gba/G.srm"]);
    assert_eq!(b.read("gba/G.srm").as_deref(), Some(&b"a plays"[..]));
    assert_eq!(std::fs::read(b.store.backup_dir().join("gba/G.srm")).unwrap(), b"b plays");
}

#[test]
fn nothing_is_written_while_a_game_runs_on_either_side() {
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    paired(&a, &b);
    a.write("gba/G.srm", b"v1");
    b.busy.store(true, Ordering::Relaxed);
    assert_eq!(a.sync(&b), Err(SyncError::Busy));
    assert!(b.read("gba/G.srm").is_none());
    b.busy.store(false, Ordering::Relaxed);
    a.busy.store(true, Ordering::Relaxed);
    assert_eq!(a.sync(&b), Err(SyncError::Busy));
    a.busy.store(false, Ordering::Relaxed);
    assert_eq!(a.sync(&b).unwrap().sent, ["gba/G.srm"]);
}

#[test]
fn a_pocket_that_is_off_is_unreachable_and_nothing_changes() {
    let a = Pocket::new("A");
    let b = Pocket::new("B");
    paired(&a, &b);
    a.write("gba/G.srm", b"v1");
    let peer = a.peer(&b);
    drop(b);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(sync_with(&a.store, &peer, &|| false), Err(SyncError::Unreachable));
    assert_eq!(a.read("gba/G.srm").as_deref(), Some(&b"v1"[..]));
}

#[test]
fn three_pockets_no_server_no_false_question_and_one_question_for_independent_play() {
    let (a, b, c) = (Pocket::new("A"), Pocket::new("B"), Pocket::new("C"));
    paired(&a, &b);
    paired(&b, &c);
    paired(&a, &c);
    // A plays, B takes it from A, C from B; A plays on, C takes it from A: no question.
    a.write("gba/G.srm", b"a1");
    b.sync(&a).unwrap();
    c.sync(&b).unwrap();
    a.write("gba/G.srm", b"a2");
    let r = c.sync(&a).unwrap();
    assert_eq!(r.fetched, ["gba/G.srm"]);
    // B, still on a1, meets C: takes a2; the old one does not walk back.
    assert_eq!(b.sync(&c).unwrap().fetched, ["gba/G.srm"]);
    assert_eq!(a.sync(&b).unwrap().same, 1);
    // A and C play apart: one question, whoever meets whom.
    a.write("gba/G.srm", b"a3");
    c.write("gba/G.srm", b"c1");
    assert_eq!(c.sync(&a).unwrap().asked, ["gba/G.srm"]);
    for (x, y) in [(&a, &c), (&b, &a), (&b, &c), (&c, &b), (&a, &b)] {
        let r = x.sync(y).unwrap();
        assert!(r.asked.is_empty(), "{r:?}");
    }
    let open: usize = [&a, &b, &c].iter().map(|p| p.copies("gba/G.srm").len()).sum();
    assert_eq!(open, 1, "exactly one question on all three Pockets");
    // C keeps its own; it reaches everyone without a new question.
    c.choose_current("gba/G.srm");
    for (x, y) in [(&c, &a), (&b, &a), (&b, &c), (&a, &b)] {
        let r = x.sync(y).unwrap();
        assert!(r.asked.is_empty(), "{r:?}");
    }
    for p in [&a, &b, &c] {
        assert_eq!(p.read("gba/G.srm").as_deref(), Some(&b"c1"[..]));
        assert!(p.copies("gba/G.srm").is_empty());
    }
}

#[test]
fn new_play_joins_the_open_question() {
    let (a, b, d) = (Pocket::new("A"), Pocket::new("B"), Pocket::new("D"));
    paired(&a, &b);
    paired(&d, &a);
    a.write("gba/G.srm", b"a1");
    b.write("gba/G.srm", b"b1");
    d.write("gba/G.srm", b"d1");
    assert_eq!(a.sync(&b).unwrap().asked, ["gba/G.srm"]);
    // D played apart too and meets A, which is asking: D's version joins A's question.
    assert_eq!(d.sync(&a).unwrap().offered, ["gba/G.srm"]);
    let mut shown = a.copies("gba/G.srm");
    shown.sort();
    assert_eq!(shown, vec![b"b1".to_vec(), b"d1".to_vec()]);
    // The choice covers all three: D takes it without a question.
    a.choose_current("gba/G.srm");
    assert_eq!(d.sync(&a).unwrap().fetched, ["gba/G.srm"]);
    assert_eq!(a.sync(&b).unwrap().sent, ["gba/G.srm"]);
}

#[test]
fn finding_pockets_on_the_wifi() {
    let a = Pocket::new("A");
    let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
    let to = sock.local_addr().unwrap();
    let (svc, stop) = (a.svc.clone(), a.stop.clone());
    std::thread::spawn(move || answer_discovery(sock, svc, stop));
    let found = discover(to, Duration::from_millis(600));
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!((found[0].id.as_str(), found[0].name.as_str(), found[0].addr.port(), found[0].pairing), (a.store.me.id.as_str(), "A", a.port, false));
    *a.svc.pairing.lock().unwrap() = Some(Pairing::new().unwrap());
    assert!(discover(to, Duration::from_millis(600))[0].pairing, "a Pocket showing a code says so");
}

#[test]
fn the_device_id_is_made_once_and_names_are_cleaned() {
    let d = TestDir::new("psync-me");
    let me = Me::load_or_new(&d, None).unwrap();
    assert!(super::super::valid_id(&me.id) && me.id.len() == 16);
    assert_eq!(Me::load_or_new(&d, None).unwrap(), me);
    let renamed = Me::load_or_new(&d, Some("Alex's\nPlus")).unwrap();
    assert_eq!((renamed.id.as_str(), renamed.name.as_str()), (me.id.as_str(), "Alex'sPlus"));
    assert_eq!(clean_name("   "), "Pocket");
}

#[test]
fn a_first_sync_of_a_big_library_goes_in_bundles_both_ways() {
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    paired(&a, &b);
    for i in 0..150 {
        a.write(&format!("snes/a{i:03}.srm"), format!("a save {i}").as_bytes());
        b.write(&format!("gba/b{i:03}.srm"), format!("b save {i}").as_bytes());
    }
    let r = a.sync(&b).unwrap();
    assert_eq!((r.sent.len(), r.fetched.len(), r.failed.len()), (150, 150, 0));
    for i in [0, 63, 64, 149] {
        assert_eq!(b.read(&format!("snes/a{i:03}.srm")).unwrap(), format!("a save {i}").as_bytes());
        assert_eq!(a.read(&format!("gba/b{i:03}.srm")).unwrap(), format!("b save {i}").as_bytes());
    }
    let r = b.sync(&a).unwrap();
    assert_eq!((r.same, r.sent.len(), r.fetched.len()), (300, 0, 0));
}

#[test]
fn a_bundle_from_the_network_is_checked() {
    let parts: [&[u8]; 2] = [b"one", b"three"];
    let b = bundle(&json!({"files": [{"size": 3}, {"size": 5}]}), &parts);
    let (_, got) = unbundle(&b).unwrap();
    assert_eq!(got, parts);
    let mut longer = b.clone();
    longer.push(b'x');
    assert!(unbundle(&longer).is_none(), "bytes left over");
    assert!(unbundle(&b[..b.len() - 1]).is_none(), "cut off");
    assert!(unbundle(b"99999999999\n{}").is_none());
    assert!(unbundle(b"2\n{}").is_none(), "no files list");
}

// The file-fault rule (tests/data_sources.rs, scripts/reader-inventory.py): the paired Pockets, this Pocket's id, a
// question's copy and a save another Pocket asks for, each missing, empty, cut, flipped or in another encoding: no
// panic; a damaged list of Pockets pairs nobody, a damaged id makes a new one, a damaged save is sent as it is.
#[test]
fn file_source_rule_sync_peers_device_copies_and_saves() {
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    paired(&a, &b);
    let peers_path = a.store.dir.join("peers.json");
    let peers_seed = std::fs::read(&peers_path).unwrap();
    let put = |p: &Path, bytes: &Option<Vec<u8>>| {
        let _ = std::fs::remove_file(p);
        if let Some(x) = bytes {
            std::fs::write(p, x).unwrap();
        }
    };
    for (label, bytes) in crate::file_faults::faults(&peers_seed) {
        put(&peers_path, &bytes);
        let peers = Peers::load(&a.store.dir);
        if label == "valid" {
            assert_eq!(peers.get(&b.store.me.id).map(|p| p.key), Some(a.peer(&b).key));
        }
    }
    put(&peers_path, &Some(peers_seed));
    let d = TestDir::new("psync-me-faults");
    let me = Me::load_or_new(&d, Some("Plus")).unwrap();
    let me_seed = std::fs::read(d.join("device.json")).unwrap();
    for (label, bytes) in crate::file_faults::faults(&me_seed) {
        put(&d.join("device.json"), &bytes);
        let got = Me::load_or_new(&d, None).unwrap();
        assert!(super::super::valid_id(&got.id), "{label}");
        if label == "valid" {
            assert_eq!(got, me);
        }
    }
    a.write("gba/G.srm", b"the save");
    for (label, bytes) in crate::file_faults::faults(b"the other version") {
        let copy = a.roms.join("gba/G.sync-conflict-20261118-000000-B.srm");
        put(&copy, &bytes);
        let copies = a.store.copies("gba/G.srm");
        assert_eq!(copies.len(), usize::from(bytes.is_some()), "{label}");
        let _ = std::fs::remove_file(&copy);
    }
    let peer = b.peer(&a);
    let c = Conn { peer: &peer, me: &b.store.me.id };
    for (label, bytes) in crate::file_faults::faults(b"the save") {
        put(&a.roms.join("gba/G.srm"), &bytes);
        let (st, _, body) = c.call("GET", "/file?rel=gba/G.srm", b"").unwrap();
        match &bytes {
            None => assert_eq!(st, 404, "{label}"),
            Some(x) => assert_eq!((st, &body), (200, x), "{label}"),
        }
        assert!(b.sync(&a).is_ok(), "{label}");
    }
}

// Sol review 19.11.2026 #2 over real connections: two games with one file name in two folders keep their own saves,
// and a save never lands on a file the card has in other letters.
#[test]
fn saves_in_two_folders_with_one_name_stay_apart_and_letters_never_collide() {
    let (a, b) = (Pocket::new("A"), Pocket::new("B"));
    paired(&a, &b);
    a.write("gba/A/Game.srm", b"game in A");
    a.write("gba/B/Game.srm", b"game in B");
    b.write("gba/B/Game.srm", b"game in B there");
    let r = a.sync(&b).unwrap();
    assert!(r.asked.contains(&"gba/B/Game.srm".to_string()), "{r:?}");
    assert_eq!(b.read("gba/A/Game.srm").as_deref(), Some(&b"game in A"[..]), "A's own folder, not B's");
    assert_eq!(b.read("gba/B/Game.srm").as_deref(), Some(&b"game in B there"[..]), "nothing overwritten");
    // The other Pocket has "Mario.SRM": a new "Mario.srm" from here does not go over it.
    a.write("nes/Mario.srm", b"from a");
    b.write("nes/Mario.SRM", b"from b");
    a.sync(&b).unwrap();
    assert_eq!(b.read("nes/Mario.SRM").as_deref(), Some(&b"from b"[..]));
}

// Android (0.6.0): the handheld view's player keeps a game's saves in `files/saves/<key>/`, not beside the game. The Linux
// Pocket and the Android one sync as two ROCKNIX Pockets do, every file where its own player reads it; a save of a game the
// Android Pocket doesn't have is nowhere to go there (not a failure), and comes once the game does.
#[test]
fn an_android_pocket_keeps_saves_where_its_player_reads_them_both_ways() {
    let (plus, mini) = (Pocket::new("Plus"), Pocket::android("Mini"));
    paired(&mini, &plus);
    plus.write("gba/Pokemon Emerald (USA).gba", b"emerald rom");
    mini.write("GBA/Emerald.gba", b"emerald rom");
    plus.write("gba/Pokemon Emerald (USA).srm", b"emerald on the plus");
    plus.write("gb/Tetris.gb", b"tetris rom");
    mini.write("gb/Tetris.gb", b"tetris rom");
    mini.write("gb/Tetris.srm", b"tetris on the mini");
    plus.write("nes/Mario.nes", b"mario rom");
    plus.write("nes/Mario.srm", b"mario save");
    // A save in the player's folder whose game is gone from the ROMs folder is not in the index.
    std::fs::create_dir_all(mini.dir_saves().join("0123456789abcdef")).unwrap();
    std::fs::write(mini.dir_saves().join("0123456789abcdef/Gone.srm"), b"x").unwrap();
    let idx: Vec<String> = mini.store.index().into_iter().map(|e| e.rel).collect();
    assert_eq!(idx, ["gb/Tetris.srm"]);

    let r = mini.sync(&plus).unwrap();
    assert_eq!((r.sent.len(), r.fetched.len(), r.failed.len(), r.no_place), (1, 1, 0, 1), "{r:?}");
    let key = (test_key())(&mini.roms.join("GBA/Emerald.gba"), "gba").unwrap();
    assert_eq!(std::fs::read(mini.dir_saves().join(&key).join("Emerald.srm")).unwrap(), b"emerald on the plus", "the player's file for the game");
    assert_eq!(plus.read("gb/Tetris.srm").as_deref(), Some(&b"tetris on the mini"[..]));
    assert!(!mini.roms.join("nes").exists() && !mini.roms.join("GBA/Emerald.srm").exists(), "nothing written beside the games");

    // The Plus starts the next sync: Mario is still nowhere to go, nothing failed; then the Mini gets the game.
    plus.write("gb/Tetris.srm", b"tetris on the plus");
    let r = plus.sync(&mini).unwrap();
    assert_eq!((r.sent.len(), r.failed.len(), r.no_place), (1, 0, 1), "{r:?}");
    assert_eq!(mini.read("gb/Tetris.srm").as_deref(), Some(&b"tetris on the plus"[..]));
    assert_eq!(std::fs::read(mini.store.backup_dir().join("gb/Tetris.srm")).unwrap(), b"tetris on the mini", "the old save in the app's backups");
    mini.write("nes/Super Mario Bros.nes", b"mario rom");
    let r = plus.sync(&mini).unwrap();
    assert_eq!((r.sent, r.no_place), (vec!["nes/Mario.srm".to_string()], 0));
    assert_eq!(mini.read("nes/Super Mario Bros.srm").as_deref(), Some(&b"mario save"[..]), "under the Mini's name for the game");

    // Played on both: the question's copy is beside the player's save, where the "Two versions" page looks.
    mini.write("GBA/Emerald.srm", b"mini plays");
    plus.write("gba/Pokemon Emerald (USA).srm", b"plus plays");
    assert_eq!(mini.sync(&plus).unwrap().asked, ["GBA/Emerald.srm"]);
    let copies = mini.store.copies("GBA/Emerald.srm");
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].0.parent(), Some(mini.dir_saves().join(&key).as_path()));
    let layout = crate::sync_conflicts::android(&mini.dir_saves().parent().unwrap().to_path_buf());
    let groups = crate::sync_conflicts::find(&crate::sync_conflicts::android_roots(&layout), &layout);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].kind, crate::sync_conflicts::Kind::Save);
    let game = mini.roms.join("GBA/Emerald.gba");
    let games = [crate::sync_conflicts::Game { system: "gba", path: &game }];
    let k = test_key();
    assert_eq!(crate::sync_conflicts::owner_by_key(&groups[0], &games, &layout, k.as_ref()), crate::sync_conflicts::Owner::Game(game.clone()));
    assert_eq!(mini.store.layout.rel_of(&groups[0].original).as_deref(), Some("GBA/Emerald.srm"), "the page's choice reaches the record");
    // The player keeps the Plus's version on the Mini: it goes everywhere, no second question.
    mini.choose_copy("GBA/Emerald.srm");
    let r = mini.sync(&plus).unwrap();
    assert!(r.asked.is_empty() && r.waiting == 0, "{r:?}");
    assert_eq!(plus.read("gba/Pokemon Emerald (USA).srm").as_deref(), Some(&b"plus plays"[..]));
    assert_eq!(mini.read("GBA/Emerald.srm").as_deref(), Some(&b"plus plays"[..]));
}
