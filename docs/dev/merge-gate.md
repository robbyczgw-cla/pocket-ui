# Merge gate

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you queue a change, read a gate log or change gate.sh / the train.*

## Merge gate

The two first-start text regressions in `text_size` share their identical layout cells through
`tests/common/snapshot.rs`. Their common state list replaces duplicate fixture definitions, not assertions.
Measured unique matrices (`long_titles`, `spine_names`, `cabinet_navigation`, `insert_rigs`) render directly.
`empty_views` also asserts saved reading preferences; those flows must execute and write to their original data. Each full gate creates a fresh cache; keys
cover the executable SHA-256, state, dimensions, look, language, explicit mock clock, effective environment,
ROM/configuration and private-data contents. Random fixture prefixes become canonical paths so independently
created copies of the same library can share a render. PNG and the requested text/navigation recorders are captured
once; the recorder set is part of the key, and unrequested probes stay off. Each caller receives its requested files and still runs its original assertions. Locks serialize identical
concurrent misses, successful complete responses are verified by SHA-256, and failed renders are never reused.
Unsupported/symlinked inputs fall back to an actual render. Snapshot status time and static fixture file dates use the injected mock clock;
these helpers do not test file age or write-back behavior. Unexpected writes to the shared ROM fixture fail.
Interactive navigation, user journeys and serial timing always execute the actual program; the gate unsets the
cache before timing checks. `POCKETUI_SNAPSHOT_CACHE_DISABLE=1` runs each request again through the same staged
fixture for controlled comparisons. The gate reports requests, reuse and renderer starts before deleting its cache; raw counts remain in `/tmp/gate-snapshots-<worktree>.jsonl`.

On a train host with `GATE_EXCLUSIVE=1`, `scripts/gate-cpus.py` splits the allowed CPU affinity into disjoint
SMT sibling groups: at least six logical CPUs for Rust and one separate physical core group for each shell lane.
The optional `GATE_CPUSET` is intersected with the current affinity and enforced for every tier, including docs
and quick checks; an invalid or empty intersection fails. Without exclusivity, `taskset`, enough separate cores,
or with `GATE_PARALLEL=0`, the gate keeps its serial schedule.

After the leak scan, `scripts/check-compile.sh` checks Linux `--all-targets` (including examples),
Brick `--all-targets` and Android, in that order, stopping at the first error. It derives optional checks from
the checked tree: the Brick feature and Android manifests, so a removed platform is never requested. Both gate tiers run this stage before
code generation, fixtures or tests; the strictly docs-only path keeps its short checks. The queue client uses the same
helper before publishing requests, in a heavy slot with two Cargo jobs. `MQ_SKIP_PRECHECK=1` is an explicit emergency
override for the client only; it never skips the gate. The client rejects a dirty or changed caller and never publishes a
partially checked batch.

Default test compilation then finishes. Ordinary Rust tests/doctests, the launcher lane and the small shell lane then
overlap on their own groups. Both shell coordinators join before the Brick feature build: separate CPUs do not
isolate compiler I/O or memory pressure from watchdogs. Brick runs after default integration tests, since its build
can replace the unversioned UI executable those tests launch. Timing runs alone after every lane and build; it
keeps one test thread, no render cache and no scheduler retry. `scripts/gate-shell.sh` preserves every selected
shell suite and its exit-code file, unsets the layout cache, and inherits the train's process group for aborts.
No persistent CPU reservation or cgroup is created.

The gate prints separate wall times for the Rust runner plus doctests (including compilation), the Brick corpus/UI,
serial timing, platform checks and the actual shell interval (`gate: stages …`). Overlapping durations do not add
to the total. A `gate: schedule` JSON line records CPU groups and stage windows; nextest's Summary and Cargo's
Finished lines distinguish execution from compilation. `python3 scripts/test-gate-schedule.py` checks lane overlap,
join ordering, serial fallback, failures, missing coordinator results and group aborts with bounded stand-in
processes. It also checks disjoint SMT groups for all nonempty caps of a small topology. Both scheduler fixtures
and path-selection fixtures run before test execution, including on the short path.

Private builds run `--features brick --test brick_settings_walk` and the `text_size` group's
`textfit_brick_031_` cases alongside the Brick input corpus. The ordinary
integration tier excludes that walk and those functions through their feature guards; the explicit run checks the Brick's settings,
sleep explanations and complete hints with the actual Brick build. The Brick navigation stage runs every `nav_explorer`
test whose name contains `brick`, plus `on_the_smart_pro`; libtest ORs these substring filters. New Brick cases therefore
need no hand-maintained list. The gate path-selection self-test compares those actual invocations with all Brick-named
functions in the real suite and future-name fixtures, and checks that a red Brick navigation result fails the gate. It uses host-rendered mock screens. A red walk
fails the gate without a retry; docs-only changes and public builds without the private feature keep their existing
tiers. Real in-game device-family smokes remain a separate release requirement.

**Two tiers (since 2026-11-10).** The merge gate is the *fast tier*, aimed at 8 minutes on the train host. The *full gate*
(`GATE_FULL=1 scripts/gate.sh`) runs every night on `origin/main` (maintainer tooling, result in a status file) and
inside `scripts/release-check.sh`; red there means a fix on main before the release, not a blocked merge queue.

The fast tier compiles every program (`cargo test --no-run`: all integration test programs, so a broken suite still
fails), runs all unit tests (with the documentation checks), the doctests and every integration suite except the slow
layout matrices listed in `scripts/gate-parts.py` (`text_size`, `long_titles`, `nav_explorer`, `small_screen`, …)
and the absolute CPU budgets `settings_open` and `shelf_moves` (they run alone and go red on an overloaded host).
The compile-first stage checks Brick and Android. `scripts/gate-parts.py` adds what the change touches:

| Change | Also runs |
|---|---|
| a slow suite's own `tests/<suite>.rs`, `tests/snapshots/`, `tests/fixtures/` | that suite (all of them for the shared folders) |
| fonts, `src/text*` (also `text_size.rs`), `src/textfit*`, `src/type_scale*`, `src/font*`, `src/letters*`, `src/name_lines*`, the shared layout parts `src/kit.rs`, `src/layouts.rs`, `src/scaled_layout.rs`, `src/wiki_text.rs`, `src/auto_texts.rs` | the text-fit suites and the Brick's own text-fit cases (Brick tier) |
| `assets/i18n/` (not `skip.txt`), `docs/user/`, `tests/i18n*`, `src/i18n.rs`, the state lists | the German/Spanish matrix, `text_size`, `names_never_translated` |
| a path with `brick` in it, `src/input_corpus*`, `src/fn_keys*`, `src/manual*` | the Brick corpus, manual pages, slider and UI walk |
| a script/deploy/asset/unknown path, or a Rust file a shell script names | the shell suites |
| `Cargo.*`, `build.rs`, `.cargo/`, `.config/`, `tests/common/`, the gate's own scripts | the full gate |

The timing tier and the remaining slow suites never run in the fast tier. The gate prints the tier and what it chose
(`gate: fast tier …`, `gate: tier fast; compile N s`).

In the full gate the German and Spanish snapshot matrix (~45 s), every manual page under 30 000 px (default pages, and
the Brick's with the private feature) and the source translation audit run as well; a docs-only gate skips them.

The short path is unchanged. Only a strictly
allowlisted train takes it: unembedded root `*.md`, `docs/**/*.md` outside `docs/user/`, and
`assets/i18n/skip.txt` (extraction notes). It runs the same active `doc_checks::` link/index/document checks and
`i18n::` checks as the other tiers, plus the private-data scan. The on-device manual, `LICENSE`, `NOTICE.md`,
runtime translation TSVs and documents embedded with `include_str!`/`include_bytes!` or read by other Rust
contracts leave the short path.
Unknown dynamic embed expressions, symlinks and mixed changes also leave the short path; a missing change list selects the full gate.
A failed document, link/index, translation or private-data check cannot merge through the short path.

`sh scripts/test-gate-paths.sh` verifies this selection with temporary fixtures and a fake cargo command, without building;
every gate except the quick tier runs it before selecting suites. Missing or incomplete results of selected shell suites fail the
gate. An interrupted parallel coordinator fails without retrying beside a lane it may have left running.
The compile-first stage runs `scripts/check-platforms.sh android`: `cargo check --target aarch64-linux-android` of the
two Android crates (`android/`, `android/player/`, and with them the shared `src/` under `cfg(target_os = "android")`;
[ANDROID-MVP.md](../ANDROID-MVP.md)). A few seconds each once warm. A compile error there fails the gate (`GATE: compile-first
FAILED`); the handheld build never compiles that code. The Android part needs the Rust target and an NDK and says
`android SKIPPED` where they are missing. A crate added to the root `Cargo.toml` also has to reach the Android crates' own
lock files (`cargo update -w --manifest-path android/Cargo.toml`, the same for `android/player/`).

During development run `scripts/gate.sh --quick` from your worktree. It runs all library and binary unit tests
(including documentation checks), then selects integration suites from committed branch changes, local edits and
untracked tests against `main` (`GATE_BASE=<ref>` changes the base). Source, assets and build changes select all integration suites; script or deploy changes also run the five shell
suites. Render suites use a small representative matrix plus every newly added snapshot state in this tier.
The absolute CPU benchmarks (`settings_open`, `shelf_moves`) stay in the full opt-level 3 gate: timing an opt-level 1
build against a production frame-time limit gives a false result. Their thresholds and full-gate coverage are unchanged.
The target is under three minutes
on an idle machine with a warm build; a clean build or a heavily loaded shared builder can take longer.

On Linux, `pocket-ui --test-driver <temporary-directory>` requires `POCKETUI_MOCK=1`. It runs the ordinary event loop without
a window or device startup jobs, using `<directory>/ui.in` instead of the shared FIFO. Normal FIFO commands drive
it (the UI holds the FIFO open read+write, so back-to-back writers never see "Broken pipe" or lose a command); `frame` paints the current UI to `<directory>/frame.png`. Set `POCKETUI_VISIBLE_TEXT` for the matching text. `nav` paints it
without a picture and writes `POCKETUI_NAV_PROBE`'s record ([The navigation explorer](tests.md#the-navigation-explorer)).
`<directory>/run-game` is its stand-in emulator; game process groups and exit notifications follow the ordinary path.
With `POCKETUI_TRACE=<file>`, each requested frame records the usual `frame` and `paint` events before capture
diagnostics, scaling and PNG encoding. Use `paint` for UI paint time; the FIFO-to-PNG response also includes that
capture work and filesystem latency.
Captures use the live appearance painter, including saved Light mode and appearance changes made during a journey.
Power commands and RTC paths must point at fixture files when exercising shutdown. A successful fake shutdown
leaves the UI waiting for the system to turn off; use a failing command to exercise recovery and continue a journey.
This is a host journey harness,
not proof of game rendering, audio, Wayland surfaces or physical hardware.

Agent rule: use the quick tier while editing; let the merge queue run `scripts/gate.sh` (the fast tier) before merging. A quick pass is never merge approval.
The full gate (nightly, release check) retains all shell, documentation, translation, layout and timing checks. The full gate uses `profile.gate`; quick uses `profile.quick` (our crate at opt-level 1,
256 codegen units, dependencies at opt-level 3/16 units). Both retain release's checks and unwinding, disable LTO and
compile incrementally. `GATE_PROFILE` overrides either tier. Shipping builds still use `profile.release`. Builds default
to four jobs and low priority.

Baseline timings (shared build machine, load 34–53 on 10 cores) and the slowest 40 individual tests are in
[test-suite-baseline.tsv](../test-suite-baseline.tsv). The baseline cold compile took 19m 04s; test executables 216.5s,
then shell suites 132.7s. The components total 24m 53s; this is not a green gate: three fake shutdown tests timed out
in the host-wide `sync`, and the known Azahar header timing test failed. Individual measurements were made separately
with four processes; do not add those durations or compare them directly with the whole-binary run at ten threads.
No idle-machine sample was available.

After measurements (lower shared load, around 8; these are not controlled idle-machine comparisons) are in
[test-suite-after.tsv](../test-suite-after.tsv), including every test binary and the slowest 40 nextest cases.

| Measurement | Before | After |
| --- | --- | --- |
| Gate compilation | 19m 04s cold | 5m 29s incremental rebuild; not a cold/profile comparison |
| Test executables, separately timed | 216.5s (four failures) | 71.2s (all pass) |
| Text-size executable | 82.9s | 19.3s |
| Full outer gate, cached Cargo | Not green; component total 24m 53s including cold build | 2m 59s, exit 0 |
| Full outer gate, cached nextest | — | 3m 17s, exit 0; one permitted frame-benchmark retry |
| Rebase onto `55223de`, full outer gate | — | 4m 56s including rebuild, exit 0 |
| Final cached quick tier, all selected suites + five shell suites | — | 1m 59s, exit 0; shared load 7–13, concurrent ARM build |

At the measured 601 states and 39 risk states, layout coverage uses 718 renders instead of 2,404. The 1,686 omitted
renders repeat the three non-anchor combinations for states outside the risk list. No snapshot state is removed.
Cargo's test phase took 73s versus nextest's 106s including doctests on the lightly loaded build machine (2026-10-29). On the shared build server under load, with a retry and 12 threads, nextest was the faster one, see "The full gate runs the tests with `cargo nextest`" below; the gate now defaults to it. The shell phases
were 105s and 91s respectively, illustrating the host-load variation. The large library harness dominates compilation; merging small integration executables
was therefore not justified by these measurements.

The runtime reductions preserve behaviours:

- Every layout state still runs at Large/German; `tests/layout-risk-states.txt` adds Medium/English,
  Medium/German and Large/English for risky layouts. The omitted permutations are exactly those three combinations
  for states outside that list. Normal/German still runs for every state in the translation suite; appearance and
  alternate-screen regressions retain their dedicated suites.
- `bezel::tests::the_handheld_frames_with_a_power_light_have_one_where_there_is_room` is merged into
  `new_frames_never_draw_on_the_picture_and_keep_the_panel_edge_black`, reusing its existing images for all LED
  presence, native-scale and green/base-colour assertions. `every_style_surrounds_the_picture_at_every_scale` becomes
  `classic_styles_surround_the_picture_at_every_scale`; all six geometries for the newer styles move to the same
  combined full-pixel test (including NES 3x). No frame style or asserted property is dropped.
- `proc::tests::a_budget_is_shared_and_runs_out` advances its deadline instead of sleeping 350ms.
  `kept::tests::the_caller_never_reads_and_never_waits` uses a channel barrier instead of a 200ms sleep and a
  load-sensitive 50ms assertion. Both test names and behaviours remain.
- `logs::tests::only_one_logger_runs` keeps a cloned file description while dropping the lock guard, modelling a
  helper forked by another test. The guard explicitly unlocks before close, so that helper cannot temporarily retain
  the logger lock. This replaces a scheduling-dependent failure with a deterministic regression check; no retry.
- Fake shutdown/suspend paths skip a real flush of the build host's filesystems. Production power paths keep it.
  Every sync(2) goes through `fsutil::flush_all()`, which tests and mock runs skip (a source rule refuses direct
  calls). Shell fixtures must stub `sync` too: with uutils coreutils (a build computer's `/usr/bin/sync`) `sync FILE` flushes the whole disk.
- After the RTC fix, the early-shutdown test blocks its worker on a channel before disarm rather than running
  `sleep 1`. Restart and failed-power-off tests consume the worker’s completion message instead of polling time.
  Unit tests skip the three-second failure-notice delay; the production notice duration is unchanged. All new
  verified-disarm, shutdown-log, SleepDir cleanup and alarm-race tests from main remain.

The full gate profile and integration binary separation remain: the measured cold build is dominated by the large
library/test harness, while the small integration targets finish much sooner. `game_names` also explicitly relies
on process isolation. Consolidating them would add global-state coupling without fixing the large compilation cost.
Cold versus incremental times are reported separately, not as a profile speedup.

Five more journeys (2026-10-29, found by `scripts/monkey.py`) walk the Xbox guest start, HOME menu and Quit with the stand-in, a game that never starts (its notice), the empty card's starter pack offline, deleting a game (two A presses, the file is gone) and the Bluetooth page connecting the mock's JBL Flip 5.
The empty-card starter journey holds its real config scan on a FIFO to show that the page header arrives before
Confirm is accepted. It releases the scan and waits for the rendered `Get all N games` result before pressing A.
Only that child receives offline `ip`/`nmcli`/`curl` fixtures on PATH: mock hardware alone does not mock network
queries. No timing sleep, larger timeout, retry or production mock-network switch is needed.
The nine player journeys use the private FIFO for fresh-card onboarding, starting a stand-in game and quitting back,
Quick settings Wi-Fi off/on checked against the Wi-Fi page, and Power off with exactly one mock command and one
verified RTC disarm (a readable alarm fixture and `shutdown.log` assert one verified clear with `tries=1`,
readback `after=none`, and the disarm before the power command). Snapshot journeys check both
offline sign-ins, typo search, a readable Large search result, and the complete `bios_CD_U.bin` filename.
Every assertion reads painted text or layout diagnostics, and the game/power outcomes also check their mock boundary.
The final nine cases measured 5.59s together. The stand-in game has no core command pipe/window, so its Quit
case also exercises the ordinary five-second SIGTERM fallback and verifies both start and exit markers. Dark/light Large German offline screens were inspected in
`/tmp/suite-ui-review/`; they have no layout collisions. Device follow-up: toggle the real radio off/on and compare
the page, verify one real shutdown cancels the RTC wake alarm, and launch/quit with real video/audio and a card.

The full gate runs the tests with `cargo nextest` whenever it is installed and `.config/nextest.toml` exists (2026-10-30; on
the build server it lives in its tool folder; `GATE_RUNNER=cargo` opts out, `GATE_RUNNER=nextest` insists). It runs every test in
its own process and all test binaries side by side (`GATE_TEST_THREADS`, default the cores, at most 8; remote-gate.sh gives
the build server 12), then the doctests through Cargo. Measured on the shared build server for the same one-comment change, three runs each, shared
load 10-30: tests 163 / 164 / 169 s with `cargo test`, 115 / 114 / 112 s with nextest (the gate 188-192 s against 139-140 s).
**One automatic retry** for every test (`retries = 1`): a test that fails once and passes on the retry is listed in the
gate log as `gate:   flaky: <test>` and the gate stays green; a test that fails twice makes it red. The result lines are
unchanged (`GATE: ok (N passed)`; N is now every test, about 2,900, not only the first Cargo binary's count). Tests that
assert a time budget (thread CPU time per frame, the Curator's and the search's 5,000 games) are in an override that runs
them alone (`threads-required = "num-test-threads"`), so a busy gate cannot fail them and their thresholds stay as they
are; a new budget test belongs in that filter. Compile speed was measured too: a small change rebuilds in about 20 s, so
the test run, not the compile, is the gate's time; `mold` as the linker and `opt-level = 1` for the gate profile gained
nothing measurable (and `opt-level = 1` made the frame benchmarks fail), so `[profile.gate]` is unchanged. The JUnit
report is under `target/nextest/default/junit.xml`.

To measure without nightly Rust, build with `nice -n 10 env CARGO_BUILD_JOBS=4 cargo test --profile gate --no-run
--message-format=json > /tmp/test-build.json`, timing that command separately, then run
`nice -n 10 python3 scripts/time-tests.py /tmp/test-build.json /tmp/test-times.json --individual`.
The report separates binary durations from individual test durations and prints the slowest 40. Individual timings
include process startup and are diagnostic measurements, not performance assertions. Keep the machine load with the
report. Do not compare a clean build with an incremental build as evidence of a profile improvement.


**The merge train and artifacts (maintainers).** Branches are merged through a queue (`tools/merge-queue.sh <worktree>:<branch>`
in the maintainer's checkout, not part of this repository): it runs one combined gate for up to four branches, halves a red
batch down to the branch that broke it, and fast-forwards `main` only on green. The gate uses `cargo nextest` (see
below). After each merge the queue builds one ARM release package per `main` commit; `scripts/dev.sh deploy --artifact
<sha|latest-main>` puts such a package on a device without compiling.

`scripts/gate.sh [worktree]` is what has to be green before a branch is merged: the whole `cargo test` at release level
(all unit tests, integration tests that run the binaries, and the doc-drift checks) and `deploy/test-run-pocket-ui.sh`
(the launcher), then `scripts/test-verify-device.sh` (about 3 s) and `scripts/test-check-core-licences.sh` (about 2 s). It prints `GATE: ok (N passed)` and exits 0, or
`GATE: tests FAILED` / `GATE: launcher tests FAILED` / `GATE: verify-device tests FAILED` / `GATE: core-licence check tests FAILED` with the log's name and exits
1 (a line for each that fails). Logs: `/tmp/gate-test-<worktree>.log`, `/tmp/gate-launcher-<worktree>.log` and
`/tmp/gate-verify-<worktree>.log` and `/tmp/gate-licences-<worktree>.log`, copied to `/tmp/gate-test.log`, `/tmp/gate-launcher.log` and `/tmp/gate-verify.log`.

**Private-data scan (`scripts/check-leaks.sh`, about 5 s).** The first step of the full gate and of `--quick`, before any
compile: it copies the tracked files to a temporary folder, applies what the public export applies
(`scripts/public-exclude.txt`, then `scripts/public-scrub.txt`) and runs the export's private-data check on the result
(`scripts/public-tree.py check --fast` with `scripts/public-allow.txt`): the owner's name, home paths, host names of the
build machine and the device, MAC and private IP addresses, mail addresses, logins in URLs, tokens and serial numbers. A new
leak stops the gate with `GATE: leak scan FAILED` and one `FINDING<TAB>category<TAB>file:line` per place (the matched text is
never printed: open the line). Fix it where it stands: documentation values in tests and examples (IPv4 `192.0.2.0/24`,
`198.51.100.0/24`, `203.0.113.0/24`; IPv6 `2001:db8::/32`; MAC `00:00:5E:00:53:xx`, RFC 7042), neutral words in docs ("the
build machine", "the device", `/home/you/...`), and a login in a URL built at test time (`format!("http://user{}pw@...", ':')`).
Only a made-up value that has to look private, because the test is about the private-range logic (`192.168.x`, `10.x`,
`100.64/10` told apart from public addresses), gets a rule in `scripts/public-allow.txt` with a reason; never a key, token,
password or name. `--fast` finds the lines worth a look with one regular-expression pass per pattern over each file and scans only
those; the export itself scans every line and gives the same findings (`scripts/test-export-public.sh` compares the two).

What makes it fast, and why none of it tests less:

- **`[profile.gate]`** (Cargo.toml) runs the tests: `cargo test --profile gate`. It inherits `release` (opt-level 3, no
  debug assertions, no overflow checks, the same code the device runs apart from inlining across crates) and turns off
  what only matters for shipping: LTO and the single codegen unit (the library is 130,000 lines in one crate, so one
  codegen unit is one core for minutes), and it compiles incrementally. Nothing that ships uses it:
  `scripts/release-app.sh` and `cargo zigbuild --release` build with `[profile.release]` (thin LTO, one codegen unit,
  `panic = "abort"`).
- **One strategy for panics.** Cargo builds test targets with unwinding whatever the profile says, so with
  `panic = "abort"` in `release` it compiled the library (and every dependency that the tests link) twice, once for the
  binaries and once for the tests. `gate` unwinds, so everything is built once. The binaries that the integration tests
  start (`pocket-ui --snapshot ...`, only to check what they paint and how long it takes) therefore unwind on a panic
  instead of aborting; no test looks at how the process dies. What the abort does on a device is not something
  `cargo test` could check in any profile (unit tests always unwind).
- **Incremental.** The second gate in a worktree, after a change, only recompiles what the change touches.
- **Timing tier:** `sh scripts/test-timing.sh gate` runs the `timing_` tests with
  `--ignored --test-threads=1`, after the ordinary suite. The full gate executes it with either cargo or nextest;
  the public export executes it with the release profile. A failure has no retry. `--quick` remains a development
  shortcut and does not replace this gate. Only tests marked `ignore = "timing tier: ..."` use this name prefix;
  release checks and hardware checks keep their existing ignores.
- **Wait for the event:** fixture startup, save handoff, held config locks and controller startup use readiness,
  EOF or explicit acknowledgements. The Wayland socket appears only once listening. A save test keeps its child
  alive until the reader releases it. No minimum number of heartbeats per second is required. The system.cfg writer
  flushes the file and parent directory and reports flush failures; its test stub records that barrier instead of
  running `sync`, which is a whole-disk flush with uutils coreutils.
  Upload test clients also await a test-only request-completion event after TCP EOF; a response can precede the
  release of the server connection slot. Scope/reset tests observe their thread reaching the actual pending-write
  barrier before releasing the older card write.
- **Bounded waits under load:** `src/test_clock.rs`, included only by tests, scales hang guards by
  `max(1, ceil(load1 / available_CPUs))` (capped at 64). `POCKETUI_TEST_TIME_FACTOR=1..64` supplies an explicit extra
  scheduling allowance for a constrained runner. Product deadlines and simulated clocks are unchanged. CPU budgets
  use `CLOCK_THREAD_CPUTIME_ID` with their original thresholds. An event must still occur: expiry fails by name.
- **Launcher tests:** run after the Rust timing tier and platform checks, without a concurrent compilation or shell
  suite. They observe the fan handback and writer completion; a missed event still fails the gate. Their existing
  progress guard catches a launcher that stops making progress.

Measured on the build machine (10 cores, 16 GB, load average 5 to 13 from other builds), the same 1,774 tests plus the launcher:

| | `cargo test --release` then launcher (before) | `scripts/gate.sh` (after) |
|---|---|---|
| New worktree (empty `target/`) | 9 min 26 s (9 min 18 s with 4 jobs) | 3 min 55 s (3 min 58 s in a second run without incremental) |
| After a small change in one module | 7 min 33 s (everything is compiled again) | 1 min 3 s to 1 min 15 s |

`GATE_PROFILE=release scripts/gate.sh` runs the tests with the production profile: use it when you changed
`[profile.*]` or the build flags, or want the shipped build under test (it costs the old time). If a gate fails with a
compiler error that makes no sense (incremental caches can break, rarely), delete `target/gate` and run it again.

What was tried and not done: a shared `target/` between worktrees (cargo locks it, so gates would queue, and a build
from another branch overwrites the artefacts); `sccache` (it would only cache the dependencies, about 50 s of the 235 s on
a new worktree, and the library itself cannot be cached across worktrees; not worth an install); merging the four
integration test crates (three of them do not link the library and link in under 3 s each; the cost is the library's
code generation, not linking); lower `opt-level` for the tests (faster still, but then the tests no longer run the code
the device runs).
