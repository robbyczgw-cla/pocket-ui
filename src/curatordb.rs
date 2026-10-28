//! Curator feel tags (docs/CURATOR.md, layer B): how a game plays and feels, on a fixed vocabulary of
//! about 170 tags (family, sub-genre, mechanics, pace, controls, mood, multiplayer, setting, ...) with a
//! weight of 1-3, plus length, session, difficulty and whether it is text-heavy.
//!
//! Built offline by `scripts/build-curatordb.py` (a batch labeller over the Wikidata facts, checked
//! against a hand-labelled golden set and a second model family) into `assets/curatordb.bin` and compiled
//! into the binary. The tags are our own derived data; no Wikipedia or other copyrighted text is in it.
//!
//! The file follows `gamedb.bin`: a text header and one zlib stream per system, so asking about a game
//! unpacks only its system. The first stream, `_vocab`, is the vocabulary itself (the JSON of
//! `assets/data/curator-tags.json`), so tags and vocabulary always come from the same build.
//!
//! ```text
//! PCDB1
//! _vocab 0 9876
//! gba 9876 20480
//! ...
//!
//! <zlib><zlib>...
//! ```
//!
//! A system's stream holds one game per line, sorted by QID, tab-separated: QID, tags as
//! `id:weight,id:weight`, length, session, difficulty, text-heavy (`1`/`0`/empty), confidence (1-3),
//! cross-checked (`1`/empty). An empty field is "unknown".
//!
//! # Use
//!
//! ```no_run
//! use pocket_ui::curatordb;
//! let vocab = curatordb::vocabulary();
//! if let Some(game) = curatordb::lookup("ps2", "Q2207176") {
//!     for &(tag, weight) in &game.tags {
//!         println!("{} ({weight})", vocab.tag(tag).label); // "crash takedowns (3)"
//!     }
//! }
//! ```
//!
//! Everything is lazy and thread-safe: nothing is parsed until the first call, each system is unpacked
//! once (a few milliseconds) and shared, so the Curator index can be built on a background thread.
//! Games without tags (obscure ones, or not labelled) return `None`; use the Wikidata genres through
//! [`Vocabulary::genre_tags`] for them.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// The data file shipped with pocket-ui.
pub static EMBEDDED: &[u8] = include_bytes!("../assets/curatordb.bin");

const MAGIC: &str = "PCDB1";
const VOCAB_SECTION: &str = "_vocab";
/// No section unpacks to more than this; a damaged length can't make us allocate the card full.
const MAX_SECTION: usize = 16 << 20;

/// Index of a tag in [`Vocabulary::tags`]; stable within one build of the data (not across builds: store
/// tag ids, the strings, if anything is kept on disk).
pub type TagId = u16;

/// One axis of the vocabulary ("family", "pace", "mood", ...).
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Axis {
    pub id: String,
    /// "Family", "Pace" ...: a facet title.
    pub label: String,
    /// One value per game (pace, length, session, difficulty, text). Length, session, difficulty and text
    /// are [`GameTags`] fields; pace is a tag like the others, exactly one per game.
    pub single: bool,
    #[serde(default)]
    pub hint: String,
}

/// One tag.
#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Tag {
    /// "crash-takedowns": stable across builds.
    pub id: String,
    /// The axis id ("mechanics").
    pub axis: String,
    /// Short English label written for reason lines: "crash takedowns", "arcade racing", "short sessions".
    pub label: String,
    /// What the tag means (for the labeller; may be shown as help).
    #[serde(default)]
    pub hint: String,
}

#[derive(serde::Deserialize)]
struct VocabFile {
    version: u32,
    axes: Vec<Axis>,
    tags: Vec<Tag>,
    genre_map: HashMap<String, Vec<String>>,
}

/// The tag vocabulary of the data file.
#[derive(Debug, Default)]
pub struct Vocabulary {
    pub version: u32,
    pub axes: Vec<Axis>,
    pub tags: Vec<Tag>,
    by_id: HashMap<String, TagId>,
    /// Wikidata genre label ("racing video game") -> tags (family, sub-genre, setting).
    genres: HashMap<String, Vec<TagId>>,
}

impl Vocabulary {
    fn from_json(text: &str) -> Result<Vocabulary, String> {
        let f: VocabFile = serde_json::from_str(text).map_err(|e| format!("vocabulary: {e}"))?;
        if f.tags.len() > TagId::MAX as usize {
            return Err("vocabulary: too many tags".into());
        }
        let by_id: HashMap<String, TagId> = f.tags.iter().enumerate().map(|(i, t)| (t.id.clone(), i as TagId)).collect();
        let genres = f
            .genre_map
            .into_iter()
            .map(|(g, ids)| (g, ids.iter().filter_map(|id| by_id.get(id).copied()).collect()))
            .collect();
        Ok(Vocabulary {
            version: f.version,
            axes: f.axes,
            tags: f.tags,
            by_id,
            genres,
        })
    }

    /// The tag `id` ("arcade-racing").
    pub fn id(&self, id: &str) -> Option<TagId> {
        self.by_id.get(id).copied()
    }

    /// The tag behind a [`TagId`] of this vocabulary. Panics on an id from elsewhere.
    pub fn tag(&self, t: TagId) -> &Tag {
        &self.tags[t as usize]
    }

    /// The tags of `axis`, in vocabulary order (the order a facet lists them).
    pub fn axis(&self, axis: &str) -> impl Iterator<Item = TagId> + '_ {
        let axis = axis.to_string();
        self.tags.iter().enumerate().filter(move |(_, t)| t.axis == axis).map(|(i, _)| i as TagId)
    }

    /// Tags a Wikidata genre label stands for (family, sometimes sub-genre or setting): "kart racing game" ->
    /// racing, kart-racing. Works for every game of gamedb, tagged or not; empty for a genre that says
    /// nothing we use ("Christmas video game") or one the vocabulary doesn't know yet.
    pub fn genre_tags(&self, wikidata_genre: &str) -> &[TagId] {
        self.genres.get(wikidata_genre).map_or(&[], Vec::as_slice)
    }
}

/// How long the main game takes a typical player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Length {
    /// Under 5 hours.
    Short,
    /// 5-15 hours.
    Medium,
    /// 15-40 hours.
    Long,
    /// 40 hours and more.
    Epic,
    /// No real end: score attack, sports, versus, sandbox.
    Endless,
}

/// How a typical session feels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Session {
    /// Fun in 15 minutes or less.
    Pickup,
    /// Needs longer sittings.
    Sitdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Difficulty {
    Easy,
    Moderate,
    Hard,
    Brutal,
}

impl Length {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "short" => Self::Short,
            "medium" => Self::Medium,
            "long" => Self::Long,
            "epic" => Self::Epic,
            "endless" => Self::Endless,
            _ => return None,
        })
    }
    /// The vocabulary tag for this value ("length-short"), for its label and for facet indexes.
    pub fn tag_id(self) -> &'static str {
        match self {
            Self::Short => "length-short",
            Self::Medium => "length-medium",
            Self::Long => "length-long",
            Self::Epic => "length-epic",
            Self::Endless => "length-endless",
        }
    }
}

impl Session {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "pickup" => Self::Pickup,
            "sitdown" => Self::Sitdown,
            _ => return None,
        })
    }
    pub fn tag_id(self) -> &'static str {
        match self {
            Self::Pickup => "session-pickup",
            Self::Sitdown => "session-sitdown",
        }
    }
}

impl Difficulty {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "easy" => Self::Easy,
            "moderate" => Self::Moderate,
            "hard" => Self::Hard,
            "brutal" => Self::Brutal,
            _ => return None,
        })
    }
    pub fn tag_id(self) -> &'static str {
        match self {
            Self::Easy => "difficulty-easy",
            Self::Moderate => "difficulty-moderate",
            Self::Hard => "difficulty-hard",
            Self::Brutal => "difficulty-brutal",
        }
    }
}

/// The feel of one game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameTags {
    pub qid: String,
    /// (tag, weight 1-3), in vocabulary order. 3 = defining, 2 = clearly present, 1 = minor (only shipped
    /// when a second model family confirmed it).
    pub tags: Vec<(TagId, u8)>,
    pub length: Option<Length>,
    pub session: Option<Session>,
    pub difficulty: Option<Difficulty>,
    /// Needs a lot of reading (in a language you read: see `romfacts` for the file's language).
    pub text_heavy: Option<bool>,
    /// How well the labeller knew the game: 3 well, 2 roughly, 1 barely (then only obvious tags).
    pub confidence: u8,
    /// A second model family labelled the game too; only tags both gave were kept.
    pub cross_checked: bool,
}

impl GameTags {
    /// The weight of tag `id` ("drifting"), 0 when the game doesn't have it.
    pub fn weight(&self, vocab: &Vocabulary, id: &str) -> u8 {
        vocab.id(id).and_then(|t| self.tags.iter().find(|(x, _)| *x == t)).map_or(0, |&(_, w)| w)
    }

    /// Every facet value of the game as tags: its tags plus the tags for length, session, difficulty and
    /// text-heavy, for building one bitset per tag.
    pub fn facet_tags(&self, vocab: &Vocabulary) -> Vec<TagId> {
        let mut out: Vec<TagId> = self.tags.iter().map(|&(t, _)| t).collect();
        let scalars = [
            self.length.map(Length::tag_id),
            self.session.map(Session::tag_id),
            self.difficulty.map(Difficulty::tag_id),
            (self.text_heavy == Some(true)).then_some("text-heavy"),
        ];
        out.extend(scalars.into_iter().flatten().filter_map(|id| vocab.id(id)));
        out
    }

    /// One line of the file. Tags the vocabulary doesn't know and weights outside 1-3 are left out.
    fn from_line(line: &str, vocab: &Vocabulary) -> Option<GameTags> {
        let f: Vec<&str> = line.split('\t').collect();
        let qid = *f.first()?;
        if !qid.starts_with('Q') || qid.len() < 2 {
            return None;
        }
        let field = |i: usize| f.get(i).copied().unwrap_or("");
        let mut tags: Vec<(TagId, u8)> = field(1)
            .split(',')
            .filter_map(|tw| {
                let (id, w) = tw.split_once(':')?;
                let w: u8 = w.parse().ok().filter(|w| (1..=3).contains(w))?;
                Some((vocab.id(id)?, w))
            })
            .collect();
        // Vocabulary order; a tag listed twice counts once, as first listed (the sort is stable).
        tags.sort_by_key(|&(t, _)| t);
        tags.dedup_by_key(|(t, _)| *t);
        Some(GameTags {
            qid: qid.to_string(),
            tags,
            length: Length::parse(field(2)),
            session: Session::parse(field(3)),
            difficulty: Difficulty::parse(field(4)),
            text_heavy: match field(5) {
                "1" => Some(true),
                "0" => Some(false),
                _ => None,
            },
            confidence: field(6).parse().ok().filter(|c| (1..=3).contains(c)).unwrap_or(1),
            cross_checked: field(7) == "1",
        })
    }
}

/// The tagged games of one system.
#[derive(Debug, Default)]
pub struct SystemTags {
    games: HashMap<String, GameTags>,
}

impl SystemTags {
    pub fn get(&self, qid: &str) -> Option<&GameTags> {
        self.games.get(qid)
    }
    pub fn len(&self) -> usize {
        self.games.len()
    }
    pub fn is_empty(&self) -> bool {
        self.games.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &GameTags> {
        self.games.values()
    }
}

/// The data file: header read, sections unpacked when first asked for. Thread-safe.
pub struct CuratorDb {
    bytes: Cow<'static, [u8]>,
    /// (section, start, length) of each stream in `bytes`.
    sections: Vec<(String, usize, usize)>,
    vocab: OnceLock<Vocabulary>,
    loaded: Mutex<HashMap<String, Arc<SystemTags>>>,
}

impl CuratorDb {
    /// Reads the header; the streams stay packed until asked for.
    pub fn parse(bytes: Cow<'static, [u8]>) -> Result<CuratorDb, String> {
        let end = bytes.windows(2).position(|w| w == b"\n\n").ok_or("no header")?;
        let header = std::str::from_utf8(&bytes[..end]).map_err(|_| "header is not text")?;
        let mut lines = header.lines();
        if lines.next() != Some(MAGIC) {
            return Err(format!("not a {MAGIC} file"));
        }
        let body = end + 2;
        let mut sections = Vec::new();
        for line in lines {
            let mut parts = line.split(' ');
            let (Some(name), Some(off), Some(len), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
                return Err(format!("bad header line {line:?}"));
            };
            let (off, len): (usize, usize) = (off.parse().map_err(|_| "bad offset")?, len.parse().map_err(|_| "bad length")?);
            let start = body.checked_add(off).ok_or("bad offset")?;
            if start.checked_add(len).is_none_or(|e| e > bytes.len()) {
                return Err(format!("{name} runs past the end of the file"));
            }
            sections.push((name.to_string(), start, len));
        }
        Ok(CuratorDb {
            bytes,
            sections,
            vocab: OnceLock::new(),
            loaded: Mutex::new(HashMap::new()),
        })
    }

    /// The file compiled into pocket-ui, parsed once.
    pub fn embedded() -> &'static CuratorDb {
        static DB: OnceLock<CuratorDb> = OnceLock::new();
        DB.get_or_init(|| {
            Self::parse(Cow::Borrowed(EMBEDDED)).unwrap_or_else(|e| {
                eprintln!("curatordb: {e}");
                Self::empty()
            })
        })
    }

    fn empty() -> CuratorDb {
        CuratorDb {
            bytes: Cow::Borrowed(&[]),
            sections: Vec::new(),
            vocab: OnceLock::new(),
            loaded: Mutex::new(HashMap::new()),
        }
    }

    fn unpack(&self, name: &str) -> Option<String> {
        let &(_, start, len) = self.sections.iter().find(|s| s.0 == name)?;
        match miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&self.bytes[start..start + len], MAX_SECTION) {
            Ok(t) => Some(String::from_utf8_lossy(&t).into_owned()),
            Err(e) => {
                eprintln!("curatordb: {name} is damaged ({:?})", e.status);
                None
            }
        }
    }

    /// The vocabulary of this file (empty when the file has none or it is damaged).
    pub fn vocabulary(&self) -> &Vocabulary {
        self.vocab.get_or_init(|| {
            self.unpack(VOCAB_SECTION)
                .and_then(|text| Vocabulary::from_json(&text).map_err(|e| eprintln!("curatordb: {e}")).ok())
                .unwrap_or_default()
        })
    }

    /// System ids in the file.
    pub fn systems(&self) -> Vec<&str> {
        self.sections.iter().map(|s| s.0.as_str()).filter(|s| *s != VOCAB_SECTION).collect()
    }

    /// The tagged games of `system` (ROCKNIX's id; "genesis" finds "megadrive"), unpacked once.
    pub fn system(&self, system: &str) -> Option<Arc<SystemTags>> {
        let id = crate::gamedb::canonical(system);
        if id == VOCAB_SECTION {
            return None;
        }
        if let Some(s) = self.loaded.lock().ok()?.get(id) {
            return Some(s.clone());
        }
        // Unpacked outside the lock: another thread asking for another system is not held up. Two threads
        // asking for the same system at once both unpack it; the first one stored wins.
        let text = self.unpack(id)?;
        let vocab = self.vocabulary();
        let games = text
            .lines()
            .filter_map(|l| GameTags::from_line(l, vocab))
            .map(|g| (g.qid.clone(), g))
            .collect();
        let tags = Arc::new(SystemTags { games });
        let mut loaded = self.loaded.lock().ok()?;
        Some(loaded.entry(id.to_string()).or_insert(tags).clone())
    }

    /// The tags of game `qid` (a gamedb QID) on `system`, looking into related systems (Game Boy and Game
    /// Boy Color, ...) when the system itself has none.
    pub fn lookup(&self, system: &str, qid: &str) -> Option<GameTags> {
        let id = crate::gamedb::canonical(system);
        std::iter::once(id)
            .chain(crate::gamedb::related_systems(id).iter().copied())
            .find_map(|s| self.system(s)?.get(qid).cloned())
    }
}

/// The vocabulary of the shipped data.
pub fn vocabulary() -> &'static Vocabulary {
    CuratorDb::embedded().vocabulary()
}

/// The tags of game `qid` on `system` in the shipped data (see [`CuratorDb::lookup`]).
pub fn lookup(system: &str, qid: &str) -> Option<GameTags> {
    CuratorDb::embedded().lookup(system, qid)
}

/// Writes a data file like the build script does (for tests and tools).
pub fn encode(vocab_json: &str, sections: &[(&str, &str)]) -> Vec<u8> {
    let all: Vec<(&str, &str)> = std::iter::once((VOCAB_SECTION, vocab_json)).chain(sections.iter().copied()).collect();
    let blobs: Vec<Vec<u8>> = all
        .iter()
        .map(|(_, text)| miniz_oxide::deflate::compress_to_vec_zlib(text.as_bytes(), 9))
        .collect();
    let mut header = String::from(MAGIC);
    let mut off = 0;
    for ((name, _), blob) in all.iter().zip(&blobs) {
        header.push_str(&format!("\n{name} {off} {}", blob.len()));
        off += blob.len();
    }
    header.push_str("\n\n");
    let mut out = header.into_bytes();
    blobs.iter().for_each(|b| out.extend_from_slice(b));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const VOCAB: &str = r#"{"version":1,"axes":[{"id":"family","label":"Family","single":false,"hint":""},
        {"id":"mechanics","label":"Mechanics","single":false},{"id":"length","label":"Length","single":true},
        {"id":"text","label":"Text","single":true}],
        "tags":[{"id":"racing","axis":"family","label":"racing"},{"id":"rpg","axis":"family","label":"role-playing"},
        {"id":"crash-takedowns","axis":"mechanics","label":"crash takedowns"},{"id":"length-short","axis":"length","label":"under 5 hours"},
        {"id":"text-heavy","axis":"text","label":"lots of reading"}],
        "genre_map":{"racing video game":["racing"],"kart racing game":["racing","kart-racing"],"Christmas video game":[]}}"#;
    const PS2: &str = "Q2207176\tracing:3,crash-takedowns:3,unknown-tag:2,racing:1\tshort\tpickup\tmoderate\t0\t3\t1\n\
Q1\trpg:2,crash-takedowns:9\tforever\t\t\t1\t\t\n";

    fn db() -> CuratorDb {
        CuratorDb::parse(Cow::Owned(encode(VOCAB, &[("ps2", PS2), ("gb", "Q5\trpg:2\n")]))).expect("a file written by encode reads back")
    }

    #[test]
    fn a_line_reads_back_with_its_tags_and_values() {
        let db = db();
        let v = db.vocabulary();
        assert_eq!(v.tags.len(), 5);
        assert_eq!(db.systems(), ["ps2", "gb"]);
        let b = db.lookup("ps2", "Q2207176").expect("Burnout Revenge");
        let ids: Vec<(&str, u8)> = b.tags.iter().map(|&(t, w)| (v.tag(t).id.as_str(), w)).collect();
        assert_eq!(ids, [("racing", 3), ("crash-takedowns", 3)], "unknown tags dropped, a tag once, vocabulary order");
        assert_eq!(b.length, Some(Length::Short));
        assert_eq!(b.session, Some(Session::Pickup));
        assert_eq!(b.difficulty, Some(Difficulty::Moderate));
        assert_eq!(b.text_heavy, Some(false));
        assert_eq!(b.confidence, 3);
        assert!(b.cross_checked);
        assert_eq!(b.weight(v, "crash-takedowns"), 3);
        assert_eq!(b.weight(v, "rpg"), 0);
        assert_eq!(v.tag(v.id("crash-takedowns").unwrap()).label, "crash takedowns");
        let facets: Vec<&str> = b.facet_tags(v).iter().map(|&t| v.tag(t).id.as_str()).collect();
        assert_eq!(facets, ["racing", "crash-takedowns", "length-short"], "values without a vocabulary tag are skipped");
        // Odd values are unknown, a weight of 9 is no tag.
        let odd = db.lookup("ps2", "Q1").unwrap();
        assert_eq!(odd.tags.len(), 1);
        assert_eq!((odd.length, odd.text_heavy, odd.confidence, odd.cross_checked), (None, Some(true), 1, false));
        assert_eq!(db.lookup("ps2", "Q404"), None);
        assert_eq!(db.lookup("snes", "Q2207176"), None, "a system the file doesn't have");
    }

    #[test]
    fn systems_unpack_once_and_know_aliases_and_related_systems() {
        let db = CuratorDb::parse(Cow::Owned(encode(VOCAB, &[("megadrive", "Q9\tracing:2\n"), ("gb", "Q5\trpg:2\n")]))).unwrap();
        let a = db.system("genesis").expect("genesis is the Mega Drive");
        let b = db.system("megadrive").unwrap();
        assert!(Arc::ptr_eq(&a, &b), "the same unpacked list");
        assert!(db.lookup("gbc", "Q5").is_some(), "a Game Boy game filed under Game Boy Color");
        assert!(db.system("_vocab").is_none(), "the vocabulary is not a system");
    }

    #[test]
    fn the_genre_map_gives_tags_for_any_game() {
        let db = db();
        let v = db.vocabulary();
        let ids = |g: &str| v.genre_tags(g).iter().map(|&t| v.tag(t).id.clone()).collect::<Vec<_>>();
        assert_eq!(ids("racing video game"), ["racing"]);
        assert_eq!(ids("kart racing game"), ["racing"], "a tag the vocabulary lacks is skipped");
        assert!(ids("Christmas video game").is_empty());
        assert!(ids("not a genre").is_empty());
        assert_eq!(v.axis("family").count(), 2);
    }

    #[test]
    fn a_damaged_file_is_an_error_or_a_missing_system_never_a_panic() {
        assert!(CuratorDb::parse(Cow::Borrowed(b"")).is_err());
        assert!(CuratorDb::parse(Cow::Borrowed(b"PCDB9\n\n")).is_err(), "unknown version");
        assert!(CuratorDb::parse(Cow::Borrowed(b"PCDB1\nps2 0 99999\n\nxx")).is_err(), "section past the end");
        assert!(CuratorDb::parse(Cow::Borrowed(b"PCDB1\nps2 0\n\n")).is_err(), "short header line");
        assert!(CuratorDb::parse(Cow::Borrowed(b"PCDB1\nps2 18446744073709551615 1\n\n")).is_err(), "offset overflow");
        // A stream that isn't zlib: that system is missing, the rest still works.
        let mut bytes = encode(VOCAB, &[("gb", "Q5\trpg:2\n"), ("ps2", PS2)]);
        let at = bytes.len() - 10;
        bytes[at..].fill(0xff);
        let db = CuratorDb::parse(Cow::Owned(bytes)).unwrap();
        assert!(db.system("ps2").is_none());
        assert!(db.lookup("gb", "Q5").is_some());
        // A damaged vocabulary: no tags, but values and no panic.
        let db = CuratorDb::parse(Cow::Owned(encode("{not json", &[("ps2", PS2)]))).unwrap();
        assert!(db.vocabulary().tags.is_empty());
        let b = db.lookup("ps2", "Q2207176").unwrap();
        assert!(b.tags.is_empty());
        assert_eq!(b.length, Some(Length::Short));
        // Garbage lines are skipped.
        let db = CuratorDb::parse(Cow::Owned(encode(VOCAB, &[("ps2", "\n\tracing:3\nnot a qid\tracing:3\nQ7\n")]))).unwrap();
        assert_eq!(db.system("ps2").unwrap().len(), 1, "only Q7");
    }

    #[test]
    fn lookups_from_several_threads_share_one_unpacked_system() {
        let db: &'static CuratorDb = Box::leak(Box::new(db()));
        let handles: Vec<_> = (0..4).map(|_| std::thread::spawn(move || db.lookup("ps2", "Q2207176").map(|g| g.tags.len()))).collect();
        for h in handles {
            assert_eq!(h.join().unwrap(), Some(2));
        }
        assert!(Arc::ptr_eq(&db.system("ps2").unwrap(), &db.system("ps2").unwrap()));
    }

    /// The source vocabulary is valid, its genre map covers every genre of the shipped gamedb, and every
    /// tag it maps to exists.
    #[test]
    fn the_vocabulary_maps_every_wikidata_genre() {
        let text = include_str!("../assets/data/curator-tags.json");
        let v = Vocabulary::from_json(text).expect("assets/data/curator-tags.json parses");
        let raw: serde_json::Value = serde_json::from_str(text).unwrap();
        for (genre, ids) in raw["genre_map"].as_object().unwrap() {
            for id in ids.as_array().unwrap() {
                assert!(v.id(id.as_str().unwrap()).is_some(), "{genre}: unknown tag {id}");
            }
        }
        let axes: Vec<&str> = v.axes.iter().map(|a| a.id.as_str()).collect();
        for t in &v.tags {
            assert!(axes.contains(&t.axis.as_str()), "{}: unknown axis {}", t.id, t.axis);
            assert!(!t.label.is_empty() && t.label.len() <= 32, "{}: label {:?}", t.id, t.label);
        }
        assert!((140..=200).contains(&v.tags.len()), "{} tags", v.tags.len());
        assert!((20..=30).contains(&v.axis("family").count()));
        let mut gamedb = crate::gamedb::GameDb::embedded();
        let mut missing = std::collections::BTreeSet::new();
        for s in gamedb.systems().iter().map(|s| s.to_string()).collect::<Vec<_>>() {
            for g in &gamedb.system(&s).unwrap().games {
                missing.extend(g.genres.iter().filter(|x| !raw["genre_map"].as_object().unwrap().contains_key(x.as_str())).cloned());
            }
        }
        assert!(missing.is_empty(), "genres without a mapping: {missing:?}");
    }

    /// The shipped file: its vocabulary is the source one, the well-known games are tagged, and it holds
    /// to the golden set (src/testdata/curator-golden.json, labelled by hand) at the precision the build
    /// promised.
    #[test]
    fn the_shipped_file_knows_the_golden_games_and_agrees_with_them() {
        let db = CuratorDb::embedded();
        let v = db.vocabulary();
        let source = Vocabulary::from_json(include_str!("../assets/data/curator-tags.json")).unwrap();
        assert_eq!(v.tags, source.tags, "the shipped vocabulary is assets/data/curator-tags.json; run scripts/build-curatordb.py write");
        let vocab = v;
        let golden: serde_json::Value = serde_json::from_str(include_str!("testdata/curator-golden.json")).unwrap();
        let (mut right, mut given, mut found) = (0, 0, 0);
        for (qid, g) in golden.as_object().unwrap() {
            let system = g["system"].as_str().unwrap();
            let Some(tags) = db.lookup(system, qid) else {
                println!("not tagged: {system} {qid} {}", g["title"]);
                continue;
            };
            found += 1;
            let good: Vec<&str> = ["must", "ok"].iter().flat_map(|k| g[*k].as_array().unwrap().iter().map(|x| x.as_str().unwrap())).collect();
            for &(t, _) in &tags.tags {
                given += 1;
                right += good.contains(&vocab.tag(t).id.as_str()) as usize;
            }
        }
        let n = golden.as_object().unwrap().len();
        let precision = right as f32 / given.max(1) as f32;
        println!("golden set: {found} of {n} tagged, tag precision {:.1} %", 100.0 * precision);
        assert!(found * 10 >= n * 9, "{found} of {n}");
        assert!(precision >= 0.85, "{:.1} %", 100.0 * precision);
        // Burnout Revenge is an arcade racer about crash takedowns, not a simulation.
        let br = db.lookup("ps2", "Q2207176").expect("Burnout Revenge");
        assert!(br.weight(vocab, "crash-takedowns") >= 2 && br.weight(vocab, "arcade-racing") >= 2, "{br:?}");
        assert_eq!(br.weight(vocab, "sim-racing"), 0);
        let gt4 = db.lookup("ps2", "Q845695").expect("Gran Turismo 4");
        assert!(gt4.weight(vocab, "sim-racing") >= 2);
        // Chrono Trigger: the same tags on every system it is on (one work), touch only on the DS.
        let ct = db.lookup("snes", "Q761815").expect("Chrono Trigger");
        assert!(ct.weight(vocab, "time-travel") >= 2 && ct.weight(vocab, "jrpg") >= 2, "{ct:?}");
        assert_eq!(ct.weight(vocab, "touch-screen"), 0, "no touch screen on the SNES");
        assert!(db.lookup("psp", "Q761815").is_some());
    }
}
