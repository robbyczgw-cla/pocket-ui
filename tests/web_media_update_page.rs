//! The web companion's More hub, Photo album & screenshots, and Software update views as a player sees them
//! (`tests/web_media_update_page.js` drives `src/web/page/media-update.js`): every view says something clear when
//! empty or in every update state, and dates follow the Pocket's language in 24-hour format. Needs Node; where there
//! is none the test says so and passes, as the server side is tested in `src/web/tests.rs`.

use std::process::Command;

#[test]
fn every_media_and_update_view_says_something_clear() {
    let root = &std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out = match Command::new("node")
        .arg(format!("{root}/tests/web_media_update_page.js"))
        .arg(format!("{root}/src/web/page/media-update.js"))
        .output()
    {
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
