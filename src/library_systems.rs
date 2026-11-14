//! Read a card's system entries independently. A broken entry cannot hide its neighbours.
//! Repairs are in memory only; the player's configuration is never rewritten.
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Issue {
    pub name: String,
    pub line: usize,
    pub reason: String,
}

/// The entries that had to be left out or repaired; an unreadable file is the I/O error (the library says that one itself).
pub fn issues(path: &Path) -> std::io::Result<Vec<Issue>> {
    std::fs::read_to_string(path).map(|xml| recover(&xml).1)
}

pub(super) fn read(path: &Path) -> Result<(String, Vec<Issue>), String> {
    let xml = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(recover(&xml))
}

fn required(sys: roxmltree::Node, tag: &str) -> Option<String> {
    let node = sys.children().find(|n| n.has_tag_name(tag))?;
    (!node.children().any(|n| n.is_element())).then_some(())?;
    node.text().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

fn valid(sys: roxmltree::Node) -> bool {
    required(sys, "name").is_some() && required(sys, "path").is_some()
}

fn recover(xml: &str) -> (String, Vec<Issue>) {
    // Keep valid documents verbatim, including namespaces, comments and CDATA.
    if let Ok(doc) = roxmltree::Document::parse(xml) {
        if doc.root_element().has_tag_name("systemList") && doc.descendants().filter(|n| n.has_tag_name("system")).all(valid) {
            return (xml.to_string(), Vec::new());
        }
    }
    let repaired = repair_text(xml);
    if let Ok(doc) = roxmltree::Document::parse(&repaired) {
        if doc.root_element().has_tag_name("systemList") && doc.descendants().filter(|n| n.has_tag_name("system")).all(valid) {
            return (repaired, Vec::new());
        }
    }
    let mut header = String::from("<systemList>");
    let mut issues = Vec::new();
    let mut start = None;
    let mut spans = Vec::new();
    let mut at = 0;
    while let Some((from, end, tag)) = next_tag(xml, at) {
        at = end;
        let tag = tag.trim();
        let name = tag.split_whitespace().next().unwrap_or("");
        match name {
            "systemList" if !tag.ends_with('/') => {
                let candidate = repair_text(&xml[from..end]);
                let empty = format!("{}</systemList>", candidate);
                if roxmltree::Document::parse(&empty).is_ok() { header = candidate; }
            }
            "system" if !tag.ends_with('/') => {
                if let Some(prev) = start.replace(from) {
                    spans.push((prev, from)); // no closing tag: resume at the next system
                }
            }
            "/system" => {
                if let Some(prev) = start.take() { spans.push((prev, end)); }
            }
            "system/" | "system" => spans.push((from, end)),
            _ => {}
        }
    }
    if let Some(prev) = start { spans.push((prev, xml.len())); }
    let mut out = header.clone();
    out.push('\n');
    for (from, end) in spans {
        let fragment = repair_text(&xml[from..end]);
        // Preserve namespace declarations on the list when checking a surviving system.
        let wrapped = format!("{header}{fragment}</systemList>");
        let parsed = roxmltree::Document::parse(&wrapped);
        let reason = match &parsed {
            Ok(doc) if doc.root_element().first_element_child().is_some_and(valid) => {
                out.push_str(&fragment);
                out.push('\n');
                continue;
            }
            Ok(_) => "missing system name or ROM folder".to_string(),
            Err(e) => e.to_string(),
        };
        let name = parsed.as_ref().ok().and_then(|d| required(d.root_element().first_element_child()?, "name"))
            .or_else(|| fragment.split_once("<name>")?.1.split_once("</name>").map(|(s, _)| s.trim().to_string()))
            .filter(|s| !s.is_empty()).unwrap_or_else(|| "es_systems.cfg".to_string());
        issues.push(Issue { name, line: xml[..from].bytes().filter(|b| *b == b'\n').count() + 1, reason });
    }
    if out.len() == header.len() + 1 && issues.is_empty() {
        issues.push(Issue { name: "es_systems.cfg".into(), line: 1, reason: "no readable system list".into() });
    }
    out.push_str("</systemList>");
    (out, issues)
}

/// Small lexical boundary finder, not an XML parser: leave validation to roxmltree per entry.
/// Comments, CDATA, processing instructions and quoted attributes cannot introduce a system boundary.
fn next_tag(xml: &str, mut at: usize) -> Option<(usize, usize, &str)> {
    while let Some(rel) = xml[at..].find('<') {
        let from = at + rel;
        let rest = &xml[from..];
        let special = [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>")];
        if let Some((open, close)) = special.iter().find(|(open, _)| rest.starts_with(open)) {
            at = rest[open.len()..].find(close).map_or(from + open.len(), |n| from + open.len() + n + close.len());
            continue;
        }
        if rest[1..].chars().next().is_none_or(|c| c.is_whitespace() || c.is_ascii_digit()) {
            at = from + 1;
            continue;
        }
        let mut quote = None;
        for (i, c) in xml[from + 1..].char_indices() {
            let pos = from + 1 + i;
            match c {
                '\'' | '"' if quote == Some(c) => quote = None,
                '\'' | '"' if quote.is_none() => quote = Some(c),
                '>' if quote.is_none() => return Some((from, pos + 1, &xml[from + 1..pos])),
                // '<' is illegal even inside an attribute; recover at the next tag after a broken quote.
                '<' => return Some((from, pos, &xml[from + 1..pos])),
                _ => {}
            }
        }
        return Some((from, xml.len(), &xml[from + 1..]));
    }
    None
}

/// Bare ampersands in text and attributes are literal; existing XML escapes stay escaped once.
/// A '<' followed by space/a digit is text too. Ambiguous broken markup is left for per-entry rejection.
fn repair_text(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut at = 0;
    while at < xml.len() {
        let rest = &xml[at..];
        if let Some((open, close)) = [("<!--", "-->"), ("<![CDATA[", "]]>"), ("<?", "?>")].iter().find(|(open, _)| rest.starts_with(open)) {
            let end = rest[open.len()..].find(close).map_or(rest.len(), |n| open.len() + n + close.len());
            out.push_str(&rest[..end]);
            at += end;
            continue;
        }
        let c = rest.chars().next().unwrap();
        if c == '&' {
            let entity = rest[1..].bytes().take(16).position(|b| b == b';').map(|n| &rest[1..n + 1]);
            let escaped = entity.is_some_and(|s| matches!(s, "amp" | "lt" | "gt" | "apos" | "quot")
                || s.strip_prefix("#x").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_hexdigit()))
                || s.strip_prefix('#').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())));
            out.push_str(if escaped { "&" } else { "&amp;" });
        } else if c == '<' && rest[1..].chars().next().is_some_and(|c| c.is_whitespace() || c.is_ascii_digit()) {
            out.push_str("&lt;");
        } else {
            out.push(c);
        }
        at += c.len_utf8();
    }
    out
}
