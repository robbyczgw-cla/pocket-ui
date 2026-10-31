//! Part of the pocket-ui binary: the look step's previews. Each look's shelf is painted by the same code as the
//! real shelf (`paint_home`), with the sample from `pocket_ui::look_sample` in place of the library, into a
//! frame of the screen's size, and scaled down to `setup::look_preview`. Made once when the step opens.

use std::rc::Rc;
use std::time::Instant;

use pocket_ui::gfx::{mix, Canvas};
use pocket_ui::image::{self, Image};
use pocket_ui::look_sample::{self, Cover, Sample};
use pocket_ui::palette::*;
use pocket_ui::settings::Theme;
use pocket_ui::text::Face;

use super::{stripe, system_short, App, Item, Screen, PULL};


impl App {
    /// One preview per theme (in `Theme::ALL` order), and whether they show the built-in sample.
    pub(crate) fn render_look_previews(&mut self) -> (Vec<Image>, bool) {
        let started = Instant::now();
        let data_dir = self.data_dir.clone();
        // A game hidden on its card is no sample either (`app_storage`).
        let recents: Vec<_> = self.recents.list.iter().filter(|r| !self.library.is_hidden(&r.path)).cloned().collect();
        let sample = look_sample::pick(&self.systems, &self.hidden, &recents, |system, rom| {
            let path = pocket_ui::covers::cover_path(&data_dir, system, rom);
            path.exists().then_some(path)
        });
        let cover = match &sample.cover {
            Cover::File(path) => image::load_image(path).unwrap_or_else(|| self.sample_box(&sample)),
            Cover::Drawn => self.sample_box(&sample),
        };

        // The shelf as the sample has it: the game (to continue when it was played), then the systems; the game
        // selected.
        let first = if sample.played {
            Item::Recent(0)
        } else {
            let at = sample.systems.iter().enumerate().find_map(|(s, sys)| sys.games.iter().position(|g| g.path == sample.game.path).map(|g| (s, g)));
            at.map_or(Item::Recent(0), |(s, g)| Item::Game(s, g))
        };
        let items: Vec<Item> = std::iter::once(first).chain((0..sample.systems.len()).map(Item::System)).collect();
        let systems = std::mem::replace(&mut self.systems, sample.systems.clone());
        let shown_recents = if matches!(first, Item::Recent(_)) { vec![sample.game.clone()] } else { Vec::new() };
        let recents = std::mem::replace(&mut self.recents.list, shown_recents);
        let items = std::mem::replace(&mut self.items, items);
        let (sel, prev_sel, screen, theme, moved_at) = (self.sel, self.prev_sel, self.screen, self.theme, self.moved_at);
        let (toast, notice, launch_at) = (self.toast.take(), self.notice.take(), self.launch_at.take());
        let marquee = std::mem::take(&mut self.marquee);
        let hits = std::mem::take(&mut self.hits);
        // The previews show each look at rest, without touching the glides of the shelf behind (`app_motion`).
        let motion = std::mem::take(&mut self.motion);
        let covers = std::mem::take(&mut self.covers);
        let cover_run = std::mem::take(&mut self.cover_run);
        // The cabinet's shelves and pictures are the library's; the sample gets its own, read right away.
        let cabinet = std::mem::take(&mut self.cabinet);
        self.cabinet.inline = true;
        self.sample_cover = Some((sample.game.path.clone(), Rc::new(cover)));
        (self.sel, self.prev_sel, self.screen) = (0, 0, Screen::Home);
        // Fully pulled out, as it rests.
        self.moved_at = Instant::now() - PULL * 4;

        // Painted at this screen's size (the Mini V2's 1240x1080, the Pocket 5's 1920x1080, ...), so each look
        // shows as it will be here, and scaled down to the step's preview.
        let (frame_w, frame_h) = self.screen_size();
        let (pw, ph) = pocket_ui::setup::look_preview((frame_w as i32, frame_h as i32));
        let mut previews = Vec::new();
        let mut buf = vec![0u32; (frame_w * frame_h) as usize];
        for t in Theme::ALL {
            self.theme = t;
            // Each look at rest: nothing glides over from the one painted before.
            self.motion = Default::default();
            // Minimal shows a console only next to a selected system, so its preview selects the first one.
            self.sel = if t == Theme::Minimal { 1 } else { 0 };
            self.prev_sel = self.sel;
            let mut c = Canvas::new(&mut buf, frame_w, frame_h);
            // As `App::paint` has it: the Shelf look in the column on a wide screen (`app_column`).
            if t == Theme::Shelf {
                self.in_column(&mut c, |app, c| app.paint_home(c, true));
            } else {
                self.paint_home(&mut c, true);
            }
            let frame = Image { w: frame_w, h: frame_h, px: buf.clone() };
            previews.push(frame.resized(pw, ph));
        }

        self.systems = systems;
        self.recents.list = recents;
        self.items = items;
        (self.sel, self.prev_sel, self.screen, self.theme, self.moved_at) = (sel, prev_sel, screen, theme, moved_at);
        (self.toast, self.notice, self.launch_at) = (toast, notice, launch_at);
        self.marquee = marquee;
        self.hits = hits;
        self.motion = motion;
        self.covers = covers;
        self.cover_run = cover_run;
        self.cabinet = cabinet;
        self.sample_cover = None;
        println!("look previews: {} looks in {:?}{}", previews.len(), started.elapsed(), if sample.demo { ", built-in sample" } else { "" });
        (previews, sample.demo)
    }

    /// Box art for a sample game without any: a plain box in its system's colour with the console and the name,
    /// clearly not a real game's.
    fn sample_box(&mut self, sample: &Sample) -> Image {
        let (w, h) = (300u32, 420u32);
        let system = sample.game.system.clone();
        let color = stripe(&system);
        let mut px = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut px, w, h);
        // Darker towards the bottom.
        for y in 0..h as i32 {
            c.rect(0, y, w as i32, 1, mix(color, 0x000000, 40 + (y as u32 * 120 / h)));
        }
        c.rect(0, 0, w as i32, 54, mix(color, 0x000000, 190));
        let short = system_short(&sample.systems, &system);
        let short = self.text.fit(Face::Body, 24, &short, w as i32 - 40);
        self.text.draw(&mut c, Face::Body, 24, 20, 36, CHALK, &short);
        if let Some(art) = pocket_ui::system_art::get(&system).map(|a| a.fitted(w as i32 - 60, 170)) {
            c.blit_sprite(&art, (w as i32 - art.w as i32) / 2, 80 + (170 - art.h as i32) / 2, 0);
        }
        let mut y = 310;
        for line in self.text.wrap(Face::Title, 50, &sample.game.name, w as i32 - 40).iter().take(2) {
            self.text.draw(&mut c, Face::Title, 50, 20, y, 0xffffff, line);
            y += 50;
        }
        c.frame(0, 0, w as i32, h as i32, 3, mix(color, 0xffffff, 90));
        Image { w, h, px }
    }
}

/// The colours, font and button symbols in use (Settings → Look → Colours and Font, Playing → Buttons): the previews are
/// painted again when one changes.
pub(crate) fn style_mark() -> u32 {
    pocket_ui::button_prompts::get() as u32 | (pocket_ui::palette::colours() as u32) << 4 | (pocket_ui::font_style::get() as u32) << 8
}

/// What the previews were painted with: appearance, language, text size and the corner's clock ("14:02", None while the
/// clock was never set).
pub(crate) type PreviewKey<'a> = (bool, pocket_ui::i18n::Lang, pocket_ui::type_scale::TextSize, &'a Option<String>);

/// Whether the previews must be painted again: any of those changed. The clock is in it so that the small clock in
/// each preview is the real one, also minutes after the step opened or after the zone step.
pub(crate) fn previews_stale(made: PreviewKey, now: PreviewKey) -> bool {
    made != now
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocket_ui::i18n::Lang;
    use pocket_ui::type_scale::TextSize;

    #[test]
    fn the_previews_are_painted_again_when_the_minute_changes() {
        let (a, b) = (Some("14:02".to_string()), Some("14:03".to_string()));
        let made = (false, Lang::En, TextSize::Normal, &a);
        assert!(!previews_stale(made, (false, Lang::En, TextSize::Normal, &a)));
        assert!(previews_stale(made, (false, Lang::En, TextSize::Normal, &b)), "a new minute: the clock in the previews is the real one");
        assert!(previews_stale(made, (false, Lang::En, TextSize::Normal, &None)));
        assert!(previews_stale(made, (true, Lang::En, TextSize::Normal, &a)));
    }
}
