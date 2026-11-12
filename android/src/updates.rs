//! Signed channel reads run on a worker; Android streams and installs the verified APK offer.
use super::*;
use pocket_ui::{signed_data::PUBLIC_KEY, update_channel::{self, Manifest}, update_hint};

#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct Status { phase: String, bytes: u64, total: u64, message: String, cancellable: bool }

fn fetch(url: &str) -> Result<Manifest, String> {
    let read = |url: &str, limit: &str| -> Result<String, String> {
        let result = pocket_ui::platform::android::strings("updateRead", &[url, limit])?;
        result.strip_prefix("OK\n").map(String::from).ok_or_else(|| result.strip_prefix("ERR\n").unwrap_or("Couldn't reach the update server. Check Wi-Fi.").to_string())
    };
    let json = read(url, "1048576")?;
    let signature = read(&format!("{url}.sig"), "1024")?;
    let manifest = update_channel::parse_manifest(json.as_bytes(), &signature, &PUBLIC_KEY)?;
    update_channel::android_offer(&manifest, url)?;
    Ok(manifest)
}
impl App {
    pub(crate) fn update_check(&mut self, automatic: bool) {
        if self.update_receiver.is_some() { return; }
        self.ui.updates.checking = true;
        self.ui.updates.phase.clear();
        self.ui.updates.message.clear();
        let (send, receive) = mpsc::channel();
        self.update_receiver = Some(receive);
        self.update_automatic = automatic;
        let url = self.update_url.clone();
        std::thread::spawn(move || { let _ = send.send(fetch(&url)); });
        self.redraw();
    }
    pub(crate) fn update_install(&mut self) {
        let Some(release) = self.ui.updates.available() else { self.ui.updates.phase.clear(); return; };
        let request = serde_json::json!({ "urls": release.download_urls(&self.update_url), "version": release.version, "sha256": release.sha256, "size": release.size });
        // Starting the worker is cheap. Android owns the session and returns status on subsequent looks.
        let _ = bridge("updateStart", &[&request.to_string()]);
        self.redraw();
    }
    pub(crate) fn updates_tick(&mut self) {
        if let Some(result) = self.update_receiver.as_ref().and_then(|r| r.try_recv().ok()) {
            self.update_receiver = None;
            if !self.update_automatic || !self.ui.updates.hint.off {
                self.update_failures = if result.is_ok() { 0 } else { self.update_failures.saturating_add(1) };
                self.update_after = Instant::now() + if result.is_ok() { update_hint::EVERY } else { update_hint::retry_after(self.update_failures) };
                self.ui.updates_checked(result);
            } else { self.ui.updates.checking = false; }
            self.redraw();
        }
        if !self.ui.updates.phase.is_empty() {
            if let Ok(status) = serde_json::from_str::<Status>(&bridge("updateStatus", &[])) {
                let u = &mut self.ui.updates;
                if u.phase != status.phase || u.bytes != status.bytes || u.message != status.message || u.cancellable != status.cancellable {
                    u.phase = status.phase; u.bytes = status.bytes; u.total = status.total; u.message = status.message; u.cancellable = status.cancellable;
                    self.redraw();
                }
            }
        }
        if Instant::now() < self.update_after || !self.ui.prefs.onboarding_done { return; }
        let facts = update_hint::Facts {
            auto: !self.ui.updates.hint.off,
            game: self.insert.is_some() || self.window.is_none(),
            busy: self.ui.updates.busy(),
            wifi: self.ui.wifi_ready,
            idle: self.update_idle.elapsed() >= update_hint::IDLE_FOR,
            shelf_free: self.ui.screen == ui::Screen::Library,
            ..Default::default()
        };
        if update_hint::step(facts) == update_hint::Step::Look { self.update_check(true); }
    }
}
