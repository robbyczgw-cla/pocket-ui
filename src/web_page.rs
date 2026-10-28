//! Settings > Connections > Web companion: the state of the page and what a press on it asks for. The App starts and stops the
//! server, makes pairing codes and revokes browsers (`app_web.rs`); painting is in the binary too. Nothing here
//! touches the system, so every path is a plain test.

use crate::pad::Button;
use crate::web::Prefs;

/// A row of the page, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    Switch,
    Pair,
    Browsers,
    Tailnet,
}

pub const ROWS: [Row; 4] = [Row::Switch, Row::Pair, Row::Browsers, Row::Tailnet];

/// A paired browser as the page lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct Paired {
    pub id: String,
    pub name: String,
    /// "23 Oct".
    pub when: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum View {
    List,
    /// The pairing code and its QR code.
    Pairing,
    /// The paired browsers. A asks, A again revokes the selected one.
    Browsers { sel: usize, confirm: bool },
}

/// What a press asks the App to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Nothing,
    Close,
    /// Save these choices; the server starts, stops or listens anew as they say.
    Save(Prefs),
    /// Make a pairing code and show it.
    StartPairing,
    /// The code is no longer shown: it can't be used any more.
    StopPairing,
    Revoke(String),
}

pub struct WebPage {
    pub sel: usize,
    pub view: View,
    pub prefs: Prefs,
    pub browsers: Vec<Paired>,
    /// The code on screen while pairing.
    pub code: Option<String>,
    /// Seconds the code is still valid.
    pub code_left: u64,
    /// "http://pocket", once the server listens on the tailnet; the addresses it listens on.
    pub address: Option<String>,
    pub addresses: Vec<String>,
    /// Why it does not listen although it is on (port 80 taken).
    pub problem: Option<String>,
}

pub fn on_off(on: bool) -> &'static str {
    if on {
        "On"
    } else {
        "Off"
    }
}

impl WebPage {
    pub fn new(prefs: Prefs) -> WebPage {
        WebPage { sel: 0, view: View::List, prefs, browsers: Vec::new(), code: None, code_left: 0, address: None, addresses: Vec::new(), problem: None }
    }

    pub fn row(&self) -> Row {
        ROWS[self.sel.min(ROWS.len() - 1)]
    }

    pub fn press(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        match self.view.clone() {
            View::List => self.press_list(b, confirm, cancel),
            View::Pairing => {
                if b == cancel || b == confirm {
                    self.view = View::List;
                    self.code = None;
                    return Action::StopPairing;
                }
                Action::Nothing
            }
            View::Browsers { sel, confirm: asked } => {
                let n = self.browsers.len();
                match b {
                    b if b == cancel => {
                        self.view = if asked { View::Browsers { sel, confirm: false } } else { View::List };
                        Action::Nothing
                    }
                    Button::Up if n > 0 => {
                        self.view = View::Browsers { sel: (sel + n - 1) % n, confirm: false };
                        Action::Nothing
                    }
                    Button::Down if n > 0 => {
                        self.view = View::Browsers { sel: (sel + 1) % n, confirm: false };
                        Action::Nothing
                    }
                    b if b == confirm && n > 0 => {
                        if !asked {
                            self.view = View::Browsers { sel, confirm: true };
                            return Action::Nothing;
                        }
                        let id = self.browsers[sel.min(n - 1)].id.clone();
                        self.browsers.retain(|p| p.id != id);
                        let left = self.browsers.len();
                        self.view = if left == 0 { View::List } else { View::Browsers { sel: sel.min(left - 1), confirm: false } };
                        Action::Revoke(id)
                    }
                    _ => Action::Nothing,
                }
            }
        }
    }

    fn press_list(&mut self, b: Button, confirm: Button, cancel: Button) -> Action {
        let n = ROWS.len();
        match b {
            Button::Up => self.sel = (self.sel + n - 1) % n,
            Button::Down => self.sel = (self.sel + 1) % n,
            b if b == cancel => return Action::Close,
            b if b == confirm || b == Button::Left || b == Button::Right => {
                let toggle_only = b != confirm;
                match self.row() {
                    Row::Switch => {
                        self.prefs.enabled = !self.prefs.enabled;
                        return Action::Save(self.prefs);
                    }
                    Row::Tailnet => {
                        self.prefs.trust_tailnet = !self.prefs.trust_tailnet;
                        return Action::Save(self.prefs);
                    }
                    Row::Pair if !toggle_only && self.prefs.enabled => {
                        self.view = View::Pairing;
                        return Action::StartPairing;
                    }
                    Row::Browsers if !toggle_only && !self.browsers.is_empty() => self.view = View::Browsers { sel: 0, confirm: false },
                    _ => {}
                }
            }
            _ => {}
        }
        Action::Nothing
    }

    pub fn label(&self, row: Row) -> &'static str {
        match row {
            Row::Switch => "Web companion",
            Row::Pair => "Pair a browser",
            Row::Browsers => "Paired browsers",
            Row::Tailnet => "Your Tailscale devices",
        }
    }

    pub fn value(&self, row: Row) -> String {
        match row {
            Row::Switch => on_off(self.prefs.enabled).to_string(),
            Row::Pair => if self.prefs.enabled { ">" } else { "" }.to_string(),
            Row::Browsers if self.browsers.is_empty() => "None".to_string(),
            Row::Browsers => format!("{} >", self.browsers.len()),
            Row::Tailnet => if self.prefs.trust_tailnet { "Open" } else { "Code" }.to_string(),
        }
    }

    /// The line for the selected row, in the hint bar.
    pub fn describe(&self, row: Row) -> &'static str {
        match row {
            Row::Switch => "Your games, play time and uploads in a browser.",
            Row::Pair if self.prefs.enabled => "A one-time code that lets a browser in.",
            Row::Pair => "Switch the web companion on first.",
            Row::Browsers => "Browsers paired with a code. A revokes one.",
            Row::Tailnet if self.prefs.trust_tailnet => "Open: your own Tailscale devices need no code.",
            Row::Tailnet => "Code: every browser needs a code, yours too.",
        }
    }
}

/// "23 Oct" for a Unix time (the device's time zone).
pub fn short_date(t: u64) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    // SAFETY: localtime_r fills the zeroed struct from a valid time.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let tt = t as libc::time_t;
        libc::localtime_r(&tt, &mut tm);
        tm
    };
    format!("{} {}", tm.tm_mday, MONTHS[tm.tm_mon.clamp(0, 11) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Button = Button::East;
    const B: Button = Button::South;

    fn page() -> WebPage {
        WebPage::new(Prefs::default())
    }

    #[test]
    fn the_switch_saves_and_pairing_needs_it_on() {
        let mut p = page();
        assert_eq!(p.press(A, A, B), Action::Save(Prefs { enabled: true, ..Prefs::default() }));
        p.sel = 1;
        assert_eq!(p.press(A, A, B), Action::StartPairing);
        assert_eq!(p.view, View::Pairing);
        assert_eq!(p.press(B, A, B), Action::StopPairing, "closing the code makes it useless");
        p.prefs.enabled = false;
        assert_eq!(p.press(A, A, B), Action::Nothing, "no code while it is off");
        assert_eq!(p.view, View::List);
    }

    #[test]
    fn revoking_takes_two_presses() {
        let mut p = page();
        p.browsers = vec![Paired { id: "a1".into(), name: "Safari on iPhone".into(), when: "1 Oct".into() }, Paired { id: "b2".into(), name: "Chrome on Mac".into(), when: "2 Oct".into() }];
        p.sel = 2;
        p.press(A, A, B);
        assert_eq!(p.view, View::Browsers { sel: 0, confirm: false });
        p.press(Button::Down, A, B);
        assert_eq!(p.press(A, A, B), Action::Nothing);
        assert_eq!(p.view, View::Browsers { sel: 1, confirm: true });
        assert_eq!(p.press(B, A, B), Action::Nothing, "B takes the question back");
        assert_eq!(p.view, View::Browsers { sel: 1, confirm: false });
        p.press(A, A, B);
        assert_eq!(p.press(A, A, B), Action::Revoke("b2".into()));
        assert_eq!(p.browsers.len(), 1);
        p.press(A, A, B);
        assert_eq!(p.press(A, A, B), Action::Revoke("a1".into()));
        assert_eq!(p.view, View::List, "the last one gone: back to the list");
    }

    #[test]
    fn every_row_has_words() {
        let p = page();
        let mut on = page();
        on.prefs = Prefs { enabled: true, trust_tailnet: false };
        for r in ROWS {
            assert!(!p.label(r).is_empty() && !p.describe(r).is_empty());
            // The line shares the bottom bar with the hints: longer ones are cut off on the screen.
            for d in [p.describe(r), on.describe(r)] {
                assert!(d.chars().count() <= 50, "{d}");
            }
        }
        assert_eq!(p.value(Row::Browsers), "None");
        assert_eq!(p.value(Row::Tailnet), "Open");
    }
}
