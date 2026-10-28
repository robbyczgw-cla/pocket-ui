//! Curator on real data, for numbers and a look at Echoes and the Lens (docs/CURATOR.md, "Developer notes"):
//!
//!     cargo run --release --example curator_check [-- <es_systems.cfg>]
//!
//! 1. A library made of gamedb's own entries (their labels as No-Intro-style file names, `CURATOR_PER_SYSTEM`
//!    per system, 160 by default: about 3,800 games): the catalog with the real gamedb matching and the shipped
//!    curatordb tags, the index, a Lens keypress, Echoes for a seed.
//! 2. With an es_systems.cfg (for example the one `scripts/curator-fake-library.py` writes): that library's Echoes
//!    for the report's seeds and a few Lens examples with their counts. The player of `curator-fake-library.py`'s
//!    data is not loaded here: no statuses, no taste.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use pocket_ui::curator::{self, echoes, CuratorDbFeel, Facet, Index, Lens, LibraryGame, SystemName, UserState};
use pocket_ui::gamedb::GameDb;

fn main() {
    let per_system: usize = std::env::var("CURATOR_PER_SYSTEM").ok().and_then(|n| n.parse().ok()).unwrap_or(160);
    big(per_system);
    if let Some(cfg) = std::env::args().nth(1) {
        library(&cfg);
    }
}

fn big(per_system: usize) {
    let mut db = GameDb::embedded();
    let ids: Vec<String> = db.systems().iter().map(|s| s.to_string()).collect();
    let mut games = Vec::new();
    for (s, id) in ids.iter().enumerate() {
        let Some(list) = db.system(id) else { continue };
        for (g, info) in list.games.iter().filter(|i| i.sitelinks > 0).take(per_system).enumerate() {
            let name = info.label.replace([':', '/'], " -");
            games.push(LibraryGame { at: (s, g), system: id.clone(), path: PathBuf::from(format!("/storage/roms/{id}/{name} (USA).bin")), name, choice: None, identified: None });
        }
    }
    let systems: Vec<SystemName> = ids.iter().map(|id| SystemName { id: id.clone(), name: id.clone(), short: id.to_uppercase() }).collect();
    // Fresh readers, as the index thread has them: every system of gamedb and curatordb is unpacked once.
    let mut db = GameDb::embedded();
    let t = Instant::now();
    let catalog = Arc::new(curator::build_catalog(&games, systems, &mut db, &CuratorDbFeel));
    let catalog_took = t.elapsed();
    let matched = catalog.games.iter().filter(|g| g.qid.is_some()).count();
    let tagged = catalog.games.iter().filter(|g| g.terms.iter().any(|t| t.axis.is_some_and(|a| a != curator::features::Axis::Family))).count();
    let t = Instant::now();
    let ix = Index::build(catalog, UserState::default());
    let index_took = t.elapsed();
    println!("{} games on {} systems ({matched} matched in gamedb, {tagged} with feel tags)", ix.len(), ids.len());
    println!("  catalog (gamedb matching + curatordb tags): {catalog_took:?}");
    println!("  index: {index_took:?}");
    println!("  facets with values: {}", ix.shown_facets().len());
    let mut lens = Lens::default();
    lens.toggle(Facet::Family, "racing");
    lens.toggle(Facet::Session, "session-pickup");
    let mut worst = std::time::Duration::ZERO;
    for f in ix.shown_facets() {
        let t = Instant::now();
        let n = ix.count(&ix.matches(&lens), true);
        let c = ix.counts(&lens, f, true);
        let lines: Vec<String> = ix.shown_facets().into_iter().map(|f| ix.picks_line(&lens, f)).collect();
        std::hint::black_box((n, c, lines));
        worst = worst.max(t.elapsed());
    }
    println!("  Lens keypress (count, the counts of a facet, the picks lines; worst facet): {worst:?}");
    let mut times = Vec::new();
    for seed in (0..ix.len()).step_by(ix.len() / 64 + 1) {
        let t = Instant::now();
        std::hint::black_box(ix.echoes(seed, echoes::FULL_LIST));
        times.push((t.elapsed(), seed));
    }
    times.sort();
    let (worst, seed) = times[times.len() - 1];
    println!("  Echoes for one seed, {} seeds: median {:?}, worst {worst:?} ({})", times.len(), times[times.len() / 2].0, ix.game(seed).name);
}

fn library(cfg: &str) {
    let systems = pocket_ui::library::load(std::path::Path::new(cfg)).expect("es_systems.cfg");
    let mut games = Vec::new();
    for (s, sys) in systems.iter().enumerate() {
        for (g, game) in sys.games.iter().enumerate() {
            games.push(LibraryGame { at: (s, g), system: sys.name.clone(), path: game.path.clone(), name: game.name.clone(), choice: None, identified: None });
        }
    }
    let names: Vec<SystemName> = systems.iter().map(|s| SystemName { id: s.name.clone(), name: s.fullname.clone(), short: s.name.to_uppercase() }).collect();
    let mut db = GameDb::embedded();
    let t = Instant::now();
    let cat = Arc::new(curator::build_catalog(&games, names, &mut db, &CuratorDbFeel));
    println!("\n{} ({} games): catalog {:?}", cfg, cat.len(), t.elapsed());
    let ix = Index::build(cat, UserState::default());
    for (name, system) in [
        ("Burnout Revenge", "ps2"),
        ("Chrono Trigger", "snes"),
        ("F-Zero - Maximum Velocity", "gba"),
        ("Super Mario All-Stars", "snes"),
        ("Mario Kart - Double Dash!!", "gamecube"),
        ("Celeste Classic", "ports"),
    ] {
        let Some(seed) = ix.catalog.games.iter().position(|g| g.name == name && g.system == system) else {
            println!("\n{name} ({system}): not in this library");
            continue;
        };
        let t = Instant::now();
        let e = ix.echoes(seed, echoes::FULL_LIST);
        println!("\nEchoes of {name} ({system}), {:?}:", t.elapsed());
        if e.is_empty() {
            println!("  (none: no facts or tags for this game)");
        }
        for x in e {
            let g = ix.game(x.game);
            println!("  {:5.2}  {:<38} {:<9} {}", x.score, g.name, g.system, x.reason);
        }
    }
    println!();
    for picks in [
        &[(Facet::System, "gba"), (Facet::Family, "action"), (Facet::Session, "session-pickup")][..],
        &[(Facet::Players, "couch-coop")],
        &[(Facet::Family, "racing"), (Facet::Players, "couch-versus")],
        &[(Facet::Mood, "cosy")],
    ] {
        let mut lens = Lens::default();
        for (f, k) in picks {
            lens.toggle(*f, k);
        }
        let t = Instant::now();
        let n = ix.count(&ix.matches(&lens), true);
        let took = t.elapsed();
        let list: Vec<String> = ix.list(&ix.matches(&lens), true).into_iter().map(|g| format!("{} ({})", ix.game(g).name, ix.game(g).system)).collect();
        println!("Lens \"{}\": {n} games ({took:?}): {}", ix.suggest_name(&lens), list.join(", "));
    }
}
