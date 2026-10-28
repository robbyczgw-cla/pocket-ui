//! Echoes: "more like this". For a seed game, every other game of the library gets a score:
//!
//! - **Base:** cosine similarity of the IDF-weighted term vectors (layer B tags, Wikidata genres and families).
//! - **Bonuses:** same series + 0.25, same developer + 0.15, released within 5 years of each other + 0.05.
//! - **Taste:** × (1 + 0.3 · affinity), the cosine of the player's taste profile and the candidate.
//! - **Status:** Not started + 0.1, Completed − 0.15, Dropped left out.
//! - **Re-ranking:** MMR (λ = 0.7) over the best 60, so the list is not one series; at most 3 per series.
//!
//! The reason line is made of the two largest contributions, from templates ("Same series", "Also by Criterion
//! Games", "Arcade racing · crash takedowns", "Short sessions, fast pace", "Like Burnout 3, which you finished").
//! Other versions of the seed are not Echoes (they are its Versions), nor are its remakes ("Related: remake" on the
//! Versions page); each work appears once.

use crate::library_meta::Status;

use super::features::capitalise;
use super::index::Index;

pub const SERIES_BONUS: f32 = 0.25;
pub const DEVELOPER_BONUS: f32 = 0.15;
pub const ERA_BONUS: f32 = 0.05;
pub const ERA_YEARS: u16 = 5;
pub const TASTE_FACTOR: f32 = 0.3;
pub const NOT_STARTED_BONUS: f32 = 0.1;
pub const COMPLETED_PENALTY: f32 = 0.15;
/// MMR: how much the score counts against the likeness to what is already listed.
pub const MMR_LAMBDA: f32 = 0.7;
/// Candidates the re-ranking looks at.
const POOL: usize = 60;
/// Games of one series in one list.
pub const PER_SERIES: usize = 3;
/// The full list on the Echoes page.
pub const FULL_LIST: usize = 12;
/// Base and bonuses together below this are no likeness worth showing.
pub const MIN_SCORE: f32 = 0.1;
/// "Like X" only for a game of the profile at least this alike.
const LIKE_AT_LEAST: f32 = 0.5;
/// Up next suggests nothing less alike than this (an Echo's score, taste and status included).
pub const UP_NEXT_MIN: f32 = 0.3;
/// Up next skips a game played this recently (seconds).
pub const UP_NEXT_RECENT: u64 = 24 * 3600;

/// One game in the Echoes of a seed.
#[derive(Clone, Debug, PartialEq)]
pub struct Echo {
    pub game: usize,
    pub score: f32,
    pub reason: String,
    /// What taste added to (or took from) the score; `score - lift` is the score without it ("Why these?").
    pub lift: f32,
}

/// What a part of the score was, for the reason line.
#[derive(Clone, Debug, PartialEq)]
enum Part {
    /// A shared term (its id).
    Term(u32),
    Series,
    Developer(String),
    Era,
    /// Taste; the reason names the game of the profile closest to the candidate (found for the listed games only).
    Taste,
}

impl Index {
    /// The Echoes of `seed`: at most `max` games, best first, each work once, with their reason lines.
    pub fn echoes(&self, seed: usize, max: usize) -> Vec<Echo> {
        let cat = &self.catalog;
        if seed >= cat.len() {
            return Vec::new();
        }
        let seed_work = cat.work_of[seed];
        // Its remakes (or the game it remakes) are shown with its versions, as "Related: remake".
        let remade: Vec<u32> = self.remakes(seed).into_iter().map(|g| cat.work_of[g]).collect();
        // The seed's vector spread out, so each candidate costs one pass over its own few terms.
        let mut dense = vec![0.0f32; cat.term_labels.len()];
        for (t, w) in &cat.vectors[seed] {
            dense[*t as usize] = *w;
        }
        let mut scored: Vec<(usize, f32)> = Vec::new();
        for c in 0..cat.len() {
            if cat.work_of[c] == seed_work || remade.contains(&cat.work_of[c]) || self.user.games[c].status == Status::Dropped {
                continue;
            }
            if let Some(score) = self.score(seed, c, &dense, None) {
                scored.push((c, score));
            }
        }
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        // Each work once: its preferred version (unless dropped), else the candidate itself.
        let mut seen = vec![false; cat.works.len()];
        let mut pool: Vec<(usize, f32)> = Vec::new();
        for (c, score) in scored {
            let w = cat.work_of[c] as usize;
            if std::mem::replace(&mut seen[w], true) {
                continue;
            }
            let pref = self.preferred_game[w] as usize;
            let shown = if self.user.games[pref].status != Status::Dropped { pref } else { c };
            pool.push((shown, score));
            if pool.len() >= POOL {
                break;
            }
        }
        // MMR with the per-series cap.
        let mut picked: Vec<(usize, f32)> = Vec::new();
        let mut per_series: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        while picked.len() < max && !pool.is_empty() {
            let best = pool
                .iter()
                .enumerate()
                .filter(|(_, (c, _))| cat.series_of[*c].is_none_or(|sid| per_series.get(&sid).copied().unwrap_or(0) < PER_SERIES))
                .map(|(i, (c, score))| {
                    let likeness = picked.iter().map(|(p, _)| self.likeness(*c, *p)).fold(0.0, f32::max);
                    (i, MMR_LAMBDA * score - (1.0 - MMR_LAMBDA) * likeness)
                })
                .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))
                .map(|(i, _)| i);
            let Some(i) = best else { break };
            let item = pool.remove(i);
            if let Some(sid) = cat.series_of[item.0] {
                *per_series.entry(sid).or_default() += 1;
            }
            picked.push(item);
        }
        // Reasons for the listed games only; one game of the profile is named at most twice in a list, and one line
        // is not said three times while another part has something to say.
        let mut named: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        let mut used: Vec<String> = Vec::new();
        picked
            .into_iter()
            .map(|(game, score)| {
                let mut parts = Vec::new();
                self.score(seed, game, &dense, Some(&mut parts));
                let lift = self.lift(seed, game, &dense);
                let like = self.closest_taste_game(game, seed).filter(|g| named.get(g).copied().unwrap_or(0) < 2);
                let reason = self.reason(parts, like, &used);
                if let Some(g) = like.filter(|_| reason.starts_with("Like ") || reason.contains("· like ")) {
                    *named.entry(g).or_default() += 1;
                }
                used.push(reason.clone());
                Echo { game, score, reason, lift }
            })
            .collect()
    }

    /// The score of candidate `c` for `seed` (None below [`MIN_SCORE`]); with `parts`, what it is made of.
    fn score(&self, seed: usize, c: usize, seed_dense: &[f32], mut parts: Option<&mut Vec<(Part, f32)>>) -> Option<f32> {
        let cat = &self.catalog;
        let (s, g) = (&cat.games[seed], &cat.games[c]);
        let mut base = 0.0;
        let denom = cat.norms[seed] * cat.norms[c];
        if denom > 0.0 {
            for (t, w) in &cat.vectors[c] {
                let x = seed_dense[*t as usize];
                if x != 0.0 {
                    let share = x * w / denom;
                    base += share;
                    if let Some(p) = parts.as_deref_mut() {
                        p.push((Part::Term(*t), share));
                    }
                }
            }
        }
        let mut bonus = 0.0;
        if s.series.iter().any(|x| g.series.contains(x)) {
            bonus += SERIES_BONUS;
            if let Some(p) = parts.as_deref_mut() {
                p.push((Part::Series, SERIES_BONUS));
            }
        }
        if let Some(dev) = s.developers.iter().find(|d| g.developers.contains(d)) {
            bonus += DEVELOPER_BONUS;
            if let Some(p) = parts.as_deref_mut() {
                p.push((Part::Developer(dev.clone()), DEVELOPER_BONUS));
            }
        }
        if let (Some(a), Some(b)) = (s.year, g.year) {
            if a.abs_diff(b) <= ERA_YEARS {
                bonus += ERA_BONUS;
                if let Some(p) = parts.as_deref_mut() {
                    p.push((Part::Era, ERA_BONUS));
                }
            }
        }
        let before = base + bonus;
        if before < MIN_SCORE {
            return None;
        }
        let affinity = self.affinity(c);
        let mut score = before * (1.0 + TASTE_FACTOR * affinity);
        if affinity > 0.0 {
            if let Some(p) = parts {
                p.push((Part::Taste, before * TASTE_FACTOR * affinity));
            }
        }
        match self.user.games[c].status {
            Status::NotStarted => score += NOT_STARTED_BONUS,
            Status::Completed => score -= COMPLETED_PENALTY,
            _ => {}
        }
        Some(score)
    }

    /// What taste did to candidate `c`'s score: `before × TASTE_FACTOR × affinity` (negative for a game like the
    /// ones you dropped).
    fn lift(&self, seed: usize, c: usize, seed_dense: &[f32]) -> f32 {
        let mut parts = Vec::new();
        if self.score(seed, c, seed_dense, Some(&mut parts)).is_none() {
            return 0.0;
        }
        let before: f32 = parts.iter().filter(|(p, _)| *p != Part::Taste).map(|(_, x)| *x).sum();
        before * TASTE_FACTOR * self.affinity(c)
    }

    /// "Why these?" on the Echoes page: when your taste changed the order of `echoes` (a game listed above another
    /// that would be below it without taste), the games of your profile behind it: "You finished Burnout 3 and
    /// starred Chrono Trigger". None when taste changed nothing or no game of the profile is alike enough to name.
    pub fn why(&self, seed: usize, echoes: &[Echo]) -> Option<String> {
        let mut named: Vec<usize> = Vec::new();
        for (i, a) in echoes.iter().enumerate() {
            let lifted = a.lift > 0.0 && echoes[i + 1..].iter().any(|b| a.score >= b.score && a.score - a.lift < b.score - b.lift);
            if !lifted {
                continue;
            }
            if let Some(g) = self.closest_taste_game(a.game, seed) {
                if !named.contains(&g) {
                    named.push(g);
                }
            }
        }
        let words: Vec<String> = named.iter().take(2).map(|&g| self.taste_words(g)).collect();
        match words.as_slice() {
            [] => None,
            [one] => Some(capitalise(&format!("you {one}"))),
            [one, two, ..] => Some(capitalise(&format!("you {one} and {two}"))),
        }
    }

    /// "finished Burnout 3", "starred Chrono Trigger", "played Gran Turismo 4 for 30 h": what put a game in the
    /// taste profile.
    fn taste_words(&self, g: usize) -> String {
        let u = &self.user.games[g];
        let name = &self.catalog.games[g].name;
        match u.status {
            Status::Completed => format!("finished {name}"),
            Status::Playing if u.secs >= 3600 => format!("are playing {name} ({} h so far)", u.secs / 3600),
            Status::Playing => format!("are playing {name}"),
            _ if u.starred => format!("starred {name}"),
            _ if u.secs >= 3600 => format!("played {name} for {} h", u.secs / 3600),
            _ => format!("played {name}"),
        }
    }

    /// "Up next" after `played` ended (Roon Radio's continuation): the first of its Echoes you might play next. Echoes
    /// already leave out its versions, remakes and dropped games; this also skips games you completed and games
    /// played in the last [`UP_NEXT_RECENT`] seconds (no back and forth between two games), and suggests nothing
    /// below [`UP_NEXT_MIN`]. Runs on the index thread (one Echoes list, well under a millisecond).
    pub fn up_next(&self, played: usize) -> Option<Echo> {
        self.echoes(played, FULL_LIST).into_iter().find(|e| {
            let u = &self.user.games[e.game];
            let recent = u.last_played.is_some_and(|t| self.user.now.saturating_sub(t) < UP_NEXT_RECENT);
            e.score >= UP_NEXT_MIN && !matches!(u.status, Status::Completed | Status::Dropped) && !recent && e.game != played
        })
    }

    /// How alike two candidates are for the re-ranking: their cosine, raised to at least 0.5 within a series.
    fn likeness(&self, a: usize, b: usize) -> f32 {
        let cat = &self.catalog;
        let mut sim = cat.cosine(a, b);
        if cat.series_of[a].is_some() && cat.series_of[a] == cat.series_of[b] {
            sim = sim.max(0.5);
        }
        sim
    }

    /// The game that shaped the taste profile (positively) and is most alike to `c`, at least [`LIKE_AT_LEAST`]; not
    /// a version of the seed or of `c` itself. For "Like X, which you finished".
    fn closest_taste_game(&self, c: usize, seed: usize) -> Option<usize> {
        let cw = self.catalog.work_of[c];
        let sw = self.catalog.work_of[seed];
        self.taste_games
            .iter()
            .filter(|(g, w)| *w > 0.0 && self.catalog.work_of[*g as usize] != cw && self.catalog.work_of[*g as usize] != sw)
            .map(|(g, w)| (*g as usize, self.catalog.cosine(*g as usize, c), *w))
            .filter(|(_, sim, _)| *sim >= LIKE_AT_LEAST)
            .max_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)))
            .map(|(g, _, _)| g)
    }

    /// The reason line: the two largest parts of the score as words (a part that says the same words as one before
    /// it, such as the family "racing" after the genre "racing", is skipped).
    ///
    /// A family or a plain genre ("racing") is only used when nothing more particular is left: that both are racing
    /// games is what the player knows already. Taste counts only when a game of the profile is alike enough to name.
    /// `used`: lines already in the list; a line said twice is not said a third time when the next-largest part
    /// gives another one.
    fn reason(&self, mut parts: Vec<(Part, f32)>, like: Option<usize>, used: &[String]) -> String {
        if like.is_none() {
            parts.retain(|(p, _)| *p != Part::Taste);
        }
        // The series first; then the strongest of a particular tag, the developer and taste; the era; plain terms.
        let tier = |p: &Part| match p {
            Part::Series => 0,
            Part::Term(t) if self.catalog.term_plain[*t as usize] => 3,
            Part::Era => 2,
            _ => 1,
        };
        parts.sort_by(|a, b| tier(&a.0).cmp(&tier(&b.0)).then(b.1.total_cmp(&a.1)));
        let mut words: Vec<(&Part, String)> = Vec::new();
        for (p, _) in &parts {
            let w = self.part_words(p, like);
            if !words.iter().any(|(_, x)| x.eq_ignore_ascii_case(&w)) {
                words.push((p, w));
            }
        }
        let feel = |p: &Part| matches!(p, Part::Term(t) if self.catalog.term_axes[*t as usize].is_some_and(|a| a.is_feel()));
        let line = |a: usize, b: Option<usize>| {
            let l = match b {
                Some(b) if feel(words[a].0) && feel(words[b].0) => format!("{}, {}", words[a].1, words[b].1),
                Some(b) => format!("{} · {}", words[a].1, words[b].1),
                None => words[a].1.clone(),
            };
            capitalise(&l)
        };
        match words.len() {
            0 => String::new(),
            1 => line(0, None),
            n => {
                let pairs = [(0, 1), (0, 2), (1, 2), (0, 3), (1, 3)];
                let fresh = pairs.iter().filter(|(_, b)| *b < n).map(|&(a, b)| line(a, Some(b))).find(|l| used.iter().filter(|u| *u == l).count() < 2);
                fresh.unwrap_or_else(|| line(0, Some(1)))
            }
        }
    }

    fn part_words(&self, p: &Part, like: Option<usize>) -> String {
        match p {
            Part::Term(t) => self.catalog.term_labels[*t as usize].clone(),
            Part::Series => "same series".into(),
            Part::Developer(d) => format!("also by {}", short_developer(d)),
            Part::Era => "same era".into(),
            Part::Taste => {
                let Some(g) = like else { return String::new() };
                let u = &self.user.games[g];
                let name = &self.catalog.games[g].name;
                let what = match u.status {
                    Status::Completed => "which you finished".to_string(),
                    Status::Playing => "which you're playing".to_string(),
                    _ if u.starred => "which you starred".to_string(),
                    _ if u.secs >= 3600 => format!("which you played for {} h", u.secs / 3600),
                    _ => "which you played".to_string(),
                };
                format!("like {name}, {what}")
            }
        }
    }
}

/// The name a reason uses for a developer: Nintendo's long studio names as people say them.
fn short_developer(d: &str) -> &str {
    match d {
        "Nintendo Entertainment Analysis & Development" => "Nintendo EAD",
        "Nintendo Research & Development 1" => "Nintendo R&D1",
        "Nintendo Research & Development 2" => "Nintendo R&D2",
        "Nintendo Software Planning & Development" => "Nintendo SPD",
        "Nintendo Entertainment Planning & Development" => "Nintendo EPD",
        "Sony Interactive Entertainment" => "Sony",
        "Sega AM2" | "Sega AM3" => d,
        other => other,
    }
}
