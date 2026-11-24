//! The web companion's page in the Pocket's language (`tests/web_page_texts.js` drives `src/web/page/app.js`, with the
//! page scripts it calls loaded first): every
//! view, with things in it and with nothing, shows no English text that skipped `t("…")`, and its dates, numbers and
//! sizes come in the Pocket's language, not the browser's. Needs Node; where there is none the test says so and
//! passes (the tables' side is `web::texts`' `every_text_of_the_page_scripts_is_in_every_table`).

use std::process::Command;

#[test]
fn every_view_of_the_page_is_in_the_pockets_language() {
    let root = &std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out = match Command::new("node").arg(format!("{root}/tests/web_page_texts.js")).arg(format!("{root}/src/web/page/app.js")).arg(format!("{root}/src/web/page/cover-diagnostics.js")).output() {
        Ok(out) => out,
        Err(e) => {
            eprintln!("SKIPPED: no node ({e})");
            return;
        }
    };
    assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
}
