//! Where a page's main pictures stand, for tests/brick_proportions.rs (off unless `POCKETUI_SHARES` is set): a page
//! marks its chart, preview, QR code or code boxes by name, and a snapshot prints the last frame's marks as
//! `share <name> x=… y=… w=… h=… canvas=<w>x<h>` lines, in the canvas' pixels. The Brick's 1024x768 is painted natively
//! and the RG35XX Plus's 640x480 at 1280x960, so the test compares each mark's share of the canvas between the two.
use std::sync::{Mutex, OnceLock};

fn record() -> Option<&'static Mutex<Vec<(&'static str, [i32; 4])>>> {
    static REC: OnceLock<Option<Mutex<Vec<(&'static str, [i32; 4])>>>> = OnceLock::new();
    REC.get_or_init(|| std::env::var_os("POCKETUI_SHARES").is_some_and(|v| !v.is_empty()).then(|| Mutex::new(Vec::new()))).as_ref()
}

/// `name`'s rectangle (x, y, w, h) in this frame: a later mark of the same name replaces it.
pub fn mark(name: &'static str, x: i32, y: i32, w: i32, h: i32) {
    if let Some(rec) = record() {
        let mut v = rec.lock().unwrap_or_else(|e| e.into_inner());
        v.retain(|(n, _)| *n != name);
        v.push((name, [x, y, w, h]));
    }
}

/// The marks as lines for a canvas `w` by `h` (none when off), and forgets them.
pub fn take(w: u32, h: u32) -> Vec<String> {
    let Some(rec) = record() else { return Vec::new() };
    let mut v = rec.lock().unwrap_or_else(|e| e.into_inner());
    v.drain(..).map(|(n, [x, y, rw, rh])| format!("share {n} x={x} y={y} w={rw} h={rh} canvas={w}x{h}")).collect()
}
