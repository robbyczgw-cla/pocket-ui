//! A small support ZIP for the paired browser. The ordinary on-device report has a wider scope.

use std::path::{Path, PathBuf};

use serde_json::json;

use super::http::Response;
use crate::i18n::{self, Lang};

const LOG_TAIL: u64 = 256 << 10;
const MAX_EVENTS: usize = 200;

const NO_LOG: &str = "No shareable log entries yet.";
const UNKNOWN_DEVICE: &str = "Not available";
const LOG_VERSION: &str = "App version";
const LOG_DEVICE: &str = "Device model";

/// Only emit event names we define here. Raw text, timestamps, paths, pairing events, and values in the original
/// lines never travel. This also covers unlabeled credentials, credentials split across lines, and arbitrary text
/// from other programs that write to the same log.
fn events(raw: &str) -> Vec<&'static str> {
    const EVENTS: &[(&str, &str)] = &[
        ("first frame after ", "ui.first_frame"),
        ("library: ", "library.read"),
        ("launch: game process started ", "game.started"),
        ("audio: the stream failed;", "audio.stream_failed"),
        ("audio: new stream ", "audio.stream_reopened"),
        ("audio: no new stream;", "audio.stream_unavailable"),
        ("audio: output stalled;", "audio.output_stalled"),
        ("covers: ", "covers.finished"),
        ("sleep: suspending", "sleep.suspending"),
        ("sleep: woke after ", "sleep.woke"),
    ];
    let mut last = raw.lines().rev().filter_map(|line| {
        // The on-card copy begins each line with the uptime. Drop it without copying it into the output.
        let message = line.split_once("] ").filter(|(stamp, _)| {
            stamp.strip_prefix('[').is_some_and(|s| s.trim().bytes().all(|b| b.is_ascii_digit() || b == b'.'))
        }).map_or(line, |(_, message)| message);
        if message.starts_with("thread '") && message.contains("' panicked at ") {
            return Some("process.panic");
        }
        EVENTS.iter().find_map(|(prefix, event)| message.starts_with(prefix).then_some(*event))
    }).take(MAX_EVENTS).collect::<Vec<_>>();
    last.reverse();
    last
}

pub(super) fn log_events(data: &Path, live: &Path) -> Vec<&'static str> {
    let card = data.join(crate::logs::DIR_NAME).join(crate::logs::UI_LOG);
    for path in [&card, live] {
        if !std::fs::metadata(path).is_ok_and(|m| m.is_file()) {
            continue;
        }
        if let Some(raw) = crate::report::tail(path, LOG_TAIL) {
            let entries = events(&raw);
            if !entries.is_empty() {
                return entries;
            }
        }
    }
    Vec::new()
}

fn read_events(data: &Path) -> Vec<&'static str> {
    let live = std::env::var_os("POCKETUI_LOG").map_or_else(|| PathBuf::from("/var/log/pocket-ui.log"), PathBuf::from);
    log_events(data, &live)
}

fn device_model() -> Option<&'static str> {
    crate::device::current().model.map(|model| model.name())
}

pub(super) fn entries(version: &str, device: Option<&str>, log: &[&str], lang: Lang) -> Vec<(String, Vec<u8>)> {
    let tr = |s| i18n::tr_in(lang, s);
    let device = device.map(str::to_owned).unwrap_or_else(|| tr(UNKNOWN_DEVICE));
    let system = format!("{}: {version}\n{}: {device}\n", tr(LOG_VERSION), tr(LOG_DEVICE));
    let mut log = if log.is_empty() { tr(NO_LOG) } else { log.join("\n") };
    log.push('\n');
    vec![("system.txt".into(), system.into_bytes()), ("pocket-ui.log".into(), log.into_bytes())]
}

pub(super) fn status(data: &Path) -> Response {
    Response::json(200, &json!({
        "version": crate::update::app_version(),
        "device": device_model(),
        "hasLog": !read_events(data).is_empty(),
    }))
}

pub(super) fn download(data: &Path) -> Response {
    let files = entries(crate::update::app_version(), device_model(), &read_events(data), i18n::current());
    let bytes = crate::report::zip(&files, &crate::screenshot::Stamp::now());
    Response::new(200, "application/zip", bytes)
        .with("Content-Disposition", "attachment; filename=\"pocket-diagnostics.zip\"")
}
