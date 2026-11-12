//! Part of the pocket-ui binary: "Where was I?" (`pocket_ui::where_was_i`). The card is decided at the launch from
//! what pocket-ui holds (recent games, play log, the game card's note), painted over the launch picture while the
//! moment loads, and gone with the first button or when the game covers pocket-ui. A pocket-core game stands still at its
//! moment meanwhile (`where_was_i_hold`): the card can be read even where the moment is back in 0.1 s, and nothing of the
//! game is missed.

use std::path::Path;
use std::time::{Duration, Instant};

use pocket_ui::gfx::Canvas;
use pocket_ui::where_was_i::{self, Facts};

use super::App;

impl App {
    /// At the launch of `path`, after the moment's check (`fresh`: it is left out, the game starts anew): the card this
    /// start shows once its picture is there, or none.
    pub(crate) fn where_was_i_at_launch(&mut self, path: &Path, fresh: bool, resumable: bool) {
        self.where_was_i = None;
        if fresh || !self.remind_where {
            return;
        }
        let recent = self.recents.list.iter().find(|r| r.path == path).map(|r| (r.last_played, r.play_secs));
        let log = if recent.is_none() { pocket_ui::play_log::view() } else { None };
        let Some((last, secs)) = where_was_i::last_played(recent, log.as_deref(), &path.to_string_lossy()) else { return };
        let note = self.library.get(path).note;
        let facts = Facts { on: self.remind_where, kids: self.kids_live(), easy: self.big_on(), last_played: Some(last), play_secs: secs, note: &note };
        self.where_was_i = where_was_i::card(&facts, pocket_ui::recents::now_secs());
        // The worker validated the moment before preparing this exact launch; a stale preview adds no reminder.
        if !resumable { self.where_was_i = None; }
        if let Some(card) = &self.where_was_i {
            println!("where was i: {} (note: {})", card.ago, if card.note.is_some() { "yes" } else { "no" });
        }
    }

    /// Requests a still hold for a readable reminder. The animation has a deadline; the reminder waits for a
    /// button (`startup::HOLD_STILL`). None without a card, a preview, or pocket-core hosting the game.
    pub(crate) fn where_was_i_hold(&self, hosted: bool, preview: bool) -> Option<Duration> {
        if !hosted || self.where_was_i.is_none() {
            return None;
        }
        // The moment's picture is what the card stands on: without one there is nothing to hold for (a stat or two,
        // only at a start after a long break).
        if !preview { return None; }
        Some(self.cabinet.launch_delay() + super::FADE)
    }

    /// Whether the card is on the screen: a launch with the moment's picture, nothing over it.
    fn where_was_i_shown(&self) -> bool {
        self.where_was_i.is_some() && self.launch_at.is_some() && self.launch_picture.is_some() && !self.sys.is_open()
    }

    /// Over the launch picture, mixed in with it (`p` 0 to 1). Clear of the spinner a big emulator shows meanwhile.
    pub(crate) fn paint_where_was_i(&mut self, c: &mut Canvas, p: f32) {
        if !self.where_was_i_shown() {
            return;
        }
        let Some(card) = self.where_was_i.as_ref() else { return };
        let bottom = if self.frozen_wait.is_some() || self.loading.since.is_some() { c.h - 112 } else { c.h - pocket_ui::kit::PAD };
        where_was_i::paint(c, &mut self.text, card, bottom, (p * 255.0) as u8);
    }

    /// A button while a game is starting: the launch animation is skipped and the card goes at once, and nothing holds
    /// the game's window back (`startup::skip_hold`). Nothing happens when neither is on the screen.
    pub(crate) fn launch_button(&mut self) -> bool {
        let preparing = self.pending_launch.as_mut().is_some_and(|p| { p.skipped = true; true });
        let reminder = self.where_was_i_skip();
        if preparing | reminder | self.cabinet.skip_insert(Instant::now()) {
            pocket_ui::startup::skip_hold();
            self.launch_at = self.launch_at.map(|t| t.min(Instant::now() - super::FADE));
            self.request_redraw();
        }
        reminder
    }

    /// The card goes, if it was up (false when there was none).
    fn where_was_i_skip(&mut self) -> bool {
        // A press during the picture's read must cancel the pending card too. Otherwise a slow/failed read swallows
        // the press and pocket-core holds for the whole reminder even though the player asked to start.
        if self.launch_at.is_none() || self.where_was_i.is_none() {
            return false;
        }
        self.where_was_i = None;
        true
    }

    /// The picture worker finished for the current launch. A missing/corrupt picture cannot show a card: release its
    /// extra hold as well. The game path is checked by the event handler, including when decoding returned None.
    pub(crate) fn where_was_i_picture(&mut self, picture: Option<pocket_ui::launch_picture::Prepared>) {
        if picture.is_none() && self.where_was_i_skip() {
            pocket_ui::startup::skip_hold();
        }
        self.launch_picture = picture;
        if self.where_was_i_shown() {
            let _ = std::fs::File::create(pocket_ui::startup::ready_path());
        }
    }

    /// `--snapshot where-was-i[:<plain|long|easy|wait|press>]`: the card over a made-up moment (or `picture`, a game's
    /// frame), as a launch shows it once its fade is done. `plain`: no note; `long`: a note of many words; `easy`: the
    /// Easy look's plain card; `wait`: a big emulator's spinner under it; `press`: after a button.
    pub(crate) fn where_was_i_snapshot(&mut self, state: &str, picture: Option<pocket_ui::image::Image>, w: u32, h: u32) {
        let what = state.strip_prefix("where-was-i").unwrap_or_default().trim_start_matches(':');
        let note = match what {
            "plain" | "easy" => "",
            "long" => "Fire temple: the key behind the waterfall is still missing. Talk to the old man in the village first, he wanted the red herb from the swamp.",
            _ => "Fire temple, key still missing.",
        };
        let facts = Facts { on: self.remind_where, kids: false, easy: what == "easy", last_played: Some(1_800_000_000 - 23 * 86_400), play_secs: 12 * 3600 + 5 * 60, note };
        self.where_was_i = where_was_i::card(&facts, 1_800_000_000);
        self.launch_at = Some(Instant::now() - Duration::from_secs(5));
        if what == "early-press" {
            self.launch_button();
        }
        let img = picture.unwrap_or_else(made_up_moment);
        let src = pocket_ui::launch_picture::Source::Resume(Default::default());
        let r = pocket_ui::launch_picture::placement(&src, "snes", img.w, img.h, Some((pocket_ui::video::Scaling::Aspect, 0)), w, h);
        let img = pocket_ui::launch_picture::scaled(&img, r.w, r.h);
        self.where_was_i_picture((what != "missing").then_some(pocket_ui::launch_picture::Prepared { rom: Default::default(), x: r.x as i32, y: r.y as i32, img }));
        if what == "wait" {
            self.frozen_wait = Some(Instant::now());
        }
        if what == "loading" {
            self.loading.since = Some(Instant::now());
        }
        // A button right after the start: what `launch_button` leaves on the screen.
        if what == "press" {
            self.launch_button();
        }
    }
}

/// A moment of a made-up game for the snapshot: sky, hills, ground and a figure, 256 x 224 like a SNES frame.
fn made_up_moment() -> pocket_ui::image::Image {
    let (w, h) = (256u32, 224u32);
    let mut px = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let hill = 150 - ((x as f32 / 22.0).sin() * 14.0) as u32;
            let v = if y >= 184 {
                if (x / 16 + y / 16) % 2 == 0 { 0x8a5a2b } else { 0x7a4e24 }
            } else if y >= 176 {
                0x3f9a3a
            } else if y >= hill {
                0x2f6f4a
            } else if (150..166).contains(&x) && (146..176).contains(&y) {
                0xd04030
            } else {
                let t = y * 255 / 176;
                (0x40 + t / 4) << 16 | (0x80 + t / 3) << 8 | 0xe0
            };
            px.push(v);
        }
    }
    pocket_ui::image::Image { w, h, px }
}
