//! Existing curl transport, kept byte-for-byte in argument and fallback behaviour.
use std::path::Path;
use std::process::Command;

pub(crate) fn curl(url: &str, out: Option<&Path>) -> Option<Vec<u8>> {
    curl_with(url, out, &[])
}

/// curl exit code 2: it was started wrong, here by an option this build lacks (`--compressed` without zlib).
const CURL_BAD_OPTION: i32 = 2;

pub(crate) fn curl_with(url: &str, out: Option<&Path>, extra: &[&str]) -> Option<Vec<u8>> {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsSL", "--max-time", "60"]).args(extra).arg(url);
    if let Some(path) = out {
        cmd.arg("-o").arg(path);
    }
    let o = cmd.output().ok()?;
    o.status.success().then_some(o.stdout)
}

/// A directory listing of the collection. Asked for compressed (a listing is 1.5 MB of HTML, 120 KB gzipped),
/// plain when this curl can't.
pub(crate) fn curl_listing(url: &str) -> Option<Vec<u8>> {
    let o = Command::new("curl").args(["-fsSL", "--max-time", "60", "--compressed", url]).output().ok()?;
    if o.status.success() {
        return Some(o.stdout);
    }
    (o.status.code() == Some(CURL_BAD_OPTION)).then(|| curl(url, None)).flatten()
}
