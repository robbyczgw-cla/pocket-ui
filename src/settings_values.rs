//! The values at the right of the Settings rows (a category's list): "Home >" on Wi-Fi, "Shelf · Dark >" on Look,
//! "332 games >" on Add games. The words for every value are here; what they are worked out from:
//! - **memory** (the App's own state): Look, Add games, Performance & fan (what games run with), Battery (the status
//!   corner's figure), Updates and About (the version, and a newer one when a check found it). Painted at once.
//! - **a process or a file** (`Facts`): the Wi-Fi network's name (nmcli), Remote access (systemctl), the time zone
//!   (/etc/localtime) and the paired PCs (pocket-ui's list and ROCKNIX's `moonlight.host`). Read on a thread
//!   (`refresh`) when a category's list is shown, at most every `EVERY`; the row shows its plain ">" until the
//!   value is in, and the UI never waits for it.
//!
//! Values are short; one that is too long for the row is cut with "…" (`MAX_W`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::net_state::{NetState, Radio};
use crate::settings::{Appearance, Theme};
use crate::system_ctl::{Hw, SharedHw};

/// How old the facts may get while a list is shown before they are read again.
pub const EVERY: Duration = Duration::from_secs(5);
/// The most a value may take of the row (36 px type); longer ones are cut with "…".
pub const MAX_W: i32 = 440;

/// Wi-Fi as its row says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wifi {
    /// Switched off (ROCKNIX's Wi-Fi switch, rfkill).
    Off,
    /// The device has no Wi-Fi chip (the RG28XX).
    Absent,
    NotConnected,
    /// Connected; the network's name when NetworkManager gave it.
    Connected(Option<String>),
}

impl Wifi {
    pub fn label(&self) -> String {
        match self {
            Wifi::Off => "Off".into(),
            Wifi::Absent => "None on this device".into(),
            Wifi::NotConnected => "Not connected".into(),
            Wifi::Connected(Some(name)) => name.clone(),
            Wifi::Connected(None) => "Connected".into(),
        }
    }
}

/// What the thread read. Each is None until it is known.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    pub wifi: Option<Wifi>,
    /// Remote access (SSH) is on, for whatever reason (our switch, ROCKNIX's, a developer device).
    pub ssh: Option<bool>,
    /// The time zone, "Europe/Vienna".
    pub zone: Option<String>,
    /// How many PCs are paired for streaming.
    pub pcs: Option<usize>,
}

/// A row that opens a page: its value, then ">".
pub fn page(value: &str) -> String {
    if value.is_empty() {
        ">".into()
    } else {
        format!("{value} >")
    }
}

/// "On" or "Off".
pub fn on_off(on: bool) -> &'static str {
    if on {
        "On"
    } else {
        "Off"
    }
}

/// Look: "Shelf · Dark", "Cabinet · Automatic".
pub fn look(theme: Theme, appearance: Appearance) -> String {
    format!("{} · {}", theme.name(), appearance.name())
}

/// Performance & fan: what games run with unless their console or the game chose, "Auto · Quiet" (the words of the
/// page and the quick settings, `perf_fan_page`). Without a fan (the H700 handhelds, the Brick) only the performance: "Auto · Quiet"
/// named a fan the device hasn't (settings walk 06.11.).
pub fn perf_fan(performance: crate::power::Performance, fan: crate::power::Fan) -> String {
    perf_fan_with(performance, fan, crate::device::current().has_fan())
}

fn perf_fan_with(performance: crate::power::Performance, fan: crate::power::Fan, has_fan: bool) -> String {
    let performance = crate::perf_fan_page::performance_label(performance);
    if has_fan { format!("{performance} · {}", crate::perf_fan_page::fan_label(fan)) } else { performance.to_string() }
}

/// Add games: "No games", "1 game", "332 games".
pub fn games(n: usize) -> String {
    match n {
        0 => "No games".into(),
        1 => "1 game".into(),
        n => format!("{n} games"),
    }
}

/// Systems on the shelf: nothing while every system with games is on Home, else "1 hidden", "2 hidden", so the way back
/// is visible on the row itself (daily-use walk: a hidden system left no trace on Home or in Settings).
pub fn hidden_systems(n: usize) -> String {
    if n == 0 {
        String::new()
    } else {
        format!("{n} hidden")
    }
}

/// PC streaming: "No PC paired", "1 PC", "2 PCs".
pub fn pcs(n: usize) -> String {
    match n {
        0 => "No PC paired".into(),
        1 => "1 PC".into(),
        n => format!("{n} PCs"),
    }
}

/// Time: the zone's city ("Vienna", "Indianapolis, Indiana"), "UTC" for UTC and its aliases.
pub fn zone(zone: &str) -> String {
    if crate::timezone::is_utc(zone) {
        "UTC".into()
    } else {
        crate::timezone::city_here(zone)
    }
}

/// Updates: the version, or the newer one a check found ("0.43.0 available").
pub fn updates(current: &str, newer: Option<&str>) -> String {
    match newer {
        Some(v) => format!("{v} available"),
        None => current.to_string(),
    }
}

/// The radio as the shared picture of the Wi-Fi has it (`net_state`: what quick settings and Settings → Wi-Fi show),
/// looked at here first when nobody has yet. On the reading thread.
fn radio_of(net: &Arc<NetState>) -> Radio {
    if net.snapshot().radio == Radio::Unknown {
        net.poll();
    }
    net.snapshot().radio
}

/// Reads the facts now: a process or two and a few small files. Never on the UI thread (`refresh`). `connected`:
/// as the status corner sees it (`status_corner::status`). With `POCKETUI_MOCK` the network name and SSH are made up
/// (snapshots, the desktop).
pub fn read(hw: &dyn Hw, net: &Arc<NetState>, data_dir: &Path, connected: bool) -> Facts {
    let mock = std::env::var_os("POCKETUI_MOCK").is_some();
    let wifi = if connected {
        Wifi::Connected(if mock { Some("Home".into()) } else { crate::wifi::connected_ssid() })
    } else if crate::device::darkos_port() {
        // dArkOS's Wi-Fi, not connected: the radio is dArkOS's, not "none on this device" (its page, `setup::DARKOS_WIFI`).
        Wifi::NotConnected
    } else if radio_of(net) == Radio::Absent {
        Wifi::Absent
    } else if radio_of(net).is_off() {
        Wifi::Off
    } else {
        Wifi::NotConnected
    };
    let ssh = if mock { false } else { crate::remote::current().running() };
    let mut paired = crate::moonlight::load_pcs(data_dir);
    if let Some(h) = crate::moonlight::host().filter(|h| !paired.contains(h)) {
        paired.push(h);
    }
    Facts { wifi: Some(wifi), ssh: Some(ssh), zone: hw.timezone(), pcs: Some(paired.len()) }
}

static FACTS: Mutex<Option<Facts>> = Mutex::new(None);


/// When a read was last started.
static ASKED: Mutex<Option<Instant>> = Mutex::new(None);
static READING: AtomicBool = AtomicBool::new(false);

/// What the thread read last (all None before the first read).
pub fn get() -> Facts {
    FACTS.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_default()
}

/// Sets the facts (snapshots, which read them once on their own thread).
pub fn set(facts: Facts) {
    *FACTS.lock().unwrap_or_else(|e| e.into_inner()) = Some(facts);
}

/// The status corner's Wi-Fi when the last read started (0 none yet, 1 off, 2 on): a change reads again at once.
static CORNER_WIFI: AtomicU8 = AtomicU8::new(0);

/// Whether a read is due at `now`: none started yet, or the last one `EVERY` ago and none under way.
pub fn due(asked: Option<Instant>, reading: bool, now: Instant) -> bool {
    !reading && asked.is_none_or(|at| now.duration_since(at) >= EVERY)
}

/// The next `refresh` reads at once (the network changed): Connections said "Not connected" for up to a minute after
/// a connect, until the clock's minute painted it again (Wi-Fi bench, 2026-10-27).
pub fn stale() {
    *ASKED.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Whether the network in use or the address differ from the last look (`seen`, which becomes `now`); the first look
/// sets it only.
fn moved<T: PartialEq>(seen: &mut Option<T>, now: T) -> bool {
    let moved = seen.as_ref().is_some_and(|s| *s != now);
    *seen = Some(now);
    moved
}

/// Calls `notify` (a repaint) and makes the facts stale whenever the shared Wi-Fi picture names another network in use
/// or another address. Once per run; the subscription lives as long as pocket-ui.
pub fn watch(net: &Arc<NetState>, notify: impl Fn() + Send + Sync + 'static) {
    static SUB: OnceLock<Arc<dyn Fn() + Send + Sync>> = OnceLock::new();
    if SUB.get().is_some() {
        return;
    }
    let watched = Arc::downgrade(net);
    let seen = Mutex::new(None::<(Option<String>, Option<String>)>);
    let sub: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        let Some(snap) = watched.upgrade().map(|n| n.snapshot()) else { return };
        let now = (snap.connected().map(str::to_string), snap.saving.ip.clone());
        if moved(&mut seen.lock().unwrap_or_else(|e| e.into_inner()), now) {
            stale();
            notify();
        }
    });
    if SUB.set(sub.clone()).is_ok() {
        net.subscribe(&sub);
        sub();
    }
}

/// Called while a category's list is painted: starts a read on a thread when one is due (two atomics and a lock
/// otherwise). `notify` is called from the thread when what it read differs from what was there.
pub fn refresh(hw: SharedHw, net: Arc<NetState>, data_dir: PathBuf, notify: impl Fn() + Send + 'static) {
    let now = Instant::now();
    {
        let mut asked = ASKED.lock().unwrap_or_else(|e| e.into_inner());
        let corner = if crate::status_corner::status().wifi { 2 } else { 1 };
        let corner_moved = CORNER_WIFI.load(Ordering::SeqCst) != corner;
        if !due(*asked, READING.load(Ordering::SeqCst), now) && !(corner_moved && !READING.load(Ordering::SeqCst)) {
            return;
        }
        CORNER_WIFI.store(corner, Ordering::SeqCst);
        *asked = Some(now);
        READING.store(true, Ordering::SeqCst);
    }
    let spawned = std::thread::Builder::new().name("settings-values".into()).spawn(move || {
        let facts = read(&*hw, &net, &data_dir, crate::status_corner::wifi_now());
        let changed = {
            let mut kept = FACTS.lock().unwrap_or_else(|e| e.into_inner());
            let changed = kept.as_ref() != Some(&facts);
            *kept = Some(facts);
            changed
        };
        READING.store(false, Ordering::SeqCst);
        if changed {
            notify();
        }
    });
    if spawned.is_err() {
        READING.store(false, Ordering::SeqCst);
    }
}

/// Fits `value` ("Home net >") into `max_w` pixels as `width` measures it, cutting the part before " >" with "…":
/// the ">" of a page always stays.
pub fn fit(value: &str, max_w: i32, mut width: impl FnMut(&str) -> i32) -> String {
    if width(value) <= max_w {
        return value.to_string();
    }
    let (core, tail) = value.strip_suffix(" >").map_or((value, ""), |c| (c, " >"));
    let mut chars: Vec<char> = core.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let cut: String = chars.iter().collect::<String>().trim_end().to_string();
        let s = format!("{cut}…{tail}");
        if width(&s) <= max_w {
            return s;
        }
    }
    format!("…{tail}")
}

/// A value split into its decoration and its words: ("‹ ", "On", " ›") for a switch, ("", "Home net", " >") for a page,
/// ("", "", ">") for a page with no value yet. The painter draws the words alone (`kit::row_value`), so the words are
/// what is looked up in the translation table: "Shelf >" is no line of it, and asking for it ran every pattern.
pub fn split_decoration(value: &str) -> (&str, &str, &str) {
    if let Some(v) = value.strip_prefix("‹ ").and_then(|v| v.strip_suffix(" ›")) {
        return ("‹ ", v, " ›");
    }
    match value.strip_suffix('>') {
        Some(v) if v.trim_end().is_empty() => ("", "", ">"),
        Some(v) => ("", v.trim_end(), " >"),
        None => ("", value, ""),
    }
}

/// `words` in `lang` where the code can say it: "Auto · Quiet" is two lines of the table with a dot between, "2 min" a
/// figure with a unit, in the language's own way ("2분": the Korean table's "{n} min" line; 22.11.2026 the System page
/// said "2 min" in Korean because a figure was taken to read the same in every language). `None`: a line of the table
/// itself (its exact lookup is no pattern), English, or something only the patterns know ("332 games").
pub fn translated_in_code(lang: crate::i18n::Lang, words: &str) -> Option<String> {
    let table = lang.table()?;
    if table.exact(words).is_some() {
        return None;
    }
    let mut out = Vec::new();
    for part in words.split(" · ") {
        match table.exact(part) {
            Some(t) => out.push(t.to_string()),
            // Written as in English (French's no-break space aside: the painter keeps that one's "2 min" as it was).
            None if is_measure(part) => match table.translate(part).filter(|t| t.replace('\u{a0}', " ") != part) {
                Some(own) => out.push(own.into_owned()),
                None => out.push(part.to_string()),
            },
            None => return None,
        }
    }
    let joined = out.join(" · ");
    // The result is painted as it is: a line of the table would be translated a second time.
    table.exact(&joined).is_none().then_some(joined)
}

/// "2 min", "30 s", "8 h": a number and a unit.
fn is_measure(s: &str) -> bool {
    s.split_once(' ').is_some_and(|(n, unit)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) && matches!(unit, "min" | "s" | "h"))
}

/// [`fit`] for a value the painter translates when it paints it. The cut must be made in the words the player reads:
/// cutting the English and painting the piece left "No PC paire…" in German, a string the table can't know. So the
/// value is measured as painted (`width` translates); if it fits it stays as it is (the painter translates it), else
/// `translate` gives its words and those are cut.
pub fn fit_translated(value: &str, max_w: i32, translate: impl FnOnce(&str) -> String, mut width: impl FnMut(&str) -> i32) -> String {
    if width(value) <= max_w {
        return value.to_string();
    }
    fit(&translate(value), max_w, width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_ctl::{MockHw, MockState};
    use crate::testutil::TestDir;

    #[test]
    fn a_value_is_split_into_its_decoration_and_its_words() {
        assert_eq!(split_decoration("Shelf >"), ("", "Shelf", " >"));
        assert_eq!(split_decoration(">"), ("", "", ">"));
        assert_eq!(split_decoration("‹ Home ›"), ("‹ ", "Home", " ›"));
        assert_eq!(split_decoration("Off"), ("", "Off", ""));
    }

    /// What the code says in the language without the patterns: parts of the table joined, a figure and its unit.
    #[test]
    fn a_value_of_table_lines_and_figures_is_said_in_code() {
        use crate::i18n::Lang;
        for lang in [Lang::De, Lang::Es, Lang::Pt, Lang::Fr, Lang::It] {
            let t = lang.table().unwrap();
            let quiet = t.exact("Quiet").unwrap();
            assert_eq!(translated_in_code(lang, "Auto · Quiet"), Some(format!("{} · {quiet}", t.exact("Auto").unwrap())), "{lang:?}");
            assert_eq!(translated_in_code(lang, "2 min").as_deref(), Some("2 min"), "{lang:?}");
            assert_eq!(translated_in_code(lang, "30 s").as_deref(), Some("30 s"), "{lang:?}");
            // A line of the table is the painter's (an exact look-up), what only a pattern knows stays with it.
            assert_eq!(translated_in_code(lang, "Quiet"), None, "{lang:?}");
            assert_eq!(translated_in_code(lang, "332 games"), None, "{lang:?}");
            assert_eq!(translated_in_code(lang, "Auto · Zzz"), None, "{lang:?}");
        }
        assert_eq!(translated_in_code(Lang::En, "Auto · Quiet"), None);
        // A language that writes a figure's unit its own way says it so (Korean, not offered yet but with its lines).
        assert_eq!(translated_in_code(Lang::Ko, "2 min").as_deref(), Some("2분"));
        assert_eq!(translated_in_code(Lang::Ko, "30 s").as_deref(), Some("30초"));
        assert_eq!(translated_in_code(Lang::Ko, "Auto · 8 h").as_deref(), Some("자동 · 8시간"));
    }

    #[test]
    fn values_are_short_and_plain() {
        assert_eq!(look(Theme::Shelf, Appearance::Dark), "Shelf · Dark");
        assert_eq!(look(Theme::Cabinet, Appearance::Auto), "Cabinet · Automatic");
        let defaults = crate::perf_fan::Prefs::default();
        assert_eq!(perf_fan_with(defaults.games_performance, defaults.games_fan, true), "Auto · Quiet", "the defaults");
        assert_eq!(perf_fan_with(crate::power::Performance::Performance, crate::power::Fan::Cool, true), "Max · Cool");
        // No fan (the H700 handhelds, the Brick): the row names none.
        assert_eq!(perf_fan_with(defaults.games_performance, defaults.games_fan, false), "Auto");
        assert_eq!([games(0), games(1), games(332)], ["No games", "1 game", "332 games"]);
        assert_eq!([pcs(0), pcs(1), pcs(2)], ["No PC paired", "1 PC", "2 PCs"]);
        assert_eq!([hidden_systems(0), hidden_systems(1), hidden_systems(3)], ["", "1 hidden", "3 hidden"]);
        assert_eq!(zone("Europe/Vienna"), "Vienna");
        assert_eq!(zone("America/Indiana/Indianapolis"), "Indianapolis, Indiana");
        assert_eq!(zone("Etc/UTC"), "UTC");
        assert_eq!(updates("0.42.0", None), "0.42.0");
        assert_eq!(updates("0.42.0", Some("0.43.0")), "0.43.0 available");
        // 17.11.2026 (Android, German): the Updates row said "0.5.0-android.10 available" in English in every language.
        use crate::i18n::{tr_in, Lang};
        for (lang, word) in [(Lang::De, "verfügbar"), (Lang::Es, "disponible"), (Lang::Fr, "disponible"), (Lang::It, "disponibile"), (Lang::Pt, "disponível")] {
            assert_eq!(tr_in(lang, &updates("0.5.0", Some("0.5.0-android.10"))), format!("0.5.0-android.10 {word}"), "{lang:?}");
        }
        assert_eq!(page("On"), "On >");
        assert_eq!(page(""), ">", "nothing known yet: the plain arrow");
        let wifi = [Wifi::Off, Wifi::NotConnected, Wifi::Connected(Some("Home net".into())), Wifi::Connected(None)];
        assert_eq!(wifi.map(|w| w.label()), ["Off", "Not connected", "Home net", "Connected"]);
    }

    #[test]
    fn a_long_value_is_cut_with_an_ellipsis_and_keeps_its_arrow() {
        // 10 px a character.
        let w = |s: &str| s.chars().count() as i32 * 10;
        assert_eq!(fit("Home >", 200, w), "Home >", "short enough: as it is");
        assert_eq!(fit("A very long network name >", 120, w), "A very lo… >");
        let cut = fit("A very long network name >", 120, w);
        assert!(w(&cut) <= 120 && cut.ends_with("… >"), "{cut:?}");
        let cut = fit("Indianapolis, Indiana", 100, w);
        assert!(w(&cut) <= 100 && cut.ends_with('…'), "{cut:?}");
        assert_eq!(fit("Whatever >", 30, w), "… >", "no room at all: the arrow stays");
    }

    #[test]
    fn a_value_too_long_in_the_players_language_is_cut_in_that_language() {
        // 10 px a character; the painter measures the German of "No PC paired >" and paints a cut piece as it is.
        let de = |s: &str| if s == "No PC paired >" { "Kein PC gekoppelt >".to_string() } else { s.to_string() };
        let w = |s: &str| de(s).chars().count() as i32 * 10;
        assert_eq!(fit_translated("No PC paired >", 300, de, w), "No PC paired >", "fits as painted: left to the painter");
        let cut = fit_translated("No PC paired >", 150, de, w);
        assert_eq!(cut, "Kein PC geko… >", "cut from the German, not from the English");
        assert!(!cut.contains("PC paire"), "{cut:?}");
    }

    #[test]
    fn a_new_network_or_address_makes_the_facts_stale_at_once() {
        let mut seen = None;
        assert!(!moved(&mut seen, (None::<String>, None::<String>)), "the first look only sets it");
        assert!(!moved(&mut seen, (None, None)));
        assert!(moved(&mut seen, (Some("Cafe".into()), None)), "connected");
        assert!(moved(&mut seen, (Some("Cafe".into()), Some("192.0.2.5".into()))), "the address came");
        assert!(!moved(&mut seen, (Some("Cafe".into()), Some("192.0.2.5".into()))), "a scan that changes nothing else");
        *ASKED.lock().unwrap() = Some(Instant::now());
        assert!(!due(*ASKED.lock().unwrap(), false, Instant::now()));
        stale();
        assert!(due(*ASKED.lock().unwrap(), false, Instant::now()), "Connections reads again at its next paint");
    }

    #[test]
    fn a_read_is_due_once_every_few_seconds_and_never_twice_at_once() {
        let t0 = Instant::now();
        assert!(due(None, false, t0), "the first list shown");
        assert!(!due(Some(t0), false, t0 + EVERY / 2));
        assert!(due(Some(t0), false, t0 + EVERY));
        assert!(!due(Some(t0), true, t0 + EVERY * 3), "one read at a time");
    }

    #[test]
    fn read_says_off_or_not_connected_and_counts_the_pcs() {
        let dir = TestDir::new("values");
        crate::moonlight::save_pcs(&dir, &["192.168.1.40".to_string()]).unwrap();
        let hw = Arc::new(MockHw::new(MockState { wifi: false, wifi_wanted: false, zone: "Europe/Vienna".into(), ..MockState::default() }));
        let net = NetState::inline(Arc::new(crate::net_state::HwNet::new(hw.clone())));
        let f = read(&*hw, &net, &dir, false);
        assert_eq!(f.wifi, Some(Wifi::Off), "switched off");
        assert_eq!(f.zone.as_deref(), Some("Europe/Vienna"));
        assert!(f.pcs.is_some_and(|n| n >= 1), "our list, and ROCKNIX's host if this machine has one");
        let on = Arc::new(MockHw::new(MockState { wifi: true, ..MockState::default() }));
        let net = NetState::inline(Arc::new(crate::net_state::HwNet::new(on.clone())));
        assert_eq!(read(&*on, &net, &dir, false).wifi, Some(Wifi::NotConnected), "on, but no network");
        assert!(matches!(read(&*on, &net, &dir, true).wifi, Some(Wifi::Connected(_))));
        // The row says what the shared picture says: off by NetworkManager's switch counts, as on the other screens.
        let fake = crate::net_state::fake::FakeNet::new(Radio::Off(crate::net_state::Cause::Manager));
        let net = NetState::inline(fake);
        assert_eq!(read(&*on, &net, &dir, false).wifi, Some(Wifi::Off));
    }
}
