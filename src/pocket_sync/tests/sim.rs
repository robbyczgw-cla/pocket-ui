//! Four Pockets, one game, thousands of random runs: play, sync any two (either one starting), choose in a question,
//! then a quiet end where everybody meets everybody. What a player would notice is checked at every step:
//! - **nothing lost without a choice**: a save is only replaced by one that contains all its play, or by the
//!   player's choice in a question that showed it;
//! - **no false question**: a conflict only between two versions that were really played independently (or two
//!   choices that kept different saves after seeing the same versions);
//! - **asked again rarely**: a pair of versions asked about a second time only on a Pocket that could not have heard
//!   of the first question, or after the player answered it both ways; counted, and must stay rare;
//! - **it settles**: after the last play, every Pocket ends with the same save and no question open.
//!
//! The devices use exactly the functions the transport and the page will: `plan`/`decide` for the step, `bookkeep`
//! after every game and before a choice, `chose` for a choice, `received` for an arrival, `same` for equal saves,
//! `conflict_found` + `mark` for a question, `offered` + `mark` for an offer, `answered` to clear the copies.

use super::super::*;

/// A tiny, fixed-seed random source (no crate needed; each seed is one reproducible run).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// A save's bytes, as the simulation knows them: a unique id (its hash is derived from it) and every play event it
/// contains, or that the player decided against in a question that showed it (`covers`).
#[derive(Clone, Debug)]
struct Ver {
    id: u64,
    covers: BTreeSet<u64>,
    /// Kept by the player in a question (and not played on since).
    chosen: bool,
    /// Saved on a Pocket after its record was lost (or a version that came from one): a question about it may be
    /// one the lost record would have avoided (asked rather than guessed: allowed, counted).
    lost: bool,
}

impl Ver {
    fn sha(&self) -> String {
        format!("{:064x}", self.id)
    }
}

struct Dev {
    /// The id its changes are counted under: a new one after its record was lost (`Versions::open`).
    id: String,
    losses: u32,
    file: Option<Ver>,
    rec: Option<Rec>,
    /// `.sync-conflict-` copies beside the save.
    copies: Vec<Ver>,
}

struct World {
    devs: Vec<Dev>,
    next: u64,
    asked: BTreeSet<(u64, u64)>,
    conflicts: usize,
    /// Pairs asked again because the player chose differently on two Pockets (allowed, counted).
    contradictions: usize,
    /// Pairs asked again on a Pocket that could not know: it got both versions from Pockets that had not heard of
    /// the question yet (allowed, counted, must stay rare).
    unheard: usize,
    /// Questions about a version saved after a lost record (allowed, counted).
    after_loss: usize,
    log: Vec<String>,
}

impl World {
    fn new(n: usize) -> World {
        let devs = (0..n).map(|i| Dev { id: format!("P{i}"), losses: 0, file: None, rec: None, copies: Vec::new() }).collect();
        World { devs, next: 1, asked: BTreeSet::new(), conflicts: 0, contradictions: 0, unheard: 0, after_loss: 0, log: Vec::new() }
    }

    fn fresh(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    /// A game saves on device `d`.
    fn play(&mut self, d: usize) {
        let id = self.fresh();
        let mut covers = self.devs[d].file.as_ref().map(|f| f.covers.clone()).unwrap_or_default();
        covers.insert(id);
        let lost = self.devs[d].file.as_ref().is_some_and(|f| f.lost);
        self.devs[d].file = Some(Ver { id, covers, chosen: false, lost });
        self.log.push(format!("play {d} -> v{id}"));
        // pocket-ui looks at the saves after every game (the index then counts this play).
        self.entry(d);
    }

    /// The player answers an open question on `d`: one of the shown versions stays, the others go to the backup
    /// folder. The kept bytes now stand for every version shown.
    fn choose(&mut self, d: usize, pick: usize) {
        if self.devs[d].copies.is_empty() {
            return;
        }
        // The page looks at the saves before it applies a choice (as after every game).
        self.entry(d);
        let dev = &mut self.devs[d];
        let mut shown = vec![dev.file.clone().unwrap()];
        shown.append(&mut dev.copies);
        let kept = shown[pick % shown.len()].clone();
        // The player saw every version shown: the choice stands for all of them (also over copies of a question a
        // lost record no longer remembers; `chose` counts such a choice as news).
        let recorded = dev.rec.as_ref().is_some_and(|r| r.asking.is_some());
        let all: BTreeSet<u64> = shown.iter().flat_map(|v| v.covers.iter().copied()).collect();
        self.log.push(format!("choose {d} keeps v{}", kept.id));
        let dev = &mut self.devs[d];
        let v = Ver { id: kept.id, covers: all, chosen: true, lost: !recorded || shown.iter().any(|v| v.lost) };
        let me = dev.id.clone();
        chose(dev.rec.as_mut().unwrap(), &v.sha(), &me);
        dev.file = Some(v);
    }

    /// Device `d`'s record is lost (a damaged card, no whole copy): a new history under a new id, its saves as they are.
    fn lose_record(&mut self, d: usize) {
        let dev = &mut self.devs[d];
        dev.losses += 1;
        dev.id = super::super::new_writer(&format!("P{d}"), &format!("L{}", dev.losses));
        dev.rec = None;
        if let Some(f) = dev.file.as_mut() {
            f.lost = true;
        }
        self.log.push(format!("record of {d} lost: counts as {}", self.devs[d].id));
        self.entry(d);
    }

    /// What device `d` sends or uses: its save with its record brought up to date.
    fn entry(&mut self, d: usize) -> Option<Entry> {
        let dev = &mut self.devs[d];
        let f = dev.file.as_ref()?;
        let r = bookkeep(&dev.id, dev.rec.as_ref(), &f.sha(), !dev.copies.is_empty());
        dev.rec = Some(r.clone());
        Some(Entry {
            rel: "gba/G.srm".into(),
            system: "gba".into(),
            stem: "G".into(),
            ext: "srm".into(),
            crc: Some(7),
            size: 8,
            mtime: 0,
            sha: f.sha(),
            vv: r.vv,
            asking: r.asking,
            marks: r.marks,
            open_copy: !dev.copies.is_empty(),
        })
    }

    /// A save replaces device `to`'s: it must contain everything the old one did.
    fn replace(&mut self, to: usize, v: &Ver, e: &Entry) {
        if let Some(old) = &self.devs[to].file {
            assert!(
                v.covers.is_superset(&old.covers),
                "device {to}: v{} replaced v{} and lost play {:?}\n{}",
                v.id,
                old.id,
                old.covers.difference(&v.covers).collect::<Vec<_>>(),
                self.log.join("\n")
            );
        }
        let r = received(self.devs[to].rec.as_ref(), e);
        if answered(self.devs[to].rec.as_ref(), &r) {
            self.log.push(format!("  question on {to} answered elsewhere"));
            self.devs[to].copies.clear();
        }
        self.devs[to].file = Some(v.clone());
        self.devs[to].rec = Some(r);
    }

    /// Device `a` syncs with device `b` (a starts).
    fn sync(&mut self, a: usize, b: usize) {
        let (la, lb) = (self.entry(a), self.entry(b));
        let steps = plan(la.as_slice(), lb.as_slice());
        for s in steps {
            let show = |e: &Option<Entry>| {
                e.as_ref().map_or("-".to_string(), |e| {
                    let id = u64::from_str_radix(&e.sha[48..], 16).unwrap_or(0);
                    let marks: String = e.marks.iter().map(|q| format!(" in({})", vv_str(q))).collect();
                    let ask = e.asking.as_ref().map_or(String::new(), |a| format!(" asking({})", vv_str(a)));
                    format!("v{id} [{}]{marks}{ask}", vv_str(&e.vv))
                })
            };
            self.log.push(format!("sync {a}<->{b}: {} | {} => {:?}", show(&s.local), show(&s.remote), s.action));
            match s.action {
                Action::Same { .. } => {
                    let (l, r) = (s.local.clone().unwrap(), s.remote.clone().unwrap());
                    for (d, peer) in [(a, &r), (b, &l)] {
                        let before = self.devs[d].rec.clone();
                        let rec = self.devs[d].rec.as_mut().unwrap();
                        same(rec, peer);
                        if answered(before.as_ref(), rec) {
                            self.log.push(format!("  question on {d} answered elsewhere"));
                            self.devs[d].copies.clear();
                        }
                    }
                    // Same bytes: the same content; both stand for what either covered. A lost record's new count
                    // spreads with the merged vectors, so both carry the "after a loss" mark.
                    let all: BTreeSet<u64> =
                        [a, b].iter().flat_map(|&d| self.devs[d].file.as_ref().unwrap().covers.clone()).collect();
                    let lost = [a, b].iter().any(|&d| self.devs[d].file.as_ref().unwrap().lost);
                    for d in [a, b] {
                        let f = self.devs[d].file.as_mut().unwrap();
                        f.covers = all.clone();
                        f.lost |= lost;
                    }
                }
                Action::Push { .. } => {
                    let v = self.devs[a].file.clone().unwrap();
                    self.replace(b, &v, s.local.as_ref().unwrap());
                }
                Action::Pull => {
                    let v = self.devs[b].file.clone().unwrap();
                    self.replace(a, &v, s.remote.as_ref().unwrap());
                }
                Action::Conflict => {
                    let (va, vb) = (self.devs[a].file.clone().unwrap(), self.devs[b].file.clone().unwrap());
                    // The same bytes are beside the save already (the question open here showed them): no second
                    // copy and no new question, only the question's vector grows (`conflict_found`).
                    let shown = self.devs[a].copies.iter().position(|c| c.id == vb.id);
                    match shown {
                        Some(i) => {
                            let c = &mut self.devs[a].copies[i];
                            c.covers.extend(vb.covers.iter().copied());
                            c.lost |= vb.lost;
                        }
                        None => self.check_question(&va, &vb),
                    }
                    let (l, r) = (s.local.unwrap(), s.remote.unwrap());
                    let (rec, mark_vv) = conflict_found(self.devs[a].rec.as_ref(), &l, &r);
                    self.devs[a].rec = Some(rec);
                    if shown.is_none() {
                        self.devs[a].copies.push(vb);
                    }
                    if let Some(rb) = self.devs[b].rec.as_mut() {
                        mark(rb, &r.sha, &mark_vv);
                    }
                }
                Action::Offer => {
                    let (va, vb) = (self.devs[a].file.clone().unwrap(), self.devs[b].file.clone().unwrap());
                    if !self.devs[b].copies.iter().any(|c| c.id == va.id) {
                        self.check_question(&va, &vb);
                    }
                    let l = s.local.unwrap();
                    let whole = offered(self.devs[b].rec.as_mut().unwrap(), &l);
                    match self.devs[b].copies.iter_mut().find(|c| c.id == va.id) {
                        Some(c) => {
                            c.covers.extend(va.covers.iter().copied());
                            c.lost |= va.lost;
                        }
                        None => self.devs[b].copies.push(va),
                    }
                    mark(self.devs[a].rec.as_mut().unwrap(), &l.sha, &whole);
                }
                Action::Waiting { .. } => {}
            }
        }
    }

    /// A question about `va` and `vb` is put to the player: they must really have been played apart, and this pair
    /// must never have been asked about before, unless both are choices the player made on two Pockets that
    /// contradict each other (questions that began apart, answered both ways: asking once more is right).
    fn check_question(&mut self, va: &Ver, vb: &Ver) {
        // Two choices that kept different saves after seeing the same versions contradict each other: asked again.
        let contradiction = va.chosen && vb.chosen && va.covers == vb.covers;
        if va.lost || vb.lost {
            // After a lost record the question is the safe answer, even where the record would have known better.
            self.after_loss += 1;
            self.conflicts += 1;
            return;
        }
        assert!(
            contradiction || (!va.covers.is_superset(&vb.covers) && !vb.covers.is_superset(&va.covers)),
            "false question between v{} and v{}\n{}",
            va.id,
            vb.id,
            self.log.join("\n")
        );
        let key = (va.id.min(vb.id), va.id.max(vb.id));
        if !self.asked.insert(key) || contradiction {
            if va.chosen && vb.chosen {
                self.contradictions += 1;
            } else {
                self.unheard += 1;
            }
        }
        self.conflicts += 1;
    }

    fn settled(&mut self) -> bool {
        let first = self.devs[0].file.as_ref().map(|f| f.id);
        self.devs.iter().all(|d| d.copies.is_empty() && d.file.as_ref().map(|f| f.id) == first)
    }
}

/// How often the player was asked in a run.
#[derive(Clone, Copy, Debug, Default)]
struct Stats {
    questions: usize,
    contradictions: usize,
    unheard: usize,
}

fn run(seed: u64, n: usize, steps: usize) -> Stats {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut w = World::new(n);
    for _ in 0..steps {
        // Now and then a Pocket's record is lost (Sol review 19.11.2026 #3): nothing may be lost or walk back for it.
        if rng.below(80) == 0 {
            let d = rng.below(n);
            w.lose_record(d);
            continue;
        }
        match rng.below(10) {
            0..=2 => {
                let d = rng.below(n);
                w.play(d);
            }
            3 => {
                let (d, p) = (rng.below(n), rng.below(4));
                w.choose(d, p);
            }
            _ => {
                let a = rng.below(n);
                let b = (a + 1 + rng.below(n - 1)) % n;
                w.sync(a, b);
            }
        }
    }
    // The quiet end: nobody plays; whoever is asked chooses; everybody meets everybody until nothing changes.
    for round in 0..12 {
        for d in 0..n {
            let p = rng.below(4);
            w.choose(d, p);
        }
        for a in 0..n {
            for b in 0..n {
                if a != b {
                    w.sync(a, b);
                }
            }
        }
        if w.settled() {
            return Stats { questions: w.conflicts, contradictions: w.contradictions, unheard: w.unheard };
        }
        assert!(round < 11, "seed {seed}: did not settle\n{}", w.log.join("\n"));
    }
    Stats { questions: w.conflicts, contradictions: w.contradictions, unheard: w.unheard }
}

#[test]
fn four_pockets_random_play_never_lose_rarely_ask_again_and_settle() {
    let mut t = Stats::default();
    for seed in 0..3000 {
        let s = run(seed, 4, 120);
        t.questions += s.questions;
        t.contradictions += s.contradictions;
        t.unheard += s.unheard;
    }
    eprintln!("four Pockets, 3000 runs: {t:?}");
    // The runs must actually produce questions, or the checks above prove nothing. A pair asked again (after the
    // player chose both ways on two Pockets, or on a Pocket that had not heard of the question) happens, but rarely,
    // even with four Pockets all playing apart this often (far more than a player does).
    assert!(t.questions > 1000, "{t:?}");
    assert!((t.contradictions + t.unheard) * 20 < t.questions, "{t:?}");
}

#[test]
fn two_and_three_pockets_too() {
    for seed in 0..1000 {
        run(seed, 2, 80);
        run(seed, 3, 100);
    }
}

#[test]
fn the_three_pocket_story_from_the_design_page() {
    let mut w = World::new(3);
    // A plays, B fetches, C fetches from B, A plays on, C fetches from A: no question.
    w.play(0);
    w.sync(1, 0);
    w.sync(2, 1);
    w.play(0);
    w.sync(2, 0);
    w.sync(1, 2);
    assert_eq!(w.conflicts, 0);
    assert!(w.settled());
    // A and C play without meeting: asked exactly once, whoever meets whom afterwards.
    w.play(0);
    w.play(2);
    w.sync(2, 0);
    for (a, b) in [(0, 2), (1, 0), (1, 2), (2, 1), (0, 1), (2, 0)] {
        w.sync(a, b);
    }
    assert_eq!(w.conflicts, 1);
    assert_eq!(w.devs.iter().map(|d| d.copies.len()).sum::<usize>(), 1);
    // C keeps its own; it reaches everyone without a new question.
    w.choose(2, 0);
    for (a, b) in [(2, 0), (1, 0), (1, 2), (0, 1)] {
        w.sync(a, b);
    }
    assert!(w.settled());
    assert_eq!(w.conflicts, 1);
}

#[test]
fn a_lost_record_asks_instead_of_letting_an_older_save_win() {
    let mut w = World::new(2);
    for _ in 0..6 {
        w.play(0);
        w.sync(1, 0);
    }
    // Pocket 0 plays once more, then its record is lost; Pocket 1 still has the sixth save.
    w.play(0);
    w.lose_record(0);
    w.sync(0, 1);
    w.sync(1, 0);
    let newest = w.devs[0].file.clone().unwrap();
    assert_eq!(w.after_loss, 1, "one question\n{}", w.log.join("\n"));
    assert!(newest.covers.len() == 7 && w.devs[0].copies.len() == 1, "the newest save stays, the older one beside it");
    w.choose(0, 0);
    w.sync(0, 1);
    assert!(w.settled());
    assert_eq!(w.devs[1].file.as_ref().unwrap().id, newest.id);
}
