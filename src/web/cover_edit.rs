//! Requests resolve opaque game IDs through the library. The App alone applies a cover edit.

use std::io::Read;
use std::path::Path;
use std::sync::atomic::Ordering;

use serde_json::json;

use super::api::Ctx;
use super::http::{self, Request, Response};
use super::Edit;
use crate::covers::{self, custom};

pub(super) fn describe(data: &Path, system: &str, rom: &Path) -> (Option<String>, bool) {
    let own = custom::existing(data, system, rom);
    let is_custom = own.is_some();
    let path = own.unwrap_or_else(|| covers::cover_path(data, system, rom));
    let revision = std::fs::metadata(path).ok()
        .filter(|m| m.is_file() && (1..=custom::MAX_BYTES).contains(&m.len()))
        .map(|m| {
            let modified = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
            format!("{}-{}-{}", u8::from(is_custom), m.len(), modified.map_or(0, |t| t.as_nanos()))
        });
    (revision, is_custom)
}

pub(super) fn change(ctx: &Ctx, req: &Request, body: &mut impl Read, reset: bool) -> Result<Response, Response> {
    let lib = ctx.library();
    let (s, g) = req.query("id").and_then(|id| lib.game(id))
        .ok_or_else(|| Response::error(404, "That game is not on the Pocket any more."))?;
    let system = &lib.systems[s];
    let rom = &system.games[g].path;
    let png = if reset {
        None
    } else {
        let bytes = http::read_body(body, req, custom::MAX_BYTES)
            .map_err(|status| Response::error(status, if status == 413 { "The cover is over 8 MB." } else { "The upload was cut off." }))?;
        if req.header("content-type") != Some("image/png") || !custom::valid_png(&bytes) {
            return Err(Response::error(400, "Choose a PNG up to 1024 pixels per side."));
        }
        Some(bytes)
    };
    if ctx.shared.playing() {
        return Err(Response::json(503, &json!({ "playing": true })));
    }
    if ctx.stopped.load(Ordering::Relaxed) {
        return Err(Response::error(503, "The web companion is off."));
    }
    ctx.shared.edit(Edit::Cover { system: system.name.clone(), rom: rom.clone(), png })
        .map_err(|msg| Response::error(409, &msg))?;
    let (revision, is_custom) = describe(&ctx.shared.data_dir, &system.name, rom);
    Ok(Response::json(200, &json!({ "cover": revision.is_some(), "coverRevision": revision, "coverCustom": is_custom })))
}
