//! HTTPS through Android's certificate store. Blocking work belongs on media/identification workers.
use std::path::Path;

pub(crate) fn curl_listing(url: &str) -> Option<Vec<u8>> { curl_with(url, None, &[]) }
pub(crate) fn curl_with(url: &str, out: Option<&Path>, extra: &[&str]) -> Option<Vec<u8>> {
    let limit = match extra {
        [] => 8 * 1024 * 1024,
        ["--max-filesize", n] => n.parse::<usize>().ok()?,
        _ => return None, // Never silently ignore a size or safety constraint.
    };
    // Java writes the bounded response atomically to the caller's .part file, or returns text.
    let path = out.map(|p| p.to_str()).unwrap_or(Some(""))?;
    let result = super::android::strings("httpGet", &[url, path, &limit.to_string()]).ok()?;
    result.strip_prefix("OK\n").map(|s| s.as_bytes().to_vec())
}
