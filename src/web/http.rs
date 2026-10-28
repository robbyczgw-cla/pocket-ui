//! The smallest HTTP/1.1 the companion needs, on std only: one request per connection (`Connection: close`), a head
//! of at most [`MAX_HEAD`] bytes, a body only with `Content-Length` (no chunked requests), and every response with the
//! same security headers ([`SECURITY_HEADERS`]). No crate: the whole of it is here and in its tests.

use std::io::{self, BufRead, Read, Write};

/// The request line and the headers together.
pub const MAX_HEAD: usize = 16 * 1024;
/// Headers in one request.
const MAX_HEADERS: usize = 64;
/// A JSON body (edits, the pairing code).
pub const MAX_JSON: u64 = 64 * 1024;

/// Sent with every response. The page is one HTML file, one script and one style sheet from this server; nothing may
/// come from anywhere else, nothing may frame it, and no inline script runs (`blob:` images: the screenshot, fetched
/// with its headers and shown from memory).
pub const SECURITY_HEADERS: &[(&str, &str)] = &[
    (
        "Content-Security-Policy",
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; font-src 'self'; connect-src 'self'; \
         manifest-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
    ),
    ("X-Content-Type-Options", "nosniff"),
    ("X-Frame-Options", "DENY"),
    ("Referrer-Policy", "no-referrer"),
    ("Cross-Origin-Opener-Policy", "same-origin"),
    ("Cross-Origin-Resource-Policy", "same-origin"),
    ("Permissions-Policy", "camera=(), microphone=(), geolocation=(), usb=(), payment=()"),
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Request {
    pub method: String,
    /// The path without the query, as sent ("/api/games").
    pub path: String,
    /// Decoded query pairs.
    pub query: Vec<(String, String)>,
    /// Header names in lower case.
    pub headers: Vec<(String, String)>,
    pub content_length: Option<u64>,
}

/// Why a request could not be read; each is one status code.
#[derive(Debug, PartialEq)]
pub enum BadRequest {
    /// The client closed the connection before a request (nothing to answer).
    Closed,
    Malformed,
    HeadTooLarge,
    /// `Transfer-Encoding` (chunked uploads) is not taken.
    LengthRequired,
}

impl BadRequest {
    pub fn status(&self) -> u16 {
        match self {
            BadRequest::Closed | BadRequest::Malformed => 400,
            BadRequest::HeadTooLarge => 431,
            BadRequest::LengthRequired => 411,
        }
    }
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    pub fn query(&self, name: &str) -> Option<&str> {
        self.query.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    /// The value of cookie `name` (the first one sent).
    pub fn cookie<'a>(&'a self, name: &'a str) -> Option<&'a str> {
        self.cookies(name).next()
    }

    /// Every value sent for cookie `name`, in order: a browser sends several when another site set one of that name
    /// for a wider domain.
    pub fn cookies<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.headers
            .iter()
            .filter(|(n, _)| n == "cookie")
            .flat_map(|(_, v)| v.split(';'))
            .filter_map(|c| c.trim().split_once('='))
            .filter(move |(n, _)| *n == name)
            .map(|(_, v)| v.trim())
            .take(8)
    }

    /// The host the browser asked for, without the port, in lower case ("pocket", "100.101.102.103", "fd7a::1").
    pub fn host(&self) -> Option<String> {
        let h = self.header("host")?.trim();
        let name = if let Some(rest) = h.strip_prefix('[') {
            rest.split(']').next()?
        } else {
            h.rsplit_once(':').filter(|(_, p)| p.bytes().all(|b| b.is_ascii_digit())).map_or(h, |(n, _)| n)
        };
        (!name.is_empty()).then(|| name.trim_end_matches('.').to_ascii_lowercase())
    }
}

/// Reads the request line and headers. `r` must be buffered; the body (if any) follows in it.
pub fn read_head(r: &mut impl BufRead) -> Result<Request, BadRequest> {
    let mut used = 0usize;
    let mut line = Vec::new();
    let mut next_line = |r: &mut dyn BufRead, line: &mut Vec<u8>| -> Result<usize, BadRequest> {
        line.clear();
        let n = (&mut *r).take((MAX_HEAD - used.min(MAX_HEAD)) as u64 + 1).read_until(b'\n', line).map_err(|_| BadRequest::Closed)?;
        used += n;
        if used > MAX_HEAD {
            return Err(BadRequest::HeadTooLarge);
        }
        if n > 0 && !line.ends_with(b"\n") {
            return Err(BadRequest::Malformed);
        }
        while line.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
            line.pop();
        }
        Ok(n)
    };
    // Browsers may send a stray empty line between requests; there is only one request here.
    if next_line(r, &mut line)? == 0 {
        return Err(BadRequest::Closed);
    }
    let first = std::str::from_utf8(&line).map_err(|_| BadRequest::Malformed)?.to_string();
    let mut parts = first.split(' ');
    let (method, target, version) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if parts.next().is_some() || !matches!(version, "HTTP/1.1" | "HTTP/1.0") || !method.bytes().all(|b| b.is_ascii_uppercase()) || method.is_empty() {
        return Err(BadRequest::Malformed);
    }
    if !target.starts_with('/') || target.bytes().any(|b| b <= b' ' || b >= 0x7f) {
        return Err(BadRequest::Malformed);
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut req = Request {
        method: method.to_string(),
        path: path.to_string(),
        query: parse_query(query).ok_or(BadRequest::Malformed)?,
        ..Request::default()
    };
    loop {
        if next_line(r, &mut line)? == 0 {
            return Err(BadRequest::Malformed);
        }
        if line.is_empty() {
            break;
        }
        if req.headers.len() >= MAX_HEADERS {
            return Err(BadRequest::HeadTooLarge);
        }
        let text = std::str::from_utf8(&line).map_err(|_| BadRequest::Malformed)?;
        let (name, value) = text.split_once(':').ok_or(BadRequest::Malformed)?;
        if name.is_empty() || name.bytes().any(|b| !(b.is_ascii_alphanumeric() || b == b'-' || b == b'_')) {
            return Err(BadRequest::Malformed);
        }
        req.headers.push((name.to_ascii_lowercase(), value.trim().to_string()));
    }
    if req.header("transfer-encoding").is_some() {
        return Err(BadRequest::LengthRequired);
    }
    let lengths: Vec<&str> = req.headers.iter().filter(|(n, _)| n == "content-length").map(|(_, v)| v.as_str()).collect();
    match lengths.as_slice() {
        [] => {}
        [one] => req.content_length = Some(one.parse().map_err(|_| BadRequest::Malformed)?),
        _ => return Err(BadRequest::Malformed),
    }
    if req.headers.iter().filter(|(n, _)| n == "host").count() > 1 {
        return Err(BadRequest::Malformed);
    }
    Ok(req)
}

/// `a=1&b=x%20y` into pairs; None when an escape is broken or the text is not UTF-8.
pub fn parse_query(q: &str) -> Option<Vec<(String, String)>> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            Some((percent_decode(k)?, percent_decode(v)?))
        })
        .collect()
}

/// `%xx` escapes and `+` for a space.
pub fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Reads a body of at most `max` bytes (the whole `Content-Length`).
pub fn read_body(r: &mut impl Read, req: &Request, max: u64) -> Result<Vec<u8>, u16> {
    let len = req.content_length.unwrap_or(0);
    if len > max {
        return Err(413);
    }
    let mut body = Vec::with_capacity(len as usize);
    r.take(len).read_to_end(&mut body).map_err(|_| 400u16)?;
    if (body.len() as u64) < len {
        return Err(400);
    }
    Ok(body)
}

pub enum Body {
    Bytes(Vec<u8>),
    Static(&'static [u8]),
}

pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

impl Response {
    pub fn new(status: u16, content_type: &'static str, body: Vec<u8>) -> Response {
        Response { status, content_type, headers: Vec::new(), body: Body::Bytes(body) }
    }

    pub fn json(status: u16, v: &serde_json::Value) -> Response {
        Response::new(status, "application/json; charset=utf-8", serde_json::to_vec(v).unwrap_or_default())
    }

    /// `{"error": "..."}` with a sentence the page shows as it is.
    pub fn error(status: u16, message: &str) -> Response {
        Response::json(status, &serde_json::json!({ "error": message }))
    }

    pub fn with(mut self, name: &str, value: &str) -> Response {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    fn body(&self) -> &[u8] {
        match &self.body {
            Body::Bytes(b) => b,
            Body::Static(b) => b,
        }
    }
}

pub fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        411 => "Length Required",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        421 => "Misdirected Request",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        507 => "Insufficient Storage",
        _ => "Status",
    }
}

/// Writes `resp` with the security headers, `Connection: close` and, unless it says otherwise, `Cache-Control:
/// no-store` (the data is private; only the fonts and the page's own files may be kept).
pub fn write_response(w: &mut impl Write, resp: &Response, head_only: bool) -> io::Result<()> {
    let body = resp.body();
    let mut head = format!("HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n", resp.status, reason(resp.status), resp.content_type, body.len());
    for (n, v) in SECURITY_HEADERS {
        head.push_str(&format!("{n}: {v}\r\n"));
    }
    if !resp.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("cache-control")) {
        head.push_str("Cache-Control: no-store\r\n");
    }
    for (n, v) in &resp.headers {
        // A value can never start another header.
        if !v.contains(['\r', '\n']) && !n.contains(['\r', '\n', ':']) {
            head.push_str(&format!("{n}: {v}\r\n"));
        }
    }
    head.push_str("\r\n");
    w.write_all(head.as_bytes())?;
    if !head_only {
        w.write_all(body)?;
    }
    w.flush()
}
