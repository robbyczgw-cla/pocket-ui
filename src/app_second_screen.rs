//! Part of the pocket-ui binary: the window on the second screen (`pocket_ui::second_screen`, docs/DUAL-SCREEN.md): the
//! RG DS Plus's lower panel, or an explicitly named test output.
//!
//! After the first frame a controller thread watches the outputs (on the RG DS Plus it logs the panel table's line
//! too). Only with a second screen is a second window made; elsewhere nothing here runs. The window shows the game the shelf rests on as its box, built the way X
//! builds it (`App::build_card`) once the selection has rested `FOLLOW_MS`, so scrolling paints nothing below; after
//! `TURN_MS` the info page takes its place (facts, your play, the save slots). Rows that are no game show their
//! console or a short line. In a game the screen is switched off (output and backlight), but for DS and 3DS, whose
//! emulator uses it: our window goes to sway's scratchpad then. Settings → System → Second screen: Off keeps it off.

use std::rc::Rc;
use std::time::{Duration, Instant};

use pocket_ui::game_card::Summary;
use pocket_ui::gfx::Canvas;
use pocket_ui::palette::pal;
use pocket_ui::second_screen::{self, Power};
use pocket_ui::text::Face;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use super::app_card::OpenCard;
use super::{App, Item, UserEvent};

pub(crate) struct Second {
    window: Rc<Window>,
    surface: pocket_ui::frame::Presenter<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    /// What the shelf rests on, and since when.
    wanted: Option<Option<Item>>,
    wanted_since: Instant,
    /// What the window shows: a game's box (`card`), or the row's console / a line.
    shown: Option<Option<Item>>,
    card: Option<OpenCard>,
    turn_at: Option<Instant>,
    /// Repaints after a new game, for its pictures, which come from threads (`AGAIN_MS`).
    again: usize,
    again_from: Instant,
    /// Last successful presentation: unrelated main-window events must not accelerate this window.
    frame_at: Instant,
    visible: std::collections::HashSet<pocket_ui::picture_loader::Key>,
    power: Power,
    /// The first power change was sent (`second_screen_tick`).
    powered: bool,
}

/// After a game appears below: painted again this long after, for the cover and the case that were still being read.
const AGAIN_MS: [u64; 3] = [150, 400, 900];

/// Animation frames on the second window: the game sliding out of its box.
const FRAME: Duration = Duration::from_millis(16);

impl Second {
    pub(crate) fn request_redraw(&self) { self.window.request_redraw(); }
}

impl App {
    /// The controller that watches the two outputs (`after_first_frame`).
    pub(crate) fn second_screen_start(&mut self) {
        #[cfg(target_os = "linux")]
        {
            let proxy = std::sync::Mutex::new(self.proxy.clone());
            self.second_control = second_screen::Controller::start(move |outputs| {
                proxy.lock().is_ok_and(|p| p.send_event(UserEvent::SecondScreen(outputs)).is_ok())
            });
        }
    }

    /// A second screen is there: the second window, which sway's rule puts on its output (`second_screen::rule_cmd`).
    pub(crate) fn second_screen_found(&mut self, el: &ActiveEventLoop, _main: String, _output: String) {
        if self.second.is_some() {
            return;
        }
        println!("second screen: on {_output}, the shelf on {_main} (placed)");
        let attrs = Window::default_attributes().with_title(second_screen::TITLE);
        #[cfg(target_os = "linux")]
        let attrs = {
            use winit::platform::wayland::WindowAttributesExtWayland;
            attrs.with_name(second_screen::APP_ID, second_screen::APP_ID)
        };
        let window = match el.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                eprintln!("second screen: no window ({e})");
                return;
            }
        };
        let mut surface = pocket_ui::frame::Presenter::empty();
        let for_surface = window.clone();
        surface.set_factory(move || {
            let context = softbuffer::Context::new(for_surface.clone()).map_err(|e| format!("context: {e}"))?;
            softbuffer::Surface::new(&context, for_surface.clone()).map_err(|e| format!("surface: {e}"))
        });
        window.request_redraw();
        self.second = Some(Second {
            window,
            surface,
            wanted: None,
            wanted_since: Instant::now(),
            shown: None,
            card: None,
            turn_at: None,
            again: AGAIN_MS.len(),
            again_from: Instant::now(),
            // Lit or not from the first tick on: ROCKNIX leaves the lower panel powered off or on, depending on its
            // rules, so the first change powers it as it should be.
            frame_at: Instant::now(),
            visible: Default::default(),
            power: Power::Off,
            powered: false,
        });
    }

    pub(crate) fn is_second_window(&self, id: WindowId) -> bool {
        self.second.as_ref().is_some_and(|s| s.window.id() == id)
    }

    /// Follows the shelf and the game (from `about_to_wait`); the next time it needs to look again.
    pub(crate) fn second_screen_tick(&mut self, now: Instant) -> Option<Instant> {
        let power = second_screen::power_awake(self.running.as_ref().map(|r| r.system.as_str()), self.second_screen_on,
            self.sleep.state() == pocket_ui::sleep::State::Awake);
        if let Some(control) = self.second_control.as_mut() { control.set_power(power); }
        let sec = self.second.as_mut()?;
        if power != sec.power || !sec.powered {
            sec.power = power;
            sec.powered = true;
            if power == Power::Lit {
                // Rebuild after playing: the player's time and save slots may have changed.
                sec.shown = None;
                sec.wanted_since = now.checked_sub(Duration::from_millis(second_screen::FOLLOW_MS)).unwrap_or(now);
                sec.window.request_redraw();
            }
        }
        if power != Power::Lit { return None; }
        // The shelf, nothing over it: what it rests on. A card, a menu or Settings over it keep the screen below as it is.
        let on_shelf = self.card.is_none() && self.menu.is_none() && self.switcher.is_none() && self.settings_page.is_none();
        if on_shelf {
            let want = self.items.get(self.sel).copied();
            if sec.wanted != Some(want) {
                sec.wanted = Some(want);
                sec.wanted_since = now;
            }
        }
        if sec.wanted.is_some() && sec.wanted != sec.shown {
            let due = sec.wanted_since + Duration::from_millis(second_screen::FOLLOW_MS);
            if now < due {
                return Some(due);
            }
            let want = sec.wanted.flatten();
            sec.shown = sec.wanted;
            sec.card = None;
            sec.turn_at = None;
            sec.again = 0;
            sec.again_from = now;
            if let Some(open) = want.and_then(|i| self.second_card(i)) {
                let sec = self.second.as_mut()?;
                sec.card = Some(open);
                sec.turn_at = Some(now + Duration::from_millis(second_screen::TURN_MS));
            }
            let sec = self.second.as_mut()?;
            sec.window.request_redraw();
        }
        let sec = self.second.as_mut()?;
        let mut wake: Option<Instant> = sec.surface.retry_at().map(|at| {
            if now >= at { sec.window.request_redraw(); }
            at.max(now + FRAME)
        });
        if let Some(ms) = AGAIN_MS.get(sec.again) {
            let at = sec.again_from + Duration::from_millis(*ms);
            if now >= at {
                sec.again += 1;
                sec.window.request_redraw();
            }
            if let Some(at) = AGAIN_MS.get(sec.again).map(|ms| sec.again_from + Duration::from_millis(*ms)) {
                wake = Some(wake.map_or(at, |w| w.min(at)));
            }
        }
        if let Some(at) = sec.turn_at {
            if now >= at {
                sec.turn_at = None;
                // The page with the facts, your play and the save slots (`game_info`).
                if let Some(open) = sec.card.as_mut() {
                    open.card.show_info();
                }
                // Its cover is read at the page's size: painted again for it as for a new game.
                sec.again = 0;
                sec.again_from = now;
                sec.window.request_redraw();
                wake = Some(now + Duration::from_millis(AGAIN_MS[0]));
            } else {
                wake = Some(wake.map_or(at, |w| w.min(at)));
            }
        }
        if let Some(b) = sec.card.as_ref().and_then(|o| o.card.boxed.as_ref()) {
            let next = if b.animating(now) { Some(sec.frame_at + FRAME) } else { b.spin_wake(now) };
            if let Some(next) = next {
                if now >= next {
                    sec.window.request_redraw();
                }
                let next = next.max(now + FRAME);
                wake = Some(wake.map_or(next, |w| w.min(next)));
            }
        }
        wake
    }

    /// The Wikipedia lead for the box below, if it waits for that one.
    pub(crate) fn second_card_text(&mut self, title: &str, text: &Result<String, String>) {
        let Some(open) = self.second.as_mut().and_then(|s| s.card.as_mut()).filter(|o| o.article.as_deref() == Some(title)) else {
            return;
        };
        open.card.data.summary = match text {
            Ok(t) => Summary::Wikipedia { text: t.clone(), title: title.to_string() },
            Err(why) => Summary::Unavailable(why.clone()),
        };
        if let Some(s) = &self.second {
            s.window.request_redraw();
        }
    }

    /// A frame of the second window. The shelf's card and toast are put aside for it, so `paint_card` paints the box.
    pub(crate) fn draw_second(&mut self) {
        let Some(sec) = self.second.as_mut() else { return };
        if sec.power != Power::Lit {
            return;
        }
        let size = sec.window.inner_size();
        let (Some(w), Some(h)) = (std::num::NonZeroU32::new(size.width), std::num::NonZeroU32::new(size.height)) else { return };
        let mut surface = std::mem::replace(&mut sec.surface, pocket_ui::frame::Presenter::empty());
        let mut card = sec.card.take();
        let shown = sec.shown.flatten();
        // Each window keeps its own image dependencies: a late cover updates only the window showing it.
        let primary_visible = std::mem::take(&mut self.pictures.visible);
        let drawn = surface.draw_scaled(w, h, &mut |pixels, lw, lh| {
            self.paint_in_appearance(pixels, lw, lh, &mut |app, c| app.paint_second(c, &mut card, shown));
        });
        let visible = std::mem::replace(&mut self.pictures.visible, primary_visible);
        if let Some(sec) = self.second.as_mut() {
            sec.surface = surface;
            sec.card = card;
            if drawn {
                sec.visible = visible;
                sec.frame_at = Instant::now();
                pocket_ui::mark!("second-frame");
            }
        }
    }

    pub(crate) fn second_picture_changed(&self, key: &pocket_ui::picture_loader::Key) {
        if let Some(sec) = self.second.as_ref().filter(|s| s.power == Power::Lit && s.visible.contains(key)) {
            sec.window.request_redraw();
        }
    }

    /// A game's box for the screen below: the card X builds, as its box, without its hint bar (no key acts on it).
    fn second_card(&mut self, item: Item) -> Option<OpenCard> {
        let (system, name, path) = self.card_game(item)?;
        let mut open = self.build_card(item, system, name, path, false);
        open.card.boxed = Some(pocket_ui::game_box::BoxView::new());
        open.card.bare = true;
        Some(open)
    }

    /// What the screen below shows: the box (or its info page) of `card`, else the row `shown`. The shelf's card and
    /// toast are put aside for it, so `paint_card` paints the box.
    fn paint_second(&mut self, c: &mut Canvas, card: &mut Option<OpenCard>, shown: Option<Item>) {
        // A rescan/card removal can invalidate the passive card's shelf indices while Settings keeps it on screen.
        if card.as_ref().is_some_and(|open| self.card_game(open.item).is_none_or(|(_, _, path)| path != open.path)) {
            *card = None;
            if let Some(sec) = self.second.as_mut() { sec.shown = None; }
        }
        if card.is_some() {
            let (top_card, toast) = (std::mem::replace(&mut self.card, card.take()), self.toast.take());
            self.paint_card(c);
            *card = std::mem::replace(&mut self.card, top_card);
            self.toast = toast;
        } else {
            self.paint_second_rest(c, shown);
        }
    }

    /// `--snapshot second:<box|info>:<system>:<name>`, `second:system:<system>` or `second:none`: the screen below as
    /// it shows that game (the box, or the info page after `TURN_MS`), a system, or a row that is no game.
    pub(crate) fn second_snapshot(&mut self, rest: &str, pixels: &mut [u32], w: u32, h: u32) {
        let parts: Vec<&str> = rest.split(':').collect();
        let system = |app: &App| parts.get(1).and_then(|n| app.systems.iter().position(|s| s.name == *n));
        let item = match parts.first().copied() {
            Some("box" | "info") => system(self).and_then(|s| {
                let name = parts.get(2)?.to_lowercase();
                let g = self.systems[s].games.iter().position(|g| g.name.to_lowercase().contains(&name))?;
                Some(Item::Game(s, g))
            }),
            Some("system") => system(self).map(Item::System),
            _ => None,
        };
        let mut card = item.and_then(|i| self.second_card(i));
        if let Some(open) = card.as_mut() {
            if parts.first() == Some(&"info") {
                open.card.show_info();
            }
        }
        // Twice, as a running window does (`AGAIN_MS`): the first frame asks for the pictures, the second has them.
        for _ in 0..2 {
            if let Some(bx) = card.as_mut().and_then(|o| o.card.boxed.as_mut()) {
                bx.snapshot_settle();
            }
            self.paint_in_appearance(pixels, w, h, &mut |app, c| app.paint_second(c, &mut card, item));
        }
    }

    /// A row that is no game: its console's picture and name (a system), else a line saying what shows here.
    fn paint_second_rest(&mut self, c: &mut Canvas, item: Option<Item>) {
        c.clear(pal().void);
        let system = match item {
            Some(Item::System(s)) => self.systems.get(s).map(|s| (s.name.clone(), s.fullname.clone())),
            _ => None,
        };
        let (cx, cy) = (c.w / 2, c.h / 2);
        match system {
            Some((name, fullname)) => {
                if let Some(art) = pocket_ui::system_art::get(&name) {
                    c.blit_sprite(art, cx - art.w as i32 / 2, cy - 40 - art.h as i32, 0);
                }
                let w = self.text.width(Face::Title, 50, &fullname);
                self.text.draw(c, Face::Title, 50, cx - w / 2, cy + 60, pal().chalk, &fullname);
            }
            None => {
                let line = "Choose a game: its box shows here.";
                let w = self.text.width(Face::Body, 28, line);
                self.text.draw(c, Face::Body, 28, cx - w / 2, cy, pal().dust, line);
            }
        }
    }
}
