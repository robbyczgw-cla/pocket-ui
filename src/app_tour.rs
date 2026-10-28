//! Part of the pocket-ui binary: the feature cards outside the first start (`pocket_ui::features`). Settings → Tour
//! runs every card again; after an update, "New in this version" shows the features newer than the version last
//! seen, once, over the shelf. The cards change a copy of the App's switches (`features::Switches`), which
//! `adopt_switches` puts back and saves the way each Settings page does; the first start's cards (`setup.rs`) come
//! back through the same function. Nothing runs at a normal start: one comparison of two version strings, after
//! the first frame.

use pocket_ui::features::{self, Cards, Mode, Out, Switches};
use pocket_ui::gfx::Canvas;
use pocket_ui::leds::{self, Job};
use pocket_ui::pad::Button;
use pocket_ui::update;

use super::{App, SettingsPage};

/// Settings → Tour and "New in this version".
#[derive(Default)]
pub(crate) struct TourUi {
    /// The cards on screen.
    pub cards: Option<Cards>,
    /// Cards put off with Later (settings.json `tour_later`).
    pub later: Vec<String>,
    /// The version whose new cards were shown (settings.json `seen_version`).
    pub seen_version: Option<String>,
    /// "New in this version" was looked at in this run.
    checked: bool,
}

impl TourUi {
    /// What settings.json had: the cards put off and the version last seen.
    pub fn new(later: Vec<String>, seen_version: Option<String>) -> TourUi {
        TourUi {
            later,
            seen_version,
            ..Default::default()
        }
    }

    pub fn is_open(&self) -> bool {
        self.cards.is_some()
    }
}

impl App {
    /// The App's switches as the cards see them.
    pub(crate) fn switches(&self) -> Switches {
        Switches {
            motion: pocket_ui::motion::level(),
            on_start: self.on_start,
            remember_quit: self.remember_quit,
            frames: self.frames,
            over_game: self.over_game,
            curator: self.curator.prefs.clone(),
            sound: self.sound,
            haptics: pocket_ui::rumble::menu_haptics(),
            leds: self.leds,
        }
    }

    /// Takes switches back from the cards or the first start's profiles: each one that changed is used at once and
    /// saved in its own file, as its Settings page does it.
    pub(crate) fn adopt_switches(&mut self, s: &Switches) {
        let mut settings = false;
        if s.motion != pocket_ui::motion::level() {
            pocket_ui::motion::set_level(s.motion);
            settings = true;
        }
        if s.on_start != self.on_start {
            self.on_start = s.on_start;
            settings = true;
        }
        if s.remember_quit != self.remember_quit {
            self.remember_quit = s.remember_quit;
            settings = true;
        }
        if s.frames != self.frames {
            self.frames = s.frames;
            settings = true;
        }
        if s.over_game != self.over_game {
            self.over_game = s.over_game;
            settings = true;
        }
        if s.curator != self.curator.prefs {
            self.curator.prefs = s.curator.clone();
            if !self.curator.prefs.save(&self.data_dir) {
                pocket_ui::ui_sound::refuse();
            }
            // The pause menu reads the flag, not the file (`app_pixel_path.rs`).
            self.pixel_path = self.curator.prefs.pixel_path;
            self.curator_counts();
            self.rebuild();
        }
        if s.sound != self.sound {
            self.sound = s.sound;
            self.sound.save(&self.data_dir);
        }
        if s.haptics != pocket_ui::rumble::menu_haptics() {
            pocket_ui::rumble::set_menu_haptics(&self.data_dir, s.haptics);
        }
        if s.leds != self.leds {
            self.leds = s.leds;
            self.leds.save(&self.data_dir);
            leds::apply_in_background(Job::Choice(self.leds));
        }
        if settings {
            self.save_settings();
        }
    }

    /// Settings → Tour: every card, from the first.
    pub(crate) fn open_tour(&mut self) {
        self.tour.cards = Some(Cards::new(Mode::Tour, features::tour_list(), self.switches(), self.tour.later.clone()));
    }

    /// The Tour's value in the Settings list: how many cards were put off.
    pub(crate) fn tour_list_value(&self) -> String {
        match self.tour.later.iter().filter(|id| features::by_id(id).is_some()).count() {
            0 => ">".to_string(),
            n => format!("{n} put off >"),
        }
    }

    /// Called as the loop goes idle: once per run, on the shelf with nothing over it (not over a game that On start
    /// continued, not over Settings), notes this version and shows its new cards if it has any.
    pub(crate) fn tour_tick(&mut self) {
        if self.tour.checked || !self.drew_first || !self.onboarded {
            return;
        }
        let busy = self.running.is_some()
            || self.launch_at.is_some()
            || self.setup.is_some()
            || self.settings_page.is_some()
            || self.menu.is_some()
            || self.switcher.is_some()
            || self.card.is_some()
            || self.console.is_some()
            || self.curator.page.is_some()
            || self.find.is_open()
            || self.port_files.is_some()
            || self.emu_choice_on_shelf()
            || self.leave.is_some()
            || self.notice.as_ref().is_some_and(super::Notice::active)
            || self.sys.is_open();
        if busy {
            return;
        }
        self.tour.checked = true;
        let current = update::app_version();
        if !features::unseen(self.tour.seen_version.as_deref(), current) {
            return;
        }
        let due = features::due(self.tour.seen_version.as_deref(), current);
        println!("new in {current}: {} cards (last seen {:?})", due.len(), self.tour.seen_version);
        // Noted before they show: never more than once, also if the Pocket goes off with the cards open.
        self.tour.seen_version = Some(current.to_string());
        self.save_settings();
        if !due.is_empty() {
            self.tour.cards = Some(Cards::new(
                Mode::New { version: current.to_string() },
                due,
                self.switches(),
                self.tour.later.clone(),
            ));
            self.request_redraw();
        }
    }

    pub(crate) fn tour_input(&mut self, b: Button) {
        let Some(cards) = self.tour.cards.as_mut() else { return };
        let out = cards.press(b);
        let switches = cards.take_switches();
        let later = (cards.later != self.tour.later).then(|| cards.later.clone());
        if let Some(s) = switches {
            self.adopt_switches(&s);
        }
        if let Some(later) = later {
            self.tour.later = later;
            self.save_settings();
        }
        if matches!(out, Out::Done | Out::Back) {
            self.tour.cards = None;
        }
    }

    pub(crate) fn paint_tour(&mut self, c: &mut Canvas) {
        let Some(cards) = self.tour.cards.as_mut() else { return };
        let hits = cards.paint(c, &mut self.text);
        self.hits.clear();
        let rows = hits.choices.len();
        for (area, i) in hits.choices {
            self.hits.row(area, i);
        }
        for (area, b) in hits.buttons {
            self.hits.button(area, b);
        }
        if rows > 0 {
            self.hits.list(hits.sel, rows, 0.0, 102.0);
        }
    }

    /// What the cards show, for the redraw, haptics and sounds checks.
    pub(crate) fn tour_mark(&self) -> Option<(usize, usize, usize)> {
        self.tour.cards.as_ref().map(|c| c.mark())
    }

    /// `--snapshot tour[:<card id>[:<keys>]]`: Settings → Tour at that card (the first without one), then the keys
    /// (d down, u up, a, b, r R1, l L1); `--snapshot tour-new:<id>[,<id>...]`: "New in this version" with those cards.
    pub(crate) fn tour_snapshot(&mut self, state: &str) -> bool {
        if let Some(ids) = state.strip_prefix("tour-new:") {
            let list = ids.split(',').filter_map(features::by_id).collect();
            self.tour.cards = Some(Cards::new(
                Mode::New {
                    version: update::app_version().to_string(),
                },
                list,
                self.switches(),
                self.tour.later.clone(),
            ));
            return true;
        }
        let Some(rest) = state.strip_prefix("tour").filter(|r| r.is_empty() || r.starts_with(':')) else {
            return false;
        };
        let mut parts = rest.trim_start_matches(':').splitn(2, ':');
        let (id, keys) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        self.settings_page = Some(SettingsPage::List(super::settings_row("Tour")));
        self.open_tour();
        if let (Some(cards), Some(at)) = (
            self.tour.cards.as_mut(),
            features::tour_list().iter().position(|i| Some(*i) == features::by_id(id)),
        ) {
            cards.show(at);
        }
        for k in keys.chars() {
            let b = match k {
                'd' => Button::Down,
                'u' => Button::Up,
                'a' => Button::East,
                'b' => Button::South,
                'r' => Button::R1,
                'l' => Button::L1,
                _ => continue,
            };
            self.tour_input(b);
        }
        true
    }
}
