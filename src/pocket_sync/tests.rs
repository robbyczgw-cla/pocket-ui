use super::*;
use crate::testutil::TestDir;

fn vv(s: &str) -> Vv {
    vv_parse(s).unwrap()
}

fn sha(c: char) -> String {
    std::iter::repeat_n(c, 64).collect()
}

fn e(rel: &str, crc: Option<u32>, sha: &str, v: &str) -> Entry {
    let (system, stem, ext) = split_rel(rel).unwrap();
    Entry { rel: rel.into(), system, stem, ext, crc, size: 8, mtime: 1, sha: sha.into(), vv: vv(v), asking: None, marks: vec![], open_copy: false }
}

fn one(local: &[Entry], remote: &[Entry]) -> Action {
    let s = plan(local, remote);
    assert_eq!(s.len(), 1, "{s:?}");
    s[0].action.clone()
}

// ---------------------------------------------------------------------------------------------------------------
// Vectors and decisions.

#[test]
fn vector_order_merge_and_text() {
    assert_eq!(order(&vv("A:2,B:1"), &vv("A:1,B:1")), Order::Newer);
    assert_eq!(order(&vv("A:1"), &vv("A:1,C:1")), Order::Older);
    assert_eq!(order(&vv("A:2"), &vv("A:1,C:1")), Order::Concurrent);
    assert_eq!(order(&vv("A:1"), &vv("A:1")), Order::Equal);
    assert_eq!(merge(&vv("A:2,B:1"), &vv("A:1,C:3")), vv("A:2,B:1,C:3"));
    assert_eq!(vv_parse(&vv_str(&vv("ABC:12,X9:1"))), Some(vv("ABC:12,X9:1")));
    for bad in ["../x:1", "A:", "A:-1", "A", ":3", "A:1,,B:2", "Ä:1"] {
        assert_eq!(vv_parse(bad), None, "{bad}");
    }
}

#[test]
fn same_bytes_do_nothing_but_merge_vectors() {
    let a = sha('a');
    assert_eq!(one(&[e("gba/G.srm", None, &a, "A:1")], &[e("gba/G.srm", None, &a, "A:1")]), Action::Same { merged: None });
    assert_eq!(
        one(&[e("gba/G.srm", None, &a, "A:1")], &[e("gba/G.srm", None, &a, "B:1")]),
        Action::Same { merged: Some(vv("A:1,B:1")) }
    );
}

#[test]
fn only_one_side_has_it_it_goes_over() {
    assert_eq!(one(&[e("gba/G.srm", None, &sha('a'), "A:1")], &[]), Action::Push { expect: None });
    assert_eq!(one(&[], &[e("gba/G.srm", None, &sha('a'), "A:1")]), Action::Pull);
}

#[test]
fn the_version_with_more_play_wins_whoever_asks() {
    let l = [e("gba/G.srm", None, &sha('n'), "A:2")];
    let r = [e("gba/G.srm", None, &sha('o'), "A:1")];
    assert_eq!(one(&l, &r), Action::Push { expect: Some(sha('o')) });
    assert_eq!(one(&r, &l), Action::Pull);
}

#[test]
fn three_pockets_a_save_never_walks_back() {
    // A played twice; C still has the first version (it came through B). Either way round, C takes A's.
    let a = e("gba/G.srm", None, &sha('2'), "A:2");
    let c = e("gba/G.srm", None, &sha('1'), "A:1");
    assert_eq!(decide(&c, &a), Action::Pull);
    assert_eq!(decide(&a, &c), Action::Push { expect: Some(sha('1')) });
    // C played on top of the first version: that contains A's first, so it goes to a Pocket still on the first.
    assert_eq!(decide(&e("gba/G.srm", None, &sha('c'), "A:1,C:1"), &c), Action::Push { expect: Some(sha('1')) });
}

#[test]
fn independent_play_is_a_conflict_and_only_that() {
    assert_eq!(one(&[e("gba/G.srm", None, &sha('m'), "A:2")], &[e("gba/G.srm", None, &sha('t'), "A:1,C:1")]), Action::Conflict);
    // First meeting, both already had a save: also a conflict (nobody can know which has more play).
    assert_eq!(one(&[e("gba/G.srm", None, &sha('x'), "A:1")], &[e("gba/G.srm", None, &sha('y'), "C:1")]), Action::Conflict);
}

#[test]
fn a_conflict_is_asked_once_on_every_path() {
    // C asks (copy here) about A's {A:3}; A's version is in that question, whose union is {A:3,C:1}.
    let mut c = e("gba/G.srm", None, &sha('c'), "A:2,C:1");
    c.asking = Some(vv("A:3"));
    let mut a = e("gba/G.srm", None, &sha('a'), "A:3");
    a.marks = vec![vv("A:3,C:1")];
    assert_eq!(decide(&c, &a), Action::Waiting { here: true });
    assert_eq!(decide(&a, &c), Action::Waiting { here: false });
    // B took A's version (with its mark) and meets C either way round; without the mark too.
    assert_eq!(decide(&a, &c), Action::Waiting { here: false });
    let plain = e("gba/G.srm", None, &sha('a'), "A:3");
    assert_eq!(decide(&plain, &c), Action::Waiting { here: false });
    assert_eq!(decide(&c, &plain), Action::Waiting { here: true });
    // An older version of A's side is simply older than C's.
    assert_eq!(decide(&c, &e("gba/G.srm", None, &sha('o'), "A:2")), Action::Push { expect: Some(sha('o')) });
}

#[test]
fn new_play_joins_the_open_question_instead_of_a_second_one() {
    // C asks about A's {A:3}. D played apart ({D:1}) and meets C: D's version joins C's question.
    let mut c = e("gba/G.srm", None, &sha('c'), "A:2,C:1");
    c.asking = Some(vv("A:3"));
    let d = e("gba/G.srm", None, &sha('d'), "D:1");
    assert_eq!(decide(&d, &c), Action::Offer);
    assert_eq!(decide(&c, &d), Action::Conflict, "met from C's side, C's question grows");
    let mut rec = Rec { sha: sha('c'), vv: vv("A:2,C:1"), asking: Some(vv("A:3")), marks: vec![], orphan: false };
    let whole = offered(&mut rec, &d);
    assert_eq!((rec.asking.clone(), whole), (Some(vv("A:3,D:1")), vv("A:3,C:1,D:1")));
    // A version in a question elsewhere meeting new play that question doesn't show is asked about, not waited for
    // (the question's Pocket may have lost its record and would never answer, 19.11.2026); a version that is in the
    // same question waits.
    let mut a = e("gba/G.srm", None, &sha('a'), "A:3");
    a.marks = vec![vv("A:3,C:1")];
    assert_eq!(decide(&a, &d), Action::Conflict);
    assert_eq!(decide(&d, &a), Action::Conflict);
    let mut c_marked = e("gba/G.srm", None, &sha('c'), "A:2,C:1");
    c_marked.marks = vec![vv("A:3,C:1")];
    assert_eq!(decide(&a, &c_marked), Action::Waiting { here: false });
}

#[test]
fn the_choice_goes_everywhere_without_a_new_question() {
    let asked = Rec { sha: sha('c'), vv: vv("A:2,C:1"), asking: Some(vv("A:3")), marks: vec![], orphan: false };
    // Kept its own: exactly the union.
    let mut r = asked.clone();
    chose(&mut r, &sha('c'), "C");
    assert_eq!((r.vv.clone(), r.asking.clone()), (vv("A:3,C:1"), None));
    let mut chosen = e("gba/G.srm", None, &sha('c'), "");
    chosen.vv = r.vv;
    let mut a = e("gba/G.srm", None, &sha('a'), "A:3");
    a.marks = vec![vv("A:3,C:1")];
    assert_eq!(decide(&chosen, &a), Action::Push { expect: Some(sha('a')) });
    assert_eq!(decide(&a, &chosen), Action::Pull);
    // The arrival ends A's mark: A's save is now the union itself, no longer inside it.
    let got = received(Some(&Rec { sha: sha('a'), vv: vv("A:3"), asking: None, marks: vec![vv("A:3,C:1")], orphan: false }), &chosen);
    assert!(got.marks.is_empty());
    // Took the other one: A's bytes and the union; meeting A's it is the same save, and the next game counts as play.
    let mut r = asked.clone();
    chose(&mut r, &sha('a'), "C");
    assert_eq!(r.vv, vv("A:3,C:1"));
    assert_eq!(bookkeep("C", Some(&r), &sha('n'), false).vv, vv("A:3,C:2"));
}

#[test]
fn the_same_question_answered_both_ways_settles_on_one_answer() {
    // Two Pockets asked about the same two saves before hearing of each other and kept different ones.
    let x = e("gba/G.srm", None, &sha('x'), "A:1,C:1");
    let y = e("gba/G.srm", None, &sha('y'), "A:1,C:1");
    assert_eq!(decide(&y, &x), Action::Push { expect: Some(sha('x')) });
    assert_eq!(decide(&x, &y), Action::Pull);
}

#[test]
fn bookkeeping_counts_real_changes_only() {
    let r = bookkeep("A", None, &sha('x'), false);
    assert_eq!(r.vv, vv("A:1"));
    assert_eq!(bookkeep("A", Some(&r), &sha('x'), false), r);
    assert_eq!(bookkeep("A", Some(&r), &sha('y'), false).vv, vv("A:2"));
    // A save that arrived is not play of this device.
    let got = received(Some(&r), &e("gba/G.srm", None, &sha('z'), "B:4"));
    assert_eq!(bookkeep("A", Some(&got), &sha('z'), false).vv, vv("B:4"));
}

#[test]
fn playing_on_after_a_question() {
    // In a question elsewhere, played here: new play, out of that question.
    let prev = Rec { sha: sha('a'), vv: vv("A:3"), asking: None, marks: vec![vv("A:3,C:1")], orphan: false };
    let r = bookkeep("A", Some(&prev), &sha('b'), false);
    assert_eq!((r.vv, r.marks), (vv("A:4"), vec![]));
    // Asked here, played on before choosing: the question stays, the choice will contain this play.
    let prev = Rec { sha: sha('c'), vv: vv("A:2,C:1"), asking: Some(vv("A:3")), marks: vec![], orphan: false };
    let r = bookkeep("C", Some(&prev), &sha('d'), true);
    assert_eq!((r.vv, r.asking), (vv("A:2,C:2"), Some(vv("A:3"))));
    // Copies removed by hand, without a choice: closed, and changed bytes count as play.
    let r = bookkeep("C", Some(&prev), &sha('d'), false);
    assert_eq!((r.vv, r.asking), (vv("A:3,C:2"), None));
}

#[test]
fn questions_close_when_answered_elsewhere_and_marks_spread_with_the_bytes() {
    let asking = Rec { sha: sha('c'), vv: vv("A:2,C:1"), asking: Some(vv("A:3")), marks: vec![], orphan: false };
    // An arrival that contains every version shown answers the question here.
    let r = received(Some(&asking), &e("gba/G.srm", None, &sha('n'), "A:3,C:1"));
    assert!(answered(Some(&asking), &r));
    // One that contains only ours keeps it open: the question is now between the arrival and the copies.
    let r = received(Some(&asking), &e("gba/G.srm", None, &sha('n'), "A:2,C:1,B:1"));
    assert_eq!((r.vv, r.asking), (vv("A:2,B:1,C:1"), Some(vv("A:3"))));
    // Same bytes as a Pocket where these bytes are in a question: this copy is in it too.
    let mut rec = Rec { sha: sha('a'), vv: vv("A:3"), asking: None, marks: vec![], orphan: false };
    let mut peer = e("gba/G.srm", None, &sha('a'), "A:3");
    peer.marks = vec![vv("A:3,C:1")];
    same(&mut rec, &peer);
    assert_eq!(rec.marks, vec![vv("A:3,C:1")]);
    // Same bytes as the answer: an open question here closes.
    let mut rec = asking.clone();
    same(&mut rec, &e("gba/G.srm", None, &sha('c'), "A:3,C:1"));
    assert!(answered(Some(&asking), &rec));
}

#[test]
fn a_question_grows_and_marks_are_only_taken_for_the_version_asked_about() {
    let asking = Rec { sha: sha('c'), vv: vv("A:2,C:1"), asking: Some(vv("A:3")), marks: vec![], orphan: false };
    let l = e("gba/G.srm", None, &sha('c'), "A:2,C:1");
    let (r, whole) = conflict_found(Some(&asking), &l, &e("gba/G.srm", None, &sha('d'), "A:2,D:1"));
    assert_eq!(r.asking, Some(vv("A:3,D:1")));
    // The peer's version is marked with the whole question: ours and both versions shown here.
    assert_eq!(whole, vv("A:3,C:1,D:1"));
    let mut plain = Rec { sha: sha('x'), vv: vv("A:1"), asking: None, marks: vec![], orphan: false };
    assert!(!mark(&mut plain, &sha('y'), &vv("A:1,C:1")), "a game wrote it since");
    assert!(mark(&mut plain, &sha('x'), &vv("A:1,C:1")));
    assert!(mark(&mut plain, &sha('x'), &vv("A:1,D:2")));
    assert_eq!(plain.marks, vec![vv("A:1,C:1"), vv("A:1,D:2")]);
    assert!(!mark(&mut asking.clone(), &sha('c'), &vv("A:3,C:1")), "a question open here is not marked from outside");
}

// ---------------------------------------------------------------------------------------------------------------
// Matching.

#[test]
fn another_file_name_same_game_matches_by_crc() {
    let l = [e("gba/Pokemon Emerald (USA).srm", Some(0x1f1c08fb), &sha('a'), "A:2")];
    let r = [e("gba/Emerald.srm", Some(0x1f1c08fb), &sha('b'), "A:1")];
    let s = plan(&l, &r);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].action, Action::Push { expect: Some(sha('b')) });
    assert_eq!(plan(&r, &l)[0].local_rel(None).as_deref(), Some("gba/Emerald.srm"));
}

#[test]
fn same_name_other_game_never_mixes() {
    let s = plan(&[e("gba/Game.srm", Some(1), &sha('a'), "A:1")], &[e("gba/Game.srm", Some(2), &sha('b'), "B:1")]);
    assert_eq!(s.len(), 2, "{s:?}");
    assert!(s.iter().all(|s| matches!(s.action, Action::Push { expect: None } | Action::Pull)));
}

#[test]
fn a_device_without_the_game_file_matches_by_name_any_case() {
    let s = plan(&[e("gba/Game.srm", Some(7), &sha('a'), "A:1")], &[e("gba/GAME.srm", None, &sha('a'), "A:1")]);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].action, Action::Same { merged: None });
}

#[test]
fn the_save_and_its_clock_are_separate_saves() {
    let s = plan(&[e("gb/Crystal.srm", Some(3), &sha('a'), "A:1"), e("gb/Crystal.rtc", Some(3), &sha('b'), "A:1")], &[]);
    assert_eq!(s.len(), 2);
}

#[test]
fn paths_from_another_device_are_checked() {
    for ok in ["gba/Game.srm", "psx/FF7/Disc.mcd", "gb/Crystal.RTC", "gba/Pokémon Émeraude.sav"] {
        assert!(safe_rel(ok).is_some(), "{ok}");
    }
    for bad in [
        "../etc/passwd.srm", "/etc/x.srm", "gba/../../x.srm", "gba/Game.gba", "Game.srm", "gba/.hidden.srm", "a/b/c/d.srm",
        ".git/x.srm", "gba\\x.srm", "gba/G.sync-conflict-20261117-231502-ABCDEF7.srm", "gba/.srm",
    ] {
        assert!(safe_rel(bad).is_none(), "{bad}");
    }
}

#[test]
fn the_index_entry_round_trips_and_refuses_junk() {
    let mut x = e("gba/Pokémon (USA).srm", Some(0xdeadbeef), &sha('a'), "A:3,B12:1");
    x.asking = Some(vv("C:2"));
    x.marks = vec![vv("A:3,B12:1,D:1")];
    let back = Entry::from_json(&x.to_json()).unwrap();
    assert_eq!(back, x);
    let mut v = x.to_json();
    v["rel"] = "../../x.srm".into();
    assert!(Entry::from_json(&v).is_none());
    let mut v = x.to_json();
    v["sha"] = "zz".into();
    assert!(Entry::from_json(&v).is_none());
    let mut v = x.to_json();
    v["vv"] = "A:1,../:2".into();
    assert!(Entry::from_json(&v).is_none());
    let mut v = x.to_json();
    v["marks"] = serde_json::Value::Array((0..=MAX_MARKS).map(|i| format!("A:{i}").into()).collect());
    assert!(Entry::from_json(&v).is_none(), "too many marks");
    let mut v = x.to_json();
    v["crc"] = (u64::from(u32::MAX) + 1).into();
    assert_eq!(Entry::from_json(&v).unwrap().crc, None);
}

#[test]
fn the_save_endings_are_the_syncthing_saves_folders() {
    let ours: BTreeSet<&str> = SAVE_EXTS.iter().copied().collect();
    let syncthing: BTreeSet<&str> = crate::sync::save_extensions().collect();
    assert_eq!(ours, syncthing);
}

#[test]
fn the_conflict_copy_is_named_so_the_two_versions_page_reads_it() {
    // 2026-11-17 23:15:02 UTC.
    let n = conflict_name("gba/Super Mario Bros. 3.srm", 1_794_957_302, "ab12cd34ef");
    assert_eq!(n, "gba/Super Mario Bros. 3.sync-conflict-20261117-231502-AB12CD3.srm");
    let file = n.rsplit_once('/').unwrap().1;
    assert_eq!(crate::sync_conflicts::original_name(file).as_deref(), Some("Super Mario Bros. 3.srm"));
    assert_eq!(conflict_name("gb/Crystal.rtc", 0, "Z"), "gb/Crystal.sync-conflict-19700101-000000-Z.rtc");
}

// ---------------------------------------------------------------------------------------------------------------
// The card.

fn write(p: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

#[test]
fn scan_finds_the_saves_beside_the_games_and_nothing_else() {
    let d = TestDir::new("psync-scan");
    let roms = d.join("roms");
    write(&roms.join("gba/Pokemon Emerald (USA).gba"), b"emerald rom");
    write(&roms.join("gba/Pokemon Emerald (USA).srm"), b"save 1");
    write(&roms.join("gba/Pokemon Emerald (USA).png"), b"not the game");
    write(&roms.join("gb/Crystal.gbc"), b"crystal");
    write(&roms.join("gb/Crystal.srm"), b"c save");
    write(&roms.join("gb/Crystal.rtc"), b"clock");
    write(&roms.join("gb/Crystal.sync-conflict-20261117-231502-ABCDEF7.srm"), b"other");
    write(&roms.join("nes/Orphan.srm"), b"no game here");
    write(&roms.join("psx/FF7/FF7.mcd"), b"card");
    write(&roms.join("psx/FF7/deeper/x.srm"), b"too deep");
    write(&roms.join("savestates/gba/x.srm"), b"skip");
    write(&roms.join("sync-backups/2026-11-17/gba/x.srm"), b"skip");
    write(&roms.join("bios/x.srm"), b"skip");
    write(&roms.join("3ds/azahar/x.sav"), b"skip");
    write(&roms.join(".hidden/x.srm"), b"skip");
    write(&roms.join("gba/.x.srm"), b"skip");
    write(&roms.join("gba/Big.srm"), &vec![0u8; (MAX_SAVE + 1) as usize]);
    let mut crcs = CrcCache::default();
    let s = scan(&Layout::beside(roms.clone()), &mut crcs);
    let rels: Vec<&str> = s.iter().map(|e| e.rel.as_str()).collect();
    assert_eq!(rels, ["gb/Crystal.rtc", "gb/Crystal.srm", "gba/Pokemon Emerald (USA).srm", "nes/Orphan.srm", "psx/FF7/FF7.mcd"]);
    let em = &s[2];
    assert_eq!(em.crc, Some(crc32fast::hash(b"emerald rom")));
    assert_eq!((em.system.as_str(), em.stem.as_str(), em.ext.as_str()), ("gba", "Pokemon Emerald (USA)", "srm"));
    assert_eq!(em.sha, sha256_hex(b"save 1"));
    assert!(s[1].open_copy, "Crystal.srm has a copy beside it");
    assert!(!s[0].open_copy, "the clock has none");
    assert_eq!(s[0].crc, s[1].crc, "the clock belongs to the same game");
    assert_eq!(s[3].crc, None);
}

#[test]
fn the_index_keeps_vectors_across_runs_and_the_record_survives_a_restart() {
    let d = TestDir::new("psync-index");
    let layout = Layout::beside(d.join("roms"));
    write(&layout.roms.join("gba/G.gba"), b"rom");
    write(&layout.roms.join("gba/G.srm"), b"v1");
    let path = d.join("data/pocket-sync/versions.json");
    let (mut v, origin) = Versions::load(&path).unwrap();
    assert_eq!(origin, Origin::Started);
    let mut c = CrcCache::default();
    assert_eq!(index("A", &layout, &mut v, &mut c)[0].vv, vv("A:1"));
    v.save(&path).unwrap();
    let (mut v, origin) = Versions::load(&path).unwrap();
    assert_eq!(origin, Origin::Read);
    assert_eq!(index("A", &layout, &mut v, &mut c)[0].vv, vv("A:1"), "unchanged: no new change counted");
    write(&layout.roms.join("gba/G.srm"), b"v2");
    assert_eq!(index("A", &layout, &mut v, &mut c)[0].vv, vv("A:2"));
    v.save(&path).unwrap();
    // Damaged: its copy comes back; damaged and no copy: a new history under a new id; newer: refused.
    write(&path, b"{ not json");
    assert_eq!(Versions::load(&path).unwrap(), (v.clone(), Origin::Restored));
    std::fs::remove_file(crate::fsutil::backup_path(&path)).unwrap();
    assert_eq!(Versions::load(&path).unwrap().1, Origin::Started);
    let fresh = Versions::open(&path, "A", "X7").unwrap();
    assert_eq!((fresh.writer("A"), fresh.0.len()), ("AX7", 0));
    write(&path, br#"{"format": 99, "saves": {}}"#);
    assert_eq!(Versions::load(&path), Err(RecordError::Newer));
    // A card that does not give the file (here: a folder in its place) is no first start: no sync this time.
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir_all(&path).unwrap();
    assert_eq!(Versions::load(&path), Err(RecordError::Unreadable));
    std::fs::remove_dir(&path).unwrap();
    // A record for a path that isn't a save path is dropped; the id it counts under is kept.
    write(&path, br#"{"format": 1, "writer": "AQ3", "saves": {"../x.srm": {"sha": "a", "vv": "A:1"}, "gba/G.srm": {"sha": "b", "vv": "A:7"}}}"#);
    let (v, _) = Versions::load(&path).unwrap();
    assert_eq!(v.0.keys().collect::<Vec<_>>(), ["gba/G.srm"]);
    assert_eq!(v.writer("A"), "AQ3");
}

#[test]
fn a_new_save_lands_under_our_name_for_the_game() {
    let d = TestDir::new("psync-target");
    let layout = Layout::beside(d.join("roms"));
    write(&layout.roms.join("gba/Pokemon Emerald (USA).gba"), b"emerald");
    write(&layout.roms.join("gba/Other.gba"), b"other");
    let mut c = CrcCache::default();
    let crc = crc32fast::hash(b"emerald");
    assert_eq!(target(&layout, &mut c, "gba", "srm", crc).as_deref(), Some("gba/Pokemon Emerald (USA).srm"));
    assert_eq!(target(&layout, &mut c, "gba", "srm", 1), None);
    assert_eq!(target(&layout, &mut c, "..", "srm", crc), None);
    assert_eq!(target(&layout, &mut c, "gba", "exe", crc), None);
}

#[test]
fn the_crc_cache_reads_a_game_once_and_forgets_changed_ones() {
    let d = TestDir::new("psync-crc");
    let g = d.join("g.gba");
    write(&g, b"one");
    let mut c = CrcCache::default();
    assert_eq!(c.crc(&g), Some(crc32fast::hash(b"one")));
    let p = d.join("crc.json");
    c.save(&p).unwrap();
    let mut c = CrcCache::load(&p);
    assert_eq!(c.0.len(), 1);
    std::fs::remove_file(&g).unwrap();
    c.prune();
    assert!(c.0.is_empty());
}

// ---------------------------------------------------------------------------------------------------------------
// Four simulated Pockets, random play and random syncs between any two (`sim`).

mod sim;

// The file-fault rule (tests/data_sources.rs, scripts/reader-inventory.py): every file this module reads, missing,
// empty, cut, flipped or in another encoding, never panics and never stops the saves from being found.
#[test]
fn file_source_rule_sync_record_crc_cache_and_saves() {
    let d = TestDir::new("psync-faults");
    let layout = Layout::beside(d.join("roms"));
    let (rom, save) = (layout.roms.join("gba/G.gba"), layout.roms.join("gba/G.srm"));
    write(&rom, b"the game");
    write(&save, b"the save");
    let (rec_path, crc_path) = (d.join("data/versions.json"), d.join("data/crc-cache.json"));
    let (mut v, mut c) = (Versions::default(), CrcCache::default());
    index("A", &layout, &mut v, &mut c);
    v.save(&rec_path).unwrap();
    c.save(&crc_path).unwrap();
    let (rec_seed, crc_seed) = (std::fs::read(&rec_path).unwrap(), std::fs::read(&crc_path).unwrap());
    let put = |p: &Path, bytes: &Option<Vec<u8>>| {
        let _ = std::fs::remove_file(p);
        if let Some(b) = bytes {
            write(p, b);
        }
    };
    for (label, bytes) in crate::file_faults::faults(&rec_seed) {
        put(&rec_path, &bytes);
        let loaded = Versions::load(&rec_path).map(|(v, _)| v);
        // The copy written with it is whole: whatever happened to the record, it comes back as it was.
        assert_eq!(loaded.as_ref(), Ok(&v), "{label}");
        let mut got = loaded.unwrap_or_default();
        let e = index("A", &layout, &mut got, &mut CrcCache::default());
        assert_eq!(e.len(), 1, "{label}: the save is still found");
        assert!(!e[0].vv.is_empty(), "{label}");
    }
    // And with the copy damaged too: a new history, never an error that stops the saves from being found.
    for (label, bytes) in crate::file_faults::faults(&rec_seed) {
        put(&rec_path, &bytes);
        put(&crate::fsutil::backup_path(&rec_path), &bytes);
        let mut got = Versions::open(&rec_path, "A", "Z1").unwrap();
        assert_eq!(index("A", &layout, &mut got, &mut CrcCache::default()).len(), 1, "{label}");
    }
    for (label, bytes) in crate::file_faults::faults(&crc_seed) {
        put(&crc_path, &bytes);
        let mut cache = CrcCache::load(&crc_path);
        let crc = cache.crc(&rom);
        if label == "valid" {
            assert_eq!(crc, Some(crc32fast::hash(b"the game")));
        }
        assert_eq!(scan(&layout, &mut cache).len(), 1, "{label}");
    }
    for (label, bytes) in crate::file_faults::faults(b"the save") {
        put(&save, &bytes);
        let e = scan(&layout, &mut CrcCache::default());
        match &bytes {
            None => assert!(e.is_empty(), "{label}"),
            Some(b) => assert_eq!(e[0].sha, sha256_hex(b), "{label}"),
        }
    }
    write(&save, b"the save");
    for (label, bytes) in crate::file_faults::faults(b"the game") {
        put(&rom, &bytes);
        let e = scan(&layout, &mut CrcCache::default());
        assert_eq!(e[0].crc, bytes.as_ref().map(|b| crc32fast::hash(b)), "{label}");
    }
    // The same faults where the saves are elsewhere (0.6.0): in a folder of their own, and in Android's player by key.
    let own = Layout::saves_root(d.join("games"), d.join("saves"));
    let android = Layout::android(vec![d.join("ROMs")], d.join("files/saves"), d.join("files/sync-backups"), std::sync::Arc::new(test_key));
    for (layout, rom) in [(own, d.join("games/gba/G.gba")), (android, d.join("ROMs/gba/G.gba"))] {
        write(&rom, b"the game");
        let save = layout.file("gba/G.srm").unwrap();
        for (label, bytes) in crate::file_faults::faults(b"the save") {
            put(&save, &bytes);
            let e = scan(&layout, &mut CrcCache::default());
            match &bytes {
                None => assert!(e.is_empty(), "{label} {layout:?}"),
                Some(b) => assert_eq!((e[0].sha.as_str(), e[0].crc), (sha256_hex(b).as_str(), Some(crc32fast::hash(b"the game"))), "{label} {layout:?}"),
            }
        }
        write(&save, b"the save");
        for (label, bytes) in crate::file_faults::faults(b"the game") {
            put(&rom, &bytes);
            let e = scan(&layout, &mut CrcCache::default());
            match (&bytes, layout.keyed().is_some()) {
                // Android: no game, no place; the save is not this device's to give.
                (None, true) => assert!(e.is_empty(), "{label}"),
                _ => assert_eq!(e[0].crc, bytes.as_ref().map(|b| crc32fast::hash(b)), "{label} {layout:?}"),
            }
        }
    }
}

// Sol review 19.11.2026 #2: the name fallback dropped the subfolder, so gba/A/Game.srm here was paired with
// gba/B/Game.srm there although gba/B/Game.srm existed here too.
#[test]
fn the_same_path_pairs_first_and_an_ambiguous_name_never_pairs() {
    let local = [e("gba/A/Game.srm", None, &sha('a'), "A:1"), e("gba/B/Game.srm", None, &sha('b'), "A:1")];
    let remote = [e("gba/B/Game.srm", None, &sha('c'), "B:1")];
    let pairs = pair(&local, &remote);
    let b = pairs.iter().find(|(l, _)| l.as_ref().is_some_and(|l| l.rel == "gba/B/Game.srm")).unwrap();
    assert_eq!(b.1.as_ref().map(|r| r.rel.as_str()), Some("gba/B/Game.srm"), "{pairs:?}");
    let a = pairs.iter().find(|(l, _)| l.as_ref().is_some_and(|l| l.rel == "gba/A/Game.srm")).unwrap();
    assert!(a.1.is_none(), "{pairs:?}");
    // One name here, two there in different folders: nobody can know which; nothing pairs.
    let pairs = pair(&[e("gba/A/Game.srm", None, &sha('a'), "A:1")], &[e("gba/B/Game.srm", None, &sha('b'), "B:1"), e("gba/C/Game.srm", None, &sha('c'), "C:1")]);
    assert!(pairs.iter().all(|(l, r)| l.is_none() || r.is_none()), "{pairs:?}");
    // A unique name in another folder still pairs (a library sorted differently on the other Pocket).
    let pairs = pair(&[e("gba/RPG/Game.srm", None, &sha('a'), "A:1")], &[e("gba/Game.srm", None, &sha('a'), "A:1")]);
    assert_eq!(pairs.len(), 1);
}

#[test]
fn a_new_save_finds_its_game_in_a_subfolder() {
    let d = TestDir::new("psync-target-sub");
    let layout = Layout::beside(d.join("roms"));
    write(&layout.roms.join("gba/RPG/Pokemon Emerald (USA).gba"), b"emerald");
    let mut c = CrcCache::default();
    assert_eq!(target(&layout, &mut c, "gba", "srm", crc32fast::hash(b"emerald")).as_deref(), Some("gba/RPG/Pokemon Emerald (USA).srm"));
}

// Sol review 19.11.2026 #3: an unreadable record started this device's count at 1 again, so a peer's older save with
// a higher count of this device won over the newer one here.
#[test]
fn an_unreadable_record_never_makes_an_older_save_win() {
    let d = TestDir::new("psync-lost");
    let layout = Layout::beside(d.join("roms"));
    write(&layout.roms.join("gba/G.srm"), b"newest play here");
    let path = d.join("data/versions.json");
    write(&path, b"{ damaged");
    let mut v = Versions::open(&path, "A", "X1").unwrap();
    let here = index("A", &layout, &mut v, &mut CrcCache::default()).remove(0);
    // The peer still has an older save of this device's with a higher count.
    let peer = e("gba/G.srm", None, &sha('o'), "A:6");
    assert!(!matches!(decide(&here, &peer), Action::Pull), "the older save must not win: {:?}", decide(&here, &peer));
}

#[test]
fn a_name_in_other_letters_collides() {
    let d = TestDir::new("psync-collide");
    let layout = Layout::beside(d.join("roms"));
    write(&layout.roms.join("nes/Mario.SRM"), b"x");
    assert!(collides(&layout, "nes/Mario.srm"));
    assert!(!collides(&layout, "nes/Mario.SRM"), "the same file is no collision");
    assert!(!collides(&layout, "nes/Luigi.srm"));
}

/// Random libraries on two Pockets (folders, names, CRCs known or not): the pairing rules hold for every one.
#[test]
fn random_libraries_pair_by_path_then_unique_crc_then_unique_name() {
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let folders = ["", "A/", "B/"];
    let names = ["Game", "Other", "GAME"];
    for _ in 0..4000 {
        let mut side = |next: &mut dyn FnMut(u64) -> u64| {
            let mut v: Vec<Entry> = Vec::new();
            for _ in 0..next(6) {
                let rel = format!("gba/{}{}.srm", folders[next(3) as usize], names[next(3) as usize]);
                if v.iter().any(|x| x.rel == rel) {
                    continue;
                }
                let crc = match next(4) { 0 => None, n => Some(n as u32) };
                v.push(e(&rel, crc, &sha('a'), "A:1"));
            }
            v
        };
        let (local, remote) = (side(&mut next), side(&mut next));
        let pairs = pair(&local, &remote);
        // Every save exactly once.
        let ls: Vec<&str> = pairs.iter().filter_map(|(l, _)| l.as_ref().map(|x| x.rel.as_str())).collect();
        let rs: Vec<&str> = pairs.iter().filter_map(|(_, r)| r.as_ref().map(|x| x.rel.as_str())).collect();
        assert_eq!((ls.len(), rs.len()), (local.len(), remote.len()), "{pairs:?}");
        for (l, r) in pairs.iter().filter_map(|(l, r)| Some((l.as_ref()?, r.as_ref()?))) {
            // Never two games that both know different CRCs.
            assert!(!(l.crc.is_some() && r.crc.is_some() && l.crc != r.crc), "{l:?} {r:?}");
            // A save whose path exists on the other side (and may pair) is paired with that one.
            if let Some(same) = remote.iter().find(|x| x.rel == l.rel) {
                if !(l.crc.is_some() && same.crc.is_some() && l.crc != same.crc) {
                    assert_eq!(r.rel, l.rel, "{pairs:?}");
                }
            }
            if l.rel != r.rel {
                // Paired across folders: by a CRC or a name that was one save on each side of the unpaired ones.
                let by_crc = l.crc.is_some() && l.crc == r.crc;
                let by_name = l.name_key() == r.name_key();
                assert!(by_crc || by_name, "{l:?} {r:?}");
            }
        }
    }
}

// The random run with lost records (19.11.2026): a Pocket lost its record while its question was open; the save the
// player then kept had the same vector as the one it replaced, and the tie undid the choice.
#[test]
fn a_choice_without_a_recorded_question_counts_as_a_change() {
    let mut r = Rec { sha: sha('a'), vv: vv("P0:2,P0L1:1"), asking: None, marks: vec![], orphan: false };
    chose(&mut r, &sha('b'), "P0L1");
    assert_eq!(r.vv, vv("P0:2,P0L1:2"));
    let kept = e("gba/G.srm", None, &sha('b'), "P0:2,P0L1:2");
    assert_eq!(decide(&kept, &e("gba/G.srm", None, &sha('a'), "P0:2,P0L1:1")), Action::Push { expect: Some(sha('a')) });
    // Keeping what was there is a decision too: news, so another Pocket's later choice can't quietly undo it.
    let mut same = Rec { sha: sha('a'), vv: vv("P0:2"), asking: None, marks: vec![], orphan: false };
    chose(&mut same, &sha('a'), "P0L1");
    assert_eq!(same.vv, vv("P0:2,P0L1:1"));
}

// The random run with two lost records (19.11.2026): copies left beside a save by a question a lost record no longer
// shows were chosen over; with the record's own question merged in, the choice tied with another Pocket's and the
// tie undid it. Such copies make the choice news.
#[test]
fn copies_no_record_knows_make_the_choice_news() {
    let r = bookkeep("P0L1", None, &sha('a'), true);
    assert!(r.orphan, "copies beside a save the record does not know");
    let mut asked = Rec { asking: Some(vv("P1:3")), ..r };
    chose(&mut asked, &sha('b'), "P0L1");
    assert_eq!((asked.vv, asked.orphan), (vv("P0L1:2,P1:3"), false));
    // A question this record knows, no copies besides: the union, no news.
    let mut known = Rec { sha: sha('a'), vv: vv("P0:1"), asking: Some(vv("P1:3")), marks: vec![], orphan: false };
    chose(&mut known, &sha('b'), "P0");
    assert_eq!(known.vv, vv("P0:1,P1:3"));
}

// ---------------------------------------------------------------------------------------------------------------
// Where the saves are: beside the games, in a folder of their own, Android's player by key (0.6.0).

fn test_key(p: &Path, system: &str) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (p, system).hash(&mut h);
    Some(format!("{:016x}", h.finish()))
}

/// Android's handheld view: the saves are found in `saves/<key>/` of the games of every ROMs folder, under the path they
/// would have beside their game; a save there that no game of the ROMs folders has, a copy, another game's name in the
/// folder, a deeper folder are not in the index; `file`, `dir_of` and `rel_of` agree, and a peer's save finds its game by
/// CRC in a folder of other letters.
#[test]
fn android_saves_are_found_by_the_game_s_key_in_every_roms_folder() {
    let d = TestDir::new("psync-android-layout");
    let (internal, card, saves) = (d.join("emulated/ROMs"), d.join("card/ROMs"), d.join("files/saves"));
    let layout = Layout::android(vec![internal.clone(), card.clone()], saves.clone(), d.join("files/sync-backups"), std::sync::Arc::new(test_key));
    write(&internal.join("GBA/Emerald.gba"), b"emerald rom");
    write(&internal.join("gb/RPG/Crystal.gbc"), b"crystal rom");
    write(&card.join("snes/Zelda.sfc"), b"zelda rom");
    write(&card.join("GBA/Emerald.gba"), b"another emerald");
    write(&internal.join("psx/A/B/Deep.bin"), b"deep");
    let key = |root: &Path, rel: &str, system: &str| saves.join(test_key(&root.join(rel), system).unwrap());
    write(&key(&internal, "GBA/Emerald.gba", "gba").join("Emerald.srm"), b"emerald save");
    write(&key(&internal, "GBA/Emerald.gba", "gba").join("Emerald.rtc"), b"clock");
    write(&key(&internal, "GBA/Emerald.gba", "gba").join("Emerald.sync-conflict-20261119-101010-ABCDEF7.srm"), b"other");
    write(&key(&internal, "GBA/Emerald.gba", "gba").join("Other.srm"), b"not this game's");
    write(&key(&internal, "gb/RPG/Crystal.gbc", "gb").join("Crystal.srm"), b"crystal save");
    write(&key(&card, "snes/Zelda.sfc", "snes").join("Zelda.srm"), b"zelda save");
    write(&key(&card, "GBA/Emerald.gba", "gba").join("Emerald.srm"), b"the card's emerald: the internal one has the path");
    write(&key(&internal, "psx/A/B/Deep.bin", "psx").join("Deep.mcd"), b"too deep");
    write(&saves.join("0000000000000000/Gone.srm"), b"no game");
    let mut crcs = CrcCache::default();
    let s = scan(&layout, &mut crcs);
    let rels: Vec<&str> = s.iter().map(|e| e.rel.as_str()).collect();
    assert_eq!(rels, ["GBA/Emerald.rtc", "GBA/Emerald.srm", "gb/RPG/Crystal.srm", "snes/Zelda.srm"]);
    assert_eq!(s[1].sha, sha256_hex(b"emerald save"));
    assert_eq!(s[1].crc, Some(crc32fast::hash(b"emerald rom")));
    assert_eq!(s[1].system, "GBA");
    assert!(s[1].open_copy && !s[0].open_copy);
    assert_eq!(s[3].crc, Some(crc32fast::hash(b"zelda rom")), "a card's game");
    for e in &s {
        let file = layout.file(&e.rel).unwrap();
        assert_eq!(std::fs::read(&file).map(|b| sha256_hex(&b)).ok().as_deref(), Some(e.sha.as_str()), "{}", e.rel);
        assert_eq!(layout.rel_of(&file).as_deref(), Some(e.rel.as_str()), "{}", e.rel);
        assert_eq!(file.parent(), layout.dir_of(&e.rel).as_deref());
    }
    assert_eq!(layout.file("nes/Mario.srm"), None, "no game, no place");
    assert_eq!(layout.file("GBA/Emerald.gba"), None, "not a save");
    assert_eq!(layout.rel_of(&saves.join("0000000000000000/Gone.srm")), None);
    // A peer's Emerald (folder `gba`) goes to this Pocket's `GBA` game by its CRC; Zelda's new clock file to the card's game.
    assert_eq!(target(&layout, &mut crcs, "gba", "srm", crc32fast::hash(b"emerald rom")).as_deref(), Some("GBA/Emerald.srm"));
    assert_eq!(target(&layout, &mut crcs, "snes", "rtc", crc32fast::hash(b"zelda rom")).as_deref(), Some("snes/Zelda.rtc"));
    assert_eq!(target(&layout, &mut crcs, "nes", "srm", 7), None);
    assert!(!collides(&layout, "snes/Zelda.rtc"));
}

/// Saves in a folder of their own with the games' layout (the Brick's and dArkOS's cores): found there, the CRC from the
/// game at the same place in the games' folder, written there, a new save placed by CRC.
#[test]
fn saves_in_a_folder_of_their_own_take_the_crc_from_the_games_folder() {
    let d = TestDir::new("psync-saves-root");
    let (roms, saves) = (d.join("Roms"), d.join("Saves"));
    let layout = Layout::saves_root(roms.clone(), saves.clone());
    write(&roms.join("gba/Emerald.gba"), b"emerald rom");
    write(&roms.join("gb/Tetris.gb"), b"tetris rom");
    write(&saves.join("gba/Emerald.srm"), b"emerald save");
    write(&saves.join("nes/Orphan.srm"), b"no game");
    write(&saves.join("sync-backups/2026-11-19/gba/Emerald.srm"), b"skip");
    let mut crcs = CrcCache::default();
    let s = scan(&layout, &mut crcs);
    let rels: Vec<&str> = s.iter().map(|e| e.rel.as_str()).collect();
    assert_eq!(rels, ["gba/Emerald.srm", "nes/Orphan.srm"]);
    assert_eq!(s[0].crc, Some(crc32fast::hash(b"emerald rom")));
    assert_eq!(s[1].crc, None);
    assert_eq!(layout.file("gba/Emerald.srm"), Some(saves.join("gba/Emerald.srm")));
    assert_eq!(layout.rel_of(&saves.join("gba/Emerald.srm")).as_deref(), Some("gba/Emerald.srm"));
    assert_eq!(target(&layout, &mut crcs, "gb", "srm", crc32fast::hash(b"tetris rom")).as_deref(), Some("gb/Tetris.srm"));
    assert_eq!(layout.backups(), saves.join("sync-backups"));
    // ROCKNIX stays as it was: beside the games, the backups in the roms folder.
    let r = Layout::beside(roms.clone());
    assert_eq!((r.file("gba/Emerald.srm"), r.backups()), (Some(roms.join("gba/Emerald.srm")), roms.join("sync-backups")));
    assert_eq!(r.rel_of(&roms.join("gba/Emerald.srm")).as_deref(), Some("gba/Emerald.srm"));
    assert_eq!(r.rel_of(&roms.join("gba/Emerald.gba")), None);
}

/// Android's ROMs folders are often `GBA` where ROCKNIX has `gba` (found by the Android sync test, 19.11.2026): the same
/// game pairs by CRC and by name across the letters, and a save new here lands in this device's own folder, never in a
/// second folder in the peer's letters.
#[test]
fn a_system_folder_in_other_letters_is_the_same_system() {
    let local = [e("gba/Emerald.srm", Some(7), &sha('a'), "A:1")];
    let remote = [e("GBA/Pokemon Emerald.srm", Some(7), &sha('a'), "B:1")];
    let pairs = pair(&local, &remote);
    assert_eq!(pairs.len(), 1, "{pairs:?}");
    let by_name = pair(&[e("gba/Tetris.srm", None, &sha('a'), "A:1")], &[e("GBA/tetris.srm", None, &sha('b'), "B:1")]);
    assert_eq!(by_name.len(), 1, "{by_name:?}");
    let d = TestDir::new("psync-letters");
    let layout = Layout::beside(d.join("roms"));
    write(&layout.roms.join("gba/Pokemon Emerald (USA).gba"), b"emerald rom");
    let mut crcs = CrcCache::default();
    assert_eq!(target(&layout, &mut crcs, "GBA", "srm", crc32fast::hash(b"emerald rom")).as_deref(), Some("gba/Pokemon Emerald (USA).srm"));
}
