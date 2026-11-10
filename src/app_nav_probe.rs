//! Part of the pocket-ui binary: the App's half of the navigation explorer's record (`pocket_ui::nav_probe`,
//! tests/nav_explorer.rs). After the test driver's `frame` or `nav` has painted a frame and `text::boxes::finish` has
//! written its strings and marks, this adds where the App is (`@where`), the list the frame registered for touch (`@list`
//! selected row and length, `@hit` each row's area), a fingerprint of the picture (`@hash`) and `@end`, which the
//! explorer waits for. Only with `--test-driver` and `POCKETUI_NAV_PROBE`; nothing here runs otherwise.
//!
//! `@where` names the page as a player would point at it, without the selection: "settings:List Look", "setup:look
//! (settings)", "pause:Options", with what covers it after a " / " ("quick", "power", "leave", "notice"...), from
//! `App::where_now` (`app_where`). Two frames with the same `@where` are the same page; the explorer tells a page's own
//! views apart by their rows. `@painted` and `@painter` say which painter painted the frame and which one `@where`'s page
//! belongs to: the explorer fails when they differ.

use pocket_ui::touch::Target;

use super::App;

impl App {
    /// The App's lines of the record, after the strings and marks `text::boxes::finish` wrote for `buf`.
    pub(crate) fn nav_probe_done(&self, buf: &[u32]) {
        if !pocket_ui::nav_probe::on() {
            return;
        }
        let Some(path) = std::env::var_os(pocket_ui::nav_probe::ENV) else { return };
        let mut out = std::fs::read_to_string(&path).unwrap_or_default();
        let at = self.where_now();
        out += &format!("@where\t{at}\n");
        // Which painter painted the frame, and the one `@where`'s page says paints it: the explorer fails when they differ.
        out += &format!("@painted\t{}\n@painter\t{}\n", self.painted.name(), at.place.painter().name());
        // What the inputs since the last record asked for: the haptic tick and the sound (`App::feedback`).
        out += &format!("@feedback\t{}\n", self.feedback.join(" "));
        if self.setup.as_ref().is_some_and(|setup| setup.wifi_scanning()) {
            out += "@busy\twifi\n";
        }
        if self.identify.lookup_pending() {
            out += "@busy\tlookup\n";
        }
        if self.settings_page == Some(super::SettingsPage::Tools) && self.tools_animating() {
            out += "@busy\ttools\n";
        }
        if self.about_report_writing() {
            out += "@busy\treport\n";
        }
        // The quick settings, the power menu and the Date and time page keep their own touch map: the App's is the covered
        // page's (the explorer took the Settings list's row under Date and time for the page's own, 2026-11-08).
        let covered = self.sys.is_open();
        if let Some((sel, len)) = self.hits.list_state().filter(|_| !covered) {
            out += &format!("@list\t{sel}\t{len}\n");
        }
        for (a, t) in self.hits.targets().iter().filter(|_| !covered) {
            if let Target::Row(i) = t {
                out += &format!("@hit\t{}\t{}\t{}\t{}\t{i}\n", a.x, a.y, a.x + a.w, a.y + a.h);
            }
        }
        // FNV-1a over the pixels: whether two frames look the same.
        let hash = buf.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, p| (h ^ u64::from(*p)).wrapping_mul(0x0100_0000_01b3));
        out += &format!("@hash\t{hash:016x}\n@end\n");
        // Whole or not at all: the explorer reads the file as soon as it ends in "@end".
        let tmp = std::path::PathBuf::from(format!("{}.part", std::path::Path::new(&path).display()));
        if std::fs::write(&tmp, out).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}
