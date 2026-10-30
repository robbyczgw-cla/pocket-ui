//! The adapter between the data and the engine: [`features_for`] turns one game of the library into its
//! [`Features`]: the facet values the Lens filters on and the weighted terms Echoes compares.
//!
//! The data (docs/CURATOR-DATA.md):
//! - layer A, `gamedb` (Wikidata): genres, series, developers, modes, players, year, sitelinks, QID, `work()`,
//!   remakes (`related`);
//! - layer B, `curatordb`: feel tags from a fixed vocabulary, arriving through [`FeelSource`] ([`CuratorDbFeel`] on
//!   the device). The vocabulary's genre map (`Vocabulary::genre_tags`) gives every game with a Wikidata genre its
//!   family, also games without tags;
//! - layer C, `romfacts`: regions, languages and fan translations from the file name.
//!
//! Every term and facet value is a vocabulary tag (key = its id, label = its label), so reasons and the Lens speak
//! the vocabulary's words. Nothing else in the engine knows where the data comes from.

use std::path::{Path, PathBuf};

use crate::curatordb::{self, GameTags, TagId, Vocabulary};
use crate::gamedb::{GameInfo, Relation};
use crate::romfacts::{self, RomFacts};

/// The Lens's facets, in the order the Lens lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Facet {
    System,
    Family,
    SubGenre,
    Mechanics,
    Players,
    Length,
    Session,
    Pace,
    Difficulty,
    Story,
    Grinding,
    Status,
    /// The player's own rating (`library_meta::GameMeta::rating`).
    Rating,
    Played,
    Reading,
    Language,
    Controls,
    Era,
    Setting,
    Mood,
    Stickers,
    Favourites,
}

impl Facet {
    pub const ALL: [Facet; 22] = [
        Facet::System,
        Facet::Family,
        Facet::SubGenre,
        Facet::Mechanics,
        Facet::Players,
        Facet::Length,
        Facet::Session,
        Facet::Pace,
        Facet::Difficulty,
        Facet::Story,
        Facet::Grinding,
        Facet::Status,
        Facet::Rating,
        Facet::Played,
        Facet::Reading,
        Facet::Language,
        Facet::Controls,
        Facet::Era,
        Facet::Setting,
        Facet::Mood,
        Facet::Stickers,
        Facet::Favourites,
    ];

    /// The name on the Lens page.
    pub fn label(self) -> &'static str {
        match self {
            Facet::System => "System",
            Facet::Family => "Family",
            Facet::SubGenre => "Sub-genre",
            Facet::Mechanics => "Mechanics",
            Facet::Players => "Players",
            Facet::Length => "Length",
            Facet::Session => "Session",
            Facet::Pace => "Pace",
            Facet::Difficulty => "Difficulty",
            Facet::Story => "Story",
            Facet::Grinding => "Grinding",
            Facet::Status => "Status",
            Facet::Rating => "My rating",
            Facet::Played => "Played",
            Facet::Reading => "Reading",
            Facet::Language => "Language",
            Facet::Controls => "Controls",
            Facet::Era => "Era",
            Facet::Setting => "Setting",
            Facet::Mood => "Mood",
            Facet::Stickers => "Stickers",
            Facet::Favourites => "Favourites",
        }
    }

    /// The id a saved Lens (a smart shelf) uses; stable across versions.
    pub fn id(self) -> &'static str {
        match self {
            Facet::System => "system",
            Facet::Family => "family",
            Facet::SubGenre => "subgenre",
            Facet::Mechanics => "mechanics",
            Facet::Players => "players",
            Facet::Length => "length",
            Facet::Session => "session",
            Facet::Pace => "pace",
            Facet::Difficulty => "difficulty",
            Facet::Story => "story",
            Facet::Grinding => "grinding",
            Facet::Status => "status",
            Facet::Rating => "rating",
            Facet::Played => "played",
            Facet::Reading => "reading",
            Facet::Language => "language",
            Facet::Controls => "controls",
            Facet::Era => "era",
            Facet::Setting => "setting",
            Facet::Mood => "mood",
            Facet::Stickers => "stickers",
            Facet::Favourites => "favourites",
        }
    }

    pub fn from_id(id: &str) -> Option<Facet> {
        Facet::ALL.into_iter().find(|f| f.id() == id)
    }

    pub fn index(self) -> usize {
        Facet::ALL.iter().position(|f| *f == self).unwrap_or(0)
    }

    /// A line under the facet's values on the Lens, where what the values mean needs saying.
    pub fn note(self) -> Option<&'static str> {
        Some(match self {
            Facet::Length => "A rough guide: plus or minus one step. Pick two next to each other for a range, such as under 15 hours.",
            Facet::Story | Facet::Reading => "Only games Curator knows well have these values; the others are in none of them.",
            Facet::Grinding => "Grinding is marked only where it stands out. Only games Curator knows well are in either value.",
            Facet::Rating => "Your own stars. Rate a game on its card: Y, then My rating.",
            _ => return None,
        })
    }

    /// The values a facet always offers when the library has them, as the Lens labels them; empty for facets whose
    /// values come from the library (systems, genres, stickers …). For the user guide's check (`doc_checks`).
    pub fn fixed_labels(self) -> Vec<String> {
        let vocab = curatordb::vocabulary();
        let label = |id: &str| vocab.id(id).map(|t| capitalise(&vocab.tag(t).label)).unwrap_or_default();
        match self {
            Facet::Length => ["length-short", "length-medium", "length-long", "length-epic", "length-endless"].iter().map(|id| label(id)).collect(),
            Facet::Story => vec![label("story-driven"), capitalise(SOME_STORY.1), label("cinematic"), label("lore"), capitalise(LITTLE_STORY.1)],
            Facet::Grinding => vec![label("grinding"), capitalise(NO_GRINDING_NOTED.1)],
            Facet::Reading => vec![capitalise(LITTLE_READING.1), label("text-heavy")],
            _ => Vec::new(),
        }
    }
}

/// Lens values (key, label) for what a game lacks, only for games the labeller knew (confidence 2 or 3): no story tag
/// is "little story", no grinding tag "not known for grinding" (grinding is tagged only where it stands out: 193 of
/// 13,412 entries, and not every grindy RPG), "lots of reading: no" is "little reading". Lens values only, never
/// Echoes terms (weight 0).
pub const LITTLE_STORY: (&str, &str) = ("story-little", "little story");
pub const NO_GRINDING_NOTED: (&str, &str) = ("grinding-not-noted", "not known for grinding");
pub const LITTLE_READING: (&str, &str) = ("text-light", "little reading");
/// "story-driven" below this weight (a weight-1 tag, shipped only when cross-checked: "some story") is the Story
/// value "some story"; weights 2 and 3 (at 70 % for a game the labeller barely knew: 1.4) are "story-driven".
pub const SOME_STORY: (&str, &str) = ("story-some", "some story");
const STORY_DRIVEN_FROM: f32 = 1.2;

/// The axes of the feel vocabulary (`assets/data/curator-tags.json`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    Family,
    SubGenre,
    Mechanic,
    Pace,
    Session,
    Length,
    Difficulty,
    Story,
    TextHeavy,
    Grinding,
    Controls,
    Mood,
    Multiplayer,
    Setting,
}

impl Axis {
    /// The axis of a vocabulary axis id ("subgenre", "mechanics" …).
    pub fn from_vocab(id: &str) -> Option<Axis> {
        Some(match id {
            "family" => Axis::Family,
            "subgenre" => Axis::SubGenre,
            "mechanics" => Axis::Mechanic,
            "pace" => Axis::Pace,
            "session" => Axis::Session,
            "length" => Axis::Length,
            "difficulty" => Axis::Difficulty,
            "story" => Axis::Story,
            "text" => Axis::TextHeavy,
            "grinding" => Axis::Grinding,
            "controls" => Axis::Controls,
            "mood" => Axis::Mood,
            "multiplayer" => Axis::Multiplayer,
            "setting" => Axis::Setting,
            _ => return None,
        })
    }

    /// The facet a tag on this axis is a value of, if any.
    pub fn facet(self) -> Option<Facet> {
        Some(match self {
            Axis::Family => Facet::Family,
            Axis::SubGenre => Facet::SubGenre,
            Axis::Mechanic => Facet::Mechanics,
            Axis::Pace => Facet::Pace,
            Axis::Session => Facet::Session,
            Axis::Length => Facet::Length,
            Axis::Difficulty => Facet::Difficulty,
            Axis::Controls => Facet::Controls,
            Axis::Mood => Facet::Mood,
            Axis::Multiplayer => Facet::Players,
            Axis::Setting => Facet::Setting,
            Axis::Story => Facet::Story,
            Axis::Grinding => Facet::Grinding,
            Axis::TextHeavy => Facet::Reading,
        })
    }

    /// How much a tag on this axis counts in Echoes: what a game is and how it plays fully; the world it is set in
    /// a little less; how long, how hard, with which controls and how much reading half (two long games are not
    /// alike for being long).
    pub fn echo_weight(self) -> f32 {
        match self {
            Axis::Length | Axis::Session | Axis::Difficulty | Axis::Controls | Axis::TextHeavy | Axis::Grinding => 0.5,
            Axis::Setting => 0.75,
            _ => 1.0,
        }
    }

    /// Axes whose labels read as a pair with a comma ("short sessions, frantic pace") rather than a dot.
    pub fn is_feel(self) -> bool {
        matches!(self, Axis::Pace | Axis::Session | Axis::Length | Axis::Difficulty)
    }
}

/// One feel tag of a game: its axis, its vocabulary id, the short label reasons use ("crash takedowns"), weight
/// (1–3 as shipped: 3 defining, 2 clearly present, 1 minor and only shipped when cross-checked).
#[derive(Clone, Debug, PartialEq)]
pub struct FeelTag {
    pub axis: Axis,
    pub key: String,
    pub label: String,
    pub weight: f32,
}

impl FeelTag {
    /// The vocabulary's tag `id` at `weight`; None for an id the vocabulary doesn't know.
    pub fn of(vocab: &Vocabulary, id: TagId, weight: f32) -> Option<FeelTag> {
        let t = vocab.tags.get(id as usize)?;
        Some(FeelTag { axis: Axis::from_vocab(&t.axis)?, key: t.id.clone(), label: t.label.clone(), weight })
    }

    /// The same by the tag's string id ("drifting").
    pub fn by_id(vocab: &Vocabulary, id: &str, weight: f32) -> Option<FeelTag> {
        FeelTag::of(vocab, vocab.id(id)?, weight)
    }
}

/// Where layer B comes from: [`CuratorDbFeel`] on the device, a fixture in tests, [`NoFeel`] for Wikidata alone.
pub trait FeelSource {
    fn tags(&self, system: &str, info: &GameInfo) -> Vec<FeelTag>;
}

/// No layer B: Wikidata and the file name only.
pub struct NoFeel;

impl FeelSource for NoFeel {
    fn tags(&self, _system: &str, _info: &GameInfo) -> Vec<FeelTag> {
        Vec::new()
    }
}

/// Layer B as shipped: `assets/curatordb.bin`. Tags are per QID; an edition filed as its own item without tags of
/// its own gets its work's.
pub struct CuratorDbFeel;

impl FeelSource for CuratorDbFeel {
    fn tags(&self, system: &str, info: &GameInfo) -> Vec<FeelTag> {
        let tags = curatordb::lookup(system, &info.qid).or_else(|| (info.work() != info.qid).then(|| curatordb::lookup(system, info.work())).flatten());
        tags.map(|t| feel_tags(curatordb::vocabulary(), &t)).unwrap_or_default()
    }
}

/// A game's tags as [`FeelTag`]s: its tags at their weights, and length, session, difficulty and "lots of reading"
/// as tags of weight 2. A game the labeller barely knew (confidence 1) counts at 70 %.
///
/// For a game it knew (confidence 2 or 3) also what it lacks, as Lens values of weight 0 (no Echoes term): no story
/// tag is "little story", no grinding tag "little grinding", "lots of reading: no" is "little reading". A weight-1
/// story or grinding tag ships only when cross-checked, so "little" means at most a minor part of the game.
pub fn feel_tags(vocab: &Vocabulary, t: &GameTags) -> Vec<FeelTag> {
    let trust = if t.confidence <= 1 { 0.7 } else { 1.0 };
    let mut out: Vec<FeelTag> = t.tags.iter().filter_map(|&(id, w)| FeelTag::of(vocab, id, w as f32 * trust)).collect();
    let scalars = [
        t.length.map(curatordb::Length::tag_id),
        t.session.map(curatordb::Session::tag_id),
        t.difficulty.map(curatordb::Difficulty::tag_id),
        (t.text_heavy == Some(true)).then_some("text-heavy"),
    ];
    out.extend(scalars.into_iter().flatten().filter_map(|id| FeelTag::by_id(vocab, id, 2.0 * trust)));
    if t.confidence >= 2 {
        let lacks = |axis: Axis| !out.iter().any(|x| x.axis == axis);
        let little = |axis: Axis, (key, label): (&str, &str)| FeelTag { axis, key: key.into(), label: label.into(), weight: 0.0 };
        let mut more = Vec::new();
        if lacks(Axis::Story) {
            more.push(little(Axis::Story, LITTLE_STORY));
        }
        if lacks(Axis::Grinding) {
            more.push(little(Axis::Grinding, NO_GRINDING_NOTED));
        }
        if t.text_heavy == Some(false) {
            more.push(little(Axis::TextHeavy, LITTLE_READING));
        }
        out.extend(more);
    }
    out
}

/// A term of a game's Echoes vector before IDF weighting.
#[derive(Clone, Debug, PartialEq)]
pub struct Term {
    /// "t:racing", "t:crash-takedowns": a vocabulary tag.
    pub key: String,
    /// Short, lowercase, for reason lines: "racing", "crash takedowns".
    pub label: String,
    pub weight: f32,
    pub axis: Option<Axis>,
}

/// One game of the library as the engine sees it.
#[derive(Clone, Debug, Default)]
pub struct Features {
    /// The app's (system, game) indices when the catalog was built; checked against `path` before use.
    pub at: (usize, usize),
    pub system: String,
    pub path: PathBuf,
    pub name: String,
    pub qid: Option<String>,
    /// The work this file is a version of (gamedb's `work()`): the same work on several systems or files is one game.
    pub work: Option<String>,
    pub year: Option<u16>,
    pub series: Vec<String>,
    pub developers: Vec<String>,
    /// Wikidata's publishers and genres as written ("racing video game"), for Connections (`crate::connections`).
    pub publishers: Vec<String>,
    pub genres: Vec<String>,
    pub sitelinks: u32,
    /// Remakes and remasters of this game, or the game this one remakes (QIDs): "Related: remake".
    pub remakes: Vec<String>,
    /// (facet, value key, value label) for the facets that come from the data (not the player's own).
    pub facets: Vec<(Facet, String, String)>,
    pub terms: Vec<Term>,
    pub rom: RomFacts,
}

/// A game of the library: where it is and what the shelf calls it.
#[derive(Clone, Copy, Debug)]
pub struct GameRef<'a> {
    pub at: (usize, usize),
    pub system: &'a str,
    pub path: &'a Path,
    pub name: &'a str,
}

/// Weights of what the genre map gives every game from its Wikidata genres, next to tags of 1–3: the family is
/// what kind of game it is; a sub-genre or setting from the genre counts like a clearly present tag.
pub const FAMILY_WEIGHT: f32 = 2.5;
pub const GENRE_WEIGHT: f32 = 2.0;

/// "Racing" from "racing": the first letter up.
pub fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

/// Systems whose own controllers have a D-pad and buttons and nothing else: every game plays on a D-pad.
const DPAD_SYSTEMS: &[&str] = &[
    "nes", "fds", "famicom", "snes", "sfc", "gb", "gbc", "gba", "megadrive", "genesis", "mastersystem", "gamegear", "sg-1000", "sg1000", "pcengine",
    "pcenginecd", "tg16", "tg16cd", "segacd", "megacd", "sega32x", "ngp", "ngpc", "wonderswan", "wonderswancolor", "atari2600", "atari7800",
    "atarilynx", "virtualboy", "pokemini", "pico-8", "msx", "colecovision", "intellivision", "supervision",
];

/// Systems with two screens.
const TWO_SCREENS: &[&str] = &["nds", "3ds", "n3ds"];

/// THE ADAPTER: one game of the library, with what gamedb knows about it (None: no entry) and its feel tags from
/// [`FeelSource`], as the engine's [`Features`].
pub fn features_for(game: GameRef, info: Option<&GameInfo>, feel: &[FeelTag]) -> Features {
    let vocab = curatordb::vocabulary();
    let file = game.path.file_name().and_then(|n| n.to_str()).unwrap_or(game.name);
    let mut f = Features {
        at: game.at,
        system: game.system.to_string(),
        path: game.path.to_path_buf(),
        name: game.name.to_string(),
        rom: romfacts::parse(file),
        ..Default::default()
    };
    // Every tag counts once, at its largest weight; a tag on a facet's axis is also that facet's value. A tag of
    // weight 0 ("little story") is a facet value only.
    let add = |f: &mut Features, t: FeelTag| {
        let weight = t.weight.clamp(0.0, 3.0) * t.axis.echo_weight();
        if let Some(facet) = t.axis.facet() {
            let (key, label) = if t.key == "story-driven" && t.weight < STORY_DRIVEN_FROM { (SOME_STORY.0.to_string(), SOME_STORY.1) } else { (t.key.clone(), t.label.as_str()) };
            if !f.facets.iter().any(|(x, k, _)| *x == facet && *k == key) {
                f.facets.push((facet, key, capitalise(label)));
            }
        }
        if weight <= 0.0 {
            return;
        }
        match f.terms.iter_mut().find(|x| x.key == format!("t:{}", t.key)) {
            Some(x) => x.weight = x.weight.max(weight),
            None => f.terms.push(Term { key: format!("t:{}", t.key), label: t.label, weight, axis: Some(t.axis) }),
        }
    };
    let facet = |f: &mut Features, facet: Facet, key: &str, label: &str| {
        if !f.facets.iter().any(|(x, k, _)| *x == facet && k == key) {
            f.facets.push((facet, key.to_string(), label.to_string()));
        }
    };

    facet(&mut f, Facet::System, game.system, "");
    for l in f.rom.languages_or_implied() {
        let name = romfacts::language_name(&l);
        facet(&mut f, Facet::Language, &l, name);
    }
    if f.rom.translation.is_some() {
        facet(&mut f, Facet::Language, "translation", "Fan translation");
    }
    // Facts of the hardware: a D-pad-only system's games all play on a D-pad, the DS has two screens.
    let label = |id: &str| vocab.id(id).map(|t| capitalise(&vocab.tag(t).label)).unwrap_or_default();
    if DPAD_SYSTEMS.contains(&game.system) {
        facet(&mut f, Facet::Controls, "dpad-friendly", &label("dpad-friendly"));
    }
    if TWO_SCREENS.contains(&game.system) {
        facet(&mut f, Facet::Controls, "two-screens", &label("two-screens"));
    }

    if let Some(info) = info {
        f.qid = Some(info.qid.clone()).filter(|q| !q.is_empty());
        f.work = f.qid.as_ref().map(|_| info.work().to_string());
        f.year = info.year;
        f.series = info.series.clone();
        f.developers = info.developers.clone();
        f.publishers = info.publishers.clone();
        f.genres = info.genres.clone();
        f.sitelinks = info.sitelinks;
        f.remakes = info.related.iter().filter(|(r, _)| matches!(r, Relation::RemakeOf | Relation::RemadeAs)).map(|(_, q)| q.clone()).collect();
        // The genre map: a family (and sometimes a sub-genre or setting) for every Wikidata genre.
        for genre in &info.genres {
            for &id in vocab.genre_tags(genre) {
                let Some(mut t) = FeelTag::of(vocab, id, GENRE_WEIGHT) else { continue };
                if t.axis == Axis::Family {
                    t.weight = FAMILY_WEIGHT;
                }
                add(&mut f, t);
            }
        }
        // Players: Wikidata's modes and player counts, next to the multiplayer tags.
        let multi = info.players.as_deref().is_some_and(|p| p.split(['-', '–']).filter_map(|n| n.trim().parse::<u32>().ok()).any(|n| n > 1));
        for m in &info.modes {
            if m == "single-player video game" {
                facet(&mut f, Facet::Players, "solo", "Solo");
            }
            if ["multiplayer video game", "two-player video game", "shared/splitscreen multiplayer", "hotseat", "player versus player", "co-op mode"].contains(&m.as_str()) {
                facet(&mut f, Facet::Players, "multi", "Two or more");
            }
        }
        if multi {
            facet(&mut f, Facet::Players, "multi", "Two or more");
        }
        if let Some(y) = info.year {
            let decade = y / 10 * 10;
            facet(&mut f, Facet::Era, &format!("{decade}s"), &format!("{decade}s"));
        }
    }
    for t in feel {
        add(&mut f, t.clone());
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(genres: &[&str], modes: &[&str]) -> GameInfo {
        GameInfo {
            qid: "Q1".into(),
            label: "X".into(),
            year: Some(2004),
            genres: genres.iter().map(|s| s.to_string()).collect(),
            modes: modes.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    fn game<'a>(system: &'a str, path: &'a Path) -> GameRef<'a> {
        GameRef { at: (0, 0), system, path, name: "X" }
    }

    #[test]
    fn the_genre_map_gives_every_game_a_family_in_the_vocabulary_s_words() {
        let p = Path::new("/roms/ps2/Burnout 3 - Takedown (USA).chd");
        let f = features_for(game("ps2", p), Some(&info(&["racing video game"], &["single-player video game", "multiplayer video game"])), &[]);
        let facets: Vec<_> = f.facets.iter().map(|(x, k, l)| (*x, k.as_str(), l.as_str())).collect();
        assert!(facets.contains(&(Facet::Family, "racing", "Racing")), "{facets:?}");
        assert!(facets.contains(&(Facet::Players, "solo", "Solo")) && facets.contains(&(Facet::Players, "multi", "Two or more")));
        assert!(facets.contains(&(Facet::Era, "2000s", "2000s")) && facets.contains(&(Facet::Language, "en", "English")));
        assert_eq!(f.work.as_deref(), Some("Q1"), "the QID is its own work");
        let t = f.terms.iter().find(|t| t.key == "t:racing").unwrap();
        assert_eq!((t.weight, t.axis), (FAMILY_WEIGHT, Some(Axis::Family)));
        let kart = features_for(game("snes", p), Some(&info(&["kart racing game"], &[])), &[]);
        assert!(kart.facets.iter().any(|(x, k, l)| *x == Facet::SubGenre && k == "kart-racing" && l == "Kart racing"), "{:?}", kart.facets);
        assert!(kart.facets.iter().any(|(x, k, l)| *x == Facet::Controls && k == "dpad-friendly" && l == "Plays well on a D-pad"));
    }

    #[test]
    fn feel_tags_are_terms_and_facet_values_at_their_largest_weight() {
        let p = Path::new("/roms/ps2/b.chd");
        let v = curatordb::vocabulary();
        let feel = [
            FeelTag::by_id(v, "crash-takedowns", 3.0).unwrap(),
            FeelTag::by_id(v, "length-medium", 2.0).unwrap(),
            FeelTag::by_id(v, "crash-takedowns", 9.0).unwrap(),
            FeelTag::by_id(v, "couch-coop", 2.0).unwrap(),
        ];
        let f = features_for(game("ps2", p), Some(&info(&[], &[])), &feel);
        assert_eq!(f.terms.iter().filter(|t| t.key == "t:crash-takedowns").count(), 1, "a tag once");
        assert_eq!(f.terms.iter().find(|t| t.key == "t:crash-takedowns").unwrap().weight, 3.0, "weights stop at 3");
        assert_eq!(f.terms.iter().find(|t| t.key == "t:length-medium").unwrap().weight, 1.0, "length counts half");
        assert!(f.facets.iter().any(|(x, k, l)| *x == Facet::Length && k == "length-medium" && l == "5–15 hours"));
        assert!(f.facets.iter().any(|(x, k, l)| *x == Facet::Mechanics && k == "crash-takedowns" && l == "Crash takedowns"));
        assert!(f.facets.iter().any(|(x, k, l)| *x == Facet::Players && k == "couch-coop" && l == "Co-op on one screen"));
    }

    #[test]
    fn curatordb_tags_arrive_with_scalars_and_editions_get_their_work_s() {
        let mut db = crate::gamedb::GameDb::embedded();
        let revenge = db.lookup("ps2", "Burnout Revenge (USA)").unwrap();
        let tags = CuratorDbFeel.tags("ps2", &revenge);
        let keys: Vec<&str> = tags.iter().map(|t| t.key.as_str()).collect();
        assert!(keys.contains(&"crash-takedowns") && keys.contains(&"arcade-racing"), "{keys:?}");
        assert!(keys.iter().any(|k| k.starts_with("length-")) && keys.iter().any(|k| k.starts_with("session-")), "{keys:?}");
        // The DS edition item points at Chrono Trigger's work and has its tags.
        let ds = db.get("nds", "Q507828").map(|g| g.work().to_string());
        assert_eq!(ds.as_deref(), Some("Q761815"));
        let ed = db.get("nds", "Q507828").unwrap();
        assert!(CuratorDbFeel.tags("nds", &ed).iter().any(|t| t.key == "time-travel"));
    }

    #[test]
    fn what_a_known_game_lacks_is_a_lens_value_and_a_barely_known_one_says_nothing() {
        let v = curatordb::vocabulary();
        let id = |s: &str| v.id(s).unwrap();
        let known = GameTags { tags: vec![(id("arcade-racing"), 3)], text_heavy: Some(false), confidence: 3, ..Default::default() };
        let keys = |t: &GameTags| feel_tags(v, t).into_iter().map(|f| (f.key, f.weight)).collect::<Vec<_>>();
        let k = keys(&known);
        for want in [LITTLE_STORY.0, NO_GRINDING_NOTED.0, LITTLE_READING.0] {
            assert!(k.contains(&(want.to_string(), 0.0)), "{want}: {k:?}");
        }
        let barely = GameTags { confidence: 1, ..known.clone() };
        assert!(keys(&barely).iter().all(|(_, w)| *w > 0.0), "no claims about a game the labeller barely knew");
        let unknown_text = GameTags { text_heavy: None, ..known.clone() };
        assert!(!keys(&unknown_text).iter().any(|(k, _)| k == LITTLE_READING.0), "reading not answered: no value");
        let story = GameTags { tags: vec![(id("story-driven"), 1), (id("grinding"), 2)], text_heavy: Some(true), confidence: 2, ..Default::default() };
        let p = Path::new("/roms/psx/x.chd");
        let f = features_for(game("psx", p), Some(&info(&[], &[])), &feel_tags(v, &story));
        let facet = |x: Facet| f.facets.iter().filter(|(y, _, _)| *y == x).map(|(_, k, _)| k.as_str()).collect::<Vec<_>>();
        assert_eq!(facet(Facet::Story), [SOME_STORY.0], "weight 1: some story");
        assert_eq!(facet(Facet::Grinding), ["grinding"]);
        assert_eq!(facet(Facet::Reading), ["text-heavy"]);
        assert!(f.terms.iter().any(|t| t.key == "t:story-driven"), "the tag still counts for Echoes");
        let driven = GameTags { tags: vec![(id("story-driven"), 2)], confidence: 1, ..Default::default() };
        let f = features_for(game("psx", p), Some(&info(&[], &[])), &feel_tags(v, &driven));
        assert!(f.facets.iter().any(|(x, k, l)| *x == Facet::Story && k == "story-driven" && l == "Story-driven"), "weight 2 at 70%: still story-driven");
        assert!(!f.facets.iter().any(|(x, k, _)| *x == Facet::Story && k == LITTLE_STORY.0));
    }

    #[test]
    fn a_game_without_an_entry_still_has_its_system_and_language() {
        let p = Path::new("/roms/gbc/uCity (USA).gbc");
        let f = features_for(game("gbc", p), None, &[]);
        assert_eq!((f.qid.as_deref(), f.work.as_deref()), (None, None));
        assert!(f.terms.is_empty());
        assert!(f.facets.iter().any(|(x, k, _)| *x == Facet::System && k == "gbc"));
        assert!(f.facets.iter().any(|(x, k, _)| *x == Facet::Language && k == "en"));
    }
}
