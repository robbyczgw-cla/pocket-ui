//! The interface in another language (German for now): Settings → Language, docs/DEVELOPING.md "Translations".
//!
//! Nothing in the rest of the code is wrapped for translation. The text painter (`Text::draw`, `width`, `fit`, `wrap`,
//! `draw_right`, `draw_clipped`) looks every string it is given up here, by its exact English wording, and paints the
//! translation instead. So the hint bar, toasts, notices, dialogs, page titles and rows are translated wherever they
//! are drawn, and a new page needs no code for it: only lines in `assets/i18n/de.tsv`. One exception: pocket-core's
//! FPS + sensors line is put together from parts whose numbers change twice a second, so it looks its words up itself
//! (`osd_sensors::Sensors::text_in`).
//!
//! The table has two kinds of lines (`English<TAB>German`):
//! - exact ones: `Settings<TAB>Einstellungen`, looked up in a hash map (no allocation, about 30 ns; `examples/i18n_cost.rs`);
//! - patterns for formatted strings, with placeholders: `{n}` (also `{m}`, `{k}`, `{n2}`...) matches a number,
//!   `{s}` any text that is kept as it is (a game, a network), `{t}` any text that is translated in turn (a value
//!   from the table: "Set to {t}" turns "Set to Dark" into "Auf Dunkel gestellt"). The German side may use them in
//!   any order. A pattern's result is kept in the painter's cache (`Cache`), so a string seen in the last frame costs
//!   one hash lookup and no allocation.
//!
//! The painter can't tell a name from a sentence by looking at it: a game called "Black" is the word the Look page
//! translates, "Time to Play" fits the pattern "{t} to {t2}". So the places that make names say so: a game's name is
//! handed to [`learn_names`] where the library, the play history, the recents, the gallery and the hidden-games list
//! make it, and a learned name is never looked up (whole, or as a part of a line of parts). The price is a UI word
//! that is also a game's name stays English for someone who owns that game; the other way round, every owner of
//! "Black" saw "Schwarz" on the shelf.
//!
//! What isn't in the table is painted as it is (English, or a name). `POCKETUI_I18N_MISSES=<file>` writes every
//! string that had no translation (and every translation made) into that file; `tests/i18n_snapshots.rs` renders the
//! snapshot states in German with it and fails on one that is neither translated nor in `assets/i18n/keep.txt`.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

/// A language of the interface. Its name is always shown in itself ("Deutsch", never "German").
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    De,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::De];

    /// The language's own name, for the Settings row and the welcome step; never translated.
    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::De => "Deutsch",
        }
    }

    /// The next language (A or Left/Right on the row).
    pub fn next(self) -> Lang {
        Lang::ALL[(Lang::ALL.iter().position(|l| *l == self).unwrap_or(0) + 1) % Lang::ALL.len()]
    }

    /// The language of a locale name as ROCKNIX writes it (`system.language=de_DE`, `LANG=de_AT.UTF-8`); English for
    /// any other.
    pub fn from_locale(locale: &str) -> Lang {
        let l = locale.trim().to_ascii_lowercase();
        if l == "de" || l.starts_with("de_") || l.starts_with("de-") || l.starts_with("de.") {
            Lang::De
        } else {
            Lang::En
        }
    }

    /// The start value when settings.json has no language: the system's, as EmulationStation set it. The launcher
    /// passes `system.language` on as `LANG` (deploy/run-pocket-ui.sh), so system.cfg is read only without one.
    pub fn system_default() -> Lang {
        Lang::from_system(std::env::var("LANG").ok().as_deref(), || crate::system_cfg::get("system.language"))
    }

    /// `system_default` without the device: `LANG` when it is set, else `system.language`, else English. A fresh
    /// install with neither, or with any language but German, starts in English.
    fn from_system(lang: Option<&str>, system_language: impl FnOnce() -> Option<String>) -> Lang {
        match lang.filter(|l| !l.is_empty()) {
            Some(lang) => Lang::from_locale(lang),
            None => system_language().map_or(Lang::En, |l| Lang::from_locale(&l)),
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

/// Switches the whole interface at once: the next frame is painted in it (no restart, nothing to reload).
/// `POCKETUI_LANG` (`de` or `en`) overrides it, for snapshots and tests.
pub fn set(lang: Lang) {
    let lang = std::env::var("POCKETUI_LANG").ok().map_or(lang, |l| Lang::from_locale(&l));
    LANG.store(lang as u8, Ordering::Relaxed);
    // The table is read (1.3 ms on the build computer, `examples/i18n_cost.rs`) on a thread of its own, so the
    // first frame doesn't wait for it; one painted before it is ready waits only for what is left.
    if lang == Lang::De && TABLE.get().is_none() {
        std::thread::spawn(german);
    }
}

pub fn current() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::De as u8 {
        Lang::De
    } else {
        Lang::En
    }
}

/// The German table, compiled in.
const DE: &str = include_str!("../assets/i18n/de.tsv");

/// FNV-1a: the keys are short, and SipHash's protection against crafted keys buys nothing here.
#[derive(Default, Clone, Copy)]
pub struct Fnv(u64);

impl Hasher for Fnv {
    fn write(&mut self, bytes: &[u8]) {
        let mut h = if self.0 == 0 { 0xcbf2_9ce4_8422_2325 } else { self.0 };
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        self.0 = h;
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<Fnv>>;
type Memo = RefCell<HashMap<String, Option<Made>>>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Hole {
    /// A number: digits, with `.` or `,` between them, a sign in front.
    Num,
    /// Any text, kept.
    Keep,
    /// Any text, itself looked up in the table.
    Tr,
}

#[derive(Debug)]
enum Seg {
    Lit(&'static str),
    Hole(Hole, &'static str),
}

#[derive(Debug)]
struct Pattern {
    segs: Vec<Seg>,
    out: Vec<Seg>,
    /// The longest literal: a string without it can't match (a quick `contains` before the real match).
    anchor: &'static str,
    /// Words of its own ("{n} games"), not only parts to translate ("‹ {t} ›").
    wordy: bool,
    /// Has a {t} hole.
    has_tr: bool,
    /// When no pattern fits with every {t} known, every {t} of it may take any text: it has words of its own or the
    /// " · " of a line of parts ("13 games · Nintendo · 1990"). Not "{t}, {t2}": a game called "Pokémon Red, Blue,
    /// and Yellow" would come out as "Pokémon Red, Blau, …"; there only the first part may stay as it is.
    loose: bool,
    /// `!` in the table: only when every {t} is a piece the table knows.
    careful: bool,
    /// Ends in a {s} glued to a word ("Looking{s}"): the dots of a busy line, never more words ("Looking at
    /// file sharing…" is not "Looking" and something kept).
    glued_tail: bool,
    /// The length of its text without the holes: a shorter string can't match.
    min_len: usize,
}

pub struct Table {
    exact: FastMap<&'static str, &'static str>,
    patterns: Vec<Pattern>,
}

fn hole_kind(name: &str) -> Option<Hole> {
    match name.chars().next()? {
        'n' | 'm' | 'k' => Some(Hole::Num),
        's' => Some(Hole::Keep),
        't' => Some(Hole::Tr),
        _ => None,
    }
}

/// `"{n} of {m} achievements"` → literal and placeholder segments; `Err` for a brace that isn't a placeholder.
fn segments(s: &'static str) -> Result<Vec<Seg>, String> {
    let mut segs = Vec::new();
    let mut rest = s;
    while let Some(open) = rest.find('{') {
        if open > 0 {
            segs.push(Seg::Lit(&rest[..open]));
        }
        let close = rest[open..].find('}').ok_or_else(|| format!("unclosed {{ in {s:?}"))? + open;
        let name = &rest[open + 1..close];
        let kind = hole_kind(name).filter(|_| name.chars().all(|c| c.is_ascii_alphanumeric())).ok_or_else(|| format!("{{{name}}} in {s:?} is not a placeholder ({{n}}, {{s}}, {{t}})"))?;
        segs.push(Seg::Hole(kind, name));
        rest = &rest[close + 1..];
    }
    if !rest.is_empty() {
        segs.push(Seg::Lit(rest));
    }
    Ok(segs)
}

fn unescape(s: &'static str) -> &'static str {
    if s.contains('\\') {
        Box::leak(s.replace("\\n", "\n").replace("\\t", "\t").into_boxed_str())
    } else {
        s
    }
}

impl Table {
    /// Reads a table; the errors are its lines that don't parse (tested for the shipped table).
    pub fn parse(src: &'static str) -> (Table, Vec<String>) {
        let mut exact = FastMap::default();
        let mut patterns = Vec::new();
        let mut errors = Vec::new();
        for (i, line) in src.lines().enumerate() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((en, de)) = line.split_once('\t') else {
                errors.push(format!("line {}: no tab: {line:?}", i + 1));
                continue;
            };
            let (en, de) = (unescape(en.trim()), unescape(de.trim_start_matches('\t').trim()));
            // `!` in front: a pattern used only when every {t} is a piece the table knows ("!{t} and {t2}" would
            // otherwise make "Rock und Roll Racing" of a game's name, "!in {t}" "im Menüs" of German text).
            let (en, careful) = match en.strip_prefix('!') {
                Some(rest) => (rest, true),
                None => (en, false),
            };
            if en.is_empty() || de.is_empty() {
                errors.push(format!("line {}: an empty side: {line:?}", i + 1));
                continue;
            }
            if !en.contains('{') {
                if exact.insert(en, de).is_some() {
                    errors.push(format!("line {}: {en:?} is in the table twice", i + 1));
                }
                continue;
            }
            match (segments(en), segments(de)) {
                (Ok(segs), Ok(out)) => {
                    let names = |v: &[Seg]| {
                        let mut n: Vec<&str> = v.iter().filter_map(|s| if let Seg::Hole(_, n) = s { Some(*n) } else { None }).collect();
                        n.sort();
                        n
                    };
                    // A number ends by itself; text before another placeholder has no end.
                    let adjacent = segs.windows(2).any(|w| matches!(w, [Seg::Hole(Hole::Keep | Hole::Tr, _), Seg::Hole(..)]));
                    let mut unique = names(&segs);
                    unique.dedup();
                    if names(&segs) != names(&out) || unique.len() != names(&segs).len() {
                        errors.push(format!("line {}: the placeholders differ or repeat: {line:?}", i + 1));
                    } else if adjacent {
                        errors.push(format!("line {}: two placeholders side by side can't be told apart: {en:?}", i + 1));
                    } else if !segs.iter().any(|s| matches!(s, Seg::Lit(l) if l.chars().any(char::is_alphabetic)))
                        && segs.iter().any(|s| matches!(s, Seg::Hole(Hole::Num | Hole::Keep, _)))
                        && !segs.iter().any(|s| matches!(s, Seg::Hole(Hole::Tr, _)))
                    {
                        // Without words only one that translates a part: "‹ {t} ›", "{t} · {t}", "{t}, {n}:{m}".
                        errors.push(format!("line {}: a pattern needs words of its own, or only {{t}}: {en:?}", i + 1));
                    } else {
                        let anchor = segs.iter().filter_map(|s| if let Seg::Lit(l) = s { Some(*l) } else { None }).max_by_key(|l| l.len()).unwrap_or("");
                        let wordy = segs.iter().any(|s| matches!(s, Seg::Lit(l) if l.chars().any(char::is_alphabetic)));
                        let has_tr = segs.iter().any(|s| matches!(s, Seg::Hole(Hole::Tr, _)));
                        let loose = has_tr && !careful && (wordy || segs.iter().any(|s| matches!(s, Seg::Lit(l) if l.contains(PARTS.trim()))));
                        let min_len = segs.iter().map(|s| if let Seg::Lit(l) = s { l.len() } else { 1 }).sum();
                        let glued_tail = matches!(segs.as_slice(), [.., Seg::Lit(l), Seg::Hole(Hole::Keep, _)] if l.chars().last().is_some_and(char::is_alphabetic));
                        patterns.push(Pattern { segs, out, anchor, wordy, has_tr, loose, careful, glued_tail, min_len });
                    }
                }
                (Err(e), _) | (_, Err(e)) => errors.push(format!("line {}: {e}", i + 1)),
            }
        }
        // The most specific pattern first: "{n} games you own" before "{n} games".
        patterns.sort_by_key(|p| std::cmp::Reverse(p.segs.iter().map(|s| if let Seg::Lit(l) = s { l.len() } else { 0 }).sum::<usize>()));
        (Table { exact, patterns }, errors)
    }

    pub fn exact(&self, s: &str) -> Option<&'static str> {
        self.exact.get(s).copied()
    }

    pub fn len(&self) -> (usize, usize) {
        (self.exact.len(), self.patterns.len())
    }

    pub fn is_empty(&self) -> bool {
        self.exact.is_empty() && self.patterns.is_empty()
    }

    /// Every entry's English and German side, for the table's own tests.
    pub fn entries(&self) -> Vec<(String, String)> {
        let show = |v: &[Seg]| v.iter().map(|s| match s { Seg::Lit(l) => l.to_string(), Seg::Hole(_, n) => format!("{{{n}}}") }).collect::<String>();
        let mut all: Vec<(String, String)> = self.exact.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        all.extend(self.patterns.iter().map(|p| (show(&p.segs), show(&p.out))));
        all
    }

    /// The translation of `s`: exact, then the patterns (and `s` trimmed, with its spaces put back). `None`: not in
    /// the table.
    pub fn translate(&self, s: &str) -> Option<Cow<'static, str>> {
        self.translate_at(s, 0, &RefCell::default()).map(|m| m.text)
    }

    /// `translate`, and the parts of a pattern's result it left in English (a value the table doesn't have).
    pub fn translate_with_parts(&self, s: &str) -> Option<(Cow<'static, str>, Vec<String>)> {
        self.translate_at(s, 0, &RefCell::default()).map(|m| (m.text, m.left))
    }

    /// `memo`: what the patterns made of each piece already tried in this look-up, so a list of parts ("{t}, {t2}",
    /// one part a level) is split once, not once per way of trying it.
    fn translate_at(&self, s: &str, depth: u8, memo: &Memo) -> Option<Made> {
        // A part of a line ("Black · Game Boy") that is a game's name stays as it is (`learn_names`). The whole string
        // is the painter's to ask (`Cache::get`).
        if depth > 0 && is_name(s) {
            return None;
        }
        if let Some(t) = self.exact(s) {
            return Some(Made::of(Cow::Borrowed(t)));
        }
        let trimmed = s.trim();
        if trimmed.len() != s.len() && !trimmed.is_empty() {
            let at = s.find(trimmed).unwrap_or(0);
            let inner = self.translate_at(trimmed, depth, memo)?;
            return Some(Made { text: Cow::Owned(format!("{}{}{}", &s[..at], inner.text, &s[at + trimmed.len()..])), left: inner.left });
        }
        // "Arcade racing" first in a line, "arcade racing" in the table (Curator's vocabulary): the same word, capitalised.
        let mut chars = s.chars();
        if let Some(first) = chars.next().filter(|c| c.is_uppercase()) {
            let lower: String = first.to_lowercase().chain(chars.clone()).collect();
            if let Some(t) = self.exact(&lower) {
                let mut t_chars = t.chars();
                let capital: String = t_chars.next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default();
                return Some(Made::of(Cow::Owned(capital + t_chars.as_str())));
            }
        }
        // "{t} · {t2}" splits a line of parts one part at a time: a list of up to 24.
        if depth > 24 {
            return None;
        }
        if let Some(done) = memo.borrow().get(s) {
            return done.clone();
        }
        // Patterns whose every {t} the table knows, then whole sentences, then patterns with any {t}.
        let made = self.by_pattern(s, depth, memo, true).or_else(|| self.by_sentences(s, depth, memo)).or_else(|| self.by_pattern(s, depth, memo, false));
        memo.borrow_mut().insert(s.to_string(), made.clone());
        made
    }

    /// A paragraph put together from sentences the table has ("{explanation} {note}"): the longest run of whole
    /// sentences at its start that translates cleanly, then the rest the same way.
    fn by_sentences(&self, s: &str, depth: u8, memo: &Memo) -> Option<Made> {
        // After ". " and after ".) " ("…(InternalResolution 4 is 2×.) Edges…").
        let mut ends: Vec<usize> = s.match_indices(". ").map(|(i, _)| i + 1).chain(s.match_indices(".) ").map(|(i, _)| i + 2)).collect();
        ends.sort_unstable();
        for &end in ends.iter().rev() {
            let (head, rest) = (&s[..end], &s[end..]);
            let gap = rest.len() - rest.trim_start().len();
            let Some(first) = self.translate_at(head, depth + 1, memo).filter(|m| m.left.is_empty()) else { continue };
            if let Some(then) = self.translate_at(&rest[gap..], depth + 1, memo) {
                return Some(Made { text: Cow::Owned(format!("{}{}{}", first.text, &rest[..gap], then.text)), left: then.left });
            }
        }
        None
    }

    /// `strict`: every {t} has to be a piece the table knows, so "{t} {t2}" splits two sentences where the first
    /// ends, not at the first space. Not strict: a {t} takes any text, for a value the table doesn't have ("Set to
    /// Fuchsia"; `Pattern::loose`); only the patterns with a {t} are tried again.
    fn by_pattern(&self, s: &str, depth: u8, memo: &Memo, strict: bool) -> Option<Made> {
        let mut caps: Vec<(&'static str, &str)> = Vec::new();
        // "Known" is a piece the table translates whole, not one a pattern could only half translate: "a, picture and sound
        // details, b" is not "{t} and {t2}" with "sound details, b" left in English.
        let known = |_: usize, v: &str| self.translate_at(v, depth + 1, memo).is_some_and(|m| m.left.is_empty());
        let any = |_: usize, _: &str| true;
        // A list split at commas may leave its first part as it is ("PlayStation 2, completed"), no other: in
        // "Pokémon Red, Blue, and Yellow" the "Blue" is part of a name.
        let head = |hole: usize, v: &str| hole == 0 || self.translate_at(v, depth + 1, memo).is_some_and(|m| m.left.is_empty());
        // Only the patterns that can fit (most are ruled out by length and their longest piece of text).
        for p in self.patterns.iter().filter(|p| (strict || (p.has_tr && !p.careful)) && p.min_len <= s.len() && s.contains(p.anchor)) {
            caps.clear();
            let check: &dyn Fn(usize, &str) -> bool = match (strict, p.loose) {
                (true, _) => &known,
                (false, true) => &any,
                (false, false) => &head,
            };
            if match_segs(&p.segs, s, &mut caps, check) && !(p.glued_tail && caps.last().is_some_and(|(_, v)| v.chars().any(char::is_alphabetic))) {
                let mut out = String::with_capacity(s.len() + 16);
                let mut translated_part = false;
                let mut left = Vec::new();
                for seg in &p.out {
                    match seg {
                        Seg::Lit(l) => out.push_str(l),
                        Seg::Hole(kind, name) => {
                            let v = caps.iter().find(|(n, _)| n == name).map_or("", |(_, v)| *v);
                            match kind {
                                Hole::Tr => match self.translate_at(v, depth + 1, memo) {
                                    Some(t) => {
                                        // A line that keeps its word ("LEDs", "Sticker") changes nothing.
                                        translated_part |= t.text != v;
                                        out.push_str(&t.text);
                                        left.extend(t.left);
                                    }
                                    None => {
                                        left.push(v.to_string());
                                        out.push_str(v);
                                    }
                                },
                                _ => out.push_str(v),
                            }
                        }
                    }
                }
                // "‹ {t} ›" and "{t} · {t2}" have no words of their own: one of their parts has to be translated, or
                // all of them be lines that keep their word ("‹ Minimal ›"). Otherwise it is German already ("Sticker,
                // auf Home", which `Text::fit` gave back to be drawn) or not in the table at all.
                if !p.wordy && !translated_part && !left.is_empty() {
                    continue;
                }
                return Some(Made { text: Cow::Owned(out), left });
            }
        }
        None
    }
}

/// A translation and the parts of it left in English.
#[derive(Clone)]
struct Made {
    text: Cow<'static, str>,
    left: Vec<String>,
}

impl Made {
    fn of(text: Cow<'static, str>) -> Made {
        Made { text, left: Vec::new() }
    }
}

/// The length of the number at the start of `s` (0: none).
fn number_len(s: &str) -> usize {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }
    let digits_from = i;
    while i < b.len() {
        // A digit, or a `.` or `,` between digits ("1,204", "60.00").
        let between = (b[i] == b'.' || b[i] == b',') && i > digits_from && i + 1 < b.len() && b[i + 1].is_ascii_digit();
        if b[i].is_ascii_digit() || between {
            i += 1;
        } else {
            break;
        }
    }
    if i == digits_from {
        0
    } else {
        i
    }
}

/// The separator of a line of parts ("13 games · Nintendo · 1990"). A {s} never takes it in: "{s} played" is the
/// last part of "No save states · last played 40 days ago · 9 h 00 min played", not all of it.
const PARTS: &str = " · ";

/// A {s} is a value (a name, a size): never a line of parts or more than one sentence. "The panel shows {n} pictures a
/// second at {s}." must not take the rest of a paragraph as its {s}.
///
/// Not a name of a game (`learn_names`): "WarioWare, Inc. - Mega Microgame$!", "Dr. Mario" and "Super Mario Bros. 3"
/// are one value although they hold ". ", and a line of parts holds no name as a whole.
fn kept_spans_too_much(v: &str) -> bool {
    (v.contains(PARTS) || v.contains(". ")) && !is_name(v)
}

/// Matches `s` against a pattern's segments, filling `caps`; a {t} hole takes only text `tr_ok` accepts (given the
/// hole's place among the pattern's holes).
fn match_segs<'s>(segs: &[Seg], s: &'s str, caps: &mut Vec<(&'static str, &'s str)>, tr_ok: &dyn Fn(usize, &str) -> bool) -> bool {
    match segs.first() {
        None => s.is_empty(),
        Some(Seg::Lit(l)) => s.strip_prefix(l).is_some_and(|rest| match_segs(&segs[1..], rest, caps, tr_ok)),
        Some(Seg::Hole(Hole::Num, name)) => {
            let n = number_len(s);
            if n == 0 {
                return false;
            }
            caps.push((name, &s[..n]));
            if match_segs(&segs[1..], &s[n..], caps, tr_ok) {
                return true;
            }
            caps.pop();
            false
        }
        Some(Seg::Hole(kind, name)) => match segs.get(1) {
            None => {
                let ok = !s.trim().is_empty() && (*kind != Hole::Tr || tr_ok(caps.len(), s)) && (*kind != Hole::Keep || !kept_spans_too_much(s));
                if ok {
                    caps.push((name, s));
                }
                ok
            }
            Some(Seg::Lit(next)) => {
                for (at, _) in s.match_indices(next) {
                    if at == 0 || s[..at].trim().is_empty() || (*kind == Hole::Tr && !tr_ok(caps.len(), &s[..at])) || (*kind == Hole::Keep && kept_spans_too_much(&s[..at])) {
                        continue;
                    }
                    caps.push((name, &s[..at]));
                    if match_segs(&segs[1..], &s[at..], caps, tr_ok) {
                        return true;
                    }
                    caps.pop();
                }
                false
            }
            // Ruled out by `parse`.
            Some(Seg::Hole(..)) => false,
        },
    }
}

static TABLE: OnceLock<Table> = OnceLock::new();

type NameSet = HashSet<Box<str>, BuildHasherDefault<Fnv>>;

fn names() -> &'static RwLock<NameSet> {
    static NAMES: OnceLock<RwLock<NameSet>> = OnceLock::new();
    NAMES.get_or_init(Default::default)
}

/// Bumped when a name is learned that was not known: a painter's cache made before it is made again.
static NAMES_SEEN: AtomicU32 = AtomicU32::new(0);

/// Marks `names` (games as the shelf names them) as names: from now on the painter paints them as they are, in every
/// language, whatever the table has for the same words. Called where names are made (`library::load`,
/// `library_cache::read`, `library::shelf_name`); cheap and safe to call again with the same names.
pub fn learn_names<I>(new: I)
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut fresh: Vec<Box<str>> = Vec::new();
    {
        let Ok(known) = names().read() else { return };
        for n in new {
            let n = n.as_ref();
            if n.bytes().any(|b| b.is_ascii_alphabetic()) && !known.contains(n) {
                fresh.push(n.into());
            }
        }
    }
    if fresh.is_empty() {
        return;
    }
    if let Ok(mut known) = names().write() {
        known.extend(fresh);
        NAMES_SEEN.fetch_add(1, Ordering::Relaxed);
    }
}

/// `s` was learned as a name.
pub fn is_name(s: &str) -> bool {
    NAMES_SEEN.load(Ordering::Relaxed) != 0 && names().read().is_ok_and(|n| n.contains(s))
}

/// The German table, read on first use (`set` starts that on a thread).
pub fn german() -> &'static Table {
    TABLE.get_or_init(|| {
        let (table, errors) = Table::parse(DE);
        for e in &errors {
            eprintln!("i18n: de.tsv {e}");
        }
        table
    })
}

/// What the painter draws for a string: the string itself, a line of the table, or a pattern's result.
pub enum Tr<'a> {
    Same(&'a str),
    Static(&'static str),
    Shared(Arc<str>),
}

impl std::ops::Deref for Tr<'_> {
    type Target = str;
    fn deref(&self) -> &str {
        match self {
            Tr::Same(s) => s,
            Tr::Static(s) => s,
            Tr::Shared(s) => s,
        }
    }
}

/// Pattern results and strings without a translation, per painter. Game names scroll by in the thousands, so it is
/// emptied when it gets large; what the next frame paints is back after one look-up each.
#[derive(Default)]
pub struct Cache {
    seen: FastMap<Box<str>, Option<Arc<str>>>,
    /// Strings that are German already (`is_german`), until they are painted.
    german: FastMap<Box<str>, ()>,
    /// How many strings ran the table's patterns (their first sight): the benchmark of Settings' first frame counts
    /// the ones that frame made.
    patterned: usize,
    /// `NAMES_SEEN` when `seen` was filled.
    names_seen: u32,
}

const CACHE_LIMIT: usize = 4096;

impl Cache {
    /// `s` in the language of the interface. English: `s`, at the cost of one atomic load.
    pub fn get<'a>(&mut self, s: &'a str) -> Tr<'a> {
        if current() == Lang::En || !s.bytes().any(|b| b.is_ascii_alphabetic()) {
            return Tr::Same(s);
        }
        let table = german();
        let names_seen = NAMES_SEEN.load(Ordering::Relaxed);
        if names_seen != self.names_seen {
            // A name learned since: what was made of a string before it may have been made of a name.
            self.seen.clear();
            self.names_seen = names_seen;
        }
        if let Some(t) = table.exact(s) {
            // A language on its own is shown in its own name (Settings → Language, the welcome step): "English".
            // In a list of a game's languages it is a part of a line and translated ("Englisch, Französisch").
            if Lang::ALL.iter().any(|l| l.name() == s) {
                return Tr::Same(s);
            }
            // A game called like a word of the interface ("Black"): its name.
            if is_name(s) {
                record(s, None);
                return Tr::Same(s);
            }
            record(s, Some(t));
            return Tr::Static(t);
        }
        if let Some(hit) = self.seen.get(s) {
            return match hit {
                Some(t) => Tr::Shared(t.clone()),
                None => Tr::Same(s),
            };
        }
        // A line `Text::fit` or `Text::wrap` made of a translation: German already, painted as it is.
        if self.german.remove(s).is_some() {
            record(s, None);
            if self.seen.len() >= CACHE_LIMIT {
                self.seen.clear();
            }
            self.seen.insert(s.into(), None);
            return Tr::Same(s);
        }
        // A game's name, whatever pattern it would fit ("Time to Play" is not "{t} to {t2}").
        if is_name(s) {
            record(s, None);
            if self.seen.len() >= CACHE_LIMIT {
                self.seen.clear();
            }
            self.seen.insert(s.into(), None);
            return Tr::Same(s);
        }
        // A "translation" that changes nothing is German already (a line `Text::fit` gave back, a wrapped line):
        // painted as it is, and its parts are not English left over.
        self.patterned += 1;
        let t: Option<Arc<str>> = table.translate_with_parts(s).filter(|(t, _)| t.as_ref() != s).map(|(t, left)| {
            // A part left in English inside the translation: the snapshot test checks it as a string of its own.
            for part in &left {
                note('P', part);
            }
            Arc::from(t.as_ref())
        });
        record(s, t.as_deref());
        if self.seen.len() >= CACHE_LIMIT {
            self.seen.clear();
        }
        self.seen.insert(s.into(), t.clone());
        match t {
            Some(t) => Tr::Shared(t),
            None => Tr::Same(s),
        }
    }

    /// `s` is German already: a translation `Text::fit` gave back, cut short or not, or a line `Text::wrap` made of
    /// one. Painted next, it is drawn as it is. Without this its first sight ran every pattern of the table over it to
    /// find that none changes it (0.3 ms for a sentence on the build computer, a frame's worth on the Mini V2, for
    /// each line of Settings' categories).
    pub fn is_german(&mut self, s: &str) {
        if current() == Lang::En || self.seen.contains_key(s) || self.german.contains_key(s) {
            return;
        }
        if self.german.len() >= CACHE_LIMIT {
            self.german.clear();
        }
        self.german.insert(s.into(), ());
    }

    /// How many strings have run the table's patterns so far.
    pub fn patterns_run(&self) -> usize {
        self.patterned
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

/// `POCKETUI_I18N_MISSES`: the file the snapshot checks read, each line once: `M` a string painted without a
/// translation, `P` a part of a pattern's result left in English, `O` a translation made, `C` a string `Text::fit`
/// cut short with "…", `X` one that ran past the picture's right edge (the last two by their English, so a German
/// and an English run compare).
struct Recorder {
    path: String,
    written: HashSet<(char, String)>,
}

fn recorder() -> Option<&'static Mutex<Recorder>> {
    static REC: OnceLock<Option<Mutex<Recorder>>> = OnceLock::new();
    REC.get_or_init(|| std::env::var("POCKETUI_I18N_MISSES").ok().filter(|p| !p.is_empty()).map(|path| Mutex::new(Recorder { path, written: HashSet::new() }))).as_ref()
}

fn note(kind: char, line: &str) {
    let Some(rec) = recorder() else { return };
    let Ok(mut rec) = rec.lock() else { return };
    if line.contains('\n') || !rec.written.insert((kind, line.to_string())) {
        return;
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&rec.path) {
        let _ = writeln!(f, "{kind}\t{line}");
    }
}

fn record(s: &str, t: Option<&str>) {
    match t {
        Some(t) => note('O', t),
        None => note('M', s),
    }
}

/// For the snapshot checks: `Text::fit` cut `english` short.
pub fn note_cut(english: &str) {
    note('C', english);
}

/// For the snapshot checks: `english` was drawn past the right edge of the picture.
pub fn note_overflow(english: &str) {
    note('X', english);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(src: &'static str) -> Table {
        let (t, errors) = Table::parse(src);
        assert!(errors.is_empty(), "{errors:?}");
        t
    }

    #[test]
    fn the_shipped_table_parses_cleanly() {
        let (t, errors) = Table::parse(DE);
        assert!(errors.is_empty(), "assets/i18n/de.tsv:\n{}", errors.join("\n"));
        assert!(!t.is_empty());
    }

    /// VDEV-ROUND-2026-10-25: "{s} of {s2}" (for "56.1 GB of 63.1 GB") made "The Legend von Zelda" of every game
    /// name with "of" in it, on the shelf and the card. The names `assets/i18n/keep.txt` keeps in English (games,
    /// systems, emulators) come out of the shipped table as they are.
    #[test]
    fn the_names_kept_in_english_stay_english() {
        let (t, _) = Table::parse(DE);
        let keep = include_str!("../assets/i18n/keep.txt");
        let changed: Vec<String> = keep
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#') && !l.starts_with("re:"))
            // Words the table translates where they are no name (the Lens's language filter, a Curator tag).
            .filter(|l| !["English", "‹ English ›", "puzzle"].contains(l))
            .filter_map(|name| t.translate(name).filter(|g| g != name).map(|g| format!("{name:?} -> {g:?}")))
            .collect();
        assert!(changed.is_empty(), "{} names the table changes:\n{}", changed.len(), changed.join("\n"));
        for name in ["Castlevania - Symphony of the Night", "Secret of Mana", "Street Fighter II' is the first update of Street Fighter"] {
            assert_eq!(t.translate(name), None, "{name}");
        }
        assert_eq!(t.translate("56.1 GB of 63.1 GB").as_deref(), Some("56.1 GB von 63.1 GB"), "the card's sizes still are");
    }

    #[test]
    fn a_german_line_is_never_looked_up_as_english_again() {
        // `Text::fit` returns the German, which its caller then draws: it must come back as it is.
        let (t, _) = Table::parse(DE);
        for (en, de) in t.entries() {
            if !en.contains('{') {
                assert!(t.translate(&de).is_none_or(|again| again == de), "{en:?} -> {de:?} -> {:?}", t.translate(&de));
            }
        }
    }

    #[test]
    fn a_broken_line_is_left_out_and_the_rest_works() {
        let (t, errors) = Table::parse("Settings\tEinstellungen\nno tab here\n\tonly German\nonly English\t\n{n games\tSpiele\nBack\tZurück\n");
        assert_eq!(errors.len(), 4, "{errors:?}");
        assert_eq!(t.translate("Settings").as_deref(), Some("Einstellungen"));
        assert_eq!(t.translate("Back").as_deref(), Some("Zurück"));
        assert_eq!(t.translate("only English"), None, "painted in English, never empty");
        let (empty, _) = Table::parse("");
        assert_eq!(empty.translate("Settings"), None, "no table: English");
    }

    #[test]
    fn exact_lines_and_patterns() {
        let t = table("Settings\tEinstellungen\n{n} games you own\t{n} Spiele auf dem Gerät\n{n} games\t{n} Spiele\n{n} of {m} achievements\t{n} von {m} Erfolgen\nDark\tDunkel\nSet to {t}\tAuf {t} gestellt\nConnected to {s}\tVerbunden mit {s}\n");
        assert_eq!(t.translate("Settings").as_deref(), Some("Einstellungen"));
        assert_eq!(t.translate("12 games you own").as_deref(), Some("12 Spiele auf dem Gerät"));
        assert_eq!(t.translate("1,204 games").as_deref(), Some("1,204 Spiele"));
        assert_eq!(t.translate("3 of 41 achievements").as_deref(), Some("3 von 41 Erfolgen"));
        assert_eq!(t.translate("Set to Dark").as_deref(), Some("Auf Dunkel gestellt"));
        assert_eq!(t.translate("Set to Fuchsia").as_deref(), Some("Auf Fuchsia gestellt"), "an unknown value is kept");
        assert_eq!(t.translate("Connected to Dark").as_deref(), Some("Verbunden mit Dark"), "{{s}} is never translated");
        assert_eq!(t.translate("  Settings ").as_deref(), Some("  Einstellungen "));
        assert_eq!(t.translate("many games"), None, "{{n}} needs a number");
        assert_eq!(t.translate("Super Mario World"), None);
    }

    #[test]
    fn placeholders_can_change_places() {
        let t = table("{n} of {m} left\tnoch {m} von {n}\n{s} on {t}\t{t}: {s}\nSettings\tEinstellungen\n");
        assert_eq!(t.translate("2 of 9 left").as_deref(), Some("noch 9 von 2"));
        assert_eq!(t.translate("Wi-Fi on Settings").as_deref(), Some("Einstellungen: Wi-Fi"));
    }

    #[test]
    fn two_sentences_split_where_the_first_ends() {
        let t = table("Soft blends pixels.\tWeich mischt Pixel.\nAt {n}× whole it would be sharp.\tBei ganzen {n}× wäre es scharf.\nDrawn {n} times as large. {t} {t2}\t{n}-mal so groß. {t} {t2}\nSet to {t}\tAuf {t} gestellt\n");
        assert_eq!(
            t.translate("Drawn 2.5 times as large. Soft blends pixels. At 3× whole it would be sharp.").as_deref(),
            Some("2.5-mal so groß. Weich mischt Pixel. Bei ganzen 3× wäre es scharf.")
        );
        assert_eq!(t.translate("Set to Fuchsia").as_deref(), Some("Auf Fuchsia gestellt"), "still: an unknown value is kept");
    }

    #[test]
    fn a_long_list_is_translated_part_by_part() {
        let names = ["Lens", "Echoes", "Up next", "Versions", "Stickers", "Pixel Path", "Frames", "Last game", "Motion", "Sounds", "Haptics", "LEDs"];
        let mut src = String::from("On: {t}.\tAn: {t}.\n{t}, {t2}\t{t}, {t2}\n");
        for (i, n) in names.iter().enumerate() {
            src.push_str(&format!("{n}\tW{i}\n"));
        }
        let t = table(Box::leak(src.into_boxed_str()));
        let german: Vec<String> = (0..names.len()).map(|i| format!("W{i}")).collect();
        assert_eq!(t.translate(&format!("On: {}.", names.join(", "))).as_deref(), Some(format!("An: {}.", german.join(", ")).as_str()));
    }

    #[test]
    fn a_paragraph_of_known_sentences_is_translated_sentence_by_sentence() {
        let t = table("The game runs {n} % faster. It keeps its pitch.\tDas Spiel läuft {n} % schneller. Es behält seine Tonhöhe.\n{n} pictures came too late.\t{n} Bilder kamen zu spät.\n");
        assert_eq!(
            t.translate("The game runs 0.46 % faster. It keeps its pitch. 13 pictures came too late.").as_deref(),
            Some("Das Spiel läuft 0.46 % schneller. Es behält seine Tonhöhe. 13 Bilder kamen zu spät.")
        );
        assert_eq!(t.translate("Dr. Mario"), None);
        assert_eq!(t.translate("The game runs 2 % faster. It keeps its pitch. Super Mario World."), None, "a sentence it doesn't have");
    }

    #[test]
    fn a_name_with_commas_is_not_split_into_words_of_the_table() {
        let t = table("{t}, {t2}\t{t}, {t2}\n{t} · {t2}\t{t} · {t2}\nBlue\tBlau\nEnglish\tEnglisch\nFrench\tFranzösisch\nafter\tdanach\n");
        assert_eq!(t.translate("Pokémon Red, Blue, and Yellow"), None, "a game's name");
        assert_eq!(t.translate("English, French").as_deref(), Some("Englisch, Französisch"), "every part in the table");
        assert_eq!(t.translate("Game Boy Color · after").as_deref(), Some("Game Boy Color · danach"), "a line of parts");
    }

    #[test]
    fn a_careful_pattern_leaves_a_name_alone() {
        let t = table("!{t} and {t2}\t{t} und {t2}\nRock\tFels\nFXAA\tFXAA\nMSAA\tMSAA\nSharpening\tSchärfung\n");
        assert_eq!(t.translate("Rock and Roll Racing"), None);
        assert_eq!(t.translate("FXAA and Sharpening").as_deref(), Some("FXAA und Schärfung"));
    }

    #[test]
    fn dots_glued_to_a_word_are_only_dots() {
        let t = table("Looking for tools{s}\tSuche Tools{s}\n");
        assert_eq!(t.translate("Looking for tools…").as_deref(), Some("Suche Tools…"));
        assert_eq!(t.translate("Looking for tools at home"), None);
    }

    #[test]
    fn bad_lines_are_reported() {
        let (_, errors) = Table::parse("A\tB\nA\tC\nno tab\n{x} y\tz {x}\n{n} a\t{m} b\n{s}{n} c\t{s}{n} d\n{n}\t{n}\n{n}{s} e\t{n}{s} f\n");
        assert_eq!(errors.len(), 6, "{errors:?}");
    }

    #[test]
    fn locales() {
        assert_eq!(Lang::from_locale("de_DE"), Lang::De);
        assert_eq!(Lang::from_locale("de_AT.UTF-8"), Lang::De);
        assert_eq!(Lang::from_locale("de"), Lang::De);
        assert_eq!(Lang::from_locale("en_US"), Lang::En);
        assert_eq!(Lang::from_locale("delta"), Lang::En);
        assert_eq!(Lang::from_locale(""), Lang::En);
    }

    #[test]
    fn english_unless_the_system_says_german() {
        assert_eq!(Lang::from_system(None, || None), Lang::En, "a fresh install: no LANG, no system.language");
        assert_eq!(Lang::from_system(Some(""), || None), Lang::En);
        assert_eq!(Lang::from_system(Some("C.UTF-8"), || Some("de_DE".into())), Lang::En, "LANG comes first");
        assert_eq!(Lang::from_system(Some("fr_FR.UTF-8"), || None), Lang::En);
        assert_eq!(Lang::from_system(None, || Some("es_ES".into())), Lang::En);
        assert_eq!(Lang::from_system(Some(""), || Some("de_DE".into())), Lang::De);
        assert_eq!(Lang::from_system(Some("de_AT.UTF-8"), || None), Lang::De);
        assert_eq!(Lang::default(), Lang::En);
    }
}
