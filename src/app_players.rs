//! Part of the pocket-ui binary: the pause menu's Players page (`pocket_ui::players_page`), and what pocket-ui does
//! when controllers come and go (`players_event`).
//!
//! The page shows pocket-core's seats (`players::SEATS_FILE`): read when it opens, and again whenever a thread of its
//! own sees the file change (it looks every 250 ms while the page is open; closed, nothing is read). A on a controller
//! makes it the selected player: the new order goes to `players.json` and pocket-core hears `players` on its FIFO, off
//! the UI thread; pocket-core seats everyone and writes the file again, which the page then shows.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, SystemTime};

use pocket_ui::control;
use pocket_ui::gfx::Canvas;
use pocket_ui::pad::{Button, PadEvent, BUILTIN};
use pocket_ui::players::{self, Named, Order, Seats, SeatsView, MAX};
use pocket_ui::players_page::{Action, PlayersPage};
use pocket_ui::text::Face;

use winit::application::ApplicationHandler;
use winit::event_loop::ActiveEventLoop;

use super::{stripe, App, MainItem, Page, UserEvent, CANCEL, CONFIRM, DUST, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// How often the reader looks at pocket-core's seats while the page is open.
const LOOK_EVERY: Duration = Duration::from_millis(250);

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Tells the UI thread when pocket-core wrote new seats, until the page closes.
fn start_reader(page: &PlayersPage, proxy: winit::event_loop::EventLoopProxy<UserEvent>) {
    let alive = page.alive.clone();
    let spawned = std::thread::Builder::new().name("players".into()).spawn(move || {
        let path = Path::new(players::SEATS_FILE);
        let mut seen = modified(path);
        while alive.load(Ordering::Relaxed) {
            std::thread::sleep(LOOK_EVERY);
            let now = modified(path);
            if now != seen {
                seen = now;
                if proxy.send_event(UserEvent::Players).is_err() {
                    return;
                }
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("players: can't start the reader: {e}");
    }
}

/// Made-up controllers for `--snapshot players:<case>`.
fn demo(case: &str) -> Option<(SeatsView, usize)> {
    let n = |key: &str, name: &str| Some(Named { key: key.into(), name: name.into() });
    let pocket = || n(BUILTIN, "Pocket");
    Some(match case {
        "two" => (SeatsView { seats: vec![pocket(), n("bt:a", "DualSense"), None, None], spare: vec![] }, 1),
        "promoted" => (SeatsView { seats: vec![n("bt:a", "DualSense"), pocket(), None, None], spare: vec![] }, 0),
        "gap" => (SeatsView { seats: vec![pocket(), None, n("bt:b", "Xbox"), None], spare: vec![] }, 1),
        "four" => (SeatsView { seats: vec![pocket(), n("bt:a", "DualSense"), n("bt:b", "Xbox"), n("bt:c", "Pro Controller")], spare: vec![] }, 3),
        "five" => (
            SeatsView { seats: vec![pocket(), n("bt:a", "DualSense"), n("bt:b", "Xbox"), n("bt:c", "Pro Controller")], spare: vec![Named { key: "bt:d".into(), name: "8BitDo Ultimate 2C".into() }] },
            2,
        ),
        _ => return None,
    })
}

impl App {
    /// Several controllers (review SOL-09): a pad event of one device becomes what changed for all of them together
    /// (`PadState::merge`): nothing (another controller holds that button already, or still), the event, or for a
    /// device that went away the releases it causes. All but the last go through `user_event` right here, the last
    /// is returned for it. Controllers coming and going also update the Players item and page.
    pub(crate) fn merge_pad(&mut self, el: &ActiveEventLoop, ev: UserEvent) -> Option<UserEvent> {
        let pe = match ev {
            UserEvent::Merged(pe) => return Some(UserEvent::Pad(pe)),
            UserEvent::Pad(pe) => pe,
            ev => return Some(ev),
        };
        let roster = matches!(pe, PadEvent::Connected { .. } | PadEvent::Disconnected { .. });
        let mut merged = self.pad.merge(pe);
        if roster {
            self.players_event();
        }
        let last = merged.pop()?;
        for m in merged {
            self.user_event(el, UserEvent::Merged(m));
        }
        Some(UserEvent::Pad(last))
    }

    /// The pause menu lists Players: a pocket-core game, and two or more controllers that can play.
    pub(crate) fn players_item(&self) -> bool {
        self.running.as_ref().is_some_and(|r| r.hosted()) && self.pad.controllers().iter().filter(|c| players::seatable(c)).count() >= 2
    }

    /// What the page shows before pocket-core has written its seats: everyone seated as pocket-core would at a start.
    fn seats_guess(&self) -> SeatsView {
        let connected = self.pad.controllers();
        let seats = Seats::arrange(&Order::load(&self.data_dir).order, &connected, players::Mode::Compact);
        SeatsView::of(&seats, &connected)
    }

    /// A on "Players" in the pause menu.
    pub(crate) fn open_players(&mut self) {
        let view = SeatsView::read(Path::new(players::SEATS_FILE)).unwrap_or_else(|| self.seats_guess());
        let page = PlayersPage::new(view);
        start_reader(&page, self.proxy.clone());
        if let Some(menu) = self.menu.as_mut() {
            menu.players = Some(page);
            menu.page = Page::Players;
        }
    }

    /// A button on the page, from controller `source`.
    pub(crate) fn players_input(&mut self, b: Button, source: usize) {
        let from = self.pad.controller_of(source);
        let Some(menu) = self.menu.as_mut() else { return };
        let Some(page) = menu.players.as_mut() else {
            menu.page = Page::Main;
            return;
        };
        let row = page.sel;
        match page.press(b, &from, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => {
                menu.players = None;
                menu.page = Page::Main;
                menu.sel = menu.items().iter().position(|i| *i == MainItem::Players).unwrap_or(0);
            }
            Action::Assign => {
                println!("players: {} is player {}", from.key, row + 1);
                let seats = page.seats.clone();
                let data_dir = self.data_dir.clone();
                // The card write and the FIFO off the UI thread; pocket-core reads the file after it was written.
                std::thread::spawn(move || {
                    let order = Order { order: pocket_ui::players_page::order_of(&seats, &Order::load(&data_dir).order) };
                    if let Err(e) = order.save(&data_dir) {
                        eprintln!("players: can't save the order: {e}");
                    }
                    control::pocket_core(players::COMMAND);
                });
            }
        }
    }

    /// Controllers came or went (`user_event`), or pocket-core wrote new seats (`UserEvent::Players`): the menu's
    /// Players item and the open page follow.
    pub(crate) fn players_event(&mut self) {
        let item = self.players_item();
        let Some(menu) = self.menu.as_mut() else { return };
        let mut changed = false;
        if menu.players_item != item {
            // The selected row stays on the same item when Players comes or goes above it.
            let current = menu.item();
            menu.players_item = item;
            menu.sel = menu.items().iter().position(|i| *i == current).unwrap_or(0);
            changed = true;
        }
        if let Some(page) = menu.players.as_mut() {
            if let Some(view) = SeatsView::read(Path::new(players::SEATS_FILE)) {
                changed |= page.receive(view);
            }
        }
        if changed {
            self.request_redraw();
        }
    }

    /// `--snapshot players-menu[:full]` (the main page, Players selected) or `players:<two|promoted|gap|four|five>[:<row>]`.
    pub(crate) fn players_snapshot(&mut self, state: &str) {
        let mut it = state.split(':');
        let name = it.next().unwrap_or_default();
        let (case, row) = (it.next().unwrap_or("two"), it.next().and_then(|r| r.parse::<usize>().ok()));
        let Some(menu) = self.menu.as_mut() else { return };
        menu.players_item = true;
        menu.saved = None;
        if name == "players-menu" {
            // "players-menu:full": with Achievements and Pixel Path too, the longest main page there is.
            if case == "full" {
                menu.pixel_item = true;
                menu.achievements = Some(super::app_achievements::demo_snapshot());
            }
            menu.sel = menu.items().iter().position(|i| *i == MainItem::Players).unwrap_or(0);
            return;
        }
        let Some((view, sel)) = demo(case) else {
            eprintln!("players: no such case {case}");
            return;
        };
        let mut page = PlayersPage::new(view);
        page.alive.store(false, Ordering::Relaxed);
        page.sel = row.unwrap_or(sel).min(MAX - 1);
        menu.players = Some(page);
        menu.page = Page::Players;
    }

    /// Player 1 to 4 as the Options pages' rows (the name in a light pill, a bar in the system's colour out to the
    /// controller), then what A does for the selected player, and who waits for a seat.
    pub(crate) fn paint_players(&mut self, c: &mut Canvas) {
        let Some(page) = self.menu.as_ref().and_then(|m| m.players.as_ref()) else { return };
        let (seats, spare, sel) = (page.seats.clone(), page.spare.clone(), page.sel.min(MAX - 1));
        let accent = self.running.as_ref().map_or(DUST, |r| stripe(&r.system));
        let pad = NU_PAD;
        self.status_pill(c);
        let head_base = pad + (NU_PILL - 20) / 2 + pocket_ui::type_scale::mid(NU_SMALL);
        self.text.draw(c, Face::Body, NU_SMALL, pad + NU_BTN_PAD, head_base, 0xcccccc, "Players");

        let (row, font) = (pocket_ui::type_scale::row(66, 36), 36u32);
        let top = pad + NU_PILL + 12;
        let right = c.w - pad;
        for (i, seat) in seats.iter().enumerate().take(MAX) {
            let y = top + i as i32 * row;
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let name = format!("Player {}", i + 1);
            let name_w = self.text.width(Face::Body, font, &name) + 2 * NU_BTN_PAD;
            if i == sel {
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, accent);
                c.round_rect(pad, y + 4, name_w, row - 8, (row - 8) / 2, 0xe9f2f1);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, 0x151515, &name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, 0xffffff, &name);
            }
            let (value, colour) = match seat {
                Some(n) => (n.name.clone(), 0xffffff),
                None => ("No controller".to_string(), if i == sel { 0xffffff } else { 0x9a978f }),
            };
            let value = self.text.fit(Face::Body, font, &value, right - NU_BTN_PAD - (pad + name_w + 40));
            self.text.draw_right(c, Face::Body, font, right - NU_BTN_PAD, base, colour, &value);
        }

        let x = pad + NU_BTN_PAD;
        let mut y = top + MAX as i32 * row + 58;
        let ask = format!("Press A on the controller for Player {}.", sel + 1);
        self.text.draw(c, Face::Body, NU_SMALL, x, y, 0xffffff, &ask);
        y += 46;
        let more = "One press per controller, then the next player is selected.";
        self.text.draw(c, Face::Body, 26, x, y, 0xaaa79f, more);
        if !spare.is_empty() {
            y += 58;
            let names: Vec<&str> = spare.iter().map(|n| n.name.as_str()).collect();
            let waiting = self.text.fit(Face::Body, 26, &format!("Waiting for a free player: {}", names.join(", ")), right - NU_BTN_PAD - x);
            self.text.draw(c, Face::Body, 26, x, y, 0xaaa79f, &waiting);
        }
        self.nu_hints(c, &[("B", "Back"), ("A", "Choose")]);
    }
}

