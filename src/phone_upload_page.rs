//! The Pocket's side of "Send from your phone" (`phone_upload`): a large QR code of the page's address, the address
//! to type, and what the phone does (waiting, connected, each file's progress, how many games were added). Opened
//! with Y on Add games (Settings → Games, and the first start's step); B closes it, and with it the server.
//! Without Wi-Fi it says so and A leads to the Wi-Fi settings.

use std::sync::Arc;

use crate::gfx::Canvas;
use crate::library::SysInfo;
use crate::pad::Button;
use crate::palette::*;
use crate::phone_upload::{Closed, Config, FileRow, FileState, Server, Status};
use crate::text::{Face, Text};
use crate::web::files::Roots;

/// A design size as Settings › Look › Text size paints it.
fn px(design: u32) -> i32 {
    crate::type_scale::px(design) as i32
}

/// What a button press asks of the setup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Close,
    /// No Wi-Fi: open the Wi-Fi settings.
    Wifi,
}

pub struct PhonePage {
    server: Option<Server>,
    /// Why it couldn't open (no Wi-Fi address, the socket refused).
    error: Option<String>,
    no_wifi: bool,
    catalog: Vec<SysInfo>,
    ip: Option<String>,
    notify: Arc<dyn Fn() + Send + Sync>,
    /// Snapshots: a made-up address and status, no server.
    demo: Option<(String, Status)>,
}

/// The warning under the code, always shown whole (security review L2): what plain HTTP on a Wi-Fi means, in plain words.
pub const HELP: &str = "Not encrypted: someone else on this Wi-Fi could see the code and the games you send, and could change them on the way. Only use it on your own Wi-Fi at home. Nothing can be downloaded from the Pocket. B ends it.";

/// Where the page's parts go on a `w` x `h` canvas: the warning's lines (all of them), the y they start at, and the
/// QR code's size, which gives way to the warning rather than the other way round.
pub fn layout(text: &mut Text, help: &str, w: i32, h: i32) -> (Vec<String>, i32, i32) {
    let width = w - 2 * MARGIN;
    let bottom = h - crate::kit::PAD - crate::kit::PILL - 20;
    let lines = text.wrap_prose(Face::Body, 24, help, width);
    let help_top = bottom - lines.len() as i32 * (px(24) + 8);
    let q = (help_top - QR_TOP - 24).min(w * 42 / 100).min(620);
    (lines, help_top, q)
}

/// The QR code's top.
const QR_TOP: i32 = 228;

/// Whether the page is offered: the handhelds, not the Steam Deck (games go into its own folders there).
pub fn offered() -> bool {
    let d = crate::device::current();
    !d.host_owned() && d.has_wifi()
}

impl PhonePage {
    /// Opens the page and starts the server on the Wi-Fi address `ip` (None: no Wi-Fi, nothing is started).
    pub fn open(ip: Option<String>, catalog: Vec<SysInfo>, notify: Arc<dyn Fn() + Send + Sync>) -> PhonePage {
        let mut p = PhonePage { server: None, error: None, no_wifi: false, catalog, ip, notify, demo: None };
        p.start();
        p
    }

    fn start(&mut self) {
        self.server = None;
        self.error = None;
        let Some(ip) = self.ip.clone() else {
            self.no_wifi = true;
            return;
        };
        self.no_wifi = false;
        let cfg = Config { roots: Roots::default(), systems: self.catalog.clone(), idle: crate::phone_upload::IDLE, notify: self.notify.clone() };
        match Server::on_wifi(&ip, cfg) {
            Ok(s) => self.server = Some(s),
            Err(e) => self.error = Some(e),
        }
    }

    /// A server is running: something listens.
    pub fn listening(&self) -> bool {
        self.server.as_ref().is_some_and(|s| s.closed().is_none())
    }

    /// A file is arriving.
    pub fn receiving(&self) -> bool {
        self.demo.is_none() && self.status().is_some_and(|s| s.receiving() > 0 && s.closed.is_none())
    }

    /// Files put in place since the page opened.
    pub fn added(&self) -> usize {
        self.status().map_or(0, |s| s.added)
    }

    fn status(&self) -> Option<Status> {
        match &self.demo {
            Some((_, s)) => Some(s.clone()),
            None => self.server.as_ref().map(Server::status),
        }
    }

    fn closed(&self) -> Option<Closed> {
        self.status().and_then(|s| s.closed)
    }

    pub fn press(&mut self, b: Button) -> Action {
        match b {
            Button::South => Action::Close,
            Button::East if self.no_wifi => Action::Wifi,
            // Closed by itself or couldn't open: A tries again, with a new code.
            Button::East if self.demo.is_none() && (self.error.is_some() || self.closed().is_some()) => {
                self.start();
                Action::None
            }
            _ => Action::None,
        }
    }

    /// `--snapshot setup:phone=<kind>`: waiting, connected, receiving, done, idle, slept, nowifi.
    pub fn demo(kind: &str) -> PhonePage {
        let row = |name: &str, done: u64, total: u64, state: FileState| FileRow::demo(name, done, total, state);
        let mut st = Status::default();
        match kind {
            "connected" => st.connected = true,
            "receiving" => {
                st.connected = true;
                st.files = vec![
                    row("Chrono Trigger.sfc", 4 << 20, 4 << 20, FileState::Added),
                    row("Advance Wars.gba", 5 << 20, 16 << 20, FileState::Receiving),
                    row("Super Metroid.sfc", 1 << 20, 3 << 20, FileState::Receiving),
                ];
                st.added = 1;
            }
            "done" => {
                st.connected = true;
                st.files = vec![
                    row("Chrono Trigger.sfc", 4 << 20, 4 << 20, FileState::Added),
                    row("Advance Wars.gba", 16 << 20, 16 << 20, FileState::Added),
                    row("Super Metroid.srm", 0, 8 << 10, FileState::Failed("Saves can't be sent this way.".into())),
                    row("Super Metroid.sfc", 3 << 20, 3 << 20, FileState::Added),
                ];
                st.added = 3;
            }
            "idle" => st.closed = Some(Closed::Idle),
            "slept" => st.closed = Some(Closed::Slept),
            "timeup" => st.closed = Some(Closed::TimeUp),
            _ => {}
        }
        let no_wifi = kind == "nowifi";
        PhonePage {
            server: None,
            error: None,
            no_wifi,
            catalog: Vec::new(),
            ip: (!no_wifi).then(|| "192.0.2.23".into()),
            notify: Arc::new(|| {}),
            demo: (!no_wifi).then(|| ("http://192.0.2.23:41234/k7m2qx9p/".to_string(), st)),
        }
    }

    fn urls(&self) -> Option<(String, String)> {
        match &self.demo {
            Some((url, _)) => Some((url.clone(), url.trim_start_matches("http://").trim_end_matches('/').to_string())),
            None => self.server.as_ref().map(|s| (s.url(), s.short())),
        }
    }

    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        let again = self.error.is_some() || self.closed().is_some();
        let mut h = Vec::new();
        if self.no_wifi {
            h.push(("A", "Wi-Fi settings"));
        } else if again {
            h.push(("A", "Open again"));
        }
        h.push(("B", "Back"));
        h
    }

    pub fn paint(&self, c: &mut Canvas, text: &mut Text) {
        let width = c.w - 2 * MARGIN;
        text.draw(c, Face::Title, 67, MARGIN, 140, pal().chalk, "Send from your phone");
        let bottom = c.h - crate::kit::PAD - crate::kit::PILL - 20;
        if self.no_wifi {
            crate::kit::empty_state(
                c,
                text,
                Some(crate::ui_art::Art::Offline),
                (MARGIN, width.min(900)),
                170,
                330,
                bottom,
                "Connect to Wi-Fi first",
                "Your phone and the Pocket need to be on the same Wi-Fi. A opens the Wi-Fi settings.",
            );
            return;
        }
        let sub = text.fit(Face::Body, 28, "Scan the code with your phone's camera and pick games there.", width);
        text.draw(c, Face::Body, 28, MARGIN, 192, pal().dust, &sub);
        // Line steps grow with Settings › Look › Text size, as the type does.
        let (l24, l21) = (px(24) + 8, px(21) + 6);
        // The warning at the bottom, every line of it, then the QR code as large as the room between.
        let (help, help_top, q) = layout(text, HELP, c.w, c.h);
        for (i, l) in help.iter().enumerate() {
            text.draw(c, Face::Body, 24, MARGIN, help_top + px(24) + i as i32 * l24, pal().chalk, l);
        }
        let top = QR_TOP;
        let x = MARGIN + q + 56;
        let w = c.w - MARGIN - x;
        let Some((url, short)) = self.urls() else {
            let why = self.error.clone().unwrap_or_else(|| "Can't open the page.".into());
            text.draw(c, Face::Title, 37, MARGIN, top + 50, pal().alert, "The page couldn't open");
            let why = text.fit(Face::Body, 28, &why, width);
            text.draw(c, Face::Body, 28, MARGIN, top + 62 + px(28), pal().dust, &why);
            text.draw(c, Face::Body, 28, MARGIN, top + 74 + 2 * px(28), pal().dust, "A tries again.");
            return;
        };
        let closed = self.closed();
        if closed.is_none() {
            crate::kit::qr(c, MARGIN, top, q, &url, 4, true);
        } else {
            // No code that leads nowhere: an empty frame where it was.
            c.round_rect(MARGIN, top, q, q, 12, pal().keycap);
            c.round_rect(MARGIN + 4, top + 4, q - 8, q - 8, 10, pal().void);
        }
        let st = self.status().unwrap_or_default();
        if let Some(why) = closed {
            let (head, line) = match why {
                Closed::Idle => ("Closed after 10 minutes", "Nothing came from the phone for 10 minutes, so the page closed."),
                Closed::Slept => ("Closed while the Pocket slept", "Files that were on their way were not kept."),
                Closed::Guessing => ("Closed: someone was guessing", "Too many tries without the right code came over the Wi-Fi."),
                Closed::TimeUp => ("Closed after an hour", "The page stays open for an hour at most."),
            };
            let head = text.fit(Face::Title, 37, head, w);
            text.draw(c, Face::Title, 37, x, top + 50, pal().chalk, &head);
            let mut y = top + 50 + px(24) + 18;
            for l in text.wrap_prose(Face::Body, 24, line, w).iter().take(3) {
                text.draw(c, Face::Body, 24, x, y, pal().dust, l);
                y += l24;
            }
            text.draw(c, Face::Body, 24, x, y + 8, pal().chalk, "A opens it again with a new code.");
            return;
        }
        let label = text.fit(Face::Body, 24, "Or type this address", w);
        text.draw(c, Face::Body, 24, x, top + px(24), pal().dust, &label);
        // The address is typed letter by letter: smaller rather than cut.
        let size = [37, 30, 24].into_iter().find(|s| text.width(Face::Title, *s, &short) <= w).unwrap_or(24);
        let addr_y = top + px(24) + 12 + px(37);
        text.draw(c, Face::Title, size, x, addr_y, pal().chalk, &short);
        let receiving = st.receiving();
        let head = if receiving > 0 {
            if receiving == 1 { "Receiving 1 file".to_string() } else { format!("Receiving {receiving} files") }
        } else if st.added > 0 {
            if st.added == 1 { "Added: 1 game".to_string() } else { format!("Added: {} games", st.added) }
        } else if st.connected {
            "Phone connected".to_string()
        } else {
            "Waiting for your phone".to_string()
        };
        let head = text.fit(Face::Title, 37, &head, w);
        let head_y = addr_y + 36 + px(37);
        text.draw(c, Face::Title, 37, x, head_y, pal().chalk, &head);
        let list_top = head_y + 14;
        if st.files.is_empty() {
            let line = if st.connected { "Pick games on the phone; they show up here as they arrive." } else { "Open the camera on your phone and point it at the code." };
            for (i, l) in text.wrap_prose(Face::Body, 24, line, w).iter().take(2).enumerate() {
                text.draw(c, Face::Body, 24, x, list_top + px(24) + i as i32 * l24, pal().dust, l);
            }
            return;
        }
        // The newest files that fit, each its name and a bar or what became of it.
        let row_h = l24 + l21 + 10;
        let fits = ((top + q - list_top) / row_h).max(1) as usize;
        let first = st.files.len().saturating_sub(fits);
        for (i, f) in st.files[first..].iter().enumerate() {
            let y = list_top + i as i32 * row_h;
            let name = text.fit(Face::Body, 24, &f.name, w);
            text.draw(c, Face::Body, 24, x, y + px(24), pal().chalk, &name);
            let (bar_y, line_y) = (y + px(24) + 12, y + l24 + px(21));
            match &f.state {
                FileState::Receiving => {
                    let pct = (f.done.min(f.total) * 100).checked_div(f.total).unwrap_or(0) as i32;
                    let label = format!("{pct} %");
                    let lw = text.width(Face::Body, 21, &label) + 14;
                    c.round_rect(x, bar_y, w - lw, 10, 5, pal().keycap);
                    c.round_rect(x, bar_y, ((w - lw) * pct / 100).max(10), 10, 5, pal().chalk);
                    text.draw_right(c, Face::Body, 21, x + w, bar_y + 12, pal().dust, &label);
                }
                FileState::Added => {
                    text.draw(c, Face::Body, 21, x, line_y, pal().dust, &format!("Added, {}", crate::ports::size_label(f.total)));
                }
                FileState::Failed(why) => {
                    let why = text.fit(Face::Body, 21, why, w);
                    text.draw(c, Face::Body, 21, x, line_y, pal().alert, &why);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_wifi_nothing_starts_and_a_leads_to_wifi() {
        let mut p = PhonePage::open(None, Vec::new(), Arc::new(|| {}));
        assert!(!p.listening() && p.server.is_none());
        assert_eq!(p.press(Button::East), Action::Wifi);
        assert_eq!(p.hints()[0], ("A", "Wi-Fi settings"));
        assert_eq!(p.press(Button::South), Action::Close);
    }

    /// Security review L2: the warning is shown whole, in English and German, at Large text on every screen, and the
    /// QR code stays large enough to scan.
    #[test]
    fn the_warning_is_shown_whole_and_the_code_stays_scannable() {
        let mut text = Text::new();
        let german = crate::i18n::german().translate(HELP).expect("HELP has a German line").to_string();
        assert!(german.contains("verschlüsselt"));
        crate::type_scale::set(crate::type_scale::TextSize::Large);
        for (w, h) in [(1240, 1080), (1280, 960), (1707, 960), (1920, 1080), (1280, 1280)] {
            for help in [HELP, german.as_str()] {
                let (lines, help_top, q) = layout(&mut text, help, w, h);
                assert!(lines.len() <= 4, "{w}x{h}: {} lines", lines.len());
                assert!(q >= 300, "{w}x{h}: the code is only {q} px");
                assert!(QR_TOP + q < help_top, "{w}x{h}: the code runs into the warning");
                let bottom = h - crate::kit::PAD - crate::kit::PILL - 20;
                assert!(help_top + lines.len() as i32 * (px(24) + 8) <= bottom, "{w}x{h}: the warning runs into the hints");
            }
        }
        crate::type_scale::set(crate::type_scale::TextSize::Normal);
    }

    #[test]
    fn an_address_that_is_not_the_pockets_opens_nothing() {
        // An address no interface has: the page says it couldn't open, nothing listens.
        let p = PhonePage::open(Some("203.0.113.9".into()), Vec::new(), Arc::new(|| {}));
        assert!(!p.listening() && p.error.is_some());
        assert_eq!(p.hints()[0], ("A", "Open again"));
    }

    /// B (South) closes the page, also after it closed by itself; A (East) opens it again with a new code and stays.
    #[test]
    fn b_closes_and_a_on_a_closed_page_opens_it_again() {
        let mut p = PhonePage::open(Some("203.0.113.9".into()), Vec::new(), Arc::new(|| {}));
        assert_eq!(p.press(Button::East), Action::None, "A on a page that couldn't open tries again and stays");
        assert_eq!(p.press(Button::South), Action::Close);
    }
}
