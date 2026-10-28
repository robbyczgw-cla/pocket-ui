//! Settings → Connections → Wi-Fi on a network that wants a sign-in (a hotel, a train, a café): the sheet the Wi-Fi
//! page's "This network wants a sign-in" row opens. It says what the network wants, where its sign-in page is, and has
//! two rows: **Sign in…** (the sign-in page in pocket-ui's own style, `portal_form`) and **Other ways** online (a
//! phone's hotspot, Wi-Fi sharing from a phone, a travel router, the front desk with this Pocket's Wi-Fi address). The
//! Wi-Fi page keeps checking by itself while the sheet is open (`setup`, `net_check::recheck`), and the sheet turns to
//! "You're online" when the network lets the Pocket in.
//!
//! Nothing here touches the network or the system: a press asks the Wi-Fi page for a request (`Out`), whose answer
//! comes back through `on_outcome`; the press logic and the texts are plain tests, painting reads only what the page
//! holds. docs/PORTAL-SIGNIN.md has what it does, what it never does and its limits.

use crate::gfx::Canvas;
use crate::html_form::Form;
use crate::pad::Button;
use crate::portal_form::{Ask, SignIn};
use crate::palette::*;
use crate::text::{Face, Text};

/// How often the Wi-Fi page asks again while it shows a sign-in: the research's 15 s (one small request each).
pub const RECHECK_SECS: u64 = 15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// What the network wants, and the ways on.
    Overview,
    /// The other ways online, with this Pocket's Wi-Fi address.
    OtherWays,
    /// The sign-in page in pocket-ui's own style (`portal_form`).
    SignIn,
}

/// What a press asks of the Wi-Fi page.
#[derive(Debug)]
pub enum Out {
    Stay,
    /// Back to the Wi-Fi page.
    Back,
    /// Load the sign-in page from `start` (a new session); its answer comes with `ticket`.
    Load { ticket: u64, start: String },
    /// Send a copy of a form the person filled in, as its button `submitter` would (in the same session).
    Send { ticket: u64, form: Box<Form>, submitter: Option<usize>, url: String },
}

impl PartialEq for Out {
    fn eq(&self, other: &Out) -> bool {
        matches!((self, other), (Out::Stay, Out::Stay) | (Out::Back, Out::Back))
    }
}

/// The rows of the overview.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    SignIn,
    OtherWays,
}

pub struct PortalPage {
    pub view: View,
    /// The network's name.
    pub ssid: String,
    /// Where the check was sent instead (`net_check::Check::portal`); None for a network that answers every address
    /// with its own page.
    pub at: Option<String>,
    /// This Pocket's Wi-Fi address (`wifi::mac`); None when it couldn't be read.
    pub mac: Option<String>,
    /// The network let the Pocket in while the sheet was open.
    pub online: bool,
    pub sel: usize,
    /// How far the other ways are scrolled down, in pixels, and how far they can be (known after a paint).
    scroll: i32,
    scroll_max: std::cell::Cell<i32>,
    /// "Sign in…", once opened; dropped (and what was typed wiped) when it is left.
    pub signin: Option<SignIn>,
    /// The ticket of the newest request; an answer with another one is late and dropped.
    ticket: u64,
}

impl PortalPage {
    pub fn new(ssid: &str, at: Option<String>, mac: Option<String>) -> PortalPage {
        PortalPage {
            view: View::Overview,
            ssid: ssid.to_string(),
            at,
            mac,
            online: false,
            sel: 0,
            scroll: 0,
            scroll_max: std::cell::Cell::new(0),
            signin: None,
            ticket: 0,
        }
    }

    pub fn rows(&self) -> Vec<Row> {
        vec![Row::SignIn, Row::OtherWays]
    }

    /// Where the sign-in starts: the address the check was sent to, else the check's own address (the network
    /// answers that with its page).
    pub fn start(&self) -> String {
        self.at.clone().unwrap_or_else(|| crate::net_check::PROBE_URL.to_string())
    }

    /// Opens "Sign in…" afresh and asks for the page.
    fn open_signin(&mut self) -> Out {
        self.ticket += 1;
        let host = crate::net_check::host_of(&self.start()).unwrap_or("").to_string();
        self.signin = Some(SignIn::new(&host, self.ticket));
        self.view = View::SignIn;
        Out::Load { ticket: self.ticket, start: self.start() }
    }

    /// `--snapshot`: "Sign in…" opened with `outcome` already in (None: still loading); "still" and "toomany" as after a
    /// form was sent. No request is made.
    pub fn demo_signin(&mut self, what: &str) {
        let _ = self.open_signin();
        let sent = matches!(what, "still" | "toomany");
        let outcome = if sent { crate::portal_form::demo("hotel") } else { crate::portal_form::demo(what) };
        if let Some(outcome) = outcome {
            self.on_outcome(self.ticket, outcome, what == "voucher");
        }
        if let (true, Some(s)) = (sent, &mut self.signin) {
            if what == "toomany" {
                if let crate::portal_form::State::Filling(f) = &mut s.state {
                    f.steps = crate::portal_form::MAX_STEPS;
                }
            }
            s.sending();
            let again = if what == "toomany" { crate::portal_form::demo("hotel") } else { None };
            let outcome = again.unwrap_or(crate::portal::Outcome::NeedsBrowser { why: crate::html_form::NoForm::Nothing, page: None });
            s.on_outcome(outcome, false);
        }
    }

    /// What a request found (`portal`), for the request `ticket`; a late one (the page was left or opened again) is
    /// dropped.
    pub fn on_outcome(&mut self, ticket: u64, outcome: crate::portal::Outcome, insecure: bool) {
        if ticket != self.ticket {
            return;
        }
        if matches!(outcome, crate::portal::Outcome::Online) {
            self.online = true;
        }
        if let Some(s) = &mut self.signin {
            s.on_outcome(outcome, insecure);
        }
    }

    /// The network answered again (`net_check::recheck`): online, or still a sign-in (perhaps at another address).
    pub fn on_check(&mut self, check: &crate::net_check::Check) {
        match check.net {
            crate::net_check::Net::Online => {
                self.online = true;
                if let Some(s) = &mut self.signin {
                    s.online();
                }
            }
            crate::net_check::Net::Portal => {
                self.online = false;
                if check.portal.is_some() {
                    self.at = check.portal.clone();
                }
            }
            _ => {}
        }
    }

    pub fn press(&mut self, b: Button) -> Out {
        match self.view {
            View::Overview => {
                let n = self.rows().len();
                match b {
                    Button::Up => self.sel = self.sel.saturating_sub(1),
                    Button::Down => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
                    Button::East => match self.rows().get(self.sel) {
                        Some(Row::SignIn) => return self.open_signin(),
                        Some(Row::OtherWays) => {
                            self.view = View::OtherWays;
                            self.scroll = 0;
                        }
                        None => {}
                    },
                    Button::South => return Out::Back,
                    _ => {}
                }
            }
            View::OtherWays => match b {
                Button::Up => self.scroll = (self.scroll - SCROLL_STEP).max(0),
                Button::Down => self.scroll = (self.scroll + SCROLL_STEP).min(self.scroll_max.get()),
                Button::South | Button::East => self.view = View::Overview,
                _ => {}
            },
            View::SignIn => {
                let Some(signin) = &mut self.signin else {
                    self.view = View::Overview;
                    return Out::Stay;
                };
                match signin.press(b) {
                    Ask::Nothing => {}
                    Ask::Back => {
                        // What was typed goes with the page (`portal_form::Filling`'s drop).
                        self.signin = None;
                        self.view = View::Overview;
                    }
                    Ask::Reload => return self.open_signin(),
                    Ask::OtherWays => {
                        self.signin = None;
                        self.view = View::OtherWays;
                        self.scroll = 0;
                    }
                    Ask::Send { form, submitter, url } => {
                        self.ticket += 1;
                        signin.ticket = self.ticket;
                        signin.sending();
                        return Out::Send { ticket: self.ticket, form, submitter, url };
                    }
                }
            }
        }
        Out::Stay
    }

    /// Something is loading or being sent (the dots move).
    pub fn busy(&self) -> bool {
        self.view == View::SignIn && self.signin.as_ref().is_some_and(|s| matches!(s.state, crate::portal_form::State::Loading | crate::portal_form::State::Sending(_)))
    }

    /// For the UI sounds: changes whenever what the sheet shows moves.
    pub fn mark(&self) -> (u8, usize, i32, bool, u64) {
        (self.view as u8, self.sel, self.scroll, self.online, self.signin.as_ref().map_or(0, |s| s.mark()))
    }

    /// The line naming the sign-in page: its host, or what the network does when the check wasn't sent anywhere.
    pub fn where_line(&self) -> String {
        match self.at.as_deref().and_then(crate::net_check::host_of) {
            Some(host) => format!("Sign-in page: {host}"),
            None => "The network answers every address with its own page.".to_string(),
        }
    }

    pub fn paint(&self, c: &mut Canvas, text: &mut Text, dots: &str) {
        match (self.view, &self.signin) {
            (View::SignIn, Some(s)) => s.paint(c, text, dots),
            (View::OtherWays, _) => self.paint_other_ways(c, text),
            _ => self.paint_overview(c, text),
        }
    }

    fn paint_overview(&self, c: &mut Canvas, text: &mut Text) {
        let w = c.w - 2 * MARGIN;
        let title = if self.online { "You're online" } else { "This network wants a sign-in" };
        let title = text.fit(Face::Title, 67, title, w);
        text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, &title);
        let step = crate::type_scale::line(28, 12);
        let mut y = 200;
        let lead = if self.online {
            format!("{} lets this Pocket in now. Games, the store and updates can go online.", self.ssid)
        } else {
            "Hotel, train and café networks often ask you to accept their terms or type a room number before anything goes online.".to_string()
        };
        y = text.paragraph(c, Face::Body, 28, (MARGIN, y), w, step, y + 2 * step, pal().dust, &lead) + 8;
        if !self.online {
            let line = text.fit(Face::Body, 28, &self.where_line(), w);
            text.draw(c, Face::Body, 28, MARGIN, y, pal().chalk, &line);
            y += step + 8;
            let again = format!("pocket-ui checks again every {RECHECK_SECS} seconds and says when you're online.");
            y = text.paragraph(c, Face::Body, 28, (MARGIN, y), w, step, y + step, pal().dust, &again);
        }
        let row = crate::type_scale::row(70, 37);
        let top = y + 12;
        for (i, r) in self.rows().iter().enumerate() {
            let ry = top + i as i32 * row;
            let selected = i == self.sel;
            c.rect(MARGIN, ry, w, row - 8, if selected { pal().label } else { pal().spine });
            let (name, side) = match r {
                Row::SignIn => ("Sign in…", "On the Pocket"),
                Row::OtherWays => ("Other ways", "Hotspot, front desk"),
            };
            let fg = if selected { pal().ink } else { pal().chalk };
            let quiet = if selected { pal().label_dust } else { pal().dust };
            let side_w = text.width(Face::Body, 24, side);
            let name = text.fit(Face::Title, 37, name, w - side_w - 100);
            text.draw(c, Face::Title, 37, MARGIN + 28, ry + (row - 8) / 2 + crate::type_scale::base(13, 37), fg, &name);
            text.draw_right(c, Face::Body, 24, MARGIN + w - 28, ry + (row - 8) / 2 + crate::type_scale::base(10, 24), quiet, side);
        }
        // Waiting for the sign-in: the hotel's bell and hourglass under the rows, when there is room (`ui_art`).
        if !self.online {
            let k = crate::ui_art::empty_scale(c.h);
            let (aw, ah) = crate::ui_art::size(crate::ui_art::Art::Hotel, k);
            let y = top + self.rows().len() as i32 * row + 40;
            if y + ah <= c.h - 130 {
                crate::ui_art::paint(c, crate::ui_art::Art::Hotel, (c.w - aw) / 2, y, k);
            }
        }
        crate::setup::hints(c, text, &[("A", "Open"), ("B", "Back")]);
    }

    fn paint_other_ways(&self, c: &mut Canvas, text: &mut Text) {
        let w = c.w - 2 * MARGIN;
        text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Other ways online");
        // The ways scroll under the title and above the hint bar.
        let (top, bottom) = (180, c.h - 100);
        let mark = crate::text::boxes::mark();
        let mut band = c.rows(top, bottom);
        let step = crate::type_scale::line(28, 12);
        let mut y = 50 - self.scroll;
        for (k, (head, body)) in other_ways().into_iter().enumerate() {
            if k > 0 {
                y += 16;
            }
            text.draw(&mut band, Face::Title, 37, MARGIN, y, pal().chalk, head);
            y += crate::type_scale::line(37, 14);
            for part in text.wrap(Face::Body, 28, body, w) {
                text.draw(&mut band, Face::Body, 28, MARGIN, y, pal().dust, &part);
                y += step;
            }
            if head == FRONT_DESK {
                y += 8;
                match &self.mac {
                    Some(mac) => {
                        text.draw(&mut band, Face::Body, 28, MARGIN, y, pal().dust, "This Pocket's Wi-Fi address");
                        y += crate::type_scale::line(50, 16);
                        text.draw(&mut band, Face::Title, 50, MARGIN, y, pal().chalk, mac);
                        y += 16;
                    }
                    None => {
                        for part in text.wrap(Face::Body, 28, "This Pocket's Wi-Fi address couldn't be read.", w) {
                            text.draw(&mut band, Face::Body, 28, MARGIN, y, pal().alert, &part);
                            y += step;
                        }
                        y -= step;
                    }
                }
            }
            y += step;
        }
        let end = y + self.scroll - step;
        self.scroll_max.set((end - (bottom - top) + 20).max(0));
        crate::text::boxes::clip_since(mark, c, top, bottom);
        let mut h = Vec::new();
        if self.scroll_max.get() > 0 {
            h.push(("Up/Down", "Scroll"));
        }
        h.push(("B", "Back"));
        crate::setup::hints(c, text, &h);
    }
}

/// How far Up and Down scroll the other ways.
const SCROLL_STEP: i32 = 120;

const FRONT_DESK: &str = "Ask the front desk";

/// The other ways online, in the order most travellers have them at hand: heading and what to do.
pub fn other_ways() -> [(&'static str, &'static str); 4] {
    [
        ("Your phone's hotspot", "Turn on the hotspot on your phone and choose it on the Wi-Fi page. It uses your mobile data, and works on every network."),
        ("Wi-Fi sharing from a phone", "Some phones (many Samsung Galaxy) can pass the hotel's Wi-Fi on: sign in on the phone, then turn on the hotspot with Wi-Fi sharing."),
        (FRONT_DESK, "Many hotels and halls of residence can let a device in by its Wi-Fi address. Give them this one:"),
        ("A travel router", "Sign in once on the router's page from your phone. The Pocket joins the router's own Wi-Fi and stays connected."),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net_check::{Check, Net};

    fn page() -> PortalPage {
        PortalPage::new("Hotel Guest", Some("http://login.hotel.example:8080/start?x=1".into()), Some("00:00:5E:00:53:7F".into()))
    }

    #[test]
    fn the_sheet_names_the_sign_in_page_or_says_what_the_network_does() {
        assert_eq!(page().where_line(), "Sign-in page: login.hotel.example");
        let own = PortalPage::new("Cafe", None, None);
        assert_eq!(own.where_line(), "The network answers every address with its own page.");
    }

    #[test]
    fn sign_in_asks_for_the_page_and_a_late_answer_is_dropped() {
        let mut p = page();
        let Out::Load { ticket, start } = p.press(Button::East) else { panic!("loads") };
        assert_eq!(start, "http://login.hotel.example:8080/start?x=1");
        assert_eq!(p.view, View::SignIn);
        assert!(p.busy());
        p.on_outcome(ticket + 7, crate::portal::Outcome::Online, false);
        assert!(!p.online, "not this request's answer");
        p.on_outcome(ticket, crate::portal::Outcome::Failed(crate::portal::Failure::Unreachable), false);
        assert!(!p.busy());
        // B leaves "Sign in…" and drops it.
        assert_eq!(p.press(Button::South), Out::Stay);
        assert_eq!(p.view, View::Overview);
        assert!(p.signin.is_none());
        // Without an address of its own the network's page is at the check's address.
        let own = PortalPage::new("Cafe", None, None);
        assert_eq!(own.start(), crate::net_check::PROBE_URL);
    }

    #[test]
    fn other_ways_opens_and_b_comes_back_then_leaves() {
        let mut p = page();
        assert_eq!(p.press(Button::Down), Out::Stay);
        assert_eq!(p.press(Button::East), Out::Stay);
        assert_eq!(p.view, View::OtherWays);
        assert_eq!(p.press(Button::South), Out::Stay);
        assert_eq!(p.view, View::Overview);
        assert_eq!(p.press(Button::South), Out::Back);
    }

    #[test]
    fn the_front_desk_way_asks_for_the_address_shown_below_it() {
        let ways = other_ways();
        assert!(ways.iter().any(|(h, b)| *h == FRONT_DESK && b.ends_with("Give them this one:")));
        assert!(ways.iter().any(|(h, _)| h.contains("hotspot")));
        assert!(ways.iter().any(|(_, b)| b.contains("Wi-Fi sharing")));
        assert!(ways.iter().any(|(h, _)| h.contains("travel router")));
    }

    #[test]
    fn a_check_turns_the_sheet_online_or_follows_the_sign_in_page() {
        let mut p = page();
        p.on_check(&Check { net: Net::Unknown, portal: None });
        assert!(!p.online, "an unknown answer says nothing");
        p.on_check(&Check { net: Net::Portal, portal: Some("https://other.example/".into()) });
        assert_eq!(p.where_line(), "Sign-in page: other.example");
        p.on_check(&Check { net: Net::Online, portal: None });
        assert!(p.online);
    }
}
