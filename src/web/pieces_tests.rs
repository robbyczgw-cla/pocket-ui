//! Uploads in pieces (`/api/upload/start`, `/api/upload/part`, `/api/upload/cancel`; `files::Partial`), end to end
//! over a real socket with the rig of `tests.rs`. Chrome on Android broke off a 1.62 GB PS2 disc sent in one request
//! at once (25.11.2026); the page now sends 8 MB pieces, and these tests hold the Pocket's side to the rules of a whole
//! upload: the size fixed at the start, each piece exactly where the file stands, nothing in place before the last
//! byte, nothing kept when it ends early.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::files::{self, Refused};
use super::tests::{rig, rig_ctx, walk, Answer, Rig};

/// The temporary files under way in the rig's uploads folder.
fn temps(r: &Rig) -> Vec<PathBuf> {
    walk(&r.root.join("roms").join(files::UPLOADS_DIR))
}

struct Browser<'a> {
    r: &'a Rig,
    cookie: String,
    csrf: String,
}

impl Browser<'_> {
    fn new(r: &Rig) -> Browser<'_> {
        let (cookie, csrf) = r.pair();
        Browser { r, cookie, csrf }
    }

    fn post(&self, path: &str, body: &[u8]) -> Answer {
        self.r.req("POST", path, &[("Cookie", &self.cookie), ("X-CSRF-Token", &self.csrf)], body)
    }

    fn start(&self, q: &str, size: usize) -> Answer {
        self.post(&format!("/api/upload/start?{q}&size={size}"), b"")
    }

    fn id(&self, q: &str, size: usize) -> String {
        let a = self.start(q, size);
        assert_eq!(a.status, 200, "{}", String::from_utf8_lossy(&a.body));
        a.json()["id"].as_str().unwrap().to_string()
    }

    fn part(&self, id: &str, offset: usize, bytes: &[u8]) -> Answer {
        self.post(&format!("/api/upload/part?id={id}&offset={offset}"), bytes)
    }

    /// The whole file in pieces of `piece` bytes; the last answer.
    fn send(&self, q: &str, data: &[u8], piece: usize) -> Answer {
        let id = self.id(q, data.len());
        let mut at = 0;
        loop {
            let end = (at + piece).min(data.len());
            let a = self.part(&id, at, &data[at..end]);
            if a.status != 200 || a.json()["path"].is_string() {
                return a;
            }
            assert_eq!(a.json()["received"], end as u64);
            at = end;
        }
    }
}

#[test]
fn pieces_in_order_make_the_file_and_only_the_last_puts_it_in_place() {
    let r = rig("pieces-order", false);
    let b = Browser::new(&r);
    let id = b.id("kind=rom&system=gba&name=Big.gba", 12);
    assert_eq!(temps(&r).len(), 1, "the temporary file, in the uploads folder");
    let a = b.part(&id, 0, b"abcd");
    assert_eq!((a.status, a.json()["received"].as_u64()), (200, Some(4)));
    assert!(!r.root.join("roms/gba/Big.gba").exists(), "nothing in place before the last byte");
    // At the wrong place, and the same piece twice: refused with where the Pocket stands; nothing written twice.
    let a = b.part(&id, 8, b"ijkl");
    assert_eq!((a.status, a.json()["received"].as_u64()), (416, Some(4)));
    let a = b.part(&id, 0, b"abcd");
    assert_eq!((a.status, a.json()["received"].as_u64()), (416, Some(4)), "a piece sent again after its answer was lost");
    assert_eq!(b.part(&id, 4, b"efgh").json()["received"], 8);
    let done = b.part(&id, 8, b"ijkl");
    assert_eq!(done.status, 200, "{}", String::from_utf8_lossy(&done.body));
    assert_eq!(done.json()["replaced"], false);
    assert!(done.json()["path"].as_str().unwrap().ends_with("roms/gba/Big.gba"));
    assert_eq!(std::fs::read(r.root.join("roms/gba/Big.gba")).unwrap(), b"abcdefghijkl");
    assert!(temps(&r).is_empty(), "no temporary file left");
    assert_eq!(b.part(&id, 12, b"x").status, 404, "done: the id is gone");
}

#[test]
fn a_piece_past_the_end_ends_the_upload_and_keeps_nothing() {
    let r = rig("pieces-past", false);
    let b = Browser::new(&r);
    let id = b.id("kind=rom&system=gba&name=Over.gba", 6);
    assert_eq!(b.part(&id, 0, b"abcd").status, 200);
    assert_eq!(b.part(&id, 4, b"efgh").status, 413, "the size was fixed at the start");
    assert_eq!(b.part(&id, 4, b"ef").status, 404);
    assert!(!r.root.join("roms/gba/Over.gba").exists());
    assert!(temps(&r).is_empty());
}

#[test]
fn a_piece_cut_off_on_the_way_is_taken_back_and_sent_again() {
    let r = rig("pieces-cut", false);
    let b = Browser::new(&r);
    let id = b.id("kind=rom&system=gba&name=Cut.gba", 8);
    assert_eq!(b.part(&id, 0, b"abcd").status, 200);
    // The second piece breaks off after two of its four bytes (the phone's Wi-Fi drops).
    let mut s = TcpStream::connect(r.addr).unwrap();
    s.set_read_timeout(Some(crate::testutil::timeout(Duration::from_secs(10)))).unwrap();
    write!(s, "POST /api/upload/part?id={id}&offset=4 HTTP/1.1\r\nHost: {}\r\nCookie: {}\r\nX-CSRF-Token: {}\r\nContent-Length: 4\r\n\r\nXX", r.addr, b.cookie, b.csrf).unwrap();
    s.shutdown(std::net::Shutdown::Write).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    assert!(out.starts_with("HTTP/1.1 400"), "{out}");
    assert!(out.contains("\"received\":4"), "{out}");
    // Sent again whole: the two bytes of the cut piece are not in the file.
    assert_eq!(b.part(&id, 4, b"efgh").status, 200);
    assert_eq!(std::fs::read(r.root.join("roms/gba/Cut.gba")).unwrap(), b"abcdefgh");
    assert!(temps(&r).is_empty());
}

#[test]
fn a_cancelled_upload_keeps_nothing_and_another_browser_cannot_add_to_one() {
    let r = rig("pieces-cancel", false);
    let b = Browser::new(&r);
    let id = b.id("kind=rom&system=gba&name=Gone.gba", 8);
    assert_eq!(b.part(&id, 0, b"abcd").status, 200);
    let other = Browser::new(&r);
    assert_eq!(other.part(&id, 4, b"efgh").status, 404, "only the browser that started it adds to it");
    other.post(&format!("/api/upload/cancel?id={id}"), b"");
    assert_eq!(temps(&r).len(), 1, "nor cancels it");
    assert_eq!(r.req("POST", &format!("/api/upload/part?id={id}&offset=4"), &[("Cookie", &b.cookie)], b"efgh").status, 403, "a piece needs the CSRF token");
    assert_eq!(b.post(&format!("/api/upload/cancel?id={id}"), b"").status, 200);
    assert!(temps(&r).is_empty(), "cancelled: the temporary file goes");
    assert_eq!(b.part(&id, 4, b"efgh").status, 404);
    assert!(!r.root.join("roms/gba/Gone.gba").exists());
}

#[test]
fn pieces_keep_the_rules_of_a_whole_upload() {
    let r = rig("pieces-rules", false);
    let b = Browser::new(&r);
    // Overwrite only when asked.
    assert_eq!(b.start("kind=rom&system=gba&name=Advance%20Wars%20(USA).gba", 4).status, 409);
    let a = b.send("kind=rom&system=gba&name=Advance%20Wars%20(USA).gba&overwrite=1", b"new!", 3);
    assert_eq!(a.json()["replaced"], true, "{}", String::from_utf8_lossy(&a.body));
    assert_eq!(std::fs::read(r.root.join("roms/gba/Advance Wars (USA).gba")).unwrap(), b"new!");
    // Limits, places and names, before anything is made.
    assert_eq!(b.start("kind=bios&name=huge.bin", (files::MAX_BIOS + 1) as usize).status, 413);
    assert_eq!(b.start("kind=rom&system=gba&name=evil.sh", 4).status, 403);
    assert_eq!(b.start("kind=rom&system=ports&name=x.sh", 4).status, 403);
    assert_eq!(b.post("/api/upload/start?kind=rom&system=gba&name=x.gba", b"").status, 411, "the size comes with the start");
    assert!(temps(&r).is_empty());
    // A program is no port file, also when its first bytes come in two pieces.
    let id = b.id("kind=port&dir=sprout&rel=data.win&name=data.win", 8);
    assert_eq!(b.part(&id, 0, b"\x7fE").status, 200);
    assert_eq!(b.part(&id, 2, b"LF\x02\x01\x01\x00").status, 403);
    assert!(!r.root.join("roms/ports/sprout/assets/data.win").exists());
    assert!(temps(&r).is_empty());
    // A BIOS file says which system it is for; a game file is read into the library.
    assert_eq!(b.send("kind=bios&name=disksys.rom", b"bios", 3).json()["bios"], "Famicom Disk System");
    assert_eq!(b.send("kind=rom&system=gba&name=New.gba", b"data", 1).status, 200);
    let news = r.edits.lock().unwrap().iter().rev().find_map(|e| match e {
        super::Edit::LibraryChanged(super::Systems(s)) => Some(s.clone()),
        _ => None,
    });
    assert!(news.expect("the App gets the new library")[0].games.iter().any(|g| g.name == "New"));
    // An empty file is one empty piece.
    assert_eq!(b.send("kind=rom&system=gba&name=Empty.gba", b"", 8).status, 200);
    assert_eq!(std::fs::metadata(r.root.join("roms/gba/Empty.gba")).unwrap().len(), 0);
}

#[test]
fn a_piece_waits_while_a_game_runs_and_a_new_upload_does_not_start() {
    let r = rig("pieces-game", false);
    let b = Browser::new(&r);
    let id = b.id("kind=rom&system=gba&name=Wait.gba", 8);
    assert_eq!(b.part(&id, 0, b"abcd").status, 200);
    r.shared.game_running.store(true, std::sync::atomic::Ordering::Relaxed);
    let refused = b.start("kind=rom&system=gba&name=Later.gba", 4);
    assert_eq!((refused.status, refused.json()["playing"].as_bool()), (503, Some(true)), "nothing new starts while a game runs");
    let piece = std::thread::scope(|s| {
        let t = s.spawn(|| b.part(&id, 4, b"efgh"));
        std::thread::sleep(Duration::from_millis(700));
        assert!(!t.is_finished(), "the piece waits for the game to end");
        assert!(temps(&r).iter().all(|p| std::fs::metadata(p).unwrap().len() == 4), "nothing written while playing");
        r.shared.game_running.store(false, std::sync::atomic::Ordering::Relaxed);
        t.join().unwrap()
    });
    assert_eq!(piece.status, 200, "{}", String::from_utf8_lossy(&piece.body));
    assert_eq!(std::fs::read(r.root.join("roms/gba/Wait.gba")).unwrap(), b"abcdefgh");
}

#[test]
fn an_upload_left_half_sent_goes_after_its_idle_time_and_frees_its_room() {
    let r = rig_ctx("pieces-idle", false, |ctx| ctx.pieces = files::Pieces::with_idle(Duration::from_millis(200)));
    let b = Browser::new(&r);
    let id = b.id("kind=rom&system=gba&name=Left.gba", 8);
    assert_eq!(b.part(&id, 0, b"abcd").status, 200);
    std::thread::sleep(Duration::from_millis(400));
    // The next upload sweeps it.
    assert_eq!(b.send("kind=rom&system=gba&name=Other.gba", b"ok", 2).status, 200);
    assert!(temps(&r).is_empty(), "the one left behind is gone");
    assert_eq!(b.part(&id, 4, b"efgh").status, 404);
    // The same file started again replaces its own upload left behind at once (its room is free for it).
    let first = b.id("kind=rom&system=gba&name=Again.gba", 8);
    assert_eq!(b.part(&first, 0, b"abcd").status, 200);
    let again = b.send("kind=rom&system=gba&name=Again.gba", b"12345678", 8);
    assert_eq!(again.status, 200);
    assert_eq!(b.part(&first, 4, b"efgh").status, 404);
    assert!(temps(&r).is_empty());
}

#[test]
fn switching_the_companion_off_keeps_nothing_of_a_half_sent_file() {
    let mut r = rig("pieces-off", false);
    let b = Browser::new(&r);
    let id = b.id("kind=rom&system=gba&name=Off.gba", 8);
    let a = b.part(&id, 0, b"abcd");
    assert_eq!(a.status, 200);
    drop(b);
    r.server.take().unwrap().stop();
    let deadline = std::time::Instant::now() + crate::testutil::timeout(Duration::from_secs(5));
    while !temps(&r).is_empty() {
        assert!(std::time::Instant::now() < deadline, "the temporary file stays after the server stopped: {:?}", temps(&r));
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!r.root.join("roms/gba/Off.gba").exists());
}

/// A flow as the page runs it, with a 100 MB file in 8 MB pieces: every byte where it belongs.
#[test]
fn a_100_mb_file_in_8_mb_pieces_arrives_byte_for_byte() {
    let r = rig("pieces-100mb", false);
    let b = Browser::new(&r);
    let data: Vec<u8> = (0..100u32 << 20).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
    let a = b.send("kind=rom&system=gba&name=Hundred.gba", &data, files::PIECE as usize);
    assert_eq!(a.status, 200, "{}", String::from_utf8_lossy(&a.body));
    assert_eq!(a.json()["bytes"], data.len() as u64);
    let got = std::fs::read(r.root.join("roms/gba/Hundred.gba")).unwrap();
    assert!(got == data, "the file on the card is the one sent");
    assert!(temps(&r).is_empty());
}

// --- files::Partial on its own ------------------------------------------------------------------------------------

fn partial(dir: &Path, name: &str, total: u64) -> files::Partial {
    let room = files::reserve(&dir.join(name), total, u64::MAX).unwrap();
    files::Partial::begin(&dir.join(name), &dir.join(files::UPLOADS_DIR), total, false, false, room).unwrap()
}

#[test]
fn a_partial_dropped_before_its_end_removes_its_temporary_file() {
    let dir = crate::testutil::TestDir::new("web-partial-drop");
    let mut p = partial(&dir, "a.gba", 8);
    p.append(&mut &b"abcd"[..], 0, 4, &files::Gate::OPEN).unwrap();
    assert_eq!(walk(&dir.join(files::UPLOADS_DIR)).len(), 1);
    assert!(matches!(p.finish(|_| Ok(())), Err(Refused::Short)), "not whole: not put in place");
    drop(p);
    assert!(walk(&dir.join(files::UPLOADS_DIR)).is_empty());
    assert!(!dir.join("a.gba").exists());
}

#[test]
fn a_partial_whose_check_fails_at_the_end_keeps_nothing() {
    let dir = crate::testutil::TestDir::new("web-partial-check");
    let mut p = partial(&dir, "a.cue", 4);
    p.append(&mut &b"FILE"[..], 0, 4, &files::Gate::OPEN).unwrap();
    assert!(p.finish(|_| Err(Refused::NotAllowed("no".into()))).is_err());
    drop(p);
    assert!(!dir.join("a.cue").exists());
    assert!(walk(&dir.join(files::UPLOADS_DIR)).is_empty());
}

#[test]
fn a_partial_stops_when_the_server_is_switched_off() {
    let dir = crate::testutil::TestDir::new("web-partial-off");
    let mut p = partial(&dir, "a.gba", 8);
    let yes = || true;
    let gate = files::Gate { abort: &yes, ..files::Gate::OPEN };
    let e = p.append(&mut &b"abcd"[..], 0, 4, &gate).unwrap_err();
    assert!(!files::Partial::goes_on(&e), "switched off: over, {e:?}");
}

/// A full 8 MB piece sent again after its answer was lost: the Pocket refuses it (it has those bytes), and the browser
/// must read that answer, not a reset connection (it would send the piece again and again, then say "dropped").
#[test]
fn a_whole_piece_at_the_wrong_place_is_answered_not_reset() {
    let r = rig("pieces-reset", false);
    let b = Browser::new(&r);
    let piece = vec![5u8; files::PIECE as usize];
    let id = b.id("kind=rom&system=gba&name=Twice.gba", piece.len() * 2);
    assert_eq!(b.part(&id, 0, &piece).status, 200);
    for _ in 0..3 {
        let a = b.part(&id, 0, &piece);
        assert_eq!((a.status, a.json()["received"].as_u64()), (416, Some(files::PIECE)), "{}", a.head);
    }
    let gone = b.part("0000000000000000", 0, &piece);
    assert_eq!((gone.status, gone.json()["gone"].as_bool()), (404, Some(true)), "{}", gone.head);
    assert_eq!(b.part(&id, files::PIECE as usize, &piece).status, 200);
}
