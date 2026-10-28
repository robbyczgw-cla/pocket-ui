//! Part of the pocket-ui binary: the App's side of `pocket_ui::display_out` (playing on a TV through USB-C,
//! docs/DISPLAY-OUT.md). The thread that listens to sway starts after the first frame; on the UI thread a change
//! costs one stored value and a toast. While the picture is on a TV the panel is dark, so touches on it are dropped
//! (`App::touch_event`) rather than tapping blindly on what the TV shows.

use pocket_ui::display_out::{self, Tv};

use super::{App, UserEvent};

impl App {
    /// The display-out thread, once (`after_first_frame`). Nothing at all on a desktop without sway.
    pub(crate) fn display_out_start(&mut self) {
        #[cfg(target_os = "linux")]
        {
            let proxy = std::sync::Mutex::new(self.proxy.clone());
            display_out::watch(move |tv| proxy.lock().is_ok_and(|p| p.send_event(UserEvent::Display(tv)).is_ok()));
        }
    }

    /// The picture went to a TV, or came back to the panel. The shelf says so for a moment; a game keeps running
    /// (sway resizes its window, pocket-core draws at the new size each frame).
    pub(crate) fn display_changed(&mut self, tv: Option<Tv>) {
        if let Some(msg) = display_out::change_message(self.tv.as_ref(), tv.as_ref()) {
            if self.running.is_none() {
                self.toast = Some((msg.into(), std::time::Instant::now()));
            }
        }
        self.tv = tv;
        // A finger that was down when the panel went dark must not finish as a tap later.
        self.touch_rec.reset();
    }

    /// The picture is on a TV now (the panel is dark).
    pub(crate) fn on_tv(&self) -> bool {
        self.tv.is_some()
    }
}
