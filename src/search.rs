//! Find any game by its name: the index over the whole library, matching and ranking, and the recent searches.
//!
//! The index is made once after the library loads, on a thread of its own (`Index::build`); a keystroke then only
//! runs `Index::search`, a pass over plain bytes with no allocation per game. Names are folded first (`fold`): lower
//! case, accents taken off ("Pokémon" is "pokemon"), punctuation turned into spaces, apostrophes dropped
//! ("Yoshi's" is "yoshis"). Region and version tags in brackets don't count, nor does a scene release number in front
//! ("0902 - Pokemon Emerald (USA)" is found as "pokemon emerald"; "0902" still finds it, at the end of the line).
//!
//! Matching goes word by word: every word typed has to match a word of the name, as the whole word, its start, a
//! piece of it (three letters or more), or with a typo (a letter missing, added, changed or two swapped: one from four
//! letters, two from seven), though never in the first letter of a word: leaving that free would match far too much.
//! Only when nothing at all matches does a last pass forgive the first letter too ("oipes" finds "Pipes"), and the
//! page calls those hits the closest matches (`Index::search_closest`). "mar ka", "mrio kart" and "mario kart" all find "Mario Kart - Double Dash!!". A query
//! typed without its spaces ("megaman") also finds "Mega Man". Ranking puts names that start with what was typed
//! first ("mario" lists "Mario Kart 64" before "Super Mario Bros."), then the words in order and next to each other,
//! then the shorter name.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many searches "Recent searches" keeps.
pub const RECENT_MAX: usize = 5;

/// Appends `c` to `out` folded: lower case without accents. Letters of other scripts stay as they are (lower case).
fn fold_char(c: char, out: &mut String) {
    let base = match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ă' | 'Ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' | 'Ç' | 'Ć' | 'Ĉ' | 'Ċ' | 'Č' => 'c',
        'ď' | 'đ' | 'Ď' | 'Đ' | 'ð' | 'Ð' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' | 'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => 'g',
        'ĥ' | 'ħ' | 'Ĥ' | 'Ħ' => 'h',
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' | 'Ì' | 'Í' | 'Î' | 'Ï' | 'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' => 'i',
        'ĵ' | 'Ĵ' => 'j',
        'ķ' | 'Ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' | 'Ĺ' | 'Ļ' | 'Ľ' | 'Ŀ' | 'Ł' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' | 'Ñ' | 'Ń' | 'Ņ' | 'Ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' | 'Ŏ' | 'Ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' | 'Ŕ' | 'Ŗ' | 'Ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' | 'ș' | 'Ś' | 'Ŝ' | 'Ş' | 'Š' | 'Ș' => 's',
        'ţ' | 'ť' | 'ŧ' | 'ț' | 'Ţ' | 'Ť' | 'Ŧ' | 'Ț' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' | 'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => 'u',
        'ŵ' | 'Ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' | 'Ý' | 'Ÿ' | 'Ŷ' => 'y',
        'ź' | 'ż' | 'ž' | 'Ź' | 'Ż' | 'Ž' => 'z',
        'ß' => return out.push_str("ss"),
        'æ' | 'Æ' => return out.push_str("ae"),
        'œ' | 'Œ' => return out.push_str("oe"),
        'þ' | 'Þ' => return out.push_str("th"),
        // "Yoshi's Island" is "yoshis island": an apostrophe joins, it doesn't split.
        '\'' | '’' | '‘' | '`' | '´' => return,
        c if c.is_alphanumeric() => {
            out.extend(c.to_lowercase());
            return;
        }
        _ => ' ',
    };
    out.push(base);
}

/// `s` lower case, without accents, every run of punctuation and spaces as one space, trimmed.
pub fn fold(s: &str) -> String {
    let mut raw = String::with_capacity(s.len());
    for c in s.chars() {
        fold_char(c, &mut raw);
    }
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A game's name without its tags in brackets, and the scene release number in front of it if it has one:
/// ("Pokemon Emerald", Some("0902")) for "0902 - Pokemon Emerald (USA)".
pub fn clean_title(name: &str) -> (String, Option<String>) {
    let mut out = String::with_capacity(name.len());
    let mut depth = 0i32;
    for ch in name.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    let trimmed = out.trim();
    // "0902 - Title", "#0902 - Title", "0902_Title": up to five digits, then a dash or an underscore.
    let body = trimmed.strip_prefix('#').unwrap_or(trimmed);
    let digits = body.chars().take_while(|c| c.is_ascii_digit()).count();
    if (1..=5).contains(&digits) {
        let rest = body[digits..].trim_start();
        if let Some(after) = rest.strip_prefix('-').or_else(|| rest.strip_prefix('_')) {
            let title = after.trim();
            if !title.is_empty() {
                return (title.to_string(), Some(body[..digits].to_string()));
            }
        }
    }
    (trimmed.to_string(), None)
}

/// Bits of the letters and digits in `bytes`: a-z, 0-9, one bit for anything else.
fn char_mask(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0u64, |m, &b| {
        m | match b {
            b'a'..=b'z' => 1 << (b - b'a'),
            b'0'..=b'9' => 1 << (26 + b - b'0'),
            b' ' => 0,
            _ => 1 << 36,
        }
    })
}

/// One game in the index.
struct Entry {
    system: usize,
    game: usize,
    /// The folded name, words separated by one space; the scene number (if any) is the last word.
    text: Box<str>,
    /// Byte ranges of the words in `text`.
    words: Box<[(u16, u16)]>,
    /// Words of the name itself (the scene number comes after them).
    primary: u8,
    /// The first word that isn't "the", "a" or "an": where a name "starts" for the ranking.
    lead: u8,
    /// `text` without spaces, for a query typed without them.
    compact: Box<str>,
    mask: u64,
}

impl Entry {
    fn word(&self, j: usize) -> &[u8] {
        let (a, b) = self.words[j];
        &self.text.as_bytes()[a as usize..b as usize]
    }
}

/// A game that matched: indices into the library's systems and their games.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub system: usize,
    pub game: usize,
    pub score: i32,
}

/// Every game's name, ready to be searched.
#[derive(Default)]
pub struct Index {
    entries: Vec<Entry>,
}

impl Index {
    /// The index of `(system, game, name)`: indices into the library and the name the shelf shows.
    pub fn build<'a>(games: impl IntoIterator<Item = (usize, usize, &'a str)>) -> Index {
        let entries = games
            .into_iter()
            .map(|(system, game, name)| {
                let (title, number) = clean_title(name);
                let mut text = fold(&title);
                let primary_count = text.split(' ').filter(|w| !w.is_empty()).count();
                if let Some(n) = number {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(&n);
                }
                // Names longer than u16 offsets allow are cut (no game is called that).
                text.truncate(u16::MAX as usize - 1);
                let mut words = Vec::new();
                let mut start = None;
                for (i, b) in text.bytes().enumerate().chain(std::iter::once((text.len(), b' '))) {
                    match (b == b' ', start) {
                        (false, None) => start = Some(i),
                        (true, Some(s)) => {
                            words.push((s as u16, i as u16));
                            start = None;
                        }
                        _ => {}
                    }
                }
                let primary = primary_count.min(words.len()).min(u8::MAX as usize) as u8;
                let lead = (0..primary as usize)
                    .find(|&j| {
                        let (a, b) = words[j];
                        !matches!(&text[a as usize..b as usize], "the" | "a" | "an")
                    })
                    .unwrap_or(0) as u8;
                let compact: String = text.chars().filter(|c| *c != ' ').collect();
                let mask = char_mask(text.as_bytes());
                Entry { system, game, text: text.into(), words: words.into(), primary, lead, compact: compact.into(), mask }
            })
            .collect();
        Index { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The games matching `query`, best first. `shown(system)`: false for a system hidden from the shelf.
    pub fn search(&self, query: &str, shown: &dyn Fn(usize) -> bool) -> Vec<Hit> {
        self.search_closest(query, shown).0
    }

    /// Like `search`, and whether the hits are only the closest matches: nothing matched as typed, so a pass that also
    /// forgives a typo in the first letter ("oipes" for "Pipes") was made as the last resort. The normal pass and its
    /// ranking are the same either way; the last resort only runs when the normal one found nothing.
    pub fn search_closest(&self, query: &str, shown: &dyn Fn(usize) -> bool) -> (Vec<Hit>, bool) {
        let q = fold(query);
        if q.is_empty() {
            return (Vec::new(), false);
        }
        let tokens: Vec<Token> = q.split(' ').map(Token::new).collect();
        let compact: String = q.chars().filter(|c| *c != ' ').collect();
        let hits = self.pass(&tokens, &compact, shown, false);
        // Words of three letters or fewer take no typos, so the first letter changes nothing for them.
        if hits.is_empty() && tokens.iter().any(|t| t.typos > 0) {
            let closest = self.pass(&tokens, &compact, shown, true);
            let any = !closest.is_empty();
            return (closest, any);
        }
        (hits, false)
    }

    /// One pass over the index. `first_free`: a typo in the first letter of a word is forgiven too.
    fn pass(&self, tokens: &[Token], compact: &str, shown: &dyn Fn(usize) -> bool, first_free: bool) -> Vec<Hit> {
        let q_mask = char_mask(compact.as_bytes());
        let mut hits: Vec<(Hit, u16)> = Vec::new();
        for e in &self.entries {
            if !shown(e.system) {
                continue;
            }
            // Letters the name doesn't have at all can only be typos: more than a word allows, no match.
            if tokens.iter().any(|t| (t.mask & !e.mask).count_ones() > t.typos as u32) {
                continue;
            }
            let score = score(e, tokens, first_free).or_else(|| {
                // "megaman" for "Mega Man", "supermario" for "Super Mario": the query without its spaces.
                (compact.len() >= 3 && q_mask & !e.mask == 0).then(|| joined(e, compact)).flatten()
            });
            if let Some(score) = score {
                hits.push((Hit { system: e.system, game: e.game, score }, e.text.len() as u16));
            }
        }
        // Best first; the shorter name, then the name itself and the library's order break ties.
        hits.sort_unstable_by(|(a, al), (b, bl)| {
            b.score.cmp(&a.score).then(al.cmp(bl)).then((a.system, a.game).cmp(&(b.system, b.game)))
        });
        hits.into_iter().map(|(h, _)| h).collect()
    }
}

/// A word of the query.
struct Token<'a> {
    bytes: &'a [u8],
    mask: u64,
    /// Typos it may have: none up to three letters, one from four, two from seven.
    typos: u8,
}

impl<'a> Token<'a> {
    fn new(s: &'a str) -> Token<'a> {
        let bytes = s.as_bytes();
        let typos = match s.chars().count() {
            0..=3 => 0,
            4..=6 => 1,
            _ => 2,
        };
        Token { bytes, mask: char_mask(bytes), typos }
    }
}

/// How well a word of the query matches a word of the name: 0 = not at all.
fn word_score(t: &[u8], w: &[u8]) -> i32 {
    if t == w {
        100
    } else if w.starts_with(t) {
        // The more of the word typed, the better: "mari" is closer to "mario" than "m".
        70 + (20 * t.len() / w.len()) as i32
    } else if t.len() >= 3 && w.windows(t.len()).any(|x| x == t) {
        40
    } else {
        0
    }
}

/// The fewest edits (a letter left out, added, changed, or two swapped) that turn `t` into the start of `w`, if
/// that is at most `k`. "mrio" is one from "mario", "zelad" one from "zelda".
fn prefix_typos(t: &[u8], w: &[u8], k: usize) -> Option<usize> {
    let (m, n) = (t.len(), w.len().min(t.len() + k));
    if m == 0 || m > 32 || n == 0 {
        return None;
    }
    // Three rows of the edit distance table (optimal string alignment), each at most 40 entries.
    let mut rows = [[0usize; 41]; 3];
    for (j, cell) in rows[0].iter_mut().enumerate().take(n + 1) {
        *cell = j;
    }
    let (mut prev2, mut prev, mut cur) = (2usize, 0usize, 1usize);
    for i in 1..=m {
        rows[cur][0] = i;
        let mut row_min = i;
        for j in 1..=n {
            let cost = (t[i - 1] != w[j - 1]) as usize;
            let mut d = (rows[prev][j] + 1).min(rows[cur][j - 1] + 1).min(rows[prev][j - 1] + cost);
            if i > 1 && j > 1 && t[i - 1] == w[j - 2] && t[i - 2] == w[j - 1] {
                d = d.min(rows[prev2][j - 2] + 1);
            }
            rows[cur][j] = d;
            row_min = row_min.min(d);
        }
        if row_min > k {
            return None;
        }
        (prev2, prev, cur) = (prev, cur, prev2);
    }
    // `prev` is the last row now: the best end anywhere in the word (it may be typed only in part).
    let best = rows[prev][1..=n].iter().copied().min()?;
    (best <= k).then_some(best)
}

/// The score of a name that holds the query typed without spaces from the start of one of its words over the next
/// ones ("megaman" in "mega man x"); None when it doesn't. Ranked like a word typed in full.
fn joined(e: &Entry, compact: &str) -> Option<i32> {
    let mut at = 0;
    for j in 0..e.words.len() {
        if e.compact[at..].starts_with(compact) {
            let start = if j == e.lead as usize || j == 0 { 60 } else { 0 };
            let left = (e.primary as i32 - 2).max(0);
            return Some(100 + start - (4 * left).min(40));
        }
        at += e.word(j).len();
    }
    None
}

/// The score of one game for the query, or None when a word of the query matches nothing in its name. `first_free`:
/// the first letter of a word may be a typo too (the last resort when nothing else matched).
fn score(e: &Entry, tokens: &[Token], first_free: bool) -> Option<i32> {
    let n = e.words.len();
    if n == 0 {
        return None;
    }
    let mut total = 0;
    let mut used = 0u64;
    let mut prev: Option<usize> = None;
    let (mut in_order, mut adjacent, mut whole) = (true, 0, 0);
    let mut first_at = None;
    for t in tokens {
        // The best word for it; a word not taken yet and one after the last match win a tie.
        let mut best: Option<(i32, usize)> = None;
        let rank = |s: i32, j: usize| (s, (used >> j.min(63)) & 1 == 0, prev.is_none_or(|p| j > p));
        for j in 0..n {
            let s = word_score(t.bytes, e.word(j));
            if s > 0 && best.is_none_or(|(bs, bj)| rank(s, j) > rank(bs, bj)) {
                best = Some((s, j));
            }
        }
        if best.is_none() && t.typos > 0 {
            for j in 0..n {
                let w = e.word(j);
                // A typo in the first letter is rare; leaving it free would match half the library. So it is only
                // forgiven as the last resort (`first_free`), when the search as typed found nothing.
                if !first_free && w.first() != t.bytes.first() && w.get(1) != t.bytes.first() {
                    continue;
                }
                if let Some(d) = prefix_typos(t.bytes, w, t.typos as usize) {
                    let s = 30 - 10 * d as i32;
                    if best.is_none_or(|(bs, bj)| rank(s, j) > rank(bs, bj)) {
                        best = Some((s, j));
                    }
                }
            }
        }
        let (s, j) = best?;
        total += s;
        if s >= 70 {
            whole += 1;
        }
        if let Some(p) = prev {
            if j <= p {
                in_order = false;
            } else if j == p + 1 {
                adjacent += 1;
            }
        }
        first_at.get_or_insert(j);
        prev = Some(j);
        used |= 1 << j.min(63);
    }
    // The name starts with what was typed ("mario" for "Mario Kart", "legend" for "The Legend of Zelda").
    if first_at == Some(e.lead as usize) || first_at == Some(0) {
        total += 60;
    }
    if in_order && tokens.len() > 1 {
        total += 20;
    }
    total += 15 * adjacent;
    // Every word of the name typed (as words or their starts): the name itself.
    let primary = e.primary as usize;
    let named = (e.lead as usize..primary).all(|j| (used >> j.min(63)) & 1 == 1);
    if named && whole == tokens.len() {
        total += 40;
    }
    // Longer names with more left over rank lower.
    let left = primary.saturating_sub(tokens.len()) as i32;
    total -= (4 * left).min(40);
    Some(total)
}

/// The last searches, newest first, kept in `search.json` in the data folder.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    pub list: Vec<String>,
}

impl Recent {
    pub fn file(data_dir: &Path) -> PathBuf {
        data_dir.join("search.json")
    }

    pub fn load(data_dir: &Path) -> Recent {
        crate::fsutil::read_json_or_backup(&Self::file(data_dir)).unwrap_or_default()
    }

    /// Puts `query` first (the same search typed another way moves up instead of showing twice); true when the
    /// list changed and should be saved.
    pub fn remember(&mut self, query: &str) -> bool {
        let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
        let key = fold(&query);
        if key.is_empty() {
            return false;
        }
        if self.list.first().is_some_and(|f| *f == query) {
            return false;
        }
        self.list.retain(|q| fold(q) != key);
        self.list.insert(0, query);
        self.list.truncate(RECENT_MAX);
        true
    }

    pub fn save(&self, data_dir: &Path) {
        if let Err(e) = crate::fsutil::write_json(&Self::file(data_dir), self) {
            eprintln!("search: couldn't save the recent searches: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TestDir;

    /// A small library: (system, name).
    const LIB: &[(usize, &str)] = &[
        (0, "Mario Kart - Double Dash!!"),
        (1, "Super Mario Kart"),
        (2, "Mario Kart 64"),
        (3, "Mario Kart - Super Circuit"),
        (2, "Mario Party"),
        (1, "Super Mario World"),
        (4, "Super Mario Bros."),
        (4, "Dr. Mario"),
        (1, "The Legend of Zelda - A Link to the Past"),
        (5, "Legend of Zelda, The - Link's Awakening DX"),
        (3, "0902 - Pokemon Emerald (USA)"),
        (5, "Pokémon - Crystal Version (USA, Europe)"),
        (6, "Mega Man X"),
        (1, "Mega Man X2"),
        (6, "Sonic the Hedgehog"),
        (6, "Sonic the Hedgehog 2"),
        (3, "Yoshi's Island - Super Mario Advance 3"),
        (2, "1080° Snowboarding"),
        (7, "Tetris (World) (Rev 1)"),
        (7, "Kirby's Dream Land"),
    ];

    fn index() -> Index {
        Index::build(LIB.iter().enumerate().map(|(g, (s, name))| (*s, g, *name)))
    }

    fn names(hits: &[Hit]) -> Vec<&'static str> {
        hits.iter().map(|h| LIB[h.game].1).collect()
    }

    fn find(q: &str) -> Vec<&'static str> {
        names(&index().search(q, &|_| true))
    }

    #[test]
    fn folding_takes_off_case_accents_and_punctuation() {
        assert_eq!(fold("Pokémon - Crystal Version!!"), "pokemon crystal version");
        assert_eq!(fold("  Yoshi's   Island "), "yoshis island");
        assert_eq!(fold("Dr. MARIO"), "dr mario");
        assert_eq!(fold("Straße Æon Œuvre"), "strasse aeon oeuvre");
        assert_eq!(fold("Spider-Man & Venom"), "spider man venom");
        assert_eq!(fold("ÉLAN Ñandú"), "elan nandu");
        assert_eq!(fold("ドラゴン"), "ドラゴン", "other scripts stay");
    }

    #[test]
    fn tags_in_brackets_and_scene_numbers_are_not_part_of_the_name() {
        assert_eq!(clean_title("0902 - Pokemon Emerald (USA)"), ("Pokemon Emerald".into(), Some("0902".into())));
        assert_eq!(clean_title("#1234 - Metroid Fusion [!]"), ("Metroid Fusion".into(), Some("1234".into())));
        assert_eq!(clean_title("0042_Golden Sun"), ("Golden Sun".into(), Some("0042".into())));
        assert_eq!(clean_title("Tetris (World) (Rev 1)"), ("Tetris".into(), None));
        assert_eq!(clean_title("1942"), ("1942".into(), None), "a name that is a number stays");
        assert_eq!(clean_title("1080 Snowboarding"), ("1080 Snowboarding".into(), None), "no dash: part of the name");
        assert_eq!(clean_title("007 - "), ("007 -".into(), None), "nothing after the dash: not a scene number");
    }

    #[test]
    fn mario_kart_finds_every_mario_kart_first() {
        let hits = find("mario kart");
        assert_eq!(hits.len(), 4, "{hits:?}");
        assert!(hits.contains(&"Mario Kart - Double Dash!!"), "{hits:?}");
        assert_eq!(hits.last(), Some(&"Super Mario Kart"), "the name doesn't start with it: last of the four");
        // The capitals, the dash and the exclamation marks don't matter.
        assert_eq!(find("MARIO KART - double dash"), vec!["Mario Kart - Double Dash!!"]);
        assert_eq!(find("Mario Kart: Double Dash")[0], "Mario Kart - Double Dash!!");
    }

    #[test]
    fn a_name_that_starts_with_the_query_comes_first() {
        let hits = find("mario");
        let pos = |n: &str| hits.iter().position(|h| *h == n).unwrap_or_else(|| panic!("{n} missing: {hits:?}"));
        assert!(pos("Mario Kart 64") < pos("Super Mario Bros."), "{hits:?}");
        assert!(pos("Mario Party") < pos("Dr. Mario"), "{hits:?}");
        assert!(pos("Mario Kart 64") < pos("Mario Kart - Double Dash!!"), "shorter first among equals: {hits:?}");
        assert!(hits.contains(&"Yoshi's Island - Super Mario Advance 3"));
        assert!(!hits.contains(&"Mega Man X"));
    }

    #[test]
    fn parts_of_words_find_games() {
        assert_eq!(find("mar ka")[0..4].iter().filter(|n| n.contains("Mario Kart")).count(), 4);
        assert_eq!(find("leg zel").len(), 2);
        assert_eq!(find("kart")[0..4].iter().filter(|n| n.contains("Kart")).count(), 4, "a word in the middle");
        assert!(find("ario").contains(&"Mario Party"), "a piece of a word (three letters or more)");
        assert!(find("ar").is_empty(), "two letters only match the start of a word");
        assert_eq!(find("hedgehog 2")[0], "Sonic the Hedgehog 2");
    }

    #[test]
    fn typos_are_forgiven() {
        assert!(find("mrio kart").contains(&"Mario Kart 64"), "a letter missing");
        assert!(find("maryo kart").contains(&"Mario Kart 64"), "a letter changed");
        assert!(find("zelad").len() == 2, "two letters swapped");
        assert!(find("pokemn crystal").contains(&"Pokémon - Crystal Version (USA, Europe)"));
        assert!(find("snoic").iter().all(|n| n.starts_with("Sonic")) && find("snoic").len() == 2);
        assert!(find("tetirs") == vec!["Tetris (World) (Rev 1)"]);
        assert!(find("xyzzy").is_empty());
        assert!(find("mrao").is_empty() || !find("mrao").contains(&"Mega Man X"), "short words take few typos");
        // Exact matches still beat typos.
        let hits = find("mario");
        assert!(hits.iter().all(|n| n.contains("Mario")), "{hits:?}");
    }

    #[test]
    fn a_typo_in_the_first_letter_is_the_last_resort() {
        let idx = Index::build([(0, 0, "Pipes"), (0, 1, "Pipe Dream"), (0, 2, "Mario Kart 64"), (0, 3, "Super Mario Kart")]);
        let all = |_: usize| true;
        // Nothing matches "oipes" as typed, so the first letter may be wrong too.
        let (hits, closest) = idx.search_closest("oipes", &all);
        assert!(closest);
        assert_eq!(hits.iter().map(|h| h.game).collect::<Vec<_>>(), vec![0], "{hits:?}");
        assert_eq!(idx.search("oipes", &all), hits, "search gives the same list");
        // Several words, one of them with a wrong first letter.
        let (hits, closest) = idx.search_closest("super nario kart", &all);
        assert!(closest && hits.len() == 1 && hits[0].game == 3, "{hits:?}");
        // A normal match is not touched: same hits, not "closest".
        let (hits, closest) = idx.search_closest("pipes", &all);
        assert!(!closest && hits[0].game == 0, "{hits:?}");
        let (hits, closest) = idx.search_closest("pipe", &all);
        assert!(!closest && hits.len() == 2, "{hits:?}");
        // Nothing near at all, and words too short for a typo: still nothing.
        assert_eq!(idx.search_closest("qzxwv", &all), (Vec::new(), false));
        assert_eq!(idx.search_closest("oip", &all), (Vec::new(), false));
        assert_eq!(idx.search_closest("", &all), (Vec::new(), false));
        // A hidden system stays hidden in the last resort too.
        assert!(idx.search("oipes", &|s| s != 0).is_empty());
    }

    #[test]
    fn the_first_letter_stays_strict_while_something_matches() {
        // "oario" only finds "Mario" as the last resort; once a name matches as typed, the strict rule is back.
        let idx = Index::build([(0, 0, "Mario Kart"), (0, 1, "Oario Land")]);
        let hits = idx.search("oario", &|_| true);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].game, 1, "the game matching as typed, not Mario");
        let (hits, closest) = index().search_closest("mrio kart", &|_| true);
        assert!(!closest && !hits.is_empty());
    }

    #[test]
    fn accents_case_scene_numbers_and_regions_are_ignored() {
        assert_eq!(find("POKEMON")[0..2].iter().filter(|n| n.contains("Pok")).count(), 2);
        assert_eq!(find("pokémon emerald"), vec!["0902 - Pokemon Emerald (USA)"]);
        assert_eq!(find("0902"), vec!["0902 - Pokemon Emerald (USA)"], "the number still finds it");
        assert!(find("usa").is_empty(), "region tags are not searched");
        assert!(find("rev").is_empty());
        assert_eq!(find("yoshis")[0], "Yoshi's Island - Super Mario Advance 3");
        assert_eq!(find("yoshi")[0], "Yoshi's Island - Super Mario Advance 3");
        assert_eq!(find("1080 snow"), vec!["1080° Snowboarding"]);
    }

    #[test]
    fn scene_numbers_dont_spoil_the_ranking() {
        // "Pokemon Emerald" starts its name, after the scene number: the start bonus applies.
        let idx = Index::build([(0, 0, "0902 - Pokemon Emerald (USA)"), (0, 1, "Hey Pokemon Emerald Fan")]);
        let hits = idx.search("pokemon", &|_| true);
        assert_eq!(hits[0].game, 0, "{hits:?}");
    }

    #[test]
    fn an_article_in_front_doesnt_count() {
        let hits = find("legend of zelda");
        assert_eq!(hits.len(), 2);
        let idx = Index::build([(0, 0, "The Legend of Zelda"), (0, 1, "Zelda Legend Of")]);
        assert_eq!(idx.search("legend of zelda", &|_| true)[0].game, 0);
    }

    #[test]
    fn a_query_without_spaces_finds_names_with_them() {
        assert_eq!(find("megaman")[0..2].iter().filter(|n| n.starts_with("Mega Man")).count(), 2);
        assert!(find("supermariokart").contains(&"Super Mario Kart"));
        assert!(find("mariokart").len() >= 4);
        // Only from the start of a word: "mar" is not in "quantuMARena".
        let idx = Index::build([(0, 0, "Quantum Arena"), (0, 1, "Mario")]);
        let hits = idx.search("mar", &|_| true);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].game, 1);
        assert!(idx.search("tumare", &|_| true).is_empty());
    }

    #[test]
    fn hidden_systems_are_left_out() {
        let idx = index();
        let hits = idx.search("mario kart", &|s| s != 1);
        assert!(!names(&hits).contains(&"Super Mario Kart"));
        assert_eq!(hits.len(), 3);
        assert!(idx.search("", &|_| true).is_empty());
        assert!(idx.search(" - !! ", &|_| true).is_empty(), "only punctuation: nothing typed");
    }

    #[test]
    fn the_index_points_back_into_the_library() {
        let idx = index();
        assert_eq!(idx.len(), LIB.len());
        for (g, (s, name)) in LIB.iter().enumerate() {
            let (title, _) = clean_title(name);
            let q = fold(&title);
            if q.is_empty() {
                continue;
            }
            let hits = idx.search(&q, &|_| true);
            assert!(hits.iter().any(|h| h.game == g && h.system == *s), "{name} not found by its own name");
        }
        let e = &idx.entries[10];
        assert_eq!(&*e.text, "pokemon emerald 0902");
        assert_eq!((e.primary, e.lead), (2, 0));
        let e = &idx.entries[8];
        assert_eq!(e.lead, 1, "after The");
        assert!(Index::build(std::iter::empty::<(usize, usize, &str)>()).is_empty());
        // A name with nothing to search by is in the index but never found.
        let idx = Index::build([(0, 0, "!!!"), (0, 1, "(USA)")]);
        assert_eq!(idx.len(), 2);
        assert!(idx.search("usa", &|_| true).is_empty());
    }

    #[test]
    fn prefix_typos_counts_edits_against_the_start_of_a_word() {
        assert_eq!(prefix_typos(b"mrio", b"mario", 1), Some(1));
        assert_eq!(prefix_typos(b"zelad", b"zelda", 1), Some(1));
        assert_eq!(prefix_typos(b"marx", b"mario", 1), Some(1), "changed letter, the rest not typed yet");
        assert_eq!(prefix_typos(b"mari", b"mario", 1), Some(0));
        assert_eq!(prefix_typos(b"xyzw", b"mario", 1), None);
        assert_eq!(prefix_typos(b"castlevania", b"castelvania", 2), Some(1));
    }

    /// About 5,000 games, the size the search is made for, with names like real ones.
    fn big_library() -> Vec<(usize, usize, String)> {
        let first = ["Super", "Mega", "Final", "Legend", "Dragon", "Street", "Mario", "Sonic", "Kirby", "Metal", "Star", "Castle", "Donkey", "Pokemon", "Crash", "Tomb", "Silent", "Resident", "Ninja", "Golden"];
        let second = ["Fighter", "Quest", "Fantasy", "Kart", "Man", "Gear", "Wars", "Kong", "Raider", "Hill", "Evil", "Gaiden", "Sun", "Adventure", "Party", "Racing", "Soccer", "Tennis", "Golf", "Island"];
        let third = ["", " II", " III", " 64", " Advance", " - The Lost Levels", " DX", " Deluxe", " (USA)", " (Europe) (En,Fr,De)", " Turbo", " Gold"];
        let mut out = Vec::new();
        let mut g = 0;
        'outer: for s in 0..25 {
            for (i, a) in first.iter().enumerate() {
                for (j, b) in second.iter().enumerate() {
                    let c = third[(i * 7 + j * 3 + s) % third.len()];
                    out.push((s, g, format!("{:04} - {a} {b}{c}", (g * 7) % 9999)));
                    g += 1;
                    if out.len() >= 5000 {
                        break 'outer;
                    }
                }
            }
        }
        out
    }

    #[test]
    fn five_thousand_games_index_and_search_in_time() {
        let lib = big_library();
        assert_eq!(lib.len(), 5000);
        // Thread CPU time, not wall time: the gate runs next to compiles, and a preempted keystroke is not a slow one
        // (the same as the curator's speed tests).
        let cpu = || std::time::Duration::from_secs_f64(crate::run_ahead::thread_cpu_ms() / 1000.0);
        let started = cpu();
        let idx = Index::build(lib.iter().map(|(s, g, n)| (*s, *g, n.as_str())));
        let build = cpu() - started;
        assert_eq!(idx.len(), 5000);
        // Every keystroke of a few searches, typos included: the worst one counts.
        let mut worst = std::time::Duration::ZERO;
        for query in ["super mario kart", "drgaon quets", "zzz", "ninja gaiden ii", "m", "castlevania", "oipes", "xqzvwk", "sper nario kart"] {
            let mut worst_here = std::time::Duration::ZERO;
            for end in 1..=query.len() {
                let started = cpu();
                let hits = idx.search(&query[..end], &|_| true);
                worst_here = worst_here.max(cpu() - started);
                std::hint::black_box(hits);
            }
            println!("  {query:?}: slowest keystroke {worst_here:?}");
            worst = worst.max(worst_here);
        }
        println!("index of 5000 games built in {build:?}; slowest keystroke {worst:?}");
        assert!(!idx.search("drgaon quets", &|_| true).is_empty(), "typos in both words");
        // A release build on this computer is far below this; the Mini V2 is a few times slower than a desktop, and its
        // budget is a frame (16 ms). Debug builds are ten times slower, so they only get a sanity bound.
        let budget = if cfg!(debug_assertions) { 200 } else { 8 };
        assert!(worst.as_millis() < budget, "slowest keystroke {worst:?}");
        assert!(build.as_millis() < budget * 20, "index built in {build:?}");
    }

    #[test]
    fn recent_searches_keep_five_newest_first_without_doubles() {
        let mut r = Recent::default();
        assert!(!r.remember("   "));
        for q in ["mario", "zelda", "sonic", "kirby", "metroid", "tetris"] {
            assert!(r.remember(q));
        }
        assert_eq!(r.list, vec!["tetris", "metroid", "kirby", "sonic", "zelda"]);
        assert!(!r.remember("tetris"), "already first");
        assert!(r.remember("Sonic "), "the same search typed another way moves up");
        assert_eq!(r.list, vec!["Sonic", "tetris", "metroid", "kirby", "zelda"]);
        let dir = TestDir::new("search");
        r.save(&dir);
        assert_eq!(Recent::load(&dir), r);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(Recent::load(&dir), Recent::default(), "no file: no recent searches");
    }
}
