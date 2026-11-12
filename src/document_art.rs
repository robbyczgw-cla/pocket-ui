//! Read-only SAF artwork and same-game cheat sidecars. Resolves only documents in the granted listing.
use crate::{document_library::Document, layouts::{self, Kind}};
use std::{collections::HashMap, path::Path};

fn root(d: &Document) -> &str { d.uri.split("/document/").next().unwrap_or(&d.authority) }
fn parent(path: &str) -> &str { path.rsplit_once('/').map_or("", |p| p.0) }
fn stem(path: &str) -> String { Path::new(path).file_stem().unwrap_or_default().to_string_lossy().to_lowercase() }
fn key(d: &Document, path: &str) -> (String, String) { (root(d).into(), path.to_lowercase()) }

#[derive(Default)]
pub struct Index {
    beside: HashMap<(String, String), Vec<(Kind, String)>>,
    es: HashMap<(String, String, String), Vec<(Kind, String)>>,
    lists: HashMap<(String, String), Vec<(Kind, String)>>,
    cheats: HashMap<(String, String), String>,
}
impl Index {
    pub fn new(docs: &[Document]) -> Self {
        let mut index = Self::default();
        let by_path: HashMap<_, _> = docs.iter().map(|d| (key(d, &d.relative), d)).collect();
        for d in docs {
            let folder = parent(&d.relative);
            let file = Path::new(&d.relative).file_name().unwrap_or_default().to_string_lossy();
            if file.to_ascii_lowercase().ends_with(".cht") && d.sheet.len() <= 262_144 {
                let path = Path::new(&d.relative).with_extension("").to_string_lossy().to_string();
                index.cheats.insert(key(d, &path), d.sheet.clone());
            }
            if let Some((name, kind)) = layouts::listed_picture(Path::new(folder).file_name().unwrap_or_default().to_str().unwrap_or(""), &file) {
                let mut rom_folder = parent(folder);
                if Path::new(rom_folder).file_name().is_some_and(|s| s.eq_ignore_ascii_case("media")) { rom_folder = parent(rom_folder); }
                index.beside.entry(key(d, &format!("{rom_folder}/{name}"))).or_default().push((kind, d.uri.clone()));
                if folder.to_lowercase().contains("/downloaded_media/") {
                    if let Some((system, ..)) = d.folder_system() {
                        index.es.entry((root(d).into(), system.into(), name)).or_default().push((kind, d.uri.clone()));
                    }
                }
            }
            if !file.eq_ignore_ascii_case("gamelist.xml") || d.sheet.len() > 262_144 { continue; }
            // A synthetic root lets the shared tolerant XML parser resolve .. without leaving this grant.
            let base = Path::new("/__saf__").join(folder);
            for entry in crate::gamelist::parse(&d.sheet, &base) {
                let Ok(rom) = entry.path.strip_prefix("/__saf__") else { continue };
                let game = key(d, &rom.to_string_lossy());
                for (paths, fallback) in [(&entry.images, Kind::Box), (&entry.shots, Kind::Likely)] {
                    for p in paths {
                        let Ok(path) = p.strip_prefix("/__saf__") else { continue };
                        if let Some(pic) = by_path.get(&key(d, &path.to_string_lossy())) {
                            let kind = layouts::listed_picture(Path::new(parent(&pic.relative)).file_name().unwrap_or_default().to_str().unwrap_or(""), Path::new(&pic.relative).file_name().unwrap_or_default().to_str().unwrap_or("")).map_or(fallback, |(_, k)| k);
                            index.lists.entry(game.clone()).or_default().push((kind, pic.uri.clone()));
                        }
                    }
                }
            }
        }
        index
    }
    /// Boxes before mixes before screenshots; all candidates can be tried when a provider revokes one.
    pub fn pictures(&self, game: &Document, snaps: bool) -> Vec<String> {
        let path = format!("{}/{}", parent(&game.relative), stem(&game.relative));
        let mut found = self.lists.get(&key(game, &game.relative)).cloned().unwrap_or_default();
        found.extend(self.beside.get(&key(game, &path)).into_iter().flatten().cloned());
        if let Some((system, ..)) = game.folder_system() {
            found.extend(self.es.get(&(root(game).into(), system.into(), stem(&game.relative))).into_iter().flatten().cloned());
        }
        found.retain(|(kind, _)| !snaps || *kind == Kind::Shot);
        found.sort_by_key(|(kind, _)| *kind);
        let mut seen = std::collections::HashSet::new();
        found.into_iter().filter_map(|(_, uri)| seen.insert(uri.clone()).then_some(uri)).collect()
    }
    pub fn cheats(&self, game: &Document) -> String {
        let path = Path::new(&game.relative).with_extension("").to_string_lossy().to_string();
        self.cheats.get(&key(game, &path)).cloned().unwrap_or_default()
    }
}
