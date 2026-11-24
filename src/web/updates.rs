//! Signed-channel update status and install trigger for the web companion (`GET /api/update`,
//! `POST /api/update/check`, `POST /api/update/install`).
//!
//! Security & rules:
//! - Only reads the App's published [`UpdateSnapshot`] and sends [`Edit::CheckUpdate`] /
//!   [`Edit::InstallUpdate`] to the App thread, which uses the existing signed channel (`update::fetch_manifest`,
//!   Ed25519 signature check, SHA-256 verification, atomic switch, rollback-safe restart).
//! - Never checks or installs while a game is running (blocked at both HTTP layer and App thread).
//! - Never installs a version that is not the currently offered signed release, and never overwrites a
//!   development build.

use std::io::Read;

use serde_json::{json, Value};

use crate::i18n;
use crate::update;

use super::api::{self, Ctx};
use super::http::{Request, Response};
use super::Edit;

/// High-level phase of the App's updater as exposed to the browser.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    Downloading,
    Unpacking,
    Installing,
    Restarting,
    Failed,
}

impl UpdateState {
    pub fn code(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Checking => "checking",
            Self::Downloading => "downloading",
            Self::Unpacking => "unpacking",
            Self::Installing => "installing",
            Self::Restarting => "restarting",
            Self::Failed => "failed",
        }
    }

    pub fn is_busy(self) -> bool {
        matches!(self, Self::Checking | Self::Downloading | Self::Unpacking | Self::Installing | Self::Restarting)
    }
}

/// What the App publishes to `Shared::update` so the browser shows the exact same update state as
/// Settings › System › Updates on the Pocket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateSnapshot {
    pub current: String,
    pub available: Option<String>,
    pub notes: Option<String>,
    pub size: Option<u64>,
    pub release_build: bool,
    pub android: bool,
    pub state: UpdateState,
    /// 1..=4 during an app install (`update::Step::number`).
    pub step: Option<u8>,
    /// 0..=100 progress within `step`.
    pub percent: Option<u8>,
    /// English status or error message (translated on read via `i18n::tr`).
    pub message: Option<String>,
    /// Low-battery warning when under 20 % and off the charger (English, translated on read).
    pub low_battery: Option<String>,
    /// True once a check has completed in this process (distinguishes "not checked yet" from "up to date").
    pub checked: bool,
}

impl Default for UpdateSnapshot {
    fn default() -> Self {
        Self::initial()
    }
}

impl UpdateSnapshot {
    pub fn initial() -> Self {
        Self {
            current: update::shown_version().to_string(),
            available: None,
            notes: None,
            size: None,
            release_build: update::release_build() || cfg!(all(target_os = "android", feature = "handheld")),
            android: cfg!(all(target_os = "android", feature = "handheld")),
            state: UpdateState::Idle,
            step: None,
            percent: None,
            message: None,
            low_battery: None,
            checked: false,
        }
    }
}

/// `GET /api/update`: current version, available signed update, install progress, and clear feedback.
pub fn status(ctx: &Ctx) -> Response {
    let snap = ctx.shared.update.lock().unwrap().clone();
    let step_line = snap.step.and_then(|n| {
        let step = match n {
            1 => Some(update::Step::Downloading),
            2 => Some(update::Step::Unpacking),
            3 => Some(update::Step::Installing),
            4 => Some(update::Step::Restarting),
            _ => None,
        }?;
        Some(i18n::tr(&step.line(snap.percent.unwrap_or(0) as u32)))
    });
    let summary = match &snap.available {
        Some(avail) => i18n::tr(&format!("{}, new: {avail}", snap.current)),
        None => snap.current.clone(),
    };
    let message = snap.message.as_deref().map(i18n::tr);
    let low_battery = snap.low_battery.as_deref().map(i18n::tr);
    Response::json(
        200,
        &json!({
            "current": snap.current,
            "available": snap.available,
            "summary": summary,
            "notes": snap.notes,
            "size": snap.size,
            "releaseBuild": snap.release_build,
            "android": snap.android,
            "state": snap.state.code(),
            "busy": snap.state.is_busy(),
            "step": snap.step,
            "percent": snap.percent,
            "stepLine": step_line,
            "message": message,
            "lowBattery": low_battery,
            "checked": snap.checked,
            "playing": ctx.shared.playing(),
        }),
    )
}

/// `POST /api/update/check`: asks the App to check the signed update channel.
pub fn check(ctx: &Ctx) -> Result<Response, Response> {
    if ctx.shared.playing() {
        return Err(Response::json(503, &json!({ "error": i18n::tr("A game is running. Quit the game before checking for updates."), "playing": true })).with("Retry-After", "10"));
    }
    if ctx.shared.update.lock().unwrap().state.is_busy() {
        return Err(Response::error(409, &i18n::tr("An update check or install is already running.")));
    }
    match ctx.shared.edit(Edit::CheckUpdate) {
        Ok(()) => Ok(Response::json(200, &json!({ "ok": true }))),
        Err(msg) => Err(Response::error(409, &i18n::tr(&msg))),
    }
}

/// `POST /api/update/install` `{"version": "0.6.1"}`: starts installing the offered signed release through the App.
pub fn install(ctx: &Ctx, req: &Request, body: &mut impl Read) -> Result<Response, Response> {
    if ctx.shared.playing() {
        return Err(Response::json(503, &json!({ "error": i18n::tr("A game is running. Quit the game before updating."), "playing": true })).with("Retry-After", "10"));
    }
    let v = api::json_body(req, body)?;
    let version = v
        .get("version")
        .and_then(Value::as_str)
        .and_then(update::observation::public_version)
        .ok_or_else(|| Response::error(400, &i18n::tr("Which version should be installed?")))?
        .to_string();
    let snap = ctx.shared.update.lock().unwrap().clone();
    if !snap.release_build {
        return Err(Response::error(409, &i18n::tr(update::DEVELOPMENT_BUILD)));
    }
    if snap.state.is_busy() {
        return Err(Response::error(409, &i18n::tr("An update is already running.")));
    }
    if snap.available.as_deref() != Some(version.as_str()) {
        return Err(Response::error(409, &i18n::tr("That update is not available any more. Check again.")));
    }
    match ctx.shared.edit(Edit::InstallUpdate { version }) {
        Ok(()) => Ok(Response::json(200, &json!({ "ok": true, "restartingSoon": !snap.android }))),
        Err(msg) => Err(Response::error(409, &i18n::tr(&msg))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_translates_new_version_label_in_german_and_other_languages() {
        for (lang, expected) in [
            (i18n::Lang::En, "0.6.0, new: 0.6.1"),
            (i18n::Lang::De, "0.6.0, neu: 0.6.1"),
            (i18n::Lang::Es, "0.6.0, nueva: 0.6.1"),
            (i18n::Lang::Pt, "0.6.0, nova: 0.6.1"),
            (i18n::Lang::Fr, "0.6.0, nouvelle\u{a0}: 0.6.1"),
            (i18n::Lang::It, "0.6.0, nuova: 0.6.1"),
        ] {
            let _g = i18n::for_this_thread(lang);
            assert_eq!(i18n::tr("0.6.0, new: 0.6.1"), expected, "{lang:?}");
        }
    }
}
