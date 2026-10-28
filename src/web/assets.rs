//! The page itself: one HTML file, one style sheet, one script (vanilla JS), the two Barlow faces of the device and an
//! icon, all compiled into pocket-ui (`include_str!`/`include_bytes!`). Nothing is fetched from anywhere else, which
//! the Content-Security-Policy (`http::SECURITY_HEADERS`) enforces in the browser too.

pub const INDEX_HTML: &str = include_str!("page/index.html");
pub const APP_CSS: &str = include_str!("page/app.css");
pub const APP_JS: &str = include_str!("page/app.js");
pub const ICON_SVG: &str = include_str!("page/icon.svg");
pub const MANIFEST: &str = include_str!("page/manifest.webmanifest");
pub const FONT_BODY: &[u8] = include_bytes!("../../assets/fonts/Barlow-Medium.ttf");
pub const FONT_TITLE: &[u8] = include_bytes!("../../assets/fonts/BarlowCondensed-SemiBold.ttf");

/// A file of the page by its path: (content type, bytes, may be kept by the browser).
pub fn find(path: &str) -> Option<(&'static str, &'static [u8], bool)> {
    Some(match path {
        "/" | "/index.html" => ("text/html; charset=utf-8", INDEX_HTML.as_bytes(), false),
        "/app.css" => ("text/css; charset=utf-8", APP_CSS.as_bytes(), true),
        "/app.js" => ("text/javascript; charset=utf-8", APP_JS.as_bytes(), true),
        "/icon.svg" | "/favicon.ico" => ("image/svg+xml", ICON_SVG.as_bytes(), true),
        "/manifest.webmanifest" => ("application/manifest+json", MANIFEST.as_bytes(), true),
        "/fonts/body.ttf" => ("font/ttf", FONT_BODY, true),
        "/fonts/title.ttf" => ("font/ttf", FONT_TITLE, true),
        _ => return None,
    })
}
