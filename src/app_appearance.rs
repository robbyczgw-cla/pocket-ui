//! Part of the pocket-ui binary: Settings > Look > Appearance on the App's side. Which palette is in use
//! (`pocket_ui::palette`), the crossfade when it changes, and Automatic following the clock
//! (`pocket_ui::appearance`). Kept out of main.rs so the shelf only carries small hooks.

use std::time::{Duration, Instant};

use pocket_ui::appearance;
use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::palette;
use pocket_ui::settings::Appearance;

use super::App;

/// How often Automatic looks at the clock.
const CHECK_EVERY: Duration = Duration::from_secs(30);

impl App {
    /// Paints one frame with `paint` in the appearance in use. While it crossfades, the frame is painted dark
    /// and light and the two pictures are mixed (a mix of the two palettes would pass the text through the grey
    /// of its background halfway).
    pub(crate) fn paint_in_appearance(&mut self, pixels: &mut [u32], w: u32, h: u32, paint: &mut dyn FnMut(&mut App, &mut Canvas)) {
        let t = self.fader.value(Instant::now());
        if t == 0 || t == 255 {
            palette::set_blend(t);
            paint(self, &mut Canvas::new(pixels, w, h));
            return;
        }
        palette::set_blend(0);
        paint(self, &mut Canvas::new(pixels, w, h));
        let dark = pixels.to_vec();
        palette::set_blend(255);
        paint(self, &mut Canvas::new(pixels, w, h));
        for (p, d) in pixels.iter_mut().zip(dark) {
            *p = mix(d, *p, t as u32);
        }
    }

    /// The menus are on screen (not hidden behind a running game).
    fn ui_on_view(&self) -> bool {
        self.running.is_none() || self.menu.is_some() || self.switcher.is_some()
    }

    /// A new choice from the look step: kept, and shown at once with a short crossfade.
    pub(crate) fn set_appearance(&mut self, choice: Appearance) {
        if choice == self.appearance {
            return;
        }
        self.appearance = choice;
        self.save_settings();
        let now = Instant::now();
        self.fader.go(appearance::wants_light_now(choice), now, true);
        self.appearance_checked = now;
        self.request_redraw();
    }

    /// Automatic: switches when the clock says so and nobody is in the middle of something. Returns when to
    /// look again.
    pub(crate) fn appearance_tick(&mut self, now: Instant) -> Option<Instant> {
        if self.appearance != Appearance::Auto {
            return None;
        }
        if now.saturating_duration_since(self.appearance_checked) >= CHECK_EVERY || self.appearance_pending {
            self.appearance_checked = now;
            let light = appearance::wants_light_now(Appearance::Auto);
            if light != self.fader.light() {
                let on_view = self.ui_on_view();
                if appearance::may_switch(now.saturating_duration_since(self.last_input), on_view) {
                    println!("appearance: automatic, now {}", if light { "light" } else { "dark" });
                    self.fader.go(light, now, on_view);
                    self.appearance_pending = false;
                    self.request_redraw();
                } else {
                    // Asked again once the player has let go for a moment.
                    self.appearance_pending = true;
                    return Some(self.last_input + appearance::QUIET);
                }
            } else {
                self.appearance_pending = false;
            }
        }
        Some(self.appearance_checked + CHECK_EVERY)
    }

    pub(crate) fn appearance_animating(&self) -> bool {
        self.fader.animating(Instant::now())
    }
}
