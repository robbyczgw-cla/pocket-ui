//! Part of the pocket-ui binary: the App's side of `pocket_ui::perf_fan`, `pocket_ui::perf_fan_page` and
//! `pocket_ui::fan_curve`. Settings > Performance & fan, the readout thread behind it, and the fan:
//! - Smooth fan on (the default): the curve's thread (`fan_curve::spawn`, started at the first sync) gets a
//!   `fan_curve::Plan` whenever the moment changes: Fan outside games on the shelf, Games: fan (or a pocket-core game's
//!   own choice, or a PortMaster port's from its card, `app_port_perf`) in every game, the kernel's while asleep or
//!   powering off. pocket-core games are told to write their
//!   choice for it (`fan_curve::REQUEST_ENV`) instead of changing the trips.
//! - Smooth fan off: the trip profiles of `perf_fan::FanKeeper`, applied while no game runs and the Pocket is awake,
//!   the system's profile for every game, for sleep and when pocket-ui ends.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pocket_ui::control;
use pocket_ui::fan_curve::{self, Plan};
use pocket_ui::gfx::Canvas;
use pocket_ui::pad::Button;
use pocket_ui::palette::*;
use pocket_ui::perf_fan::{outside_fan, FanKeeper, Moment, Prefs, Readout};
use pocket_ui::perf_fan_page::{Action, Event, PerfFanPage, PRECEDENCE_NOTE, ROWS};
use pocket_ui::power::{Fan, Performance};
use pocket_ui::text::Face;
use pocket_ui::touch::Area;

use super::{App, SettingsPage, UserEvent, CANCEL, CONFIRM, HOST_BIN, NU_BTN_PAD, NU_PAD, NU_PILL, NU_SMALL};

/// How often the open page reads the sensors.
const READ_EVERY: Duration = Duration::from_secs(1);

/// The name in the Settings list, to come back to.
pub(crate) const SETTINGS_NAME: &str = "Performance & fan";

/// What the App keeps: the choices, the fan outside games, and the open page.
pub(crate) struct PerfFanUi {
    prefs: Prefs,
    keeper: FanKeeper,
    /// `pocket-core --restore-system` runs under way (`restore_system_then_fan`): until they end the fan stays the
    /// system's, so the shelf's profile is noted on top of the system's trips and not of a killed game's.
    settling: u32,
    page: Option<PerfFanPage>,
    /// False while the Pocket sleeps: the readout thread neither reads nor wakes the UI then.
    awake: Arc<AtomicBool>,
    /// Smooth fan's thread, from the first sync with it on.
    curve: Option<fan_curve::Handle>,
    /// Games started, so the curve tells a new game from the last (`Plan::game_id`).
    games: u64,
}

impl PerfFanUi {
    pub fn load(data_dir: &Path) -> PerfFanUi {
        let prefs = Prefs::load(data_dir);
        // What the PS2, GameCube, Wii and 3DS games without a choice of their own run with (`emu_settings`).
        pocket_ui::emu_settings::set_default_performance(prefs.games_performance);
        // Whether Quick settings get a Fan row and a working Performance row: found once, on a thread.
        pocket_ui::quick_profiles::spawn_probe(fan_curve::sysfs_root());
        PerfFanUi {
            prefs,
            keeper: FanKeeper::system(),
            settling: 0,
            page: None,
            awake: Arc::new(AtomicBool::new(true)),
            curve: None,
            games: 0,
        }
    }

    /// Smooth fan's plan for now; its thread starts with the first (not in snapshots or on a desktop).
    fn curve_set(&mut self, plan: Plan) {
        if self.curve.is_none() && cfg!(target_os = "linux") && plan.on {
            self.curve = Some(fan_curve::spawn(fan_curve::Paths::system()));
        }
        if let Some(c) = &self.curve {
            c.set(plan);
        }
    }

    /// The kernel's fan again, and only then on: before the trip profiles of Smooth fan off take over, and at the end.
    fn curve_release(&mut self) {
        if let Some(c) = &self.curve {
            c.release_and_wait(Duration::from_secs(1));
        }
    }
}

impl App {
    pub(crate) fn open_perf_fan(&mut self) -> Option<SettingsPage> {
        let page = PerfFanPage::new(self.perf.prefs);
        let (alive, awake, proxy) = (page.alive.clone(), self.perf.awake.clone(), self.proxy.clone());
        // A handful of small sysfs files once a second, never on the UI thread, and not while the screen is dark.
        std::thread::spawn(move || {
            let (root, battery) = (fan_curve::sysfs_root(), pocket_ui::battery::dir());
            while alive.load(Ordering::Relaxed) {
                if awake.load(Ordering::Relaxed) {
                    let mut r = Readout::read(&root, &battery);
                    r.curve = fan_curve::Beat::now(Path::new(fan_curve::BEAT_FILE));
                    if proxy.send_event(UserEvent::PerfFan(Event::Readout(r))).is_err() {
                        return;
                    }
                }
                let until = Instant::now() + READ_EVERY;
                while alive.load(Ordering::Relaxed) && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        });
        self.perf.page = Some(page);
        Some(SettingsPage::PerfFan)
    }

    pub(crate) fn perf_fan_input(&mut self, b: Button) -> Option<SettingsPage> {
        let back = Some(SettingsPage::List(super::settings_row(SETTINGS_NAME)));
        let Some(page) = self.perf.page.as_mut() else { return back };
        match page.press(b, CONFIRM, CANCEL) {
            Action::Nothing => {}
            Action::Close => {
                self.perf.page = None;
                return back;
            }
            Action::Save(prefs) => {
                if !prefs.save(&self.data_dir) {
                    pocket_ui::ui_sound::refuse();
                }
                // The fan outside games follows at once (`perf_fan_sync`); games read theirs when they start.
                self.perf_set_prefs(prefs);
                self.perf_fan_sync();
            }
        }
        Some(SettingsPage::PerfFan)
    }

    pub(crate) fn perf_fan_event(&mut self, e: Event) {
        match e {
            Event::Settled => {
                self.perf.settling = self.perf.settling.saturating_sub(1);
                self.perf_fan_sync();
            }
            e => {
                // The page was left another way than B: its thread stops with the next reading.
                if self.settings_page != Some(SettingsPage::PerfFan) {
                    if let Some(p) = self.perf.page.take() {
                        p.close();
                    }
                    return;
                }
                if let Some(p) = self.perf.page.as_mut() {
                    p.event(e);
                }
            }
        }
    }

    /// What pocket-ui is doing, for the fan outside games.
    fn perf_fan_moment(&self) -> Moment {
        Moment {
            in_game: self.running.is_some(),
            // A game playing on behind the dark screen (the power button's Screen off) keeps its fan.
            asleep: self.sleep.state() != pocket_ui::sleep::State::Awake && !self.sleep.dark_game(),
            settling: self.perf.settling > 0,
        }
    }

    /// Puts the fan where it should be now. Called from `about_to_wait`, so it follows games, sleep and the page; does
    /// nothing (a comparison) while nothing changed.
    pub(crate) fn perf_fan_sync(&mut self) {
        // A change in Quick settings' Performance or Fan row (`app_quick_profiles`) first.
        self.quick_profiles_sync();
        let moment = self.perf_fan_moment();
        self.perf.awake.store(!moment.asleep, Ordering::Relaxed);
        let prefs = self.perf.prefs;
        if prefs.smooth_fan {
            // The trip profiles are not used with the curve (the first sync also undoes a crashed run's).
            self.perf.keeper.sync(Fan::Auto);
            let game = match &self.running {
                None => fan_curve::Game::None,
                Some(r) if r.hosted() => fan_curve::Game::Hosted,
                Some(_) => fan_curve::Game::Other,
            };
            // A port's own choice (`app_port_perf`) instead of Games: fan; without one, Games: fan as it is now (Quick
            // settings may have changed it while the port runs).
            let (game_fan, own) = match self.port_perf.fan {
                Some((fan, true)) => (fan, true),
                _ => (prefs.games_fan, false),
            };
            let plan = Plan {
                on: !moment.asleep,
                settling: moment.settling,
                fan: if moment.in_game { game_fan } else { prefs.outside_fan },
                game,
                game_id: self.perf.games,
                own: moment.in_game && own,
            };
            self.perf.curve_set(plan);
        } else {
            // The curve hands back before a trip profile notes "the system's" trips (a lock and a flag when it isn't on).
            self.perf.curve_release();
            self.perf.keeper.sync(outside_fan(prefs.outside_fan, moment));
        }
    }

    /// A game (or PC streaming) is about to start. Smooth fan: the game's request of the last pocket-core game goes,
    /// the curve goes on. Otherwise the system's fan profile goes back first, so the game, or pocket-core noting its
    /// originals, finds the system's trips.
    pub(crate) fn perf_fan_before_game(&mut self) {
        self.perf.games += 1;
        if self.perf.prefs.smooth_fan {
            let _ = std::fs::remove_file(fan_curve::GAME_REQUEST_FILE);
        } else {
            self.perf.curve_release();
            self.perf.keeper.sync(Fan::Auto);
        }
    }

    /// Settings → Performance & fan as saved, from memory (a port's defaults, `app_port_perf`).
    pub(crate) fn perf_prefs(&self) -> Prefs {
        self.perf.prefs
    }

    /// New choices (this page or Quick settings): in memory, and for the big systems' games without their own.
    pub(crate) fn perf_set_prefs(&mut self, prefs: Prefs) {
        self.perf.prefs = prefs;
        pocket_ui::emu_settings::set_default_performance(prefs.games_performance);
    }

    /// The environment for a pocket-core game: with Smooth fan it writes its fan choice for the curve.
    pub(crate) fn perf_fan_game_env(&self) -> Option<(&'static str, String)> {
        self.perf.prefs.smooth_fan.then(|| (fan_curve::REQUEST_ENV, fan_curve::GAME_REQUEST_FILE.to_string()))
    }

    /// pocket-ui ends: the system's fan control again (the launcher's clean-up does the same if we die).
    pub(crate) fn perf_fan_exit(&mut self) {
        self.perf.curve_release();
        self.perf.keeper.sync(Fan::Auto);
    }

    /// `pocket-core --restore-system` on a thread (after a pocket-core game, and at the start), with the fan outside
    /// games held at the system's profile until it is done.
    pub(crate) fn restore_system_then_fan(&mut self) {
        self.perf.settling += 1;
        let proxy = self.proxy.clone();
        control::restore_system_then(HOST_BIN, move || {
            let _ = proxy.send_event(UserEvent::PerfFan(Event::Settled));
        });
    }

    /// Where the page's selection and values stand: a change is what the menu sounds and haptics answer.
    pub(crate) fn perf_fan_row(&self) -> Option<(usize, Prefs)> {
        self.perf.page.as_ref().map(|p| (p.sel, p.prefs))
    }

    /// `--snapshot perf[:<row>][:waiting]`: the page with made-up choices and readings (the shelf's fan on Cool).
    pub(crate) fn perf_fan_snapshot(&mut self, state: &str) -> bool {
        let Some(rest) = state.strip_prefix("perf") else { return false };
        let mut parts = rest.trim_start_matches(':').split(':');
        let row: usize = parts.next().and_then(|r| r.parse().ok()).unwrap_or(0);
        let waiting = parts.next() == Some("waiting");
        let prefs = Prefs {
            games_performance: Performance::Performance,
            games_fan: Fan::Auto,
            outside_fan: Fan::Cool,
            smooth_fan: true,
        };
        let mut page = PerfFanPage::new(prefs);
        page.sel = row.min(ROWS.len() - 1);
        if !waiting {
            page.event(Event::Readout(Readout {
                fan_starts: Some(83_500),
                curve: Some(fan_curve::Beat {
                    driving: true,
                    mode: Some(fan_curve::Mode::Curve),
                    shape: Some(fan_curve::Shape::Cool),
                    ema: Some(46_800),
                }),
                ..Readout::demo()
            }));
        }
        page.close();
        self.perf.prefs = prefs;
        self.perf.page = Some(page);
        self.settings_page = Some(SettingsPage::PerfFan);
        true
    }

    pub(crate) fn paint_perf_fan(&mut self, c: &mut Canvas) {
        let Some(page) = self.perf.page.take() else { return };
        self.paint_perf_fan_page(c, &page);
        self.perf.page = Some(page);
    }

    fn paint_perf_fan_page(&mut self, c: &mut Canvas, page: &PerfFanPage) {
        c.clear(pal().void);
        let (pad, row, font) = (NU_PAD, pocket_ui::type_scale::row(66, 36), 36u32);
        self.status_pill(c);
        self.text.draw(c, Face::Title, 50, pad + NU_BTN_PAD, pad + 58, pal().chalk, SETTINGS_NAME);
        let (top, right) = (pad + NU_PILL + 12, c.w - pad);
        for (i, r) in ROWS.iter().enumerate() {
            let y = top + i as i32 * row;
            self.hits.row(Area::new(pad, y, right - pad, row), i);
            let base = y + row / 2 + pocket_ui::type_scale::mid(font);
            let name = page.label(*r);
            let selected = i == page.sel;
            if selected {
                let w = self.text.width(Face::Body, font, name) + 2 * NU_BTN_PAD;
                c.round_rect(pad, y + 4, right - pad, row - 8, (row - 8) / 2, pal().band);
                c.round_rect(pad, y + 4, w, row - 8, (row - 8) / 2, pal().select);
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().ink, name);
            } else {
                self.text.draw(c, Face::Body, font, pad + NU_BTN_PAD, base, pal().text, name);
            }
            let value = page.value(*r);
            let shown = if selected { format!("‹ {value} ›") } else { value.to_string() };
            let vx = right - NU_BTN_PAD;
            self.text.draw_right(c, Face::Body, font, vx, base, pal().text, &shown);
            // The arrows of the selected row are buttons of their own for a finger.
            if selected {
                let vw = self.text.width(Face::Body, font, &shown);
                let aw = self.text.width(Face::Body, font, "‹ ") + 24;
                self.hits.button(Area::new(vx - vw - 24, y, aw, row), Button::Left);
                self.hits.button(Area::new(vx - aw + 24, y, aw, row), Button::Right);
            }
        }
        self.hits.list(page.sel, ROWS.len(), 0.0, row as f32);

        // What the selected row is for and what its choice does, then which settings win.
        let (left, width) = (pad + NU_BTN_PAD, c.w - 2 * (pad + NU_BTN_PAD));
        let mut y = top + ROWS.len() as i32 * row + 44;
        let [what, choice] = page.describe(page.row());
        for (text, color) in [(what.as_str(), pal().dust), (choice.as_str(), pal().text)] {
            for line in self.text.wrap(Face::Body, NU_SMALL, text, width) {
                self.text.draw(c, Face::Body, NU_SMALL, left, y, color, &line);
                y += pocket_ui::type_scale::line(NU_SMALL, 12);
            }
            y += 6;
        }
        y += 14;
        // The readout, in a panel above the hints; the note takes the room left above it (with larger text it may
        // be shortened, ending with "…").
        let hints_top = c.h - pad - NU_PILL;
        let (line_h, head) = (pocket_ui::type_scale::line(32, 16), 46 + pocket_ui::type_scale::grow(24));
        let lines = page.readout_lines();
        // Two lines side by side when the four under each other would reach into the hints (the Mini V1's 960 lines,
        // docs/DEVICES.md); on 1080 lines they never do, at every text size.
        let tall = head + lines.len() as i32 * line_h + 14;
        let cols = if (hints_top - 22 - tall).max(y + 12) + tall > hints_top { 2 } else { 1 };
        let panel_h = head + (lines.len() as i32 + cols - 1) / cols * line_h + 14;
        let step = pocket_ui::type_scale::line(24, 8);
        // As many lines as leave the panel (12 px under them) clear of the hints.
        let room = ((hints_top - 8 - panel_h - 12 - y) / step).max(0) as usize;
        for line in self.text.wrap_at_most(Face::Body, 24, PRECEDENCE_NOTE, width, room) {
            self.text.draw(c, Face::Body, 24, left, y, pal().quiet, &line);
            y += step;
        }
        let panel_y = (hints_top - 22 - panel_h).max(y + 12);
        c.round_rect(pad, panel_y, right - pad, panel_h, 24, pal().pill);
        let (px, label_w) = (pad + NU_BTN_PAD, pocket_ui::type_scale::wide(210, 26));
        self.text.draw(c, Face::Body, 24, px, panel_y + 36, pal().dust, "Now");
        let col_w = (right - NU_BTN_PAD - px) / cols;
        for (i, (label, value)) in lines.into_iter().enumerate() {
            let (cx, ly) = (px + (i as i32 % cols) * col_w, panel_y + head + 30 + (i as i32 / cols) * line_h);
            self.text.draw(c, Face::Body, 26, cx, ly, pal().dust, label);
            let value = self.text.fit(Face::Title, 32, &value, col_w - label_w - if cols > 1 { 24 } else { 0 });
            self.text.draw(c, Face::Title, 32, cx + label_w, ly + 2, pal().chalk, &value);
        }

        self.nu_hints(c, &[("B", "Back"), ("A", "Next"), ("< >", "Change")]);
    }
}
