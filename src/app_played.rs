//! App's common play evidence. The directory index is made by the library worker at startup and refreshed on workers.
use std::path::Path;
use pocket_ui::play_evidence::Evidence;
use super::{App, UserEvent};

impl App {
    pub(crate) fn play_evidence(&self, rom: &Path) -> Evidence {
        Evidence {
            recorded: self.library.ever_played(rom) || self.recents.list.iter().any(|r| r.path == rom),
            files: self.played_files.contains(rom),
            progress: self.case_knows.has_progress(rom),
        }
    }

    pub(crate) fn remember_played_files(&mut self, rom: &Path) {
        if self.played_files.insert(rom.to_path_buf()) {
            self.cases_look_changed();
            self.curator.touch();
            self.request_redraw();
        }
    }

    pub(crate) fn played_refresh(&mut self) {
        self.played_scan += 1;
        let (gen, systems, proxy, data) = (self.played_scan, self.systems.clone(), self.proxy.clone(), self.data_dir.clone());
        std::thread::spawn(move || {
            pocket_ui::web::lower_priority();
            let mut files = pocket_ui::play_evidence::scan(&systems);
            if pocket_ui::device::android_profile() { files.extend(pocket_ui::android_handheld::mobile_played(&data)); }
            let _ = proxy.send_event(UserEvent::PlayedFiles(gen, files));
        });
    }
}
