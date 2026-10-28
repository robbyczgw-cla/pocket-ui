//! What the German interface costs the text painter (`pocket_ui::i18n`): reading the table, and the look-up of each
//! painted string by kind, with the allocations it makes. `cargo run --release --example i18n_cost`.
//!
//! A string painted in the frame before costs one hash look-up (two for one the table doesn't have or that a
//! pattern made) and no allocation; English costs one atomic load. Only the first sight of a string that is not an
//! exact line runs the patterns, once per painter (`i18n::Cache`).

use pocket_ui::i18n::{self, Cache, Lang, Table};
use pocket_ui::text::{Face, Text};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

/// Counts allocations, to show that a repeated look-up makes none.
struct Counting;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// What a Settings or game list frame paints: lines of the table, strings a pattern translates, names it doesn't.
const EXACT: [&str; 8] = ["Settings", "Wi-Fi", "Bluetooth", "Back", "Open", "Switch", "Performance & fan", "Choose how the shelf looks, dark or light."];
const PATTERNS: [&str; 4] = ["12 games", "Set to Dark", "3 of 41 achievements", "‹ Dark ›"];
const NAMES: [&str; 4] = ["Super Mario World (USA)", "The Legend of Zelda - A Link to the Past", "Chrono Trigger", "Castlevania - Aria of Sorrow"];

fn per_string(cache: &mut Cache, strings: &[&str], rounds: usize) -> (f64, usize) {
    for s in strings {
        black_box(&*cache.get(s));
    }
    let allocs = ALLOCS.load(Ordering::Relaxed);
    let t0 = Instant::now();
    for _ in 0..rounds {
        for s in strings {
            black_box(&*cache.get(black_box(s)));
        }
    }
    let ns = t0.elapsed().as_nanos() as f64 / (rounds * strings.len()) as f64;
    (ns, ALLOCS.load(Ordering::Relaxed) - allocs)
}

fn main() {
    let src = include_str!("../assets/i18n/de.tsv");
    let t0 = Instant::now();
    let (table, errors) = Table::parse(src);
    let parse = t0.elapsed();
    let (exact, patterns) = table.len();
    println!("table: {exact} lines, {patterns} patterns, read in {:.2} ms ({} errors)", parse.as_secs_f64() * 1000.0, errors.len());

    i18n::set(Lang::En);
    let mut cache = Cache::default();
    let all: Vec<&str> = EXACT.iter().chain(&PATTERNS).chain(&NAMES).copied().collect();
    let rounds = 200_000;
    let (en, en_allocs) = per_string(&mut cache, &all, rounds);
    println!("English, any string:            {en:6.1} ns, {en_allocs} allocations in {} look-ups", rounds * all.len());

    i18n::set(Lang::De);
    i18n::german();
    for (name, strings) in [("German, a line of the table:", &EXACT[..]), ("German, made by a pattern:", &PATTERNS[..]), ("German, a name (not in it):", &NAMES[..])] {
        let mut cache = Cache::default();
        let (ns, allocs) = per_string(&mut cache, strings, rounds);
        println!("{name:32}{ns:6.1} ns, {allocs} allocations in {} look-ups", rounds * strings.len());
    }

    // The first sight of a string that isn't a line of the table: the patterns run once.
    let t0 = Instant::now();
    let fresh: Vec<String> = (0..2000).map(|i| format!("{i} games")).chain((0..2000).map(|i| format!("Game number {i} (USA)"))).collect();
    let mut cache = Cache::default();
    for s in &fresh {
        black_box(&*cache.get(s));
    }
    println!("German, first sight (patterns):  {:6.0} ns per string", t0.elapsed().as_nanos() as f64 / fresh.len() as f64);

    // Against what measuring the string costs anyway (`Text::width`, glyphs cached).
    let mut text = Text::new();
    for lang in [Lang::En, Lang::De] {
        i18n::set(lang);
        for s in &all {
            text.width(Face::Body, 28, s);
        }
        let t0 = Instant::now();
        for _ in 0..20_000 {
            for s in &all {
                black_box(text.width(Face::Body, 28, black_box(s)));
            }
        }
        println!("Text::width, {:?}:                {:6.1} ns per string", lang, t0.elapsed().as_nanos() as f64 / (20_000 * all.len()) as f64);
    }
}
