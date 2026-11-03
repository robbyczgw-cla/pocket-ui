//! The phone's page as a player uses it (`tests/phone_upload_page.js` drives `assets/phone-upload/app.js`): each file's own
//! system, a disc's tracks with their sheet. Needs Node; where there is none the test says so and passes, as the page's
//! server side is tested in `phone_upload`.

use std::process::Command;

#[test]
fn each_file_goes_to_its_own_system_and_tracks_go_with_their_sheet() {
    let root = &std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out = match Command::new("node").arg(format!("{root}/tests/phone_upload_page.js")).arg(format!("{root}/assets/phone-upload/app.js")).output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("SKIPPED: no node ({e})");
            return;
        }
    };
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}
