//! Settings → PC streaming: the state of the page and what a press asks for. The App pairs, unpairs and gets a PC's
//! games on threads (`moonlight::pair_command`, `list_apps`, `write_games`, `unpair`) and reports back with an
//! `Event`; painting is in the binary (`app_moonlight.rs`). Nothing here touches the system, so every path is a test.
//!
//! The list: "Pair a PC", the paired PCs (the default one, which PC streaming on Home uses, marked), then the default
//! PC's games, each of which A streams. Pairing has a page of its own: the address (empty: Moonlight looks for the
//! PC), then the PIN, big, until the PC accepted it.

use crate::keyboard::{Keyboard, Outcome};
use crate::moonlight::{valid_host, PairNews};
use crate::pad::Button;

/// What the page knows when it opens (read on a thread).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loaded {
    /// Moonlight is on this system.
    pub installed: bool,
    pub pcs: Vec<String>,
    /// `moonlight.host` in ROCKNIX's system.cfg: the PC streaming on Home uses.
    pub default: Option<String>,
    /// The default PC's games (`moonlight::written_games`).
    pub games: Vec<String>,
}

/// News for the page from its threads.
#[derive(Debug)]
pub enum Event {
    Loaded(Loaded),
    /// A line of `moonlight pair`.
    Pair(PairNews),
    /// `moonlight pair` ended: the PC it paired with, or why not.
    PairEnded(Result<String, String>),
    /// The PC's games were fetched and written (`moonlight::write_games`), or why not.
    Games { host: String, result: Result<Vec<String>, String> },
    /// `moonlight unpair` ended; the PC is off pocket-ui's list either way.
    Unpaired { host: String, result: Result<(), String> },
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    /// Pair with the PC at this address, or with the one Moonlight finds.
    Pair(Option<String>),
    /// Stop the pairing under way.
    CancelPair,
    /// Make this PC the default and get its games.
    Games(String),
    Unpair(String),
    /// Stream this game of the default PC.
    Stream { host: String, app: String },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    Pair,
    Pc(usize),
    Game(usize),
}

/// Pairing under way: the address typed (None: Moonlight looks), the PC it talks to, the PIN once Moonlight said it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pairing {
    pub asked: Option<String>,
    pub searching: bool,
    pub host: Option<String>,
    pub pin: Option<String>,
}

#[derive(Default)]
pub struct MoonlightPage {
    /// None until the first load.
    pub loaded: Option<Loaded>,
    pub sel: usize,
    /// Typing the PC's address.
    pub address: Option<Keyboard>,
    pub pairing: Option<Pairing>,
    /// Work under way besides pairing: what to say.
    pub busy: Option<String>,
    /// X was pressed on this PC once: the next X unpairs it.
    pub confirm_unpair: Option<String>,
    /// The last result: text, is an error.
    pub message: Option<(String, bool)>,
    /// The Pocket has no address (`wifi::ip`, set by the app before each press): pairing, a PC's games and streaming
    /// all need the network, so A says so instead of opening the address keyboard (the first-day run, 2026-10-26).
    pub offline: bool,
}

/// A pressed on a row that needs the network, without one.
pub const OFFLINE: &str = "Connect to Wi-Fi first (Settings › Connections › Wi-Fi).";

impl MoonlightPage {
    pub fn new() -> MoonlightPage {
        MoonlightPage::default()
    }

    pub fn rows(&self) -> Vec<Row> {
        let Some(l) = &self.loaded else { return vec![Row::Pair] };
        let mut rows = vec![Row::Pair];
        rows.extend((0..l.pcs.len()).map(Row::Pc));
        if l.default.is_some() {
            rows.extend((0..l.games.len()).map(Row::Game));
        }
        rows
    }

    pub fn row(&self) -> Row {
        self.rows().get(self.sel).copied().unwrap_or(Row::Pair)
    }

    pub fn pc(&self, i: usize) -> Option<&str> {
        self.loaded.as_ref()?.pcs.get(i).map(String::as_str)
    }

    pub fn game(&self, i: usize) -> Option<&str> {
        self.loaded.as_ref()?.games.get(i).map(String::as_str)
    }

    pub fn default_pc(&self) -> Option<&str> {
        self.loaded.as_ref()?.default.as_deref()
    }

    pub fn installed(&self) -> bool {
        self.loaded.as_ref().is_some_and(|l| l.installed)
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        if self.pairing.is_some() {
            return if b == cancel { Action::CancelPair } else { Action::Nothing };
        }
        if let Some(kb) = self.address.as_mut() {
            return match kb.press(b) {
                Outcome::Editing => Action::Nothing,
                Outcome::Cancelled => {
                    self.address = None;
                    Action::Nothing
                }
                Outcome::Done(text) => {
                    let host = text.trim().to_string();
                    if host.is_empty() {
                        self.address = None;
                        self.start_pairing(None)
                    } else if valid_host(&host) {
                        self.address = None;
                        self.start_pairing(Some(host))
                    } else {
                        self.message = Some(("That isn't an address. Type a name like gaming-pc.local or numbers like 192.168.1.20.".into(), true));
                        Action::Nothing
                    }
                }
            };
        }
        let n = self.rows().len();
        let before = self.sel;
        let action = match b {
            b if b == cancel && self.confirm_unpair.take().is_some() => Action::Nothing,
            b if b == cancel => Action::Close,
            Button::Up => {
                self.sel = (self.sel + n - 1) % n;
                Action::Nothing
            }
            Button::Down => {
                self.sel = (self.sel + 1) % n;
                Action::Nothing
            }
            _ if self.busy.is_some() => Action::Nothing,
            Button::North => match self.row() {
                Row::Pc(i) => {
                    let host = self.pc(i).unwrap_or_default().to_string();
                    if self.confirm_unpair.as_deref() == Some(host.as_str()) {
                        self.confirm_unpair = None;
                        self.busy = Some(format!("Unpairing {host}"));
                        Action::Unpair(host)
                    } else {
                        self.confirm_unpair = Some(host);
                        Action::Nothing
                    }
                }
                _ => Action::Nothing,
            },
            b if b == confirm => match self.row() {
                Row::Pair if !self.installed() => {
                    self.message = Some(("Moonlight isn't on this system.".into(), true));
                    Action::Nothing
                }
                _ if self.offline => {
                    self.message = Some((OFFLINE.into(), true));
                    Action::Nothing
                }
                Row::Pair => {
                    // Empty: START lets Moonlight look for the PC; an address is typed for a PC it can't find.
                    self.address = Some(Keyboard::default());
                    self.message = None;
                    Action::Nothing
                }
                Row::Pc(i) => {
                    let host = self.pc(i).unwrap_or_default().to_string();
                    self.busy = Some(format!("Getting the games from {host}"));
                    Action::Games(host)
                }
                Row::Game(i) => match (self.default_pc(), self.game(i)) {
                    (Some(host), Some(app)) => Action::Stream { host: host.to_string(), app: app.to_string() },
                    _ => Action::Nothing,
                },
            },
            _ => Action::Nothing,
        };
        if self.sel != before {
            self.confirm_unpair = None;
            self.message = None;
        }
        action
    }

    fn start_pairing(&mut self, host: Option<String>) -> Action {
        self.message = None;
        self.pairing = Some(Pairing { asked: host.clone(), host: host.clone(), ..Pairing::default() });
        Action::Pair(host)
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Loaded(l) => {
                let n = 1 + l.pcs.len() + if l.default.is_some() { l.games.len() } else { 0 };
                self.sel = self.sel.min(n - 1);
                self.loaded = Some(l);
            }
            Event::Pair(news) => {
                let Some(p) = self.pairing.as_mut() else { return };
                match news {
                    PairNews::Searching => p.searching = true,
                    PairNews::Connecting(h) => {
                        p.searching = false;
                        p.host = Some(h);
                    }
                    PairNews::Pin(pin) => p.pin = Some(pin),
                    // The end comes with PairEnded, which also has the PC's address.
                    PairNews::Paired | PairNews::Failed(_) => {}
                }
            }
            Event::PairEnded(result) => {
                self.pairing = None;
                match result {
                    Ok(host) => {
                        self.message = Some((format!("Paired with {host}. Getting its games…"), false));
                        self.busy = Some(format!("Getting the games from {host}"));
                    }
                    Err(e) => self.message = Some((e, true)),
                }
            }
            Event::Games { host, result } => {
                self.busy = None;
                match result {
                    Ok(games) => {
                        let n = games.len();
                        if let Some(l) = self.loaded.as_mut() {
                            if !l.pcs.contains(&host) {
                                l.pcs.push(host.clone());
                            }
                            l.default = Some(host.clone());
                            l.games = games;
                        }
                        self.message = Some((format!("{host}: {n} game{}. PC streaming on Home uses this PC.", if n == 1 { "" } else { "s" }), false));
                    }
                    Err(e) => self.message = Some((format!("{host}: {e}"), true)),
                }
            }
            // The PC leaves the list either way: one that didn't answer may still know the Pocket, and says so.
            Event::Unpaired { host, result } => {
                self.busy = None;
                if let Some(l) = self.loaded.as_mut() {
                    l.pcs.retain(|h| *h != host);
                    if l.default.as_deref() == Some(host.as_str()) {
                        l.default = None;
                        l.games.clear();
                    }
                }
                self.sel = self.sel.min(self.rows().len() - 1);
                self.message = Some(match result {
                    Ok(()) => (format!("Unpaired {host}."), false),
                    Err(e) => (format!("{host}: {e} Taken off this list; the PC may still know this Pocket (Sunshine: PIN page, Clients)."), true),
                });
            }
        }
    }

    /// The hint for A.
    pub fn a_hint(&self) -> &'static str {
        match self.row() {
            Row::Pair => "Pair",
            Row::Pc(_) => "Use, get games",
            Row::Game(_) => "Stream",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Button = Button::East;
    const B: Button = Button::South;
    const X: Button = Button::North;

    fn page() -> MoonlightPage {
        let mut p = MoonlightPage::new();
        p.event(Event::Loaded(Loaded {
            installed: true,
            pcs: vec!["192.168.1.20".into(), "office-pc.lan".into()],
            default: Some("192.168.1.20".into()),
            games: vec!["Desktop".into(), "Steam".into()],
        }));
        p
    }

    #[test]
    fn the_list_is_pair_then_pcs_then_the_default_pcs_games() {
        let p = page();
        assert_eq!(p.rows(), [Row::Pair, Row::Pc(0), Row::Pc(1), Row::Game(0), Row::Game(1)]);
        let mut none = MoonlightPage::new();
        none.event(Event::Loaded(Loaded { installed: true, pcs: vec!["a.lan".into()], default: None, games: vec!["Steam".into()] }));
        assert_eq!(none.rows(), [Row::Pair, Row::Pc(0)], "games only of the PC Home uses");
    }

    #[test]
    fn pairing_types_the_address_then_shows_the_pin_until_the_pc_answers() {
        let mut p = page();
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.address.as_ref().unwrap().text, "", "empty: START looks for the PC");
        p.address.as_mut().unwrap().text = "-rf; x".into();
        assert_eq!(p.press(Button::Start, A, B), Action::Nothing);
        assert!(p.message.as_ref().is_some_and(|(_, err)| *err), "not an address");
        p.address.as_mut().unwrap().text = String::new();
        assert_eq!(p.press(Button::Start, A, B), Action::Pair(None), "empty: Moonlight looks for the PC");
        p.event(Event::Pair(PairNews::Searching));
        p.event(Event::Pair(PairNews::Connecting("192.168.1.30".into())));
        p.event(Event::Pair(PairNews::Pin("4712".into())));
        let pairing = p.pairing.clone().unwrap();
        assert_eq!((pairing.host.as_deref(), pairing.pin.as_deref(), pairing.searching), (Some("192.168.1.30"), Some("4712"), false));
        assert_eq!(p.press(A, A, B), Action::Nothing, "nothing but B while it waits");
        assert_eq!(p.press(B, A, B), Action::CancelPair);
        p.event(Event::PairEnded(Ok("192.168.1.30".into())));
        assert!(p.pairing.is_none() && p.busy.is_some());
        p.event(Event::Games { host: "192.168.1.30".into(), result: Ok(vec!["Steam".into()]) });
        let l = p.loaded.as_ref().unwrap();
        assert_eq!((l.pcs.len(), l.default.as_deref(), l.games.len()), (3, Some("192.168.1.30"), 1));
    }

    #[test]
    fn unpairing_takes_two_presses_of_x() {
        let mut p = page();
        p.sel = 1;
        assert_eq!(p.press(X, A, B), Action::Nothing);
        assert_eq!(p.confirm_unpair.as_deref(), Some("192.168.1.20"));
        assert_eq!(p.press(B, A, B), Action::Nothing, "B takes it back");
        assert_eq!(p.confirm_unpair, None);
        p.press(X, A, B);
        assert_eq!(p.press(X, A, B), Action::Unpair("192.168.1.20".into()));
        p.event(Event::Unpaired { host: "192.168.1.20".into(), result: Ok(()) });
        assert_eq!(p.rows(), [Row::Pair, Row::Pc(0)], "the default PC went, and its games with it");
        assert_eq!(p.press(B, A, B), Action::Close);
    }

    #[test]
    fn a_pc_becomes_the_default_and_a_game_streams() {
        let mut p = page();
        p.sel = 2;
        assert_eq!(p.press(A, A, B), Action::Games("office-pc.lan".into()));
        assert_eq!(p.press(A, A, B), Action::Nothing, "one thing at a time");
        p.event(Event::Games { host: "office-pc.lan".into(), result: Err("The PC didn't answer. Is it on?".into()) });
        p.sel = 4;
        assert_eq!(p.press(A, A, B), Action::Stream { host: "192.168.1.20".into(), app: "Steam".into() });
    }

    #[test]
    fn offline_a_says_connect_to_wifi_instead_of_opening_the_keyboard() {
        for sel in [0, 2, 4] {
            let mut p = page();
            p.offline = true;
            p.sel = sel;
            assert_eq!(p.press(A, A, B), Action::Nothing, "row {sel}: nothing that needs the network");
            assert!(p.address.is_none() && p.busy.is_none());
            assert_eq!(p.message, Some((OFFLINE.to_string(), true)));
        }
        assert!(crate::i18n::german().translate(OFFLINE).is_some(), "OFFLINE has a German line");
    }

    #[test]
    fn without_moonlight_there_is_nothing_to_pair() {
        let mut p = MoonlightPage::new();
        p.event(Event::Loaded(Loaded::default()));
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert!(p.address.is_none() && p.message.is_some());
    }
}
