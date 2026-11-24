//! The web companion's Saves tab and a game's saves as a player sees them (`tests/web_saves_page.js` drives
//! `src/web/page/saves.js`): every view says something when there is nothing (no saves, Pocket Sync off, nothing paired,
//! nothing synced yet). Needs Node; where there is none the test says so and passes, as the server side is tested in
//! `src/web/saves_tests.rs`.

use std::process::Command;

#[test]
fn every_saves_view_says_something_when_there_is_nothing() {
    let root = &std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out = match Command::new("node").arg(format!("{root}/tests/web_saves_page.js")).arg(format!("{root}/src/web/page/saves.js")).output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("SKIPPED: no node ({e})");
            return;
        }
    };
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}
