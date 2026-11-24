//! The web companion on a 390 px phone screen across all six languages (`tests/web_mobile_polish.js` drives
//! `src/web/page/*` with the real `assets/i18n/*.tsv` tables): no tab is clipped mid-word and every tab stays visible,
//! the update pill is not repeated on More/Updates, download buttons are unambiguous (never German "Laden"), weekday
//! labels in Stats are 7 distinct short names, and Stats tiles/bars fit 390 px cleanly. Needs Node; where there is
//! none the test says so and passes.

use std::process::Command;

#[test]
fn web_companion_mobile_390px_is_clean_in_every_language() {
    let root = &std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out = match Command::new("node").arg(format!("{root}/tests/web_mobile_polish.js")).arg(root).output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("SKIPPED: no node ({e})");
            return;
        }
    };
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
