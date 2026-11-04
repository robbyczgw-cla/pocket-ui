# Performance polish: measured changes

Base: `1e676cbe` (2026-11-04). Host: Ryzen 7 4800U,
Linux x86_64, Rust 1.99.0, `gate` profile (opt-level 3), jobs 4.
No hardware/device claims. OS file cache was not dropped.

## 10. Picture request order and redraws

The original FIFO worker and the changed worker use the same library decoder,
fixture (20 distinct keys, hardlinks of one 1200×1700 PNG), fitted size 300×400,
and burst driver. Three runs, no allocation interposer.
Newest request completed after 271.525 / 266.296 / 279.491 ms before,
21.287 / 21.180 / 30.681 ms after: medians **271.525 → 21.287 ms** (−92.2%).
Every run still answered all 20 requests. Total decode work is retained;
this change prioritizes the current selection rather than canceling requests.
The baseline example embeds the unchanged base `picture_loader.rs`; both
workers link the same decoder, avoiding mismatched library artifacts.

Live picture keys are collected anew by each `App::paint`, for covers and gallery.
An arrival absent from that frame updates its cache without requesting a frame.
For an obsolete arrival the call count is therefore **1 → 0** (code path,
not a measured count of compositor frames). Split-cycle prefetch keys are
conservatively included. The actual decoder/pixels and the cache limit remain
unchanged. Four worker tests pass, including a deterministically blocked first
callback proving that request 19 passes 1…18 and all answers still arrive.

Reproduce the burst with `scripts/perf-library.py OUT 5000`, then
`cargo run --profile gate --example polish_loader_bench -- OUT/data/covers/gbc`.
Raw before/after outputs are next to this file.

## 11. Buffered battery history

The real recorder receives 62 minute samples spanning 61 simulated minutes,
starting with 2,881 samples (46,108 bytes). The base recorder writes each
sample; the changed recorder uses the same input and an injected monotonic
clock to exercise ten-minute boundaries without an hour-long wait. Both
stop normally, load the resulting file, and compare it with the full memory
history. No sample is lost. The new benchmark also checks that samples are
published immediately and only the ten-minute boundaries write.

Measured with `strace -f -c -e fsync,rename` on the test executable, excluding
compilation: **62 → 7 history saves**, plus one backup in both cases;
**126 → 16 fsync**, **63 → 8 rename**. History payload written is
2,858,696 → 322,756 bytes (excluding backup, filesystem overhead and physical
flash amplification). The file still has exactly 46,108 bytes and the PBH1
layout/checksum/backup rules are unchanged. Elapsed times in the raw trace
runs are not a latency comparison: the new benchmark also reloads files to
verify boundaries.

History and per-game totals are dirty in memory until a periodic write.
Light sleep requests a background flush; suspend and power off flush on their
existing workers before the filesystem sync. Normal exit serializes the final
flush with all accepted samples. Failed saves retain the dirty flag. A sudden
power loss can lose the last ten minutes; ordinary shutdown preserves them.
Fifteen battery tests and 52 sleep tests pass, including retry after write
failure and exit with a buffered sample. The isolated workload benchmark passes.

Reproduce with `cargo test --profile gate --lib polish_batch_bench -- --ignored --nocapture`
(run alone because it owns the sampler globals). Raw trace counts and test
results are alongside this report.

## 12. One-pass library scan

Same 5,000-game fixture and real `library::load_shown` driver, six scans per
process, without dropping OS caches. Warm median (runs 1…5, excluding the first
scan): **38.461 → 16.139 ms** (−58.0%). First scan: 50.064 → 32.865 ms;
these are process-first scans, not an SD-card cold-start measurement.
A separate strace run over all six scans measures **60,049 → 49 statx** and
**120 → 60 getdents64** calls. The 36 metadata errors in both runs are unchanged
optional-file probes. Cached-library reads remain around 2–3 ms.

`scan_entries` partitions the directory once using `DirEntry::file_type`.
Ordinary files avoid path metadata queries; symlinks explicitly follow their
targets. Rust's directory entry implementation provides the fallback when a
filesystem does not supply the entry type. File filtering, disc-sheet rules
and the one-subfolder limit use the existing code. No new index, file format
or infrastructure was introduced.

All 32 library tests pass, including a new real-filesystem test for linked
files, linked game folders, dangling links, hidden files and the depth limit.
Existing tests cover cue/gdi/m3u validation, folder games and litter filtering.
The measurement filesystem supplies d_type; exFAT/FUSE behavior and device
latency were not measured. Reproduce with `scripts/perf-library.py OUT 5000`
and `cargo run --profile gate --example polish_library_bench -- OUT/data OUT/es_systems.cfg`.

Validation across the three changes: 4 loader, 15 battery, 52 sleep and
32 library tests pass, plus the separately run history workload. The local
`cargo check --jobs 2 --bin pocket-ui` passed with the shared development
target directory. The leak scan passes. No full gate, release build or device run was performed.
