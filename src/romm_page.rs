//! Settings → Games → RomM: the state of the page and what a press on it asks for. The App turns an [`Action`] into work on
//! a thread (`app_romm.rs`) and reports back with an [`Event`]; painting is in the binary. Nothing here touches the network
//! or a file, so every path is a plain test.
//!
//! Not paired: the server's address (typed once with the gamepad keyboard), **Pair with QR code** (RomM's device flow: the
//! code and its QR code on screen, approved on the phone) and **Pair with a code** (RomM 4.8 and newer: the 8 digits of a
//! Client API Token's pairing). Paired: the address, **Refresh the games** and **Unpair** (asks once more; downloaded games
//! stay on the card).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::keyboard::{Keyboard, Outcome};
use crate::pad::Button;
use crate::romm::{Account, Failure, Pairing};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// The server's address: A types it (a new one while paired pairs with that one).
    Server,
    PairQr,
    PairCode,
    Refresh,
    Unpair,
}

const UNPAIRED: [Row; 3] = [Row::Server, Row::PairQr, Row::PairCode];
const PAIRED: [Row; 3] = [Row::Server, Row::Refresh, Row::Unpair];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Typing {
    Server,
    Code,
}

/// What the keyboard is for once it's done: pair straight on after the address was typed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Then {
    Nothing,
    PairQr,
    PairCode,
}

pub enum View {
    List,
    /// RomM's device flow is under way: its code and address once RomM gave them.
    Pairing { code: Option<Pairing> },
    Keyboard(Typing, Keyboard, Then),
}

/// How the list looks, for the status under the rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub games: usize,
    pub systems: usize,
    pub skipped: usize,
    /// When it was read (seconds since 1970).
    pub fetched: u64,
    pub cut: bool,
}

impl Summary {
    pub fn of(cat: &crate::romm::Catalog) -> Summary {
        Summary { games: cat.games.len(), systems: cat.systems(), skipped: cat.skipped, fetched: cat.fetched, cut: cat.cut }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    Close,
    /// Keep the address typed (before it is paired).
    SaveServer(String),
    StartPairing { server: String },
    ExchangeCode { server: String, code: String },
    Refresh,
    Unpair,
}

/// News for the page from its threads.
#[derive(Debug)]
pub enum Event {
    /// RomM's code for the QR screen, or why there is none.
    Code(Result<Pairing, Failure>),
    /// Paired with this account (or why not).
    Paired(Result<Account, Failure>),
    /// The list was read.
    Listed(Result<Summary, Failure>),
}

pub struct RommPage {
    pub sel: usize,
    pub view: View,
    pub account: Option<Account>,
    /// The address typed before pairing (normalized).
    pub server: Option<String>,
    pub summary: Option<Summary>,
    /// "Reading the list" while a thread works.
    pub busy: Option<&'static str>,
    pub error: Option<String>,
    /// The first press on Unpair asks; the second does it.
    pub confirm_unpair: bool,
    /// Ends the pairing's questions when set; a new one for each pairing.
    pub cancel: Arc<AtomicBool>,
}

impl RommPage {
    pub fn new(account: Option<Account>, server: Option<String>, summary: Option<Summary>) -> RommPage {
        let server = server.or_else(|| account.as_ref().map(|a| a.server.clone()));
        RommPage { sel: 0, view: View::List, account, server, summary, busy: None, error: None, confirm_unpair: false, cancel: Arc::new(AtomicBool::new(false)) }
    }

    pub fn rows(&self) -> &'static [Row] {
        if self.account.is_some() { &PAIRED } else { &UNPAIRED }
    }

    pub fn row(&self) -> Row {
        let rows = self.rows();
        rows[self.sel.min(rows.len() - 1)]
    }

    pub fn paired(&self) -> bool {
        self.account.is_some()
    }

    pub fn label(&self, row: Row) -> &'static str {
        match row {
            Row::Server => "Server",
            Row::PairQr => "Pair with QR code",
            Row::PairCode => "Pair with a code",
            Row::Refresh => "Refresh the games",
            Row::Unpair if self.confirm_unpair => "Press A again to unpair",
            Row::Unpair => "Unpair",
        }
    }

    pub fn value(&self, row: Row) -> String {
        match row {
            Row::Server => self.server.as_deref().map(crate::romm::shown_server).map(str::to_string).unwrap_or_else(|| "Not set".into()),
            Row::Refresh => self.summary.as_ref().map(|s| games_line(s.games)).unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// The line under the rows for the selected row.
    pub fn describe(&self, row: Row) -> &'static str {
        match row {
            Row::Server => "Your RomM server's address, as you open it in a browser, e.g. 192.0.2.10:8080.",
            Row::PairQr => "A code appears. Scan it with your phone, sign in to RomM and approve this Pocket. Needs RomM 5.0 or newer.",
            Row::PairCode => "In RomM: your profile › Client API Tokens › Pair. Type the 8-digit code it shows.",
            Row::Refresh => "Read the list of games on the server again.",
            Row::Unpair => "This Pocket forgets the server. Games you downloaded stay on the card.",
        }
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let typed = match &mut self.view {
            View::Keyboard(typing, kb, then) => Some((*typing, *then, kb.press(b))),
            _ => None,
        };
        if let Some((typing, then, outcome)) = typed {
            return match outcome {
                Outcome::Editing => Action::Nothing,
                Outcome::Cancelled => {
                    self.view = View::List;
                    Action::Nothing
                }
                Outcome::Done(text) => self.typed(typing, then, &text),
            };
        }
        if matches!(self.view, View::Pairing { .. }) {
            if b == cancel {
                self.cancel.store(true, Ordering::Relaxed);
                self.view = View::List;
                self.busy = None;
            }
            return Action::Nothing;
        }
        let n = self.rows().len();
        if b != confirm {
            self.confirm_unpair = false;
        }
        if b == Button::Up {
            self.sel = (self.sel + n - 1) % n;
        } else if b == Button::Down {
            self.sel = (self.sel + 1) % n;
        } else if b == cancel {
            self.cancel.store(true, Ordering::Relaxed);
            return Action::Close;
        } else if b == confirm {
            return self.activate();
        }
        Action::Nothing
    }

    fn keyboard(&mut self, typing: Typing, then: Then) {
        let text = match typing {
            Typing::Server => self.server.as_deref().map(crate::romm::shown_server).unwrap_or_default().to_string(),
            Typing::Code => String::new(),
        };
        self.view = View::Keyboard(typing, Keyboard { text, ..Default::default() }, then);
    }

    fn activate(&mut self) -> Action {
        if self.busy.is_some() {
            return Action::Nothing;
        }
        self.error = None;
        match self.row() {
            Row::Server => {
                self.keyboard(Typing::Server, Then::Nothing);
                Action::Nothing
            }
            Row::PairQr => match self.server.clone() {
                Some(server) => self.start_pairing(server),
                None => {
                    self.keyboard(Typing::Server, Then::PairQr);
                    Action::Nothing
                }
            },
            Row::PairCode => {
                if self.server.is_some() {
                    self.keyboard(Typing::Code, Then::Nothing);
                } else {
                    self.keyboard(Typing::Server, Then::PairCode);
                }
                Action::Nothing
            }
            Row::Refresh => {
                self.busy = Some("Reading the list of games");
                Action::Refresh
            }
            Row::Unpair => {
                if !self.confirm_unpair {
                    self.confirm_unpair = true;
                    return Action::Nothing;
                }
                self.confirm_unpair = false;
                self.account = None;
                self.summary = None;
                self.sel = 0;
                Action::Unpair
            }
        }
    }

    fn start_pairing(&mut self, server: String) -> Action {
        self.cancel = Arc::new(AtomicBool::new(false));
        self.view = View::Pairing { code: None };
        self.busy = Some("Talking to RomM");
        Action::StartPairing { server }
    }

    fn typed(&mut self, typing: Typing, then: Then, text: &str) -> Action {
        self.view = View::List;
        match typing {
            Typing::Server => {
                let server = match crate::romm::normalize_server(text) {
                    Ok(s) => s,
                    Err(why) => {
                        self.error = Some(why.into());
                        return Action::Nothing;
                    }
                };
                let changed = self.account.as_ref().is_some_and(|a| a.server != server);
                self.server = Some(server.clone());
                match then {
                    Then::PairQr => self.start_pairing(server),
                    Then::PairCode => {
                        self.keyboard(Typing::Code, Then::Nothing);
                        Action::SaveServer(server)
                    }
                    // Paired, and another address: pair with that one (the old pairing stays until the new one is approved).
                    Then::Nothing if changed => self.start_pairing(server),
                    Then::Nothing => {
                        if !self.paired() {
                            self.sel = 1;
                        }
                        Action::SaveServer(server)
                    }
                }
            }
            Typing::Code => {
                let code: String = text.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
                let Some(server) = self.server.clone() else { return Action::Nothing };
                if code.len() < 6 {
                    self.error = Some(Failure::BadCode.words());
                    return Action::Nothing;
                }
                self.busy = Some("Talking to RomM");
                Action::ExchangeCode { server, code }
            }
        }
    }

    pub fn event(&mut self, e: Event) {
        match e {
            Event::Code(Ok(p)) => {
                if matches!(self.view, View::Pairing { .. }) {
                    self.view = View::Pairing { code: Some(p) };
                }
            }
            Event::Code(Err(f)) => {
                self.view = View::List;
                self.busy = None;
                if f == Failure::OldServer {
                    self.sel = self.rows().iter().position(|r| *r == Row::PairCode).unwrap_or(0);
                }
                self.error = Some(f.words());
            }
            Event::Paired(Ok(account)) => {
                self.view = View::List;
                self.server = Some(account.server.clone());
                self.account = Some(account);
                self.summary = None;
                self.sel = 1;
                self.busy = Some("Reading the list of games");
            }
            Event::Paired(Err(f)) => {
                // B ended the pairing already: nothing to say.
                if f == Failure::Stopped {
                    return;
                }
                self.view = View::List;
                self.busy = None;
                self.error = Some(f.words());
            }
            Event::Listed(r) => {
                self.busy = None;
                match r {
                    Ok(s) => {
                        self.summary = Some(s);
                        self.error = None;
                    }
                    Err(Failure::SignedOut) => {
                        self.error = Some(Failure::SignedOut.words());
                    }
                    Err(f) => self.error = Some(f.words()),
                }
            }
        }
    }
}

/// "1 game", "12 games".
pub fn games_line(n: usize) -> String {
    if n == 1 { "1 game".into() } else { format!("{n} games") }
}

/// "12 games in 3 systems", "1 game in 1 system".
pub fn summary_line(s: &Summary) -> String {
    let systems = if s.systems == 1 { "1 system".to_string() } else { format!("{} systems", s.systems) };
    format!("{} in {systems}", games_line(s.games))
}

/// "just now", "5 min ago", "3 h ago", "2 days ago".
pub fn age_line(fetched: u64, now: u64) -> String {
    let secs = now.saturating_sub(fetched);
    match secs {
        0..=89 => "Read just now".into(),
        90..=3599 => format!("Read {} min ago", secs / 60),
        3600..=172_799 => format!("Read {} h ago", secs / 3600),
        _ => format!("Read {} days ago", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn type_text(p: &mut RommPage, text: &str) -> Action {
        if let View::Keyboard(_, kb, _) = &mut p.view {
            kb.text = text.into();
        }
        p.press(Button::Start, A, B)
    }

    fn account() -> Account {
        Account { server: "http://nas:8080".into(), user: "sam".into(), device_id: None }
    }

    #[test]
    fn a_first_pairing_asks_for_the_address_then_shows_the_code() {
        let mut p = RommPage::new(None, None, None);
        assert_eq!(p.rows(), &[Row::Server, Row::PairQr, Row::PairCode]);
        assert_eq!(p.value(Row::Server), "Not set");
        p.sel = 1;
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert!(matches!(p.view, View::Keyboard(Typing::Server, _, Then::PairQr)), "no address yet: the keyboard first");
        assert_eq!(type_text(&mut p, "nas:8080"), Action::StartPairing { server: "http://nas:8080".into() });
        assert!(matches!(p.view, View::Pairing { code: None }));
        p.event(Event::Code(Ok(Pairing { device_code: "d".into(), user_code: "M9JXERPY".into(), url: "http://nas:8080/pair/device?user_code=M9JXERPY".into(), interval: 5, expires_in: 600 })));
        assert!(matches!(&p.view, View::Pairing { code: Some(c) } if c.user_code == "M9JXERPY"));
        p.event(Event::Paired(Ok(account())));
        assert!(matches!(p.view, View::List));
        assert_eq!(p.rows(), &[Row::Server, Row::Refresh, Row::Unpair]);
        assert_eq!(p.busy, Some("Reading the list of games"));
        p.event(Event::Listed(Ok(Summary { games: 12, systems: 3, ..Summary::default() })));
        assert_eq!((p.busy, p.value(Row::Refresh)), (None, "12 games".to_string()));
    }

    #[test]
    fn b_ends_the_pairing_and_the_page() {
        let mut p = RommPage::new(None, Some("http://nas:8080".into()), None);
        p.sel = 1;
        assert_eq!(p.press(A, A, B), Action::StartPairing { server: "http://nas:8080".into() });
        let cancel = p.cancel.clone();
        assert_eq!(p.press(B, A, B), Action::Nothing);
        assert!(cancel.load(Ordering::Relaxed), "the questions to RomM stop");
        assert!(matches!(p.view, View::List) && p.busy.is_none());
        p.event(Event::Paired(Err(Failure::Stopped)));
        assert!(p.error.is_none(), "a pairing B ended says nothing more");
        assert_eq!(p.press(B, A, B), Action::Close);
    }

    #[test]
    fn a_wrong_address_or_an_old_server_is_said() {
        let mut p = RommPage::new(None, None, None);
        p.press(A, A, B);
        assert_eq!(type_text(&mut p, "ftp://nas"), Action::Nothing);
        assert!(p.error.as_deref().is_some_and(|e| e.contains("http")));
        assert_eq!(p.server, None);
        p.sel = 0;
        p.press(A, A, B);
        assert_eq!(type_text(&mut p, "192.0.2.10:8080"), Action::SaveServer("http://192.0.2.10:8080".into()));
        assert_eq!(p.row(), Row::PairQr, "on to the next step");
        p.press(A, A, B);
        p.event(Event::Code(Err(Failure::OldServer)));
        assert_eq!(p.row(), Row::PairCode, "an older RomM: pair with a code instead");
        assert!(p.error.as_deref().unwrap().contains("Pair with a code"));
        p.press(A, A, B);
        assert!(matches!(p.view, View::Keyboard(Typing::Code, _, _)));
        assert_eq!(type_text(&mut p, "1234 5678"), Action::ExchangeCode { server: "http://192.0.2.10:8080".into(), code: "12345678".into() });
        p.event(Event::Paired(Err(Failure::BadCode)));
        assert_eq!(p.error, Some(Failure::BadCode.words()));
    }

    #[test]
    fn unpair_asks_once_more() {
        let mut p = RommPage::new(Some(account()), None, Some(Summary { games: 2, systems: 1, ..Summary::default() }));
        assert_eq!(p.value(Row::Server), "nas:8080");
        p.sel = 2;
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.label(Row::Unpair), "Press A again to unpair");
        p.press(Button::Up, A, B);
        p.press(Button::Down, A, B);
        assert_eq!(p.label(Row::Unpair), "Unpair", "a move forgets the question");
        p.press(A, A, B);
        assert_eq!(p.press(A, A, B), Action::Unpair);
        assert!(!p.paired());
        assert_eq!(p.rows(), &UNPAIRED);
        assert_eq!(p.value(Row::Server), "nas:8080", "the address stays for a new pairing");
    }

    #[test]
    fn a_new_address_while_paired_pairs_with_it() {
        let mut p = RommPage::new(Some(account()), None, None);
        p.press(A, A, B);
        assert_eq!(type_text(&mut p, "nas:8080"), Action::SaveServer("http://nas:8080".into()), "the same server: nothing to do");
        p.press(A, A, B);
        assert_eq!(type_text(&mut p, "https://romm.example.com"), Action::StartPairing { server: "https://romm.example.com".into() });
        assert!(p.paired(), "the old pairing holds until the new one is approved");
    }

    #[test]
    fn the_lines_read_well() {
        assert_eq!(summary_line(&Summary { games: 1, systems: 1, ..Summary::default() }), "1 game in 1 system");
        assert_eq!(summary_line(&Summary { games: 12, systems: 3, ..Summary::default() }), "12 games in 3 systems");
        assert_eq!(age_line(1000, 1030), "Read just now");
        assert_eq!(age_line(1000, 1000 + 300), "Read 5 min ago");
        assert_eq!(age_line(1000, 1000 + 3 * 3600), "Read 3 h ago");
        assert_eq!(age_line(1000, 1000 + 3 * 86_400), "Read 3 days ago");
        let p = RommPage::new(None, None, None);
        for r in [Row::Server, Row::PairQr, Row::PairCode, Row::Refresh, Row::Unpair] {
            assert!(!p.describe(r).is_empty() && !p.label(r).is_empty());
        }
    }
}
