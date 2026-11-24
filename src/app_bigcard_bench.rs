//! `--snapshot bench-bigcard[:<look>]`: a huge card's shelves in numbers (a rebuild of a player's R36S card with 179,305
//! files: its shelf switches and its cover look-ups). On the library the snapshot was started with: from Home through
//! every system's shelf (Cabinet: Down from shelf to shelf; Shelf: the system's row opened from Home), then along the biggest system's row, one
//! move per refresh. Each frame as the device paints it (`device_frame`), covers read on their threads as in the app,
//! timed in wall and thread CPU time. Prints the worst frames and how many file look-ups the covers cost
//! (`pocket_ui::dir_index::lookups`).
use super::*;
use std::sync::atomic::Ordering;

pub(crate) struct Sample {
    pub what: String,
    pub wall_ms: f64,
    pub cpu_ms: f64,
    /// `case_info` copies the frame made (`app_cabinet::CASE_INFOS`).
    pub infos: usize,
}

impl App {
    pub(crate) fn bench_bigcard(&mut self, look: &str) -> Vec<Sample> {
        super::app_console::ASSUME_POCKET_CORE.store(true, std::sync::atomic::Ordering::Relaxed);
        // `BIGCARD_SIZES=100,1000,22000`: the card's biggest system copied to shelves of these sizes instead (each game
        // its own file name), to see what grows with the number of games.
        if let Some(sizes) = std::env::var("BIGCARD_SIZES").ok().filter(|v| !v.is_empty()) {
            let src = self.systems.iter().max_by_key(|s| s.games.len()).cloned().expect("a system with games");
            self.systems = sizes
                .split(',')
                .filter_map(|n| n.trim().parse::<usize>().ok())
                .map(|n| {
                    // Under the system's own id: its gamelists and picture folders are theirs too.
                    let mut s = src.clone();
                    s.fullname = format!("Bench {n}");
                    s.games = (0..n)
                        .map(|i| {
                            let mut g = src.games[i % src.games.len()].clone();
                            g.name = format!("{} {i}", g.name);
                            let stem = g.path.file_stem().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                            let ext = g.path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
                            g.path.set_file_name(format!("{stem} {i}.{ext}"));
                            g
                        })
                        .collect();
                    s
                })
                .collect();
            self.cabinet.dirty = true;
        }
        // The card's gamelists and picture folders, read now (the app reads them on a thread after the start, which a
        // snapshot never waits for).
        let sources = pocket_ui::library::read_sources(&self.es_systems);
        let index = pocket_ui::gamelist::Index::load(&self.data_dir, &pocket_ui::gamelist::Locations::device(), &sources);
        self.gamelist_event(std::sync::Arc::new(index));
        self.theme = if look == "shelf" { Theme::Shelf } else { Theme::Cabinet };
        self.screen = Screen::Home;
        self.cabinet.dirty = true;
        self.rebuild();
        self.cabinet.inline = false;
        self.pictures.sync = false;
        self.sel = 0;
        self.prev_sel = 0;
        pocket_ui::status_corner::start(|| {});
        self.text.prewarm();
        self.text.finish_prewarm();
        let panel = crate::snapshot_panel_dims();
        let mut scaler = pocket_ui::scaled_layout::ScaledFrame::default();
        let mut screen = vec![0u32; panel.0 as usize * panel.1 as usize];
        let mut samples = Vec::new();
        pocket_ui::frame_clock::hold();
        let mut frame = |app: &mut App, what: &str, press: Option<Button>, samples: &mut Vec<Sample>| {
            let (w0, c0) = (Instant::now(), app_cabinet::thread_cpu());
            let i0 = app_cabinet::CASE_INFOS.load(Ordering::Relaxed);
            if let Some(b) = press {
                app.press(b);
                app.moved_at = pocket_ui::frame_clock::now();
            }
            app.device_frame(&mut scaler, &mut screen, panel, None);
            let infos = app_cabinet::CASE_INFOS.load(Ordering::Relaxed) - i0;
            samples.push(Sample { what: what.to_string(), wall_ms: w0.elapsed().as_secs_f64() * 1000.0, cpu_ms: (app_cabinet::thread_cpu() - c0).as_secs_f64() * 1000.0, infos });
            pocket_ui::frame_clock::step(pocket_ui::frame_clock::frames(1));
        };
        for _ in 0..3 {
            frame(self, "home", None, &mut samples);
        }
        samples.clear();
        // A slow card from here on: a test's LD_PRELOAD can wait on look-ups once this file is there.
        if let Some(mark) = std::env::var_os("BIGCARD_SLOW_MARK") {
            let _ = std::fs::write(mark, b"");
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let shelves = self.systems.len();
        let lookups0 = pocket_ui::dir_index::lookups();
        let mut reached = 0;
        // Through every system's shelf, a few settling frames after each switch (covers arriving from the thread); the
        // second time round the spines are made and only what the switch itself does is left.
        let rounds: usize = std::env::var("BIGCARD_ROUNDS").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        for (round, s) in (0..rounds).flat_map(|r| (0..shelves).map(move |s| (r, s))) {
            if round > 0 && s == 0 {
                // Back to the top: Up past the first system's shelf.
                while look != "shelf" && !matches!(self.screen, Screen::Games(0)) && samples.len() < 100_000 {
                    frame(self, "up", Some(Button::Up), &mut samples);
                }
                if look == "shelf" {
                    frame(self, "back", Some(CANCEL), &mut samples);
                }
            }
            let label = format!("round {round}: switch to {} ({} games)", self.systems[s].name, self.systems[s].games.len());
            if look == "shelf" {
                if s > 0 {
                    frame(self, &format!("round {round}: back from {}", self.systems[s - 1].name), Some(CANCEL), &mut samples);
                }
                if let Some(i) = self.items.iter().position(|it| *it == Item::System(s)) {
                    self.sel = i;
                }
                frame(self, &label, Some(CONFIRM), &mut samples);
            } else if round > 0 && s == 0 {
                frame(self, &label, None, &mut samples);
            } else {
                frame(self, &label, Some(Button::Down), &mut samples);
            }
            for k in 0..6 {
                frame(self, &format!("{label} +{}", k + 1), None, &mut samples);
            }
            // `BIGCARD_PAUSE=1`: a pause after each switch, as a player looks at the shelf before the next press: the
            // idle turns of making the next shelves ahead (`cabinet_warm_tick`), each timed like a frame.
            if look == "cabinet" && std::env::var_os("BIGCARD_PAUSE").is_some() {
                for _ in 0..60 {
                    let (w0, c0) = (Instant::now(), app_cabinet::thread_cpu());
                    let made = self.cabinet_warm(std::time::Duration::from_millis(3));
                    samples.push(Sample { what: format!("{label} warm"), wall_ms: w0.elapsed().as_secs_f64() * 1000.0, cpu_ms: (app_cabinet::thread_cpu() - c0).as_secs_f64() * 1000.0, infos: 0 });
                    if made == 0 && self.cabinet.pending == 0 {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
            if self.screen == Screen::Games(s) && self.items.len() == self.systems[s].games.len() {
                reached += 1;
            } else {
                println!("bench-bigcard missed {label}: screen {:?}, {} entries", self.screen, self.items.len());
            }
        }
        let switch_lookups = pocket_ui::dir_index::lookups() - lookups0;
        // Along the biggest system's row.
        let big = (0..shelves).max_by_key(|&s| self.systems[s].games.len()).unwrap_or(0);
        if look == "shelf" {
            frame(self, "back", Some(CANCEL), &mut samples);
            if let Some(i) = self.items.iter().position(|it| *it == Item::System(big)) {
                self.sel = i;
            }
            frame(self, "open the biggest", Some(CONFIRM), &mut samples);
        } else {
            while !matches!(self.screen, Screen::Games(s) if s == big) && samples.len() < 100_000 {
                frame(self, "up to the biggest", Some(Button::Up), &mut samples);
            }
        }
        let lookups1 = pocket_ui::dir_index::lookups();
        let along = std::env::var("BIGCARD_MOVES").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
        let next = if look == "shelf" { Button::Down } else { Button::Right };
        for k in 0..along {
            frame(self, &format!("move {} along {}", k + 1, self.systems[big].name), Some(next), &mut samples);
        }
        pocket_ui::frame_clock::thaw();
        println!(
            "bench-bigcard look={look} systems={shelves} reached={reached} games={} lookups_switching={switch_lookups} lookups_moving={} listings={}",
            self.systems.iter().map(|s| s.games.len()).sum::<usize>(),
            pocket_ui::dir_index::lookups() - lookups1,
            pocket_ui::dir_index::listings()
        );
        samples
    }
}

/// The worst frames and the spread, one line each (`bench-bigcard`).
pub(crate) fn print_bigcard(look: &str, samples: &[Sample]) {
    let mut wall: Vec<f64> = samples.iter().map(|s| s.wall_ms).collect();
    wall.sort_by(f64::total_cmp);
    let at = |q: f64| wall.get(((wall.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or(0.0);
    let over = samples.iter().filter(|s| s.wall_ms > 16.0).count();
    let cpu_over = samples.iter().filter(|s| s.cpu_ms > 16.0).count();
    let cpu_max = samples.iter().map(|s| s.cpu_ms).fold(0.0, f64::max);
    let infos_max = samples.iter().map(|s| s.infos).max().unwrap_or(0);
    println!("bench-bigcard look={look} frames={} p50_ms={:.2} p95_ms={:.2} max_ms={:.2} over16={over} cpu_max_ms={cpu_max:.2} cpu_over16={cpu_over} infos_max={infos_max}", wall.len(), at(0.5), at(0.95), at(1.0));
    if std::env::var_os("BIGCARD_ALL").is_some() {
        for s in samples {
            println!("bench-bigcard frame wall_ms={:.2} cpu_ms={:.2} infos={} {}", s.wall_ms, s.cpu_ms, s.infos, s.what);
        }
    }
    let mut worst: Vec<&Sample> = samples.iter().collect();
    worst.sort_by(|a, b| b.wall_ms.total_cmp(&a.wall_ms));
    for s in worst.iter().take(8) {
        println!("bench-bigcard worst wall_ms={:.2} cpu_ms={:.2} infos={} {}", s.wall_ms, s.cpu_ms, s.infos, s.what);
    }
}
