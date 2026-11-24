//! The interface in another language (German, Spanish, Brazilian Portuguese, French): Settings → Language, docs/dev/translations.md "Translations".
//!
//! Nothing in the rest of the code is wrapped for translation. The text painter (`Text::draw`, `width`, `fit`, `wrap`,
//! `draw_right`, `draw_clipped`) looks every string it is given up here, by its exact English wording, and paints the
//! translation instead. So the hint bar, toasts, notices, dialogs, page titles and rows are translated wherever they
//! are drawn, and a new page needs no code for it: only lines in `assets/i18n/de.tsv`, `es.tsv`, `pt.tsv`, `fr.tsv` and `it.tsv`. One exception: pocket-core's
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
//! snapshot states in each language with it and fails on one that is neither translated nor in `assets/i18n/keep.txt`.

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
    Es,
    Pt,
    Fr,
    It,
    /// Korean: the interface's lines come from `ko.tsv`, its letters
    /// from the Korean fonts pocket-ui ships (`text_fallback::korean`).
    Ko,
}

/// What differs by language besides the words. A new language is a line here, one in `Lang`, and its table.
struct Traits {
    /// The start of its locale names (`es_MX.UTF-8`).
    code: &'static str,
    /// In itself.
    name: &'static str,
    /// What the sign-in helper asks a hotel's page for (`portal`).
    accept_language: &'static str,
    /// The game search's keyboard has y and z the German way round.
    qwertz: bool,
    /// What stands between a number's whole part and its decimals ("2,3 W", Korean "2.3 W").
    decimal: char,
    /// Its table has every line the others have (`Lang::complete`): false while a language is being translated; its
    /// missing lines are painted in English, and the checks that every line is there leave it out until it is true.
    complete: bool,
}

/// In the order of `Lang`.
const TRAITS: [Traits; 7] = [
    Traits { code: "en", name: "English", accept_language: "Accept-Language: en,de;q=0.5", qwertz: false, decimal: '.', complete: true },
    Traits { code: "de", name: "Deutsch", accept_language: "Accept-Language: de,en;q=0.8", qwertz: true, decimal: ',', complete: true },
    Traits { code: "es", name: "Español", accept_language: "Accept-Language: es,en;q=0.8", qwertz: false, decimal: ',', complete: true },
    Traits { code: "pt", name: "Português (Brasil)", accept_language: "Accept-Language: pt-BR,pt;q=0.9,en;q=0.7", qwertz: false, decimal: ',', complete: true },
    Traits { code: "fr", name: "Français", accept_language: "Accept-Language: fr-FR,fr;q=0.9,en;q=0.7", qwertz: false, decimal: ',', complete: true },
    Traits { code: "it", name: "Italiano", accept_language: "Accept-Language: it,en;q=0.8", qwertz: false, decimal: ',', complete: true },
    Traits { code: "ko", name: "한국어", accept_language: "Accept-Language: ko-KR,ko;q=0.9,en;q=0.7", qwertz: false, decimal: '.', complete: true },
];

impl Lang {
    pub const ALL: [Lang; 7] = [Lang::En, Lang::De, Lang::Es, Lang::Pt, Lang::Fr, Lang::It, Lang::Ko];

    fn traits(self) -> &'static Traits {
        &TRAITS[self as usize]
    }

    /// The language's own name, for the Settings row and the welcome step; never translated.
    pub fn name(self) -> &'static str {
        self.traits().name
    }

    /// The next language on the row (A or Right; `row`), round to the first after the last.
    pub fn next(self) -> Lang {
        step_in(&Lang::row_with(self), self, true)
    }

    /// The one before (Left on the row).
    pub fn previous(self) -> Lang {
        step_in(&Lang::row_with(self), self, false)
    }

    /// Whether a player is offered it (the welcome's pills, Settings' Language row, Android's first start, the system's
    /// language at a first start): a complete language (`complete`); one still being translated only to a developer,
    /// `POCKETUI_LANG_PREVIEW=1` (every language) or `POCKETUI_I18N_TABLE=<code>:<file>` (that one); Korean was, until it
    /// was translated.
    pub fn offered(self) -> bool {
        static PREVIEW: OnceLock<(bool, Option<String>)> = OnceLock::new();
        let (all, table) = PREVIEW.get_or_init(|| {
            let all = std::env::var("POCKETUI_LANG_PREVIEW").is_ok_and(|v| matches!(v.trim(), "1" | "on" | "true"));
            (all, std::env::var("POCKETUI_I18N_TABLE").ok())
        });
        self.offered_with(*all, table.as_deref())
    }

    /// `offered` with the developer's switches given.
    fn offered_with(self, all: bool, table: Option<&str>) -> bool {
        offered_if(self.complete(), self.traits().code, all, table)
    }

    /// The languages to choose from, in `ALL`'s order: the offered ones, and the one in use if it is not (a developer's
    /// Korean), so the row can always show and leave it.
    pub fn row() -> Vec<Lang> {
        Lang::row_with(current())
    }

    fn row_with(now: Lang) -> Vec<Lang> {
        Lang::ALL.into_iter().filter(|l| l.offered() || *l == now).collect()
    }

    /// The header line a web page is asked in this language with.
    pub fn accept_language(self) -> &'static str {
        self.traits().accept_language
    }

    /// The game search's keyboard is QWERTZ.
    pub fn qwertz(self) -> bool {
        self.traits().qwertz
    }

    /// Its table is complete (`Traits::complete`): what the tests that want a language's own line for every string ask.
    pub fn complete(self) -> bool {
        self.traits().complete
    }

    /// Its table of translations; English has none. Read on first use (`set` starts that on a thread).
    pub fn table(self) -> Option<&'static Table> {
        let (slot, src, file) = self.source()?;
        Some(slot.get_or_init(|| {
            let (src, file) = override_table(std::env::var("POCKETUI_I18N_TABLE").ok(), self).unwrap_or((src, file));
            let (mut table, errors) = Table::parse(src);
            for e in &errors {
                eprintln!("i18n: {file} {e}");
            }
            table.decimal = self.traits().decimal;
            table
        }))
    }

    /// Its table: where it is kept once read, its lines as compiled in, its file's name; English has none.
    fn source(self) -> Option<(&'static OnceLock<Table>, &'static str, &'static str)> {
        Some(match self {
            Lang::En => return None,
            Lang::De => (&GERMAN, DE, "de.tsv"),
            Lang::Es => (&SPANISH, ES, "es.tsv"),
            Lang::Pt => (&PORTUGUESE, PT, "pt.tsv"),
            Lang::Fr => (&FRENCH, FR, "fr.tsv"),
            Lang::It => (&ITALIAN, IT, "it.tsv"),
            Lang::Ko => (&KOREAN, KO, "ko.tsv"),
        })
    }

    /// The language of a locale name as ROCKNIX writes it (`system.language=de_DE`, `LANG=es_MX.UTF-8`) or Android
    /// says it (`es`, `es-419`); English for any other.
    pub fn from_locale(locale: &str) -> Lang {
        let l = locale.trim().to_ascii_lowercase();
        let code = l.split(['_', '-', '.']).next().unwrap_or("");
        Lang::ALL.into_iter().find(|lang| lang.traits().code == code).unwrap_or_default()
    }

    /// The start value when settings.json has no language: the system's, as EmulationStation set it. The launcher
    /// passes `system.language` on as `LANG` (deploy/run-pocket-ui.sh), so system.cfg is read only without one.
    pub fn system_default() -> Lang {
        #[cfg(not(target_os = "android"))]
        let system_language = || crate::system_cfg::get("system.language");
        // Android has no ROCKNIX system.cfg. The Android entry sets the locale via JNI.
        #[cfg(target_os = "android")]
        let system_language = || None;
        Lang::from_system(std::env::var("LANG").ok().as_deref(), system_language)
    }

    /// `system_default` without the device: `LANG` when it is set, else `system.language`, else English. A fresh
    /// install with neither, or with a language pocket-ui doesn't have, starts in English.
    fn from_system(lang: Option<&str>, system_language: impl FnOnce() -> Option<String>) -> Lang {
        match lang.filter(|l| !l.is_empty()) {
            Some(lang) => Lang::from_system_locale(lang),
            None => system_language().map_or(Lang::En, |l| Lang::from_system_locale(&l)),
        }
    }

    /// The language of the system's or the phone's locale as a start value: `from_locale`, but English for one that is not
    /// offered yet (a Korean phone gets English, not a Korean row with English lines).
    pub fn from_system_locale(locale: &str) -> Lang {
        Some(Lang::from_locale(locale)).filter(|l| l.offered()).unwrap_or_default()
    }
}

/// `Lang::offered` of a language with the code `code`, `complete` or not, and the developer's switches.
fn offered_if(complete: bool, code: &str, all: bool, table: Option<&str>) -> bool {
    complete || all || table.and_then(|t| t.split_once(':')).is_some_and(|(c, _)| c == code)
}

/// `POCKETUI_I18N_TABLE=<code>:<file>`: the table of language `code` read from `file` instead of the one compiled in, once
/// per process. For the layout matrix of a language still being translated (`tests/text_size.rs` paints Korean
/// placeholder lines with it) and for a translator who wants to see a table without a build. Not set: nothing changes.
fn override_table(value: Option<String>, lang: Lang) -> Option<(&'static str, &'static str)> {
    let value = value?;
    let (code, path) = value.split_once(':')?;
    if code != lang.traits().code {
        return None;
    }
    match std::fs::read_to_string(path) {
        Ok(src) => Some((Box::leak(src.into_boxed_str()), Box::leak(path.to_string().into_boxed_str()))),
        Err(e) => {
            eprintln!("i18n: POCKETUI_I18N_TABLE {path}: {e}, using the built-in table");
            None
        }
    }
}

/// The language after (`forward`) or before `now` in `row`, round at the ends; the first if `now` is not on it.
fn step_in(row: &[Lang], now: Lang, forward: bool) -> Lang {
    let n = row.len().max(1);
    match row.iter().position(|l| *l == now) {
        Some(i) => row[if forward { (i + 1) % n } else { (i + n - 1) % n }],
        None => row.first().copied().unwrap_or_default(),
    }
}

/// The language of the whole program: the UI thread sets it, `Text`s on several threads read it.
static LANG: AtomicU8 = AtomicU8::new(0);

/// "This thread follows the program's language" in `THREAD_LANG`.
const PROCESS: u8 = u8::MAX;
// Tests run side by side in one process, and a test that switches the language must not change another test's words. A
// thread can have a language of its own (`for_this_thread`); then `set` and `current` on that thread use it and nothing
// else sees it. The library's own unit tests start per thread (English), the way `font_style` does; integration tests,
// where the library is built without `cfg(test)`, take a guard. The program never sets it: one `Cell` read per lookup.
const THREAD_START: u8 = if cfg!(test) { Lang::En as u8 } else { PROCESS };
thread_local! {
    static THREAD_LANG: std::cell::Cell<u8> = const { std::cell::Cell::new(THREAD_START) };
}

/// Switches the whole interface at once: the next frame is painted in it (no restart, nothing to reload).
/// `POCKETUI_LANG` (`de`, `es`, `pt`, `fr`, `it`, `ko` or `en`) overrides it, for snapshots and tests. On a thread with a language of its own
/// (`for_this_thread`) only that thread's language changes.
pub fn set(lang: Lang) {
    let lang = std::env::var("POCKETUI_LANG").ok().map_or(lang, |l| Lang::from_locale(&l));
    if THREAD_LANG.with(|t| t.get()) != PROCESS {
        THREAD_LANG.with(|t| t.set(lang as u8));
    } else {
        LANG.store(lang as u8, Ordering::Relaxed);
    }
    load_soon(lang);
}

/// The table is read (1.3 ms on the build computer, `examples/i18n_cost.rs`) on a thread of its own, so the first frame
/// doesn't wait for it; one painted before it is ready waits only for what is left.
fn load_soon(lang: Lang) {
    if lang != Lang::En {
        std::thread::spawn(move || lang.table());
    }
}

pub fn current() -> Lang {
    let own = THREAD_LANG.with(|t| t.get());
    let v = if own != PROCESS { own } else { LANG.load(Ordering::Relaxed) };
    Lang::ALL.get(v as usize).copied().unwrap_or_default()
}

/// A language for this thread only, until the guard is dropped (also when the test panics): for tests that run side by
/// side in one process. `set` on this thread changes it, other threads keep theirs. Dropping it brings back what the thread
/// had before (a nested guard restores the outer one).
#[must_use = "the thread's language goes back when the guard is dropped"]
pub fn for_this_thread(lang: Lang) -> ThreadLang {
    load_soon(lang);
    ThreadLang(THREAD_LANG.with(|t| t.replace(lang as u8)))
}

/// See `for_this_thread`.
pub struct ThreadLang(u8);

impl Drop for ThreadLang {
    fn drop(&mut self) {
        THREAD_LANG.with(|t| t.set(self.0));
    }
}

/// The tables, compiled in.
#[cfg(not(feature = "brick"))]
const DE: &str = include_str!("../assets/i18n/de.tsv");
#[cfg(feature = "brick")]
const DE: &str = concat!(include_str!("../assets/i18n/de.tsv"), include_str!("../assets/i18n/brick-de.tsv"));
#[cfg(not(feature = "brick"))]
const ES: &str = include_str!("../assets/i18n/es.tsv");
#[cfg(feature = "brick")]
const ES: &str = concat!(include_str!("../assets/i18n/es.tsv"), include_str!("../assets/i18n/brick-es.tsv"));
#[cfg(not(feature = "brick"))]
const PT: &str = include_str!("../assets/i18n/pt.tsv");
#[cfg(feature = "brick")]
const PT: &str = concat!(include_str!("../assets/i18n/pt.tsv"), include_str!("../assets/i18n/brick-pt.tsv"));
#[cfg(not(feature = "brick"))]
const FR: &str = include_str!("../assets/i18n/fr.tsv");
#[cfg(feature = "brick")]
const FR: &str = concat!(include_str!("../assets/i18n/fr.tsv"), include_str!("../assets/i18n/brick-fr.tsv"));
#[cfg(not(feature = "brick"))]
const IT: &str = include_str!("../assets/i18n/it.tsv");
#[cfg(feature = "brick")]
const IT: &str = concat!(include_str!("../assets/i18n/it.tsv"), include_str!("../assets/i18n/brick-it.tsv"));
#[cfg(not(feature = "brick"))]
const KO: &str = include_str!("../assets/i18n/ko.tsv");
#[cfg(feature = "brick")]
const KO: &str = concat!(include_str!("../assets/i18n/ko.tsv"), include_str!("../assets/i18n/brick-ko.tsv"));

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
    /// The language's decimal sign (`Traits::decimal`); a comma for a table read on its own.
    decimal: char,
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
        (Table { exact, patterns, decimal: ',' }, errors)
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

    /// Every entry's English and translated side, for the table's own tests.
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
        // A busy line's moving dots ("Turning Tailscale on.", "..", "..."; the page adds them by the clock): the
        // words are translated, the dots stay. Without this a slow start (a loaded machine) painted the English. A stem
        // the table has only with its dots ("Looking at your games{s}") is left to the patterns below: returning here
        // painted "Looking at your games.." in English two times in three (game check, 14.11.).
        let stem = s.trim_end_matches('.');
        let dots = &s[stem.len()..];
        if (2..=3).contains(&dots.len()) && !stem.is_empty() && !stem.ends_with(char::is_whitespace) {
            if let Some(inner) = self.translate_at(stem, depth, memo) {
                return Some(Made { text: Cow::Owned(format!("{}{dots}", inner.text)), left: inner.left });
            }
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
                                Hole::Num => out.push_str(&decimal_sign(v, self.decimal)),
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

/// A number in a translated line with the language's decimal sign: a comma in German, French and the others ("7,4 Go",
/// "118,1 GB frei"; a device showed "7.4 Go" on 15.11.), a point in Korean, as the number came. Only one point between
/// digits is a decimal point: "0.5.0" (a version) and "1,204" stay as they are.
fn decimal_sign(v: &str, sign: char) -> Cow<'_, str> {
    let b = v.as_bytes();
    match v.find('.') {
        Some(at) if sign != '.' && v.matches('.').count() == 1 && !v.contains(',') && at > 0 && b[at - 1].is_ascii_digit() && b.get(at + 1).is_some_and(u8::is_ascii_digit) => {
            Cow::Owned(format!("{}{sign}{}", &v[..at], &v[at + 1..]))
        }
        _ => Cow::Borrowed(v),
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

static GERMAN: OnceLock<Table> = OnceLock::new();
static SPANISH: OnceLock<Table> = OnceLock::new();
static PORTUGUESE: OnceLock<Table> = OnceLock::new();
static FRENCH: OnceLock<Table> = OnceLock::new();
static ITALIAN: OnceLock<Table> = OnceLock::new();
static KOREAN: OnceLock<Table> = OnceLock::new();

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

/// The German table.
pub fn german() -> &'static Table {
    Lang::De.table().expect("German has a table")
}

/// `s` in the language of the interface, for what isn't painted through `Text`: a part of a line put together in code,
/// a message handed to another program. Whatever the table lacks stays as it is.
pub fn tr(s: &str) -> String {
    tr_in(current(), s)
}

/// [`tr`] in `lang`.
pub fn tr_in(lang: Lang, s: &str) -> String {
    let text = lang.table().and_then(|t| t.translate(s)).map_or_else(|| s.to_string(), Cow::into_owned);
    if is_name(s) { text } else { device_text_for(s, Tr::Same(&text)).to_string() }
}

/// Pocket Sync's lines (Settings › Connections › Pocket Sync, its row there, the pairing view): "Pocket" there is any
/// handheld with pocket-ui (docs/user/sync.md, "Which Pockets"), so they keep the word on the TrimUI handhelds, where
/// `brick_words` makes "the Pocket" the device itself. A Brick said "Keep saves the same on your other Bricks" and "Pair a
/// Brick" though it pairs an RG35XX Plus or a phone as well (Neon polish 21.11.2026). Nothing else on these lines changes
/// on a TrimUI (no HOME, no fan).
pub fn sync_line(s: &str) -> bool {
    SYNC_LINES.contains(&s) || (s.starts_with("Valid for ") && s.ends_with(" more, for one Pocket."))
}

/// The lines `sync_line` knows, as the code has them in English (`pocket_sync_page`, `app_pocket_sync`, Settings'
/// Connections list). `sync_lines_with_pocket_are_all_known` checks the files for one it doesn't.
pub const SYNC_LINES: &[&str] = &[
    "Keep saves the same on your other Pockets, without a computer.",
    "Sync with my Pockets",
    "Pair a Pocket",
    "Syncs saves with your other Pockets. No account.",
    "Open on both Pockets, then enter the pairing code.",
    "Which Pockets",
    "Pockets with pocket-ui OS, pocket-ui on the TrimUI Brick and on dArkOS, and pocket-ui for Android, on the same Wi-Fi or in your Tailscale.",
    "Each Pocket",
    "Paired Pockets show by model. Played on both? Nothing gets overwritten: you choose.",
    "Pockets with pocket-ui OS, pocket-ui on the TrimUI Brick and on dArkOS, and pocket-ui for Android.",
    "That Pocket did not answer.",
    "Off. Turn it on for every Pocket you want to sync.",
    "No Pocket paired yet. Open Pair a Pocket on both devices.",
    "Direct sync over your Wi-Fi or Tailscale. No cloud.",
    "This Pocket's code",
    "Pockets nearby showing a code",
    "Searching… Open Pair a Pocket on the other Pocket on the same Wi-Fi.",
    "Pick the other Pocket, enter its code.",
];

/// Printed button names follow the hardware, after translation and before measuring/wrapping. Names of games
/// bypass this in the caller; raw manual prose uses it too. Other devices keep their HOME key.
pub fn device_text(text: Tr<'_>) -> Tr<'_> {
    device_text_for("", text)
}

/// `device_text` for the line `s` as the code has it (English): Pocket Sync's lines keep their "Pocket" (`brick_line`).
pub fn device_text_for<'a>(s: &str, text: Tr<'a>) -> Tr<'a> {
    #[cfg(feature = "brick")]
    if matches!(crate::device::current().model, Some(crate::device::Model::Brick(_)))
        && (text.contains("HOME") || text.contains("Pocket") || FANLESS_NAMES.iter().any(|(long, _)| text.contains(long)))
    {
        return Tr::Shared(Arc::from(brick_line(s, &text)));
    }
    #[cfg(not(feature = "brick"))]
    let _ = s;
    if crate::device::no_fan() && FANLESS_NAMES.iter().any(|(long, _)| text.contains(long)) {
        return Tr::Shared(Arc::from(fanless_text(&text)));
    }
    text
}

/// What the line `s` (as the code has it, in English) says on the Brick, from its translation: `brick_text`, but Pocket
/// Sync's lines keep "Pocket" (`sync_line`).
#[cfg(feature = "brick")]
pub fn brick_line(s: &str, translated: &str) -> String {
    if sync_line(s) { fanless_text(&translated.replace("HOME", "MENU")) } else { brick_text(translated) }
}

/// What a line says on the Brick: MENU for HOME, Brick for the handheld's "Pocket" (`brick_words`), its page names.
#[cfg(feature = "brick")]
pub fn brick_text(s: &str) -> String {
    fanless_text(&brick_words(&s.replace("HOME", "MENU")))
}

/// A line on a handheld without a fan of ours (`device::no_fan`): the page and the pause menu's group by their names there.
pub fn fanless_text(s: &str) -> String {
    let mut s = s.to_string();
    for (long, short) in FANLESS_NAMES {
        s = s.replace(long, short);
    }
    s
}

/// Names that drop the fan where there is none to set (`device::no_fan`: the H700, Rockchip and R36S handhelds, the RG DS,
/// the Brick, Android): Settings › Playing › Performance (`perf_fan_page::title`) and the pause menu's Options › Speed &
/// rumble, in every language as the tables have them, so every note and Tour card that names them says the name on
/// screen (review 19.11. #2: "Performance & fan" opened three dead "No fan" rows on the H700 handhelds).
pub const FANLESS_NAMES: [(&str, &str); 12] = [
    ("Performance & fan", "Performance"),
    ("Leistung & Lüfter", "Leistung"),
    ("Rendimiento y ventilador", "Rendimiento"),
    ("Desempenho e ventoinha", "Desempenho"),
    ("Performances et ventilateur", "Performances"),
    ("Prestazioni e ventola", "Prestazioni"),
    ("Speed, rumble, fan", "Speed & rumble"),
    ("Tempo, Vibration, Lüfter", "Tempo & Vibration"),
    ("Velocidad, vibración, ventilador", "Velocidad y vibración"),
    ("Velocidade, vibração, ventoinha", "Velocidade e vibração"),
    ("Vitesse, vibration, ventilateur", "Vitesse et vibration"),
    ("Velocità, vibrazione, ventola", "Velocità e vibrazione"),
];

/// The Brick's own name where a line calls the handheld "the Pocket" (the Mini V2's word, in every language: "this Pocket",
/// "dem Pocket", "el Pocket", "o Pocket", "le Pocket"): Settings, Tailscale, Sync, the power menu and Test buttons said "Pocket" on a Brick. Other things
/// called Pocket keep it: "Retroid Pocket 5", "Neo Geo Pocket", "Game Boy Pocket", "GB Pocket", PocketCore, Pocket5.
#[cfg(feature = "brick")]
pub fn brick_words(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find("Pocket") {
        let (before, after) = (&rest[..at], &rest[at + "Pocket".len()..]);
        out.push_str(before);
        let named = ["Retroid ", "Geo ", "Boy ", "GB "].iter().any(|p| out.ends_with(p));
        let word_starts = !out.ends_with(|c: char| c.is_alphanumeric() || c == '-');
        let next = after.chars().next();
        let word_ends = match next {
            Some('s') => !after[1..].starts_with(|c: char| c.is_alphanumeric()),
            Some(c) => !(c.is_alphanumeric() || c == '-' || c == '_'),
            None => true,
        };
        // "Pocket Sync" is a feature's name, the same on every handheld (Settings → Connections → Pocket Sync).
        let named = named || after.starts_with(" Sync");
        out.push_str(if !named && word_starts && word_ends { crate::brick_profile::board().unwrap_or(crate::brick_profile::Board::Brick).word() } else { "Pocket" });
        rest = after;
    }
    out.push_str(rest);
    out
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
    /// Strings that are translated already (`is_translated`), until they are painted.
    translated: FastMap<Box<str>, ()>,
    /// Lines of a wrapped game name (`name_line`), until they are painted.
    name_lines: FastMap<Box<str>, ()>,
    /// How many strings ran the table's patterns (their first sight): the benchmark of Settings' first frame counts
    /// the ones that frame made.
    patterned: usize,
    /// The first few of them, for a test to name (`pattern_log`).
    pattern_log: Vec<Box<str>>,
    /// `NAMES_SEEN` when `seen` was filled.
    names_seen: u32,
    /// The language `seen` and `translated` are in: a switch from one table to another empties them.
    lang: Lang,
}

const CACHE_LIMIT: usize = 4096;

impl Cache {
    /// `s` in the language of the interface. English: `s`, at the cost of one atomic load.
    pub fn get<'a>(&mut self, s: &'a str) -> Tr<'a> {
        let lang = current();
        let Some(table) = lang.table().filter(|_| s.bytes().any(|b| b.is_ascii_alphabetic())) else {
            return Tr::Same(s);
        };
        self.follow(lang);
        let names_seen = NAMES_SEEN.load(Ordering::Relaxed);
        if names_seen != self.names_seen {
            // A name learned since: what was made of a string before it may have been made of a name.
            self.seen.clear();
            self.names_seen = names_seen;
        }
        // A line of a wrapped name that is a word of the table too ("Adventure"): the name's, this once.
        if !self.name_lines.is_empty() && self.name_lines.remove(s).is_some() {
            record(s, None);
            return Tr::Same(s);
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
        // A line `Text::fit` or `Text::wrap` made of a translation: translated already, painted as it is.
        if self.translated.remove(s).is_some() {
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
        // A "translation" that changes nothing is translated already (a line `Text::fit` gave back, a wrapped line):
        // painted as it is, and its parts are not English left over.
        self.patterned += 1;
        if self.pattern_log.len() < 16 {
            self.pattern_log.push(s.into());
        }
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

    /// What this cache holds is in `lang` from here on: nothing of another table stays.
    fn follow(&mut self, lang: Lang) {
        if lang != self.lang {
            self.seen.clear();
            self.translated.clear();
            self.lang = lang;
        }
    }

    /// `s` is translated already: a translation `Text::fit` gave back, cut short or not, or a line `Text::wrap` made of
    /// one. Painted next, it is drawn as it is. Without this its first sight ran every pattern of the table over it to
    /// find that none changes it (0.3 ms for a sentence on the build computer, a frame's worth on the Mini V2, for
    /// each line of Settings' categories).
    pub fn name_line(&mut self, s: &str) {
        if current() == Lang::En {
            return;
        }
        if self.name_lines.len() >= CACHE_LIMIT {
            self.name_lines.clear();
        }
        self.name_lines.insert(s.into(), ());
    }

    /// `s` was put together in `current()`'s own words by the caller ("Auto · Leise"): painted as it is, whenever it is
    /// painted, and its first sight runs no pattern. Unlike `is_translated` this is no one-shot: it stays until the cache
    /// is emptied, and the caller says it again each time it makes the line.
    pub fn keep_as_is(&mut self, s: &str) {
        let lang = current();
        if lang == Lang::En {
            return;
        }
        self.follow(lang);
        if self.seen.contains_key(s) {
            return;
        }
        if self.seen.len() >= CACHE_LIMIT {
            self.seen.clear();
        }
        self.seen.insert(s.into(), None);
    }

    pub fn is_translated(&mut self, s: &str) {
        let lang = current();
        if lang == Lang::En {
            return;
        }
        self.follow(lang);
        if self.seen.contains_key(s) || self.translated.contains_key(s) {
            return;
        }
        if self.translated.len() >= CACHE_LIMIT {
            self.translated.clear();
        }
        self.translated.insert(s.into(), ());
    }

    /// How many strings have run the table's patterns so far.
    pub fn patterns_run(&self) -> usize {
        self.patterned
    }

    /// The first strings that ran them (at most 16).
    pub fn pattern_log(&self) -> &[Box<str>] {
        &self.pattern_log
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
/// cut short with "…", `X` one that ran past the picture's right edge (the last two by their English, so a run in
/// another language and an English run compare).
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

/// Whether the snapshot checks are recording (`POCKETUI_I18N_MISSES`).
pub fn recording() -> bool {
    recorder().is_some()
}

/// For the snapshot checks: a line was painted that is the first piece of a word `Text::wrap` broke inside, with no "/",
/// ".", "-" or "_" to break after (a name's word wider than its line: the painter should have made the text smaller).
pub fn note_broken(word: &str) {
    note('W', word);
}

/// For the snapshot checks: `english` was drawn past the right edge of the picture.
pub fn note_overflow(english: &str) {
    note('X', english);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn table(src: &'static str) -> Table {
        let (t, errors) = Table::parse(src);
        assert!(errors.is_empty(), "{errors:?}");
        t
    }

    /// Every complete language's table (`Lang::complete`), by its file's name: what the checks below hold to every line. A
    /// language still being translated (Korean) joins by itself the day it is set `complete: true` (nightly 22.11.2026:
    /// the list was written by hand).
    fn shipped() -> Vec<(&'static str, &'static str)> {
        Lang::ALL.into_iter().filter(|l| l.complete()).filter_map(Lang::source).map(|(_, src, file)| (file, src)).collect()
    }

    /// A table's lines: the English side as it is written, the rest of the line.
    fn lines_of(src: &'static str) -> Vec<(&'static str, &'static str)> {
        src.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')).filter_map(|l| l.split_once('\t')).collect()
    }

    #[test]
    fn the_shipped_tables_parse_cleanly() {
        for (file, src) in shipped() {
            let (t, errors) = Table::parse(src);
            assert!(errors.is_empty(), "assets/i18n/{file}:\n{}", errors.join("\n"));
            assert!(!t.is_empty());
        }
    }

    /// No line shows wrong: each line keeps its English side's placeholders and says something (`Table::parse` checks the
    /// placeholders too), and German has every line any table has (the owner tests in German; `tests/i18n_snapshots.rs`
    /// holds it to every string of `src/`). A missing Spanish or Portuguese line shows English: a warning in the nightly
    /// run, filled before a release (`every_language_has_every_line`).
    #[test]
    fn every_line_keeps_its_placeholders_and_german_has_every_line() {
        let holes = |s: &str| {
            let mut h: Vec<String> = s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(n, _)| n.to_string())).collect();
            h.sort();
            h
        };
        let mut bad = missing_lines("de.tsv");
        for (file, src) in shipped() {
            for (en, tr) in lines_of(src) {
                if tr.trim().is_empty() {
                    bad.push(format!("{file}: an empty line for {en:?}"));
                }
                if holes(en) != holes(tr) {
                    bad.push(format!("{file}: other placeholders in {tr:?} than in {en:?}"));
                }
            }
        }
        assert!(bad.is_empty(), "{} problems:\n{}", bad.len(), bad.join("\n"));
    }

    /// What one table translates, every table translates. Release check (scripts/bookkeeping.sh warns about it every
    /// night, scripts/release-check.sh fails on it): one commit fills the Spanish and Portuguese gaps before a release.
    #[test]
    #[ignore = "release check: every language complete (scripts/release-check.sh)"]
    fn every_language_has_every_line() {
        let bad: Vec<String> = shipped().into_iter().flat_map(|(file, _)| missing_lines(file)).collect();
        assert!(bad.is_empty(), "{} lines missing:\n{}", bad.len(), bad.join("\n"));
    }

    /// The lines some shipped table has and `file` has not.
    fn missing_lines(file: &str) -> Vec<String> {
        let all: BTreeSet<&str> = shipped().into_iter().flat_map(|(_, src)| lines_of(src)).map(|(en, _)| en).collect();
        let src = shipped().into_iter().find(|(f, _)| *f == file).unwrap().1;
        let has: BTreeSet<&str> = lines_of(src).into_iter().map(|(en, _)| en).collect();
        all.into_iter().filter(|en| !has.contains(en)).map(|en| format!("{file} has no line for {en:?}")).collect()
    }

    #[test]
    fn a_table_that_loses_a_placeholder_or_a_line_is_noticed() {
        let (_, errors) = Table::parse("{n} games\tjuegos\n");
        assert_eq!(errors.len(), 1, "{errors:?}");
    }

    /// VDEV-ROUND-2026-10-25: "{s} of {s2}" (for "56.1 GB of 63.1 GB") made "The Legend von Zelda" of every game
    /// name with "of" in it, on the shelf and the card. The names `assets/i18n/keep.txt` keeps in English (games,
    /// systems, emulators) come out of the shipped table as they are.
    #[test]
    fn the_names_kept_in_english_stay_english() {
        let keep = include_str!("../assets/i18n/keep.txt");
        for (file, src) in shipped() {
            let (t, _) = Table::parse(src);
            let changed: Vec<String> = keep
                .lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#') && !l.starts_with("re:"))
                // Words the table translates where they are no name (the Lens's language filter, a Curator tag).
                .filter(|l| !["English", "‹ English ›", "puzzle"].contains(l))
                // In keep.txt because they read the same in German, not because they are names.
                .filter(|l| !(file == "es.tsv" && (*l == "‹ Warm ›" || *l == "‹ Scanlines ›" || l.starts_with("Code "))))
                .filter(|l| !(file == "pt.tsv" && (*l == "‹ Warm ›" || *l == "‹ Scanlines ›" || l.starts_with("Code "))))
                .filter(|l| !(file == "fr.tsv" && (*l == "‹ Warm ›" || *l == "‹ Scanlines ›" || l.starts_with("Code "))))
                .filter(|l| !(file == "it.tsv" && (*l == "‹ Warm ›" || *l == "‹ Normal ›" || *l == "‹ Scanlines ›" || l.starts_with("Code "))))
                // Korean says Home, Auto, Max, Original and Metroidvania in its own words (홈, 자동, 최대, 오리지널, 메트로배니아), as
                // Korean menus do.
                .filter(|l| !(file == "ko.tsv" && (["Home", "Auto", "Max", "‹ Warm ›", "‹ Normal ›", "‹ Scanlines ›", "‹ Original ›", "Metroidvania"].contains(l) || l.starts_with("Code "))))
                .filter_map(|name| t.translate(name).filter(|g| g != name).map(|g| format!("{name:?} -> {g:?}")))
                .collect();
            assert!(changed.is_empty(), "{file}: {} names the table changes:\n{}", changed.len(), changed.join("\n"));
            for name in ["Castlevania - Symphony of the Night", "Secret of Mana", "Street Fighter II' is the first update of Street Fighter"] {
                assert_eq!(t.translate(name), None, "{file}: {name}");
            }
        }
        assert_eq!(german().translate("56.1 GB of 63.1 GB").as_deref(), Some("56,1 GB von 63,1 GB"), "the card's sizes still are");
        assert_eq!(Lang::Es.table().unwrap().translate("56.1 GB of 63.1 GB").as_deref(), Some("56,1 GB de 63,1 GB"));
        assert_eq!(Lang::Pt.table().unwrap().translate("56.1 GB of 63.1 GB").as_deref(), Some("56,1 GB de 63,1 GB"));
        assert_eq!(Lang::Fr.table().unwrap().translate("56.1 GB of 63.1 GB").as_deref(), Some("56,1 GB sur 63,1 GB"));
        assert_eq!(Lang::It.table().unwrap().translate("56.1 GB of 63.1 GB").as_deref(), Some("56,1 GB di 63,1 GB"));
    }

    #[test]
    fn a_translated_line_is_never_looked_up_as_english_again() {
        // `Text::fit` returns the translation, which its caller then draws: it must come back as it is.
        for (file, src) in shipped() {
            let (t, _) = Table::parse(src);
            let bad: Vec<String> = t
                .entries()
                .into_iter()
                .filter(|(en, tr)| !en.contains('{') && t.translate(tr).is_some_and(|again| again != *tr))
                .map(|(en, tr)| format!("{en:?} -> {tr:?} -> {:?}", t.translate(&tr)))
                .collect();
            assert!(bad.is_empty(), "{file}: {} lines change when looked up again:\n{}", bad.len(), bad.join("\n"));
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

    /// Each language writes a decimal number its own way, in every shipped table: a comma in German, Spanish, Portuguese,
    /// French and Italian, a point in Korean ("2,3 W" on the Battery page in Korean, 24.11.2026: every language with a
    /// table had been given the comma). The list is the languages' own usage, not the code's.
    #[test]
    fn each_language_writes_decimals_its_own_way() {
        let usage = |l: Lang| match l {
            Lang::En | Lang::Ko => '.',
            Lang::De | Lang::Es | Lang::Pt | Lang::Fr | Lang::It => ',',
        };
        for lang in Lang::ALL {
            let Some(t) = lang.table() else { continue };
            let line = t.translate("PS1 uses about 7% an hour, 2.3 W").unwrap_or_else(|| panic!("{lang:?}: the draw line is in the table"));
            let want = format!("2{}3", usage(lang));
            assert!(line.contains(&want), "{lang:?}: {line:?} should have {want:?}");
        }
    }

    #[test]
    fn a_number_with_decimals_takes_the_languages_comma() {
        // 15.11. on a pocket in French: "7.4 Go libres", and "118.1 GB frei" in German.
        let t = table("{n} GB free\t{n} Go libres\npocket-ui {s}\tpocket-ui {s}\n{n} of {m} exits\t{n} sur {m} sorties\n");
        assert_eq!(t.translate("7.4 GB free").as_deref(), Some("7,4 Go libres"));
        assert_eq!(t.translate("118 GB free").as_deref(), Some("118 Go libres"));
        assert_eq!(t.translate("6 of 96 exits").as_deref(), Some("6 sur 96 sorties"));
        assert_eq!(decimal_sign("0.5.0", ','), "0.5.0", "a version stays");
        assert_eq!(decimal_sign("1,204", ','), "1,204");
        assert_eq!(decimal_sign("60.00", ','), "60,00");
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

    /// Walk 2026-11-02: the menus were German and the manual English without a word. The row says so, in both languages.
    #[test]
    fn the_manual_is_named_as_english_in_german_and_spanish() {
        let de = table(include_str!("../assets/i18n/de.tsv"));
        let es = table(include_str!("../assets/i18n/es.tsv"));
        assert!(de.translate("Manual").is_some_and(|m| m.contains("Englisch")), "{:?}", de.translate("Manual"));
        assert!(es.translate("Manual").is_some_and(|m| m.contains("inglés")), "{:?}", es.translate("Manual"));
        assert!(es.translate("The user guide, on the device.").is_some_and(|m| m.contains("inglés")));
    }

    #[test]
    fn two_sentences_split_where_the_first_ends() {
        let t = table("Soft blends pixels.\tWeich mischt Pixel.\nAt {n}× whole it would be sharp.\tBei ganzen {n}× wäre es scharf.\nDrawn {n} times as large. {t} {t2}\t{n}-mal so groß. {t} {t2}\nSet to {t}\tAuf {t} gestellt\n");
        assert_eq!(
            t.translate("Drawn 2.5 times as large. Soft blends pixels. At 3× whole it would be sharp.").as_deref(),
            Some("2,5-mal so groß. Weich mischt Pixel. Bei ganzen 3× wäre es scharf.")
        );
        assert_eq!(t.translate("Set to Fuchsia").as_deref(), Some("Auf Fuchsia gestellt"), "still: an unknown value is kept");
    }

    #[test]
    fn a_long_list_is_translated_part_by_part() {
        let names = ["Lens", "Echoes", "Up next", "Versions", "Labels", "Pixel Path", "Frames", "Last game", "Motion", "Sounds", "Haptics", "LEDs"];
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
            Some("Das Spiel läuft 0,46 % schneller. Es behält seine Tonhöhe. 13 Bilder kamen zu spät.")
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
    fn system_card_counts_translate_and_the_last_game_keeps_its_name() {
        for (lang, one, many, last) in [
            (Lang::En, "1 game", "2 games", "last:"),
            (Lang::De, "1 Spiel", "2 Spiele", "zuletzt:"),
            (Lang::Es, "1 juego", "2 juegos", "último:"),
            (Lang::Pt, "1 jogo", "2 jogos", "último:"),
            (Lang::Fr, "1 jeu", "2 jeux", "dernier\u{a0}:"),
            (Lang::It, "1 gioco", "2 giochi", "ultimo:"),
        ] {
            for (english, expected) in [("1 game", one), ("2 games", many)] {
                assert_eq!(tr_in(lang, english), expected, "{lang:?}: without history");
                // Dark is both a possible game name and a translated UI word. The last game's name is data.
                for name in ["Dark", "KleleAtoms MD"] {
                    assert_eq!(tr_in(lang, &format!("{english} · last: {name}")),
                        format!("{expected} · {last} {name}"), "{lang:?}: with history");
                }
            }
        }
    }

    #[test]
    fn android_start_error_and_screen_reader_controls_follow_the_player_language() {
        for (lang, title, previous, next, previous_value, next_value) in [
            (Lang::En, "Game couldn't start", "Previous slot", "Next slot", "previous value", "next value"),
            (Lang::De, "Das Spiel konnte nicht starten", "Vorheriger Slot", "Nächster Slot", "vorheriger Wert", "nächster Wert"),
            (Lang::Es, "No se pudo iniciar el juego", "Ranura anterior", "Ranura siguiente", "valor anterior", "valor siguiente"),
            (Lang::Pt, "O jogo não pôde iniciar", "Slot anterior", "Próximo slot", "valor anterior", "próximo valor"),
            (Lang::Fr, "Le jeu n'a pas pu démarrer", "Emplacement précédent", "Emplacement suivant", "valeur précédente", "valeur suivante"),
            (Lang::It, "Il gioco non è partito", "Slot precedente", "Slot successivo", "valore precedente", "valore successivo"),
        ] {
            let _language = for_this_thread(lang);
            // The native AlertDialog and accessibility labels bypass Text's automatic translation.
            assert_eq!(tr("Game couldn't start"), title, "{lang:?}");
            assert_eq!(tr("Previous slot"), previous, "{lang:?}");
            assert_eq!(tr("Next slot"), next, "{lang:?}");
            let setting = tr("Scaling");
            assert_eq!(tr("Scaling · previous value"), format!("{setting} · {previous_value}"), "{lang:?}");
            assert_eq!(tr("Scaling · next value"), format!("{setting} · {next_value}"), "{lang:?}");
        }
    }

    #[test]
    fn a_careful_pattern_leaves_a_name_alone() {
        let t = table("!{t} and {t2}\t{t} und {t2}\nRock\tFels\nFXAA\tFXAA\nMSAA\tMSAA\nSharpening\tSchärfung\n");
        assert_eq!(t.translate("Rock and Roll Racing"), None);
        assert_eq!(t.translate("FXAA and Sharpening").as_deref(), Some("FXAA und Schärfung"));
    }

    #[test]
    fn the_moving_dots_of_a_busy_line_are_kept_whatever_their_number() {
        let t = table("Turning Tailscale on\tTailscale wird eingeschaltet\nScanning..\tSuche (zwei)\n");
        for dots in [".", "..", "..."] {
            let line = format!("Turning Tailscale on{dots}");
            let got = t.translate(&line);
            if dots == "." {
                assert_eq!(got, None, "one dot is a sentence's: only an entry of its own");
            } else {
                assert_eq!(got.as_deref(), Some(format!("Tailscale wird eingeschaltet{dots}").as_str()), "{line}");
            }
        }
        assert_eq!(t.translate("Scanning..").as_deref(), Some("Suche (zwei)"), "an entry of its own wins");
        assert_eq!(t.translate("Something else..."), None);
        assert_eq!(t.translate("Wait..."), None);
    }

    /// Every busy line the tables keep as "Words{s}" (the page adds one to three dots by the clock) is translated with
    /// each of them: "Looking at your games.." stayed English in the German and Spanish snapshots whenever the clock
    /// had painted two or three dots (14.11.).
    #[test]
    fn every_busy_line_is_translated_with_one_two_and_three_dots() {
        for lang in [Lang::De, Lang::Es] {
            let table = lang.table().unwrap();
            let src = std::fs::read_to_string(format!("{}/assets/i18n/{}.tsv", env!("CARGO_MANIFEST_DIR"), if lang == Lang::De { "de" } else { "es" })).unwrap();
            let busy: Vec<&str> = src.lines().filter_map(|l| l.split('\t').next()?.strip_suffix("{s}")).filter(|k| k.ends_with(char::is_alphabetic) && !k.contains('{')).collect();
            assert!(busy.contains(&"Looking at your games"), "{lang:?}");
            let mut wrong = Vec::new();
            for stem in busy {
                for dots in [".", "..", "..."] {
                    let english = format!("{stem}{dots}");
                    match table.translate(&english) {
                        Some(t) if t.ends_with(dots) && t != english => {}
                        other => wrong.push(format!("{english:?} -> {other:?}")),
                    }
                }
            }
            assert!(wrong.is_empty(), "{lang:?}:\n{}", wrong.join("\n"));
        }
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
        for spanish in ["es", "ES", "es_ES", "es_MX.UTF-8", "es-419", "es-AR", "es.UTF-8"] {
            assert_eq!(Lang::from_locale(spanish), Lang::Es, "{spanish}");
        }
        for portuguese in ["pt", "PT", "pt_BR", "pt-BR", "pt_BR.UTF-8", "pt_PT", "pt-PT"] {
            assert_eq!(Lang::from_locale(portuguese), Lang::Pt, "{portuguese}");
        }
        for french in ["fr", "FR", "fr_FR", "fr-FR", "fr_FR.UTF-8", "fr_CA", "fr-BE", "fr_CH.UTF-8"] {
            assert_eq!(Lang::from_locale(french), Lang::Fr, "{french}");
        }
        for italian in ["it", "IT", "it_IT", "it-IT", "it_CH.UTF-8", "it-CH", "it.UTF-8"] {
            assert_eq!(Lang::from_locale(italian), Lang::It, "{italian}");
        }
        for korean in ["ko", "KO", "ko_KR", "ko-KR", "ko_KR.UTF-8", "ko_KR.eucKR"] {
            assert_eq!(Lang::from_locale(korean), Lang::Ko, "{korean}");
        }
        for other in ["est", "et_EE", "eo", "ptx", "fra", "frp", "ita", "itx_IT", "kor", "kok_IN", "C.UTF-8"] {
            assert_eq!(Lang::from_locale(other), Lang::En, "{other}");
        }
    }

    #[test]
    fn the_row_walks_through_every_language_both_ways() {
        assert_eq!(Lang::ALL.map(Lang::next), [Lang::De, Lang::Es, Lang::Pt, Lang::Fr, Lang::It, Lang::Ko, Lang::En]);
        assert_eq!(Lang::ALL.map(Lang::previous), [Lang::Ko, Lang::En, Lang::De, Lang::Es, Lang::Pt, Lang::Fr, Lang::It]);
        // With every language offered, the whole round.
        let all = Lang::ALL;
        assert_eq!(all.map(|l| step_in(&all, l, true)), [Lang::De, Lang::Es, Lang::Pt, Lang::Fr, Lang::It, Lang::Ko, Lang::En]);
        assert_eq!(all.map(|l| step_in(&all, l, false)), [Lang::Ko, Lang::En, Lang::De, Lang::Es, Lang::Pt, Lang::Fr, Lang::It]);
        assert_eq!(Lang::ALL.map(Lang::name), ["English", "Deutsch", "Español", "Português (Brasil)", "Français", "Italiano", "한국어"]);
        assert_eq!(serde_json::to_string(&Lang::Es).unwrap(), "\"es\"");
        assert_eq!(serde_json::to_string(&Lang::Pt).unwrap(), "\"pt\"");
        assert_eq!(serde_json::from_str::<Lang>("\"pt\"").unwrap(), Lang::Pt);
        assert_eq!(serde_json::to_string(&Lang::Fr).unwrap(), "\"fr\"");
        assert_eq!(serde_json::from_str::<Lang>("\"fr\"").unwrap(), Lang::Fr);
        assert_eq!(serde_json::to_string(&Lang::It).unwrap(), "\"it\"");
        assert_eq!(serde_json::from_str::<Lang>("\"it\"").unwrap(), Lang::It);
        assert_eq!(serde_json::to_string(&Lang::Ko).unwrap(), "\"ko\"");
        assert_eq!(serde_json::from_str::<Lang>("\"ko\"").unwrap(), Lang::Ko);
    }

    #[test]
    fn a_table_file_named_in_the_environment_stands_in_for_its_language_only() {
        let dir = crate::testutil::TestDir::new("i18n-table");
        let file = dir.join("ko.tsv");
        std::fs::write(&file, "Settings\t설정\n").unwrap();
        let value = Some(format!("ko:{}", file.display()));
        let ko = override_table(value.clone(), Lang::Ko).map(|(src, _)| Table::parse(src).0);
        assert!(override_table(value, Lang::It).is_none(), "another language keeps its own table");
        assert!(override_table(None, Lang::Ko).is_none());
        // A file that is missing or empty: the table compiled in (missing), an empty table (empty: English everywhere).
        assert!(override_table(Some(format!("ko:{}", dir.join("missing.tsv").display())), Lang::Ko).is_none());
        std::fs::write(dir.join("empty.tsv"), "").unwrap();
        let empty = override_table(Some(format!("ko:{}", dir.join("empty.tsv").display())), Lang::Ko).map(|(src, _)| Table::parse(src).0);
        assert_eq!(empty.and_then(|t| t.translate("Settings").map(Cow::into_owned)), None);
        assert!(override_table(Some("ko".into()), Lang::Ko).is_none(), "no file named");
        assert_eq!(ko.and_then(|t| t.translate("Settings").map(Cow::into_owned)).as_deref(), Some("설정"));
    }

    /// The checks that want every line (`every_language_has_every_line` and the others over `shipped`) take exactly the
    /// complete languages, from `Lang::ALL`: Korean now, the next language by setting `complete: true`, nothing to list.
    #[test]
    fn the_completeness_checks_take_every_complete_language_and_no_other() {
        let files: Vec<&str> = shipped().into_iter().map(|(file, _)| file).collect();
        let complete: Vec<Lang> = Lang::ALL.into_iter().filter(|l| *l != Lang::En && l.complete()).collect();
        assert_eq!(files.len(), complete.len());
        for lang in Lang::ALL.into_iter().filter(|l| *l != Lang::En) {
            let file = lang.source().unwrap().2;
            assert_eq!(files.contains(&file), lang.complete(), "{lang:?} ({file})");
        }
        assert_eq!(files, ["de.tsv", "es.tsv", "pt.tsv", "fr.tsv", "it.tsv", "ko.tsv"]);
    }

    /// Lead 22.11.2026: a language still being translated (`complete: false`, Korean until it was) is not offered: not
    /// on the row (the welcome's pills and Settings walk `row`, `next`, `previous`), and its system or phone starts in
    /// English. A developer reaches it with `POCKETUI_LANG_PREVIEW=1` or `POCKETUI_I18N_TABLE=<code>:<file>`.
    #[test]
    fn a_language_still_being_translated_is_not_offered_until_it_is_complete() {
        assert!(!offered_if(false, "ko", false, None));
        assert!(offered_if(false, "ko", true, None));
        assert!(offered_if(false, "ko", false, Some("ko:/tmp/ko.tsv")) && !offered_if(false, "ko", false, Some("it:/tmp/it.tsv")));
        assert!(!offered_if(false, "ko", false, Some("ko")), "no file named");
        assert!(offered_if(true, "it", false, None));
        // Every language is complete and offered now.
        assert!(Lang::ALL.iter().all(|l| l.complete() && l.offered()));
        assert_eq!(Lang::row(), Lang::ALL);
    }

    /// Korean is switched on (`complete`): offered on the row, a Korean system or phone starts in it, and the
    /// completeness checks hold it to every line (`shipped`).
    #[test]
    fn korean_is_offered_and_held_to_every_line() {
        let (_, errors) = Table::parse(KO);
        assert!(errors.is_empty(), "{errors:?}");
        let ko = Lang::Ko.table().expect("a table of its own");
        for (line, korean) in [("Settings", "설정"), ("Continue", "계속하기"), ("Language", "언어"), ("The language of menus and messages.", "메뉴와 메시지 언어예요.")] {
            assert_eq!(ko.translate(line).as_deref(), Some(korean), "{line}");
        }
        assert!(Lang::Ko.complete() && Lang::row().contains(&Lang::Ko));
        assert!(shipped().into_iter().any(|(file, _)| file == "ko.tsv"));
        assert_eq!(Lang::from_system(Some("ko_KR.UTF-8"), || None), Lang::Ko);
        assert_eq!(Lang::from_system(None, || Some("ko_KR".into())), Lang::Ko);
        assert_eq!(Lang::from_system_locale("ko"), Lang::Ko, "a Korean phone");
        assert_eq!(Lang::It.next(), Lang::Ko);
        assert_eq!(Lang::Ko.next(), Lang::En);
        let _ko = for_this_thread(Lang::Ko);
        assert_eq!(current(), Lang::Ko);
        let mut tr = Cache::default();
        assert_eq!(&*tr.get("Settings"), "설정");
    }

    #[test]
    fn each_language_says_what_a_web_page_should_answer_in() {
        assert_eq!(Lang::De.accept_language(), "Accept-Language: de,en;q=0.8");
        assert_eq!(Lang::En.accept_language(), "Accept-Language: en,de;q=0.5");
        assert!(Lang::Es.accept_language().contains("es,"));
        assert!(Lang::Pt.accept_language().contains("pt-BR,"));
        assert!(Lang::Fr.accept_language().contains("fr-FR,"));
        assert!(Lang::It.accept_language().contains("it,"));
        assert!(Lang::Ko.accept_language().contains("ko-KR,"));
        assert!(Lang::De.qwertz() && Lang::ALL.iter().filter(|l| l.qwertz()).count() == 1);
    }

    #[test]
    fn english_unless_the_system_says_a_language_of_ours() {
        assert_eq!(Lang::from_system(None, || None), Lang::En, "a fresh install: no LANG, no system.language");
        assert_eq!(Lang::from_system(Some(""), || None), Lang::En);
        assert_eq!(Lang::from_system(Some("C.UTF-8"), || Some("de_DE".into())), Lang::En, "LANG comes first");
        assert_eq!(Lang::from_system(Some("ja_JP.UTF-8"), || None), Lang::En);
        assert_eq!(Lang::from_system(None, || Some("nl_NL".into())), Lang::En);
        assert_eq!(Lang::from_system(None, || Some("es_ES".into())), Lang::Es);
        assert_eq!(Lang::from_system(Some("es_MX.UTF-8"), || None), Lang::Es);
        assert_eq!(Lang::from_system(None, || Some("pt_BR".into())), Lang::Pt);
        assert_eq!(Lang::from_system(Some("pt_BR.UTF-8"), || None), Lang::Pt);
        assert_eq!(Lang::from_system(None, || Some("fr_FR".into())), Lang::Fr);
        assert_eq!(Lang::from_system(Some("fr_FR.UTF-8"), || None), Lang::Fr);
        assert_eq!(Lang::from_system(None, || Some("it_IT".into())), Lang::It);
        assert_eq!(Lang::from_system(Some("it_IT.UTF-8"), || None), Lang::It);
        assert_eq!(Lang::from_system(Some(""), || Some("de_DE".into())), Lang::De);
        assert_eq!(Lang::from_system(Some("de_AT.UTF-8"), || None), Lang::De);
        assert_eq!(Lang::default(), Lang::En);
    }
}

#[cfg(test)]
mod sync_line_tests {
    /// Every line of Pocket Sync's pages that says "Pocket" is one `sync_line` knows (a new one would say "Brick" on a
    /// TrimUI again), and every line it knows is still in the code.
    #[test]
    fn sync_lines_with_pocket_are_all_known() {
        let files = [include_str!("pocket_sync_page.rs"), include_str!("app_pocket_sync.rs")];
        let mut unknown = Vec::new();
        for file in files {
            // The tests at the end of a file have made-up lines of their own.
            let code = file.split("#[cfg(test)]").next().unwrap_or(file);
            for line in code.lines().filter(|l| !l.trim_start().starts_with("//")) {
                for lit in line.split('"').skip(1).step_by(2) {
                    // Names: the feature's, the models'; a placeholder name; a format string (`sync_line` matches its result).
                    let named = lit.replace("Pocket Sync", "").replace("Retroid Pocket", "");
                    if !named.contains("Pocket") || lit == "Pocket" || lit.contains('{') {
                        continue;
                    }
                    if !super::SYNC_LINES.contains(&lit) {
                        unknown.push(lit.to_string());
                    }
                }
            }
        }
        assert!(unknown.is_empty(), "Pocket Sync lines that would say Brick on a TrimUI: {unknown:?}");
        let all = format!("{}{}{}", files[0], files[1], include_str!("main.rs"));
        for line in super::SYNC_LINES {
            assert!(all.contains(&format!("\"{line}\"")), "no longer in the code: {line}");
        }
        assert!(super::sync_line("Valid for 4:59 more, for one Pocket."));
        assert!(!super::sync_line("Access this Pocket from other devices."));
    }
}

#[cfg(all(test, feature = "brick"))]
mod brick_tests {
    use super::*;

    /// Settings walk 06.11.: on the Brick the Tailscale, Sync and remote-access lines, the power menu and Test buttons called the
    /// handheld "the Pocket", in English, German, Spanish and Portuguese. Things that are really called Pocket keep the word.
    #[test]
    fn the_brick_is_called_the_brick_and_other_pockets_keep_their_name() {
        for (line, shown) in [
            ("Access this Pocket from other devices.", "Reach the Brick from your other devices."),
            ("Connect to this Pocket over SSH from a PC on your Wi-Fi.", "Log in to this Brick from a computer on the same Wi-Fi, with SSH."),
            ("Pocket", "Brick"),
            ("Erreiche den Pocket von deinen anderen Geräten.", "Erreiche den Brick von deinen anderen Geräten."),
            ("Los ajustes de este Pocket.", "Los ajustes de este Brick."),
            ("As configurações deste Pocket.", "As configurações deste Brick."),
            ("Les paramètres de ce Pocket.", "Les paramètres de ce Brick."),
            ("Le impostazioni di questo Pocket.", "Le impostazioni di questo Brick."),
            ("die Einstellungen dieses Pockets", "die Einstellungen dieses Bricks"),
            ("Retroid Pocket 5, Neo Geo Pocket Color, Game Boy Pocket, GB Pocket", "Retroid Pocket 5, Neo Geo Pocket Color, Game Boy Pocket, GB Pocket"),
            ("PocketCore, Pocket5, PocketSlim, pocket-ui", "PocketCore, Pocket5, PocketSlim, pocket-ui"),
            // A feature's name keeps its word. (Its own lines keep "Pocket" before they get here: `sync_line`.)
            ("Pocket Sync", "Pocket Sync"),
            ("Was Pocket Sync abgleicht", "Was Pocket Sync abgleicht"),
            ("Pocket Sync · With TrimUI Brick at 14:05", "Pocket Sync · With TrimUI Brick at 14:05"),
            ("Pair a Pocket", "Pair a Brick"),
        ] {
            assert_eq!(brick_words(line), shown);
        }
    }

    /// Neon polish 21.11.2026: Pocket Sync on a Brick said "Keep saves the same on your other Bricks" and "Pair a Brick",
    /// though a Brick pairs an RG35XX Plus, a Mini V2 or a phone as well. There "Pocket" is any handheld with pocket-ui,
    /// in every language; the rest of the Brick's lines still call it the Brick.
    #[test]
    fn pocket_sync_keeps_pocket_for_any_handheld_in_every_language() {
        let lines = SYNC_LINES.iter().map(|l| l.to_string()).chain(["Valid for 4:59 more, for one Pocket.".to_string()]);
        for line in lines {
            for lang in [Lang::En, Lang::De, Lang::Es, Lang::Pt, Lang::Fr, Lang::It] {
                let translated = lang.table().and_then(|t| t.translate(&line)).map_or_else(|| line.clone(), Cow::into_owned);
                let shown = brick_line(&line, &translated);
                assert!(!shown.contains("Brick") || line.contains("Brick"), "{lang:?}: {line} -> {shown}");
                assert!(!shown.contains("Smart Pro"), "{lang:?}: {line} -> {shown}");
            }
        }
        let line = "Keep saves the same on your other Pockets, without a computer.";
        assert_eq!(brick_line(line, line), line);
        assert_eq!(brick_line("Pair a Pocket", "Pair a Pocket"), "Pair a Pocket");
        let de = Lang::De.table().and_then(|t| t.translate(line)).map(Cow::into_owned).unwrap_or_default();
        assert!(de.contains("Pocket") && brick_line(line, &de) == de, "{de}");
        // Every other line still calls the handheld itself the Brick.
        assert_eq!(brick_line("Access this Pocket from other devices.", "Access this Pocket from other devices."), "Reach the Brick from your other devices.");
    }

    /// The pause menu's Performance note on a real Brick (2026-11-07) still sent players to "Performance & fan",
    /// a page the Brick calls "Performance".
    #[cfg(feature = "brick")]
    #[test]
    fn the_brick_names_its_performance_page_without_the_fan_in_every_language() {
        for (lang, page) in [(Lang::En, "Performance"), (Lang::De, "Leistung"), (Lang::Es, "Rendimiento"), (Lang::Pt, "Desempenho"), (Lang::Fr, "Performances"), (Lang::It, "Prestazioni")] {
            let line = "What games use unless you save one here: Quick settings, or Settings › Playing › Performance & fan.";
            let translated = lang.table().and_then(|t| t.translate(line)).map_or_else(|| line.to_string(), Cow::into_owned);
            let note = brick_text(&translated);
            assert!(note.ends_with(&format!("{page}.")), "{lang:?}: {note}");
            assert!(!note.contains("fan") && !note.contains("Lüfter") && !note.contains("ventilador") && !note.contains("ventoinha") && !note.contains("ventilateur") && !note.contains("ventola"), "{lang:?}: {note}");
        }
    }
}
