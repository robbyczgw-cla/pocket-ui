//! A big library as one person has it (docs/OTHER-LAYOUTS.md, "13,000 ROMs"): numbers, not a pass/fail test of speed.
//! `python3 scripts/other-layouts-big-fixture.py /tmp/fx13k` makes the tree; then
//! Not named `timing_`: the gate runs those, and this one needs the fixture. `POCKETUI_BIG_FIXTURE=/tmp/fx13k cargo test --profile gate --test big_library -- --ignored --nocapture`.

use pocket_ui::gamelist::{Index, Locations};
use pocket_ui::library::{self, Gamelists};
use std::time::Instant;

fn kb(field: &str) -> u64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    s.lines().find(|l| l.starts_with(field)).and_then(|l| l.split_whitespace().nth(1)).and_then(|n| n.parse().ok()).unwrap_or(0)
}

#[test]
#[ignore = "release check: needs POCKETUI_BIG_FIXTURE, prints the numbers"]
fn numbers_of_a_13000_rom_card() {
    let Some(root) = std::env::var_os("POCKETUI_BIG_FIXTURE").map(std::path::PathBuf::from) else { panic!("POCKETUI_BIG_FIXTURE=<dir made by scripts/other-layouts-big-fixture.py>") };
    let (cfg, data) = (root.join("es_systems.cfg"), tempfile::tempdir().unwrap());
    let places = Locations::none();
    let rss0 = kb("VmRSS:");
    let read = || library::load_with(&cfg, Some(&Gamelists { data_dir: data.path(), places: &places })).unwrap();
    let t = Instant::now();
    let systems = read();
    let first = t.elapsed();
    let games: usize = systems.iter().map(|s| s.games.len()).sum();
    let rss1 = kb("VmRSS:");
    let mut again = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        let s = read();
        again.push(t.elapsed());
        assert_eq!(s.iter().map(|s| s.games.len()).sum::<usize>(), games);
    }
    let per_system: Vec<String> = systems.iter().map(|s| format!("{}={}", s.name, s.games.len())).collect();
    println!("systems: {} ({games} games)", per_system.join(" "));
    println!("library read: first {first:?}, again {again:?}");
    // The pictures: the gamelists and picture folders (what the thread after the first frame does).
    let sources = library::read_sources(&cfg);
    let t = Instant::now();
    let index = Index::load(data.path(), &places, &sources);
    let pics1 = t.elapsed();
    let t = Instant::now();
    let index2 = Index::load(data.path(), &places, &sources);
    let pics2 = t.elapsed();
    assert!(index.same_as(&index2));
    let t = Instant::now();
    let with_box: usize = systems.iter().map(|s| s.games.iter().filter(|g| index.has_box_art(&s.name, &g.path)).count()).sum();
    println!("pictures: Index::load first {pics1:?}, again {pics2:?}; has_box_art for all {games}: {:?} ({with_box} with a box)", t.elapsed());
    let t = Instant::now();
    let cache = pocket_ui::library_cache::Key::now(&cfg).unwrap();
    pocket_ui::library_cache::write(data.path(), &cache, &systems).unwrap();
    let wrote = t.elapsed();
    let size = std::fs::metadata(pocket_ui::library_cache::path(data.path())).unwrap().len();
    let t = Instant::now();
    let back = pocket_ui::library_cache::read(data.path(), &cache).unwrap();
    println!("library cache: {} KB, written in {wrote:?}, read back in {:?} ({} games)", size / 1024, t.elapsed(), back.iter().map(|s| s.games.len()).sum::<usize>());
    println!("memory: RSS {} MB before, {} MB after the first read, {} MB at the end, peak {} MB", rss0 / 1024, rss1 / 1024, kb("VmRSS:") / 1024, kb("VmHWM:") / 1024);
}
