//! Speed guard: the `frame` line of the latency trace (`pocket_ui::trace`, `scripts/latency.sh`, docs/LATENCY.md).
//! Only called while `POCKETUI_TRACE` is set; nothing here runs otherwise.

use pocket_ui::shelf::Item;

use super::{App, Screen, SettingsPage};

impl App {
    /// `frame <screen> in=<last input shown> sel=<selected entry>` after a frame is presented. The screen follows
    /// the order in which `paint` picks what to draw. Then `paint <ms> <w>x<h>`: how long painting and handing
    /// the frame over took (`took`), and the window's size (a frame drawn before sway's resize arrived shows here).
    pub(crate) fn trace_frame(&self, took: std::time::Duration, w: u32, h: u32) {
        let screen = if self.sleep.overlay().is_some() {
            "sleep"
        } else if self.sys.is_open() {
            "quick"
        } else if self.running.as_ref().is_some_and(|r| r.is_app()) {
            "app"
        } else if self.settings_page == Some(SettingsPage::Manual) {
            "manual"
        } else if self.setup.is_some() {
            "setup"
        } else if self.settings_page.is_some() {
            "settings"
        } else if self.running.is_none() && self.port_files.is_some() {
            "port-files"
        } else if self.running.is_none() && self.card.is_some() {
            "card"
        } else if self.running.is_none() && self.console.is_some() {
            "console"
        } else if self.switcher.is_some() {
            "switcher"
        } else if self.menu.is_some() {
            "menu"
        } else if self.running.is_some() {
            // Drawn behind the game (the window is not on top).
            "behind-game"
        } else {
            match self.screen {
                Screen::Home => "shelf",
                Screen::Games(_) => "games",
                Screen::Favorites => "favorites",
                Screen::Curator | Screen::Shelf(_) | Screen::Sticker(_) | Screen::LensResults | Screen::Rediscover => "curator",
                Screen::Connection => "connection",
            }
        };
        let file = |p: &std::path::Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let sel = match self.items.get(self.sel) {
            Some(Item::System(s)) => format!("system:{}", self.systems[*s].name),
            Some(Item::Game(s, g)) => format!("game:{}/{}", self.systems[*s].name, file(&self.systems[*s].games[*g].path)),
            Some(Item::Recent(i)) => self
                .recents
                .list
                .get(*i)
                .map(|r| format!("recent:{}/{}", r.system, file(&r.path)))
                .unwrap_or_default(),
            Some(Item::Favorites) => "favorites".into(),
            Some(Item::Streaming) => "streaming".into(),
            Some(Item::Search) => "search".into(),
            Some(Item::Curator | Item::Lens | Item::Surprise | Item::SmartShelf(_) | Item::Sticker(_) | Item::History | Item::Review(_) | Item::Rediscover) => "curator".into(),
            None => "-".into(),
        };
        // Spaces in a file name would split the line; the harness reads `sel=` up to the end.
        pocket_ui::mark!("frame", "{screen} in={} sel={sel}", pocket_ui::trace::inputs());
        pocket_ui::mark!("paint", "{:.2} {w}x{h}", took.as_secs_f64() * 1000.0);
    }
}
