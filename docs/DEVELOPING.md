# Developing pocket-ui

How to build, run and test pocket-ui and pocket-core. For how the pieces fit together, read
[ARCHITECTURE.md](ARCHITECTURE.md) first.

## What you need

- A Rust toolchain (stable, edition 2021) and the `aarch64-unknown-linux-gnu` target:
  `rustup target add aarch64-unknown-linux-gnu`.
- [`cargo-zigbuild`](https://github.com/rust-cross/cargo-zigbuild) and [Zig](https://ziglang.org), to cross-build
  for the device from macOS or Linux.
- Docker with an aarch64 engine (colima or Docker Desktop on Apple silicon), for the virtual device and for the
  Linux-only tests.
- For the virtual device: a ROCKNIX SM8250 image on your disk. It is not part of this repository.
- For a real device: a Mini V2 running ROCKNIX with SSH switched on, reachable from your computer.

The scripts were written on macOS. `scripts/devlock.sh` uses BSD `stat`, and `scripts/flash-sd.sh` uses
`diskutil`, so these two need changes to run on Linux.

## Building

The device runs ROCKNIX with glibc 2.41. Link against an older glibc so a binary does not depend on a symbol the
image lacks:

```sh
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --bin pocket-ui --bin pocket-core
```

The output is in `target/aarch64-unknown-linux-gnu/release/`.

The scripts that build release binaries (`dev.sh deploy`, `vdev.sh deploy`, `release-app.sh`, `make-image.sh`,
`make-install-zip.sh`, `syscheck.sh`, the build check of `export-public.sh`) first source `scripts/release-rustflags.sh`.
It adds `--remap-path-prefix` for the repository (`/pocket-ui`), the cargo home (`/cargo`) and the Rust toolchain
(`/rustup`), so panic locations in the binaries do not carry the build user's home folder. Only release builds use it;
debug, test and gate builds are unchanged. When you run `cargo zigbuild --release` by hand for something that ships, run
`. scripts/release-rustflags.sh` first. `scripts/check-binary-paths.sh <binary>...` fails if a binary still contains
`/home/`, `/Users/` or the build paths; `make-image.sh`, `make-install-zip.sh`, `release-app.sh` and `export-public.sh` run it.
(`profile.trim-paths` would do the same in `Cargo.toml`, but it is still nightly-only in cargo 1.99.) Do not use
`env!("CARGO_MANIFEST_DIR")` outside `#[cfg(test)]`: the compiled-in string is not remapped.

### A new ROCKNIX version

pocket-ui depends on ROCKNIX's scripts, settings and libraries, not only on its kernel. Before moving the device (or
our own image) to a newer ROCKNIX, compare the two:

```sh
scripts/rocknix-diff.sh 20261001 origin/next          # two ROCKNIX git refs: the source tree
scripts/rocknix-diff.sh old/SYSTEM new/ROCKNIX-SM8250.aarch64-20261101.tar   # two images (tar, SYSTEM or folder)
scripts/rocknix-diff.sh -v 20261001 origin/next       # with the diff of every changed script
```

It lists what changed in what pocket-ui depends on (the UI-start scripts the launcher repeats, the scripts it calls,
ROCKNIX's shell functions, `system.cfg` defaults, es_systems entries, cores, standalone start scripts, the SM8250 and
Mini V2 quirks, library and package versions) and ends with a checklist; the steps are in
[UPDATES.md](UPDATES.md#a-rocknix-bump). Refs come from a clone in `~/.cache/rocknix-distribution` (`ROCKNIX_SRC`),
made on first use; images need `unsquashfs` (`brew install squashfs`) or Docker with the virtual device's tool image.

Then check the new system against everything pocket-ui relies on, one system at a time:

```sh
unsquashfs -d /tmp/rocknix-root SYSTEM                   # the SYSTEM file of the image's first partition or update tar
scripts/rocknix-contract.sh --root /tmp/rocknix-root     # an unpacked (or mounted) root
scripts/rocknix-contract.sh --ref origin/next            # ROCKNIX's source tree (only the rows it can map)
scripts/rocknix-contract.sh --host root@<device>         # a running device, read-only, over one SSH session
```

One line per promise (the lock style of `set_setting`, the functions `start_es.sh` calls, the arguments `runemu.sh`
parses, es_systems folders and extensions, cores, services, ...): `ok`, `CHANGED` or `MISSING` with the evidence and
the code that relies on it, then `CONTRACT: ok` (exit 0) or `CONTRACT: <n> changed` (exit 1). The rows are
`scripts/rocknix-contract.txt`, explained in [ROCKNIX-CONTRACT.md](ROCKNIX-CONTRACT.md); `cargo test` keeps them in
step with the code and runs the script's self-test (`scripts/test-rocknix-contract.sh`).

The package has five binaries:

| Binary | Source | What it is |
|---|---|---|
| `pocket-ui` | `src/main.rs` | The shelf, Settings, first-start setup, the pause menu |
| `pocket-core` | `src/bin/pocket_core.rs` | The libretro host that runs a game |
| `pocket-splash` | `src/bin/pocket_splash.rs` | The boot screen on the frame buffer from systemd's start until sway takes over; `--snapshot` renders a frame without a screen ([BOOT-SPLASH.md](BOOT-SPLASH.md)) |
| `store-tool` | `src/bin/store_tool.rs` | Signs catalogs and update lists, checks a signature (`verify`), lists the files of an update list (`list`), prints file hashes. Runs on your computer |
| `pocket-fb` | `src/bin/pocket_fb.rs` | Only with `--features brick` ([TRIMUI-BRICK.md](TRIMUI-BRICK.md)): Wayland and sway's IPC on a bare framebuffer |
| `brick-egl` | `src/bin/brick_egl.rs` | Only with `--features brick`: the Brick's GPU test (`--probe`, `--panel`) |
| `hello-pad` | `src/bin/hello_pad.rs` | A small program that shows button presses, for checking input |

`cargo build` and `cargo test` also work on macOS, where the Linux-only parts (evdev input, the gamma protocol) are
compiled out. That is enough for unit tests and for rendering screenshots.

`POCKETUI_VERSION=<version>` at build time sets the version that `pocket-ui --version` and the Updates page
report; without it the version from `Cargo.toml` is used. `POCKETUI_COMMIT=<sha>` names the commit in Report a
problem's `system.txt`; `scripts/release-app.sh`, `make-image.sh`, `make-install-zip.sh` and `dev.sh deploy` set it.

## The dev loop on a real device

`scripts/dev.sh` (and `syscheck.sh`, `hwplay.sh`, `harden-ssh.sh`, `autostart.sh`) talk to the device over SSH.
There is no built-in address. Set it once per shell, for example `export POCKETUI_HOST=root@<device-ip>`, or keep it
in `scripts/local.env` (not tracked; copy `scripts/local.env.example`), which `scripts/host.sh` reads when the
variable is not set. Without either, the scripts stop with a message at the first `ssh`/`scp`. The test is
`scripts/test-host.sh`.

```sh
scripts/dev.sh deploy        # build, copy to /storage/pocket-ui/releases/dev, (re)start pocket-ui, then verify-device.sh
scripts/dev.sh deploy --artifact latest-main  # install a main artifact, without building
scripts/dev.sh send down a   # press buttons through pocket-ui's command FIFO
scripts/dev.sh shot name     # screenshot to screenshots/name.png
scripts/dev.sh log           # pocket-ui's log
scripts/dev.sh stop          # stop pocket-ui; EmulationStation comes back
```

`send` accepts: `up down left right a b x y l1 r1 l2 r2 select start home quit leave`. The letters are the menus' roles
(a confirms, b goes back, x is the top button's action, y the left one's), the same in both Settings → Playing → Button
layouts: the FIFO is not a physical pad (`button_layout`). A real pad's buttons, and `vdev.sh pad press A`, which is
one, follow the layout. `quit` ends pocket-ui with
exit 0, which the launcher answers with a fresh start (not EmulationStation); `leave` opens the dialog SELECT + START
held opens. A running game reads the real pad, not this FIFO.

`deploy` overwrites `releases/dev` in place. If the launcher had marked it as the last known good version
(`known-good`, [ARCHITECTURE.md](ARCHITECTURE.md#rescue-path)), it is copied to `releases/dev-good` first, so a dev
build that keeps crashing still has a working version to go back to.

For main, use `deploy --artifact <sha|latest-main>`. The artifact directory defaults to `../artifacts` beside the
repository; set `POCKETUI_ARTIFACT_DIR` to use another location. Full or unambiguous abbreviated commits are accepted.
Every package contains all three release binaries, the deployment files and the matching device verifier. Checksums
and the full commit are checked before the device is stopped; verification compares against the artifact binaries.
The usual known-good backup and restart still apply. Plain `deploy` continues to build the current worktree.

### After a deploy or a measurement: `scripts/verify-device.sh`

`deploy` ends with `scripts/verify-device.sh`, and `scripts/latency.sh` on a device does too (exit 3 when it fails).
Run it by hand after any demo or measurement script of your own. On 2026-10-24 a demo left a second pocket-ui running
for about six hours, outside the `essway` unit, with `POCKETUI_TRACE` and `POCKETUI_NO_HISTORY` set. Later deploys
failed silently ("text busy"), real play was not recorded, and a feature was reported as running from an old log
line. The script reads the device's state in one SSH session (sshd rate-limits many quick logins) and prints one line
per check, `ok`, `FAIL` or `info`, with what it saw, then `VERIFY: ok` (exit 0) or `VERIFY: FAILED (service, cgroup,
...)` (exit 1):

| Check (name in the `FAILED` line) | Passes when |
|---|---|
| `service` | `systemctl is-active essway` is `active` (`pocketui` when the boot-UI drop-in is not installed) |
| `pocket-ui` | exactly one `pocket-ui` runs (`pidof`, never `pgrep -f`); the line says how long it has been running, so an old log line cannot pass for a fresh start |
| `cgroup` | `/proc/<pid>/cgroup` of it names `essway.service`: it was started by the unit, not by a shell or a script |
| `env` | No `/storage/pocket-ui/dev.env` exists. Every pocket-ui and pocket-core has normal data/library paths, no test hardware, power, clock or snapshot overrides, no `POCKETUI_TRACE`, and `POCKETUI_NO_HISTORY` is unset, empty or `0` (the latter two do not hold recording off, [HISTORY.md](HISTORY.md)). Explicit normal defaults and the launcher frame file are accepted |
| `md5` | the binary that runs (`/proc/<pid>/exe`) has the md5 of the deployed one (`/storage/pocket-ui/pocket-ui`, which follows `current/`), and the deployed one has the md5 of the build here (`target/aarch64-unknown-linux-gnu/release`) |
| `sound` | with `VERIFY_SOUND_BEFORE` (`dev.sh deploy` reads the sink with `wpctl get-volume` before it stops pocket-ui, puts it back after the restart when it changed, and passes it on) the sink's volume and mute are the same: `sound: muted 0.10 -> muted 0.10`. Without it only an `info` line. A deploy must not make a muted device audible; pocket-ui itself only reads the sink at start |
| `ssh` | the state could be read at all |

After testing, restore/remove `dev.env`, wait for the game to end, restart the UI unit and run verification again.
Removing the file alone does not reset the running launcher or app: their inherited `POCKETUI_DATA` can still
point at a test folder, missing the normal marker and library. Delete test data only after this check passes.

A running `pocket-core` is only reported (`info`): a game may be on. Test overrides or a stale binary fail verification. It waits up to ten seconds (`VERIFY_WAIT`) for the unit and pocket-ui to come up, since it runs right after
a restart. `--no-local` leaves out the comparison with the local build (`latency.sh` uses it: it measures whatever is
installed); no local build to compare with is an `info`, not a failure. After `dev.sh stop` it fails by design:
EmulationStation is in charge and no pocket-ui runs (the line mentions the `/tmp/pocket-ui.es-only` mark). The test is
`scripts/test-verify-device.sh` (fake `ssh`, `/proc`, `systemctl` and `pidof`; about 3 s; part of the gate).

To see safe mode on the device: put `POCKETUI_SAFE=1` into `/storage/pocket-ui/dev.env` and `deploy` (remove the line
again afterwards). Snapshots of it need no device (see below).

When pocket-ui is installed as the boot UI (`scripts/autostart.sh`, see
[INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md) phase 6), `deploy` restarts the `essway` unit instead of starting its
own, and `stop` leaves a marker so that EmulationStation takes over until the next `deploy` or restart. The launcher
itself no longer opens EmulationStation after crashes; see [Rescue path](ARCHITECTURE.md#rescue-path).

Other scripts that work against the device:

| Script | Purpose |
|---|---|
| `scripts/verify-device.sh [--no-local] [host]` | Is the device in its normal state: the unit active, exactly one pocket-ui in `essway.service`, normal paths and no test overrides or dev.env, the deployed binary running ([above](#after-a-deploy-or-a-measurement-scriptsverify-devicesh)). Run by `dev.sh deploy` and `latency.sh` |
| `scripts/devlock.sh <who> <command...>` | Takes a lock so that several sessions do not use the one device at the same time. Wrap short runs: `scripts/devlock.sh me scripts/dev.sh deploy`. A lock older than 20 minutes counts as stale |
| `scripts/syscheck.sh` | Plays a game on pocket-core with test input, takes screenshots, saves and loads, and measures CPU against RetroArch. See the header of the script |
| `scripts/hwplay.sh` | Scripted play with the virtual pad and screenshots on the way |
| Idle menu regression | `cargo test --profile gate --lib idle_shelf_requests_no_redraw`; real-binary measurement recipe and audit: [IDLE.md](IDLE.md) |
| `scripts/big-emu-bench.sh [--minutes M] [--rumble] ROM[@FPS]...` | PS2 (ARMSX2), GameCube/Wii (Dolphin) and 3DS (Azahar) games on the Mini V2: started like pocket-ui, muted and dimmed, then frame rate (average, 1 % low, long frames), audio underruns, clocks, temperature, fan and battery, and a verdict per game; stops when the fan starts, puts the emulators' folders back ([BIG-EMULATORS.md](BIG-EMULATORS.md)) |
| `scripts/pixelify-glyphs.py [FONTS_DIR]` | Patches the bundled Pixelify Sans (`assets/fonts/PixelifySans-*.ttf`, needs fonttools): C and c open (they read as O and o), a diagonal Z, a flat-topped 5; advances and bearings stay, running it twice changes nothing. An OFL modification, noted in `assets/fonts/OFL.txt`; the tests `retro_letters_are_crisp_and_digits_tell_apart_from_letters` and `pixel_font_look_alikes_tell_apart_at_every_size` (`src/text.rs`) check it from 21 to 240 px |
| `scripts/ui-sounds/prep_media_sounds.py RAW OUT KIND FROM TO`, `media-sounds.tsv`, `loudness.py FILE…` | The media sounds (case, box, cartridge, disc, card, CRT): ElevenLabs output cut, filtered and levelled into `assets/ui-sounds/` (the table: prompt and cut of each); the loudness (M max, LUFS, also as a small speaker plays it), true peak, DC and edges of WAV files ([HOST.md](HOST.md#ui-sounds-srcui_soundrs)) |
| `scripts/ui-sounds/device-capture.sh BIN [STEP] [OUT]`, `device-ui.sh POCKET_UI [OUT]` | On a handheld (`POCKETUI_HOST`, under the device lock; `MS_SINK=null` with the speaker muted): the media sounds recorded off the sink, through the mixer (`examples/media_sounds_play`) or from the buttons in a pocket-ui with its own data and two test ROMs; `analyze_capture.py` gives per sound how late, M max, true peak, D+N against the file, gaps and clipped samples |
| `scripts/audio-bench/bench.sh [seconds]` | pocket-core's sound path measured off the device: ROCKNIX's PipeWire in a container on `AUDIO_BENCH_HOST` (scripts/local.env), a 1 kHz tone on the speaker's and the Bluetooth graph, dropouts, clicks, THD+N and pitch wobble against limits, exit 1 when one is broken ([its README](../scripts/audio-bench/README.md)) |
| `scripts/autostart.sh` | Installs or removes pocket-ui as the boot UI. Dry run by default |
| `scripts/harden-ssh.sh` | Key-only SSH for development devices. Dry run by default ([SECURITY-HARDENING.md](SECURITY-HARDENING.md)) |
| `deploy/bench.sh`, `deploy/frametime*`, `deploy/thread-cpu.sh` | Benchmark helpers ([HOST.md](HOST.md)) |

## The virtual device

No device needed. `scripts/vdev.sh` runs pocket-ui and pocket-core in an aarch64 container with the real ROCKNIX
userland, a headless 1240 x 1080 sway, software GL (llvmpipe), PipeWire and a virtual gamepad and touch screen.

```sh
scripts/vdev.sh deploy          # build, start the container if needed, start pocket-ui
scripts/vdev.sh send down a     # menus, through the command FIFO
scripts/vdev.sh pad press A 150 # in-game input, through the virtual pad
scripts/vdev.sh shot name       # screenshots/name.png
scripts/vdev.sh log -f          # follow the log
scripts/vdev.sh run-core gambatte game.gbc --bench 10
```

Use `VDEV_NAME=<name>` to run several containers side by side. The first run unpacks your ROCKNIX image into a
Docker image, which takes a minute or two. Setup, ROM folders and what differs from the real device (speed,
battery, Wi-Fi, fan, suspend) are in [VIRTUAL-DEVICE.md](VIRTUAL-DEVICE.md). Do not take performance numbers from
it.

## Start time

pocket-ui should feel like a console woken from standby: the shelf as soon as sway takes a window, a game as soon as
its emulator can draw. Nothing that the first frame doesn't need runs before it on the UI thread.

- **pocket-ui** reads the type (`Text::new`) and the game list on threads of their own while it connects to sway
  and reads its settings; opens the pads, the touch screen and the command FIFO on a thread; sets the sway rules for
  game windows on a thread (`control::games_open_fullscreen_soon`; `launch` waits for them with `rules_in_place`).
- **Cold start** (every power-on: nothing of pocket-ui is in the page cache, each read waits for the SD card, and
  reads that run at the same time queue up behind each other): only what the first frame shows is read before it.
  - The game list comes from `<data>/library-cache.json`, the last scan (`library_cache.rs`): one file instead of
    es_systems.cfg and a listing of every ROM folder (205 ms of 319 ms on the Mini V2 with 332 games). After the
    first frame a thread reads the folders again (`app_library.rs`); a list that differs is swapped in like after
    Add games, at a moment nothing holds a game by its place (no game, game card, console page, menu, Settings or
    search), the selection kept on the same game or system; an equal one changes nothing (no frame, no write).
    Measured on the device with `scripts/latency.sh --cold` ([LATENCY.md](LATENCY.md#cold-start)). The cache is
    turned away when es_systems.cfg (path, mtime, size), the cache format or pocket-ui's version differ, or when it
    doesn't parse; then the folders are read before the first frame as on a first boot, and the cache is written
    after it. Add games and web uploads write it too. A game whose file is gone (deleted while the cache still lists
    it) doesn't start: `launch` says so ("… can't start: its file … is no longer there") and reads the folders
    again.
  - Moved behind the first frame (`App::after_first_frame`, which also runs if no frame came within 2 s):
    the state of Settings → Store, Sync, Tailscale, Web companion, Bluetooth and RetroAchievements
    (`startup::Later`: read on a thread then; a use before that reads it at once, logged as `startup: … needed
    before the first frame`), the GameCube/Dolphin defaults in system.cfg (`emu_settings::apply_start_defaults_once`,
    which `launch` also calls, so a game started at once still has them), and the threads that start programs or
    read files the first frame doesn't need (`startup::after_first_frame`: `pactl` for the volume, the stick
    lights, the function keys, the Wi-Fi watchdog's `nmcli`, Samba's configuration, remote access, Store tools,
    the wait for Wi-Fi). Sleep's `systemd-inhibit` starts on a thread (`sleep::InhibitorSlot`). What stays before
    the first frame because it shows: settings.json (look, appearance, frames, On start, hidden systems),
    the display look (`Sys::new`, gamma), recent games, favourites, `library.json` (pinned stickers), Curator's
    switches (Lens and Home rows), the retired fallback list and the fan profile (a game On start continues needs it),
    sleep's settings.
- **pocket-core** opens the pads, the touch screen and the sound device while its window and GL come up, and makes
  its command FIFO with `mkfifo(3)` instead of the program.
- **The launcher** asks sway every 50 ms at boot (it was 250 ms).

Both programs log where the time went, step by step since `main` (`src/startup.rs`):

```
first frame after 9.9ms (logs 0.3, crash 0.1, sounds 0.3, event loop 0.8, input 0.0, settings+hw 0.2, sys 4.2, ...)
first frame after 54.3ms, 58 ms after the game was started in pocket-ui (core 9.7, input 0.2, fifo 0.3, resume 8.7, loop 0.0, graphics 26.3, audio 0.0, first frame 9.1 ms)
```

pocket-ui hands pocket-core the moment the game was asked for (`POCKET_LAUNCHED_AT`, `CLOCK_MONOTONIC` in ms), so the
second line counts from A. Measured in the virtual device (2026-10-22, M1 Max, 6 vCPU; the window counted when sway
maps it, polled from outside; old and new build run in turns on a host busy with other builds):

| | before | after |
|---|---|---|
| launcher start → shelf window, 18 systems / 844 games | 56-130 ms (median 82), outliers to 383 ms | 22-96 ms (median 39) |
| pocket-ui `main` → first frame (its own log) | 46-83 ms | 8.5-14 ms |
| A on "Continue" → game window (uCity, resumed), quiet runs | 76-155 ms | 52-91 ms |

The rest of the game start is pocket-core's GL setup (25-55 ms on llvmpipe) and the core. Take the device's own
numbers from its log; the virtual device only shows where the time goes. On the Mini V2, pocket-ui's first frame came
about 42 ms after `main` (2026-10-23), once the display look was applied at start without being saved again.

The slow-load note has its own dark backing and light type, independent of the menu appearance, so a light launch picture or black letterbox cannot hide it.

**The launch picture** (`src/launch_picture.rs`): while a game starts, the shelf fades into the picture of where it
was left (the auto-resume preview, else the frame kept at Quit), placed where pocket-core will draw the game; for
pocket-core with the game's Picture setting (`video::dest_rect`), turned as its Screen orientation turns the game
(`video::turned_shape`, `turn_pixels`). pocket-core then fades the frame around the picture
in from black over 250 ms (`Overlay::draw_veil`), so the moment the game's window covers pocket-ui is not seen.

**Slow launch feedback** (`src/app_loading.rs`, `startup::LoadWatch`): pocket-core marks a load still busy after three seconds
on the runtime tmpfs, and removes that mark on success or error. The UI polls during a launch and paints the note with its spinner.
Each due tick requests a frame before advancing the next deadline; once the game loads, polling stops. The ordinary fast launch is unchanged.
The timer and load-watch regression tests cover the handshake; `tests/where_was_i.rs` checks the rendered message in all three languages.


**Where was I?** (`src/where_was_i.rs`, `src/app_where_was_i.rs`): a moment continued more than 14 days after the last
session gets a quiet card at the bottom of the launch picture (when it was played last, the time played, the game card's
note). Its history and note come from memory in `launch` (recent games, else the play log; `library_meta`'s note), and it is shown once
the launch picture is there and `frozen_at_launch` kept the moment. On pocket-core the start then asks for a hold
(`App::where_was_i_hold`: `HOLD_STILL=1`, until any button, with `HOLD_UNTIL` only bounding the animation): the
window stays unmapped and the core runs no frame once its moment is loaded (`startup::stands_still`; a 3D core's deferred
resume runs first), no sound, so the card can be read even where the moment is back in 0.1 s and nothing is missed. The
reminder has no timeout, including after a slow load. Any button ends it (pocket-core's own press check, and
`App::launch_button` → `startup::skip_hold`), including a press before the picture finishes loading. A preview alone
does not qualify: a matching resume state must exist. `HOLD_READY` points to a file the UI creates when its decoded
picture arrives: if the core reaches play before that, it starts immediately, so a slow read adds no invisible hold.
A missing or corrupt picture cancels the reminder and its hold;
pocket-core also drops that hold if the resume fails, is skipped, or keeps a newer battery save. Pause, sleep and screen
off end the launch hold so returning to play never waits for an obsolete card. Ordinary launches retain their 2 s
deadline safety bound; `HOLD_STILL=1` waits for a button. Other emulators can't be held: the card stays until the game covers it.
Off: settings.json `no_where_was_i`; never in Kids Mode; Easy drops the time played.
The Playing category's summary includes the reminder as its own part, keeping device-specific omissions aligned with
their rows (Steam Deck leaves performance out of both the list and its summary). Snapshots:
`where-was-i[:plain|long|easy|wait|press|early-press|missing]` (an optional third argument is a frame to show); tests:
`where_was_i::tests`, `startup::tests`, `tests/where_was_i.rs`, and `user_journeys::fifo::a_stale_preview_never_holds_a_fresh_start_for_a_reminder`.
The actual host's hold, exact game position, rejected/missing states, early quit, input, waiting past four seconds, pause, sleep and dark
screen are checked with `cargo test --profile gate --features x11 --test launch_hold -- --ignored` (Xvfb, Mesa and cc).
With `POCKET_CORE_TEST=1` the host's command FIFO is in `TMPDIR`, isolating these runs from other games.

**Settings → Playing → On start** (`src/on_start.rs`): `decide` has the rules and the tests. The resume note of a power off
(`sleep::take_resume`) always wins; Last game continues only on the first start of pocket-ui in a boot (boot id in
`<data>/started-boot`), without a crash notice, and when `<data>/last-game.json` says the game ended normally. In the
virtual device the boot id never changes: remove `started-boot` to try a "first start".

**Settings → Playing → Buttons → Button layout** (`src/button_layout.rs`): the menus' code and hints speak in four roles on
`Button::East` (A, confirm), `South` (B, back), `North` (X) and `West` (Y). The setting (Nintendo: as printed; Xbox:
A and B swapped, X and Y swapped) decides which physical button plays which role. The one place it is applied is the
closure `main` hands to `pad::linux::supervise` (`Remap::event`, on the pad threads, before the event reaches the UI
thread), so `CONFIRM`/`CANCEL`, every page's `press`, the hints and the touch pills need no knowledge of it, and a new
screen cannot forget it. A release goes back as the role its press had (the press that switched the layout is let go
after the switch). Not remapped, on purpose: games (pocket-core and the emulators read the pads themselves, so Options →
Controls and the HOME shortcuts stay the printed buttons), touch, the command FIFO, the desktop preview's keyboard and
safe mode (`app_rescue::run` has its own loop and runs before settings.json is read; the layout is Nintendo until
`main` sets it from the settings). A new screen takes its buttons as `Button::East`/`South`/`North`/`West` roles like
the others; a new thing that must stay physical belongs outside the UI thread's event path. Tests: `button_layout.rs`
and `tests/button_layout.rs`.

**Settings → Playing → Buttons → Button symbols** (`src/button_prompts.rs`, the page `src/app_buttons.rs`, which also holds Button layout):
how a hint key looks, never which button it is. Every key is drawn by `kit::key_cap` (both hint bars, the picture of the four buttons on
the Buttons page, the manual's chips; the manual's controls picture asks `button_prompts::shown`/`colours` itself): Nintendo
draws the letter on the hint's cap, Xbox the letter on its colour (`colours`), PlayStation the shape for the role
(A ✕, B ○, X □, Y △). The shapes are private-use characters (`button_prompts::CROSS` …) that `Text::glyph` rasterises with
`button_prompts::raster` at the face's cap height, so measuring, fitting and the hint bars' label rules treat them as
one letter. Unset: `device::Device::prompt_style` (Xbox on Retroid and AYN, Nintendo on Anbernic and unknown devices);
`POCKETUI_PROMPTS=nintendo|xbox|playstation` overrides settings.json for snapshots (`buttons[:<row>][:<presses>]` is the Buttons page). Prose ("Press A to …") keeps its letters.

**Look → Colours** (`palette::Colours`, `Scheme`): five sets of the palette tokens, each with a dark and a light
`Palette` and the Cabinet's stain (`wood`, `stain`), plate (`holder`, `card`, `card_ink`) and slot light (`lamp`).
`pal()` mixes the set in use; a new token or a new set needs no other change, and `palette::tests` checks every text pair
of every set for 4.5:1 in both appearances and a page colour of its own per set (caches like the Cabinet's wood are
keyed by it). The constants (`VOID`, `CHALK` …) and `DARK` stay Warm's: overlays over a game keep them.
`POCKETUI_COLOURS=warm|indigo-cube|grey-brick|green-x|blue-station` overrides settings.json for snapshots;
`POCKETUI_RETRO=cocoa|pea` paints a snapshot in the Retro style.

## Tests

| Command | What it runs |
|---|---|
| `cargo test` | The unit tests on your computer. Tests that need `/proc`, `prctl` or real process groups (`src/control.rs`) only exist on Linux and are left out on macOS |
| `scripts/test-linux.sh [filter]` | The library's unit tests on Linux (aarch64, in Docker), including the Linux-only ones. The first run builds a small Debian image with gcc and curl |
| `scripts/gate.sh [worktree]` | The merge gate: the tests at release level, then the launcher tests, the verify-device test, the core-licence check's test, the install package's test, the quiet start's test (`deploy/test-grub-quiet.sh`) and the image check's grub.cfg test (`scripts/image/test-grub-checks.sh`); prints `GATE: ok (N passed)` and exits 0, or `GATE: ... FAILED` and exits 1 ([Merge gate](#merge-gate)) |
| `sh deploy/image/test-mount-storage.sh` | Tests of the SD image's boot hook `deploy/image/mount-storage.sh`: the first-start seed and the guard against system updates pocket-ui did not stage; and of the GAMES drive's `games-first-start.sh` and `games-mount.sh` with stand-ins for what changes the card: a power cut at every tool call, a small card, a foreign partition, each failure ([SD-IMAGE.md](SD-IMAGE.md)) (part of the gate) |
| `sh deploy/image/test-hooks.sh` | Tests of the SD image's `post-flash.sh` in the virtual device's base image (Docker) |
| `sh deploy/fat/test-hook.sh` | Tests of the FAT-card install hook (`deploy/fat/mount-storage.sh.in`, built by `scripts/make-fat-install.sh`, [INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md#install-with-two-files-on-the-official-rocknix-card)) against a fake /flash and /storage with only the initramfs's applets: not grown yet is a no-op, install, third start, second run, another pocket-ui present, damaged or missing tar, STORAGE too small, wrong content, the Xbox core of `make-fat-install.sh --hakux` (with the real files in `POCKETUI_FAT_DIR`), and the H700 device check (five models accepted, the CubeXX and unknown H700 handhelds refused). About 2 s, part of the gate |
| `sh deploy/package/test-package.sh` | Tests of the install package's `install.sh` and `uninstall.sh` and of the SSH-free front end `port-installer.sh` ("Install pocket-ui.sh" in `roms/ports`: launch, checks, install, switch, second run, refusals, uninstall) (`scripts/make-install-zip.sh`, [INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md#install-on-a-rocknix-card-you-already-use)) against a fake root, under dash, bash and BusyBox sh: --check writes nothing, a fresh install, a second install that changes nothing, an upgrade over a dev build, every refusal (another SoC, architecture, ROCKNIX version or device, an H700 handheld that is not one of the table's models or the not-yet CubeXX, no essway, a damaged package, programs that do not run or report another version, pocket-ui's compatibility check finding something missing) with not one file changed, a drop-in systemd does not take, and the uninstall with what pocket-ui changed at run time. Includes `test-power-cut.py`: interruption after each filesystem command, with and without dirty data loss, for fresh installs, upgrades and same-version replacements. Fixture `sync` stays local; the observer models dirty-data loss without flushing the host. Part of the gate |
| `sh deploy/test-grub-quiet.sh` | Tests of the quiet start's script `deploy/grub-quiet.sh` ([BOOT-SPLASH.md](BOOT-SPLASH.md#quiet-start)) against a fake `/flash`, under dash, bash and BusyBox sh: apply, a second apply that changes nothing, other words kept, backup and undo byte for byte, and every refusal with nothing written. About 1 s, part of the gate |
| `sh scripts/image/test-grub-checks.sh` | Tests of how the SD image check reads `grub.cfg` (`scripts/image/grub-checks.sh`): the repository's file passes, though its comment names `fbcon=rotate`; an `echo` line (also after a tab) or an `fbcon=` word on the kernel line is refused; `deploy/grub-quiet.sh` (`boot-quiet.sh`) finds the file already quiet and a loud one not. No Docker. About 1 s, part of the gate |
| `scripts/check-image.sh <img.xz>` | Offline check of a built SD image: partitions, labels, GRUB, kernel and device tree, every file on STORAGE, no developer data ([SD-IMAGE.md](SD-IMAGE.md)) |
| `sh deploy/test-run-pocket-ui.sh` | Tests of the launcher `deploy/run-pocket-ui.sh` and the drop-in with a fake pocket-ui, a fake game and fake `systemctl`: every stage of the [rescue path](ARCHITECTURE.md#rescue-path) (crash restarts, rollback after a failed update, the last known good version, safe mode, EmulationStation only on request or when pocket-ui can't run), the exit codes, the black screen guard, the helpers' time limits, essway mode, the logger for `logs/` (src/logs.rs), Smooth fan's hand-back (clean-up, watchdog, TERM; [FAN-CURVE.md](FAN-CURVE.md#safety)), and ROCKNIX's UI-start steps (20261001's and ROCKNIX main's, [UPDATES.md](UPDATES.md#rocknixs-ui-start)). Needs Linux; elsewhere it re-runs itself in the virtual device's base image (about 45 s). Where `/bin/sh` is dash (Debian, Ubuntu) it runs in bash, the device's `/bin/sh`: dash's `kill` takes no `--`. It clears `LANG` and the other locale variables, as systemd starts the launcher without them |
| `cargo test --release --test picture_shaders` | pocket-core's picture shaders on a headless GLES 3 context (EGL's device platform, Mesa's llvmpipe on a Linux computer; also part of `cargo test`): every variant compiles and links, Sharp and Crisp at whole scales show the pixels as they are, scanlines stay even and keep a flat picture's light at Aspect, the GPU's scanlines match the CPU reference in `src/picture_fx.rs`, the LCD grid and its GBA colours, the CRT mask, Soft glow's halo, Smooth pixels' stair-steps (and flat areas left alone), a 3D core's lines, a picture turned by Screen orientation (both ways, on top of a core's own turn, its scanlines). Without a GL context the tests print SKIPPED and pass; `POCKET_REQUIRE_GL=1` makes that a failure ([HOST.md](HOST.md#picture-effects-2026-10-24)) |
| `cargo test --release --test i18n_snapshots` | The translated interfaces (also part of `cargo test`; the three coverage tests are release checks, `scripts/release-check.sh`): every string of `src/` that reads like words has a line in `assets/i18n/de.tsv` or is listed as a name or as never shown, every state of `tests/i18n-states.txt` rendered in German and Spanish shows no English sentence, and the system language (`LANG`, as the launcher passes on EmulationStation's) decides until Settings → System → Language is chosen. Each snapshot gets its own copy of the made-up data folder. Needs python3; on Linux without a compositor it starts `scripts/wayland-stub.py` itself. See [Translations](#translations) |
| `cargo test --release --test nav_explorer` | The D-pad explorer (also part of `cargo test`, [The navigation explorer](#the-navigation-explorer)): the real binary driven with buttons through every page of Settings, the quick settings, the power menu, SELECT + START's dialog, a game's pause menu, the newer-version card and the first start, at 1240x1080, 640x480 and 1024x768; fails when a drawn row can't be reached, a page can't be left with B, the hint bar names a key that does nothing or leaves out A or B that work, or the selection moves without a visible focus. Each part is its own test, so they run side by side: the gate runs Settings at 1024x768, the overlays at 640x480 and the first start at 1240x1080; `scripts/release-check.sh` the other sizes, Settings in Large text and on an Odin 2 Mini (SM8550) |
| `cargo test --release --test game_check` | Settings → Games → Game check as a player meets it (also part of `cargo test`): cards with the usual mistakes made in a temp folder, `game-check:real` rendered in English at 1280x720 and German at 640x480 Large; what must be on the screen, and no cut text |
| `cargo test --release --test text_size` | Settings → Look → Text size (also part of `cargo test`): every state of `tests/i18n-states.txt` rendered in Medium and in Large, in English and German, with the text painter recording where each string went (`POCKETUI_TEXT_BOXES`); fails on text that runs into other text or a hint bar, touches the string beside it, or is cut by the edge of what it is painted into (or, at the top or bottom, by the picture's own edge) ([DESIGN.md](DESIGN.md#text-size)). A second test reads the size from settings.json and finds it on its Settings row. About 2,200 snapshots; needs python3 as the German test. The `textfit_031_` cases also check the missing-lights note, setup/search headings, controller notes, series headings and complete system names across Barlow and Retro, Normal/Large, EN/DE/ES and 640x480, 1024x768, 1240x1080, 1280x720. The Info regression walks down past long facts and checks that a full description line is actually readable; the navigation regression reaches every Sleep & power row above the hints. The `textfit_brick_031_` cases (`--features brick`) cover the larger Brick minima at 1024x768, Normal/Medium/Large, EN/DE/ES and Barlow/Retro: complete system facts, Recent games, rating/save labels, complete system/year metadata beside Favourite (also NES with and without Favourite, allowing its existing short name when needed), credits, password and Extras explanations, Pixel Path grades clear of their explanation, and measured Performance labels clear of their readings. The Brick settings walk also checks every Sleep & power explanation in Barlow/Retro and Normal/Medium/Large, with complete rows above the existing footer. The `textfit_rest_031_` cases check Tailscale (every state), the first start with the untested-device paragraph and safe mode's rows in Barlow, Retro and Retro's pixel font; the navigation explorer's first-start case walks the welcome rows on the RG DS Plus above the hints, and the safe-mode case selects each row of safe mode (the painters' focus mark, `POCKETUI_NAV_PROBE`) and finds its whole band above the hints. A third test renders every state at 1024x768 in Large and fails on `V`, text the picture's top or bottom edge cuts. A fourth renders every state in the Retro style (`POCKETUI_RETRO=cocoa`), Large German, at 1240x1080 and 640x480, with the same record; a fifth the Retro pause menu, Save/Load, Options, quick settings and pocket-core's notes in Large German and Spanish at 1240x1080, 640x480 and 1024x768 behind a long game name, with the windows' frame lines recorded as areas no text may touch, and checks that the longest names show whole ([RETRO-STYLE.md](RETRO-STYLE.md)) |
| `cargo test --release --test font_styles` | Settings → Look → Style (also part of `cargo test`): the layout-risk states (`tests/layout-risk-states.txt`) rendered at 640x480, Large text, German and Spanish with the Pixel and the Rounded font (`POCKETUI_FONT`), plus four states where a wide font once broke a word, with the same clipping record as `text_size` (no `O`, `T` or `E`) and no name painted with a word broken in pieces (`W`, `i18n::note_broken`: a painter that only counted lines must make the name smaller, `Text::title_px`/`words_fit`); settings.json's `"font"` reaches a start (Barlow when unset or wrong) and a theme's own `title.ttf` wins over the style; a `Text` follows a change of the style by itself, keeps Barlow's capital height and a pinned one (and safe mode's `Text::builtin`) does not move. Needs python3 for the made-up library and `scripts/wayland-stub.py` |
| `cargo test --release --test small_screen` | Small screens (also part of `cargo test`): every state of `tests/i18n-states.txt` rendered at 640x480, the size of the H700 handhelds' screens, where the pages laid out for 1240x1080 must be cut off at the edge instead of slicing past the picture; fails when one does not render. The regression test for the 62 states that aborted there before (`gfx::Canvas::rect`, the game card's body). About 620 snapshots, eight at a time; needs python3 as the German test |
| `cargo test --release --test cjk_snapshots` | Japanese game names (also part of `cargo test`): a made-up library of Super Famicom and Famicom games with Japanese names on a system's list in Split and in Cards (with the jump's big letter), on a game's card and in search, dark and light, Normal and Large text, with ROCKNIX's Noto Sans CJK (`POCKETUI_CJK_FONT`) and without it; fails when the font changes nothing or its names run into other text, touch it or are cut by an edge more than the names as "?" do ([DESIGN.md](DESIGN.md#japanese-chinese-and-korean-names)). Needs the font (`POCKETUI_TEST_CJK_FONT`, else the unpacked ROCKNIX image in `/tmp/rocknix-root-20261001`; without it: `SKIPPED`) and python3 for `scripts/wayland-stub.py` on Linux without a compositor |
| `cargo test --release --test gallery_snapshots` | The screenshot gallery as the player sees it (also part of `cargo test`): the empty page, the page before the folder is read, the list, the picture full screen and a damaged file in both, dark and light, from `pocket-ui --snapshot gallery:…` on the made-up library; fails on a page that shows no words or no picture where the data is missing, or on what Y came to cut short beside the hint bar. Needs python3 |
| `cargo test --release --test controls_card_snapshots` | The controls card as the player sees it (also part of `cargo test`): `pocket-ui --snapshot controls-card:<snes\|gba\|psx\|n64>` at 1240x1080, 640x480 and 1024x768 (including Large text, keeping Control stick and the PSX R3 click readable) shows the buttons and the HOME keys, and each console's card differs; `:snes:remap` differs from the plain one; a console with no names yet (`megadrive`) says so in words; the first start's card has its hint bar; `controls-card-row:snes:chrono` (the game card's Controls row) and `controls-card-pause` (Options > Controls) draw. Needs python3 |
| `cargo test --release --test card_rows` | The game card's rows after a refresh (also part of `cargo test`): `pocket-ui --snapshot curator:card:…` scrolled to the card's end looks the same with and without the refresh a new Curator index (`f`) or the sync conflicts' scan (`c`) makes, so Screenshots and Hide or delete stay on the card. Needs python3 |
| `cargo test --release --test portal_e2e` | The Wi-Fi sign-in helper end to end (also part of `cargo test`): `scripts/portal-standin.py` plays a hotel, a café, a JavaScript-only and a redirect-loop portal on 127.0.0.1, and the real check, session and curl sign in, fail plainly or refuse ([PORTAL-SIGNIN.md](PORTAL-SIGNIN.md#testing)). Needs python3 and curl (else `SKIPPED`) |
| `cargo test --release --test first_day` | Screens with data missing (also part of `cargo test`): on a small library made by the test, Split's room under a selected system's name is never empty, with a fact sheet (Game Boy, Sega CD) or without one (Watara Supervision: Your games), dark and light. Needs python3 for `scripts/wayland-stub.py` on Linux without a compositor |
| `cargo test --release --test settings_open` | The first frame of Settings (START on the shelf) in numbers, also part of `cargo test`: `pocket-ui --snapshot bench-settings` paints the categories page as `draw` does after START, in English and German, five runs each; fails when the page draws a letter of its own, runs a string through the German patterns, or its first frame costs much more than the next ([LATENCY.md](LATENCY.md#settings-first-frame-perfsettings-open-2026-10-24)). Needs python3 for `scripts/wayland-stub.py` on Linux without a compositor |
| `cargo test --release --test stickers_tv` | Case stickers and the Cabinet's TV (also part of `cargo test`): a journey on the made-up library (the card's Stickers sheet, A on Completed, Right and A on Game of the year, B alone changing nothing) checked in `library.json`, then the box, the Cabinet and the Shelf look with and without them (English and German, dark and light, 640x480); the TV showing a made-up screenshot at 1240x1080, 1280x720 and 640x480, nothing of it below the board, none in the Shelf look, and the TV still there for a game with no picture ([DESIGN.md](DESIGN.md#case-stickers-srccase_stickersrs)). Needs python3 |
| `cargo test --release --test brick_lights_row` and `--features brick --test brick_lights_row` | Settings > Sound & feel's lights row (also part of `cargo test`): on the Brick (`POCKETUI_DEVICE=brick`, 1024x768, Large text; English, German, Spanish) the row is named like its page ("Lights & buttons"), its value is "Lights off" and B on the page comes back to the list; on a handheld the row stays "LEDs". Reads `POCKETUI_VISIBLE_TEXT`; needs python3 for `scripts/wayland-stub.py` |
| `cargo test --profile gate --test cozy_extras` | Settings → Look → Cozy extras: D-pad journeys save the extras and GBA console choice, hide/restore the cabinet TV, switch every hardware view to SP across restart and back to classic. Needs python3 |
| `cargo test --release --test disc_prints` | Printed discs (also part of `cargo test`): X on a PlayStation game in the Shelf look with a made-up cover, at 1240x1080, 1280x720 and 640x480, dark and light: the disc out of its case shows the cover's art and the black disc's rim; without a cover a plain print with its name, never an empty disc; the Cozy launch carries the printed disc to the console ([DESIGN.md](DESIGN.md#media-srcmediars-assetsmedia), "Printed discs"). Needs python3 |
| `cargo test --release --test long_titles` | The big name of the selected game (also part of `cargo test`; Brick photo 2026-11-07: "The Legend of Zelda - Ocarina of Time" cut after two lines with room under it): five real long names in Shelf, Cabinet, Cards, Split and Easy at 640x480, 1024x768 and 1240x1080, Barlow and Retro, Normal and Large text, English, German and Spanish (about 2,900 snapshots); fails when the name's painted lines (`POCKETUI_TITLE_BLOCKS`, `text::boxes::note_title`) are not the whole name or one ends in "…", when text runs into other text, a hint bar or an edge (`O`, `T`, `V`, `E`), or when the meta line moves between the games of one system (`title_block`, [DESIGN.md](DESIGN.md#a-games-big-name-srctitle_blockrs)). `POCKETUI_TEST_QUICK=1`: 1024x768, Normal, English only for the shelf matrix. `info_titles_031_` also checks both Details headers with these names and short controls, adding 1280x720, visible words and separation from system/year. Other looks route to those two painters and are checked at Retro/Large/ES. The private gate explicitly repeats this filter with `--features brick`, including Medium; `scripts/test-gate-paths.sh` proves a red result fails the gate. `POCKETUI_NAV_BINARY` can select a saved UI for before/after checks. Needs python3 |
| `cargo test --release --test spine_titles` | Long titles on the spines (also part of `cargo test`): the Shelf and the Cabinet look with long official SNES, GBA and PS2 titles (and a DS case's thin spine) at 640x480 and 1280x720, Normal and Large text, in Barlow, Pixel and Rounded (`POCKETUI_FONT`); fails when a title or its main part is cut short with "…" (`media::spine_fit`: smaller, two lines, without the subtitle, only then "…"; [DESIGN.md](DESIGN.md#media-srcmediars-assetsmedia)). Needs python3 |
| `cargo test --release --test search_titles` | The names in the search results (also part of `cargo test`; Brick photo 2026-11-08: "The Legend of Zelda: Link's Aw…", "…: Ocarina of Time 3D" cut beside the system plaque): the five long names of `long_titles` plus the Zelda games Ocarina of Time (N64, 3DS) and Link's Awakening (Game Boy, Game Boy Color, DX) found by their query (and typing over the keyboard), at 640x480, 1024x768, 1240x1080 and 1280x720, Barlow and Retro, Normal and Large text, English, German and Spanish (`title_block::row`); fails when a painted name ends in "…", when a game shows neither its whole name nor the end of it that tells it apart ("DX", "3D"), when the two Link's Awakening rows read alike, or when text runs into the plaque or an edge (`O`, `T`, `V`, `E`). `POCKETUI_TEST_QUICK=1`: 1024x768, Normal, English only; `POCKETUI_NAV_BINARY` runs another pocket-ui (the proof that it fails before the fix); with `--features brick` the Brick's own start; the private gate repeats it with `--features brick`. Needs python3 |
| `cargo test --release --test spine_names` | The names on the spines (also part of `cargo test`; Brick photo 2026-11-07: "Conker's B…", "Mario Kar…" three times, "The Legend…" twice): real Nintendo 64, SNES, GBA and DS names on their shelves in the Shelf and the Cabinet look at 640x480, 1024x768, 1240x1080 and 1280x720, Normal and Large text, Barlow and Retro (`POCKETUI_SPINE_LINES`, `media::note_spine`); fails when a spine ends in "…" (on the Game Boy Advance's and DS's 90-pixel labels at 1024x768 a cut is allowed), is under 16 px, or reads like another spine of its shelf; with `--features brick` also the Brick's own start (`POCKETUI_BRICK_STANDALONE=1`). `POCKETUI_TEST_QUICK=1`: 1024x768, Normal only; `POCKETUI_TEST_BIN` runs another pocket-ui (the proof that the test fails before a fix). Needs python3 |
| `cargo test --release --test shelf_moves` | A move on Home in numbers, also part of `cargo test`: `pocket-ui --snapshot bench-shelf:<look>` moves over the whole list of the made-up Curator library in every look (German, Split also English) and measures the first frame after each move; fails when frames draw letters of their own or run strings through the German patterns, or a move costs more than 6 ms (p50) / 8 ms (p95) of thread CPU time (the best of three runs counts, so a busy build computer doesn't fail it; [LATENCY.md](LATENCY.md#a-move-on-home-perfnav-regression-2026-10-26)). Needs python3 for `scripts/wayland-stub.py` on Linux without a compositor |
| `scripts/test-check-core-licences.sh` | Tests of `scripts/check-core-licences.sh` on fixture trees (clean, GPL-2.0-only, mixed, unlisted core, bad table, `HANDHELD_CORES` missing = exit 2) and on the real tree (every hosted core is in `scripts/core-licences.txt`). About 2 s, part of the gate |
| `scripts/check-platforms.sh [deck] [android]` | Compile checks of the Steam Deck build's code (`cargo check --features x11`) and of the Android crates (`--target aarch64-linux-android`); the gate runs both after the tests ("Merge gate") |
| `scripts/check-leaks.sh [worktree]` | The export's private-data scan on the tracked files (prune, scrub, check; no build, no history), the first step of `scripts/gate.sh`: prints `FINDING<TAB>category<TAB>file:line` and exits 1, or exits 0. About 5 s. See "Merge gate" |
| `scripts/test-verify-device.sh` | Tests of `scripts/verify-device.sh` against a fake device (fake `ssh`, `/proc`, `systemctl`, `pidof`, `wpctl`): a healthy device, a second pocket-ui outside the unit, the trace and history variables, a stale or missing binary, a running pocket-core, no drop-in, ssh down; and the restore step of `latency.sh`. About 3 s, part of the gate |
| `scripts/monkey.py --out DIR [--seed N --minutes M --look L --lang en\|de --text T --library demo\|empty\|big\|weird\|emptyfiles --profile miniv2\|rg35xx-plus\|odin\|steamdeck --data ok\|damaged\|readonly\|nonet --touch PCT --steps N --replay F --minimise]` (`scripts/test-monkey.sh`: its own quick self-test) | Monkey test: seeded random button and tap presses through `pocket-ui --test-driver` (messy demo library or the empty starter state), at nice 19, no device. Finds panics, hangs (no frame in 10 s; on a loaded host they are load, check the load printed), empty frames, stuck states (B and the pause menu's Quit must get back to Home), error-looking text, text-box overlaps (pictures of the first ones are kept), RSS growth and idle CPU; every press goes to `DIR/<tag>.cmds`, `--replay` sends one back and `--minimise` bisects its shortest failing prefix. Expect noise from the host: Wi-Fi and restart toasts without `nmcli`/systemd, overlays of toasts on pages (an `O` while a toast is up), the power-off row's exit |
| `scripts/size-sweep.sh [out]` | Every state of `tests/i18n-states.txt` at other screen sizes (`SIZES="1920x1080 1280x960"`), dark and light, Normal/Medium/Large (`TEXTS`), English (`LANGS="en de"`): per combination the painter's findings (`.problems`: text running into text or the hints, cut by an edge, cut short, a word broken inside, a column of one- and two-character strings such as a price wrapped letter by letter, text painted under 14 px; `POCKETUI_LAYOUT_AUDIT`) and the pictures' checksums (`.md5`; `KEEP=1` keeps the pictures; `KEEP=problems` keeps just the pictures with findings and their `.box`, `.audit` and `.tsv` records), in `/tmp/pocketui-size-sweep` by default ([DEVICES.md](DEVICES.md#layouts)) |
| `scripts/i18n-shots.sh [out]` | Every state of `tests/i18n-states.txt` in German, dark and light, as contact sheets (JPEG, four to a sheet, `screenshots/i18n-de/` by default), and the strings cut short or run past the edge only in German (`LOOKS=dark`, `TILE=3x3`, `STATES="settings;safe rescue:1"` to narrow it). Needs ImageMagick |
| `scripts/test-rocknix-diff.sh` | Tests of `scripts/rocknix-diff.sh` on two made-up system folders (below) |
| `scripts/test-rocknix-contract.sh` | Tests of `scripts/rocknix-contract.sh` on a tiny made-up root, a row of every kind ok and broken, `--host` through a stand-in for ssh and `--ref` on a made-up git tree; about a second, also run by `cargo test` (`os_compat::tests`) |
| `scripts/latency.sh [--compare]` | The speed guard: start, shelf moves, Settings, pause menu, switcher, game start, quit and switch, idle CPU, in the virtual device (or on a Mini V2 with `POCKETUI_HOST`). Table plus `latency.json`; `--compare` exits 1 when something is more than 15 % slower than the baseline in `docs/latency/`; `--wake` measures the wake from deep and light sleep instead ([LATENCY.md](LATENCY.md#waking)). See [LATENCY.md](LATENCY.md) |
| `scripts/test-latency.sh [--vdev]` | Tests of the speed guard's statistics, comparison and trace parser; with `--vdev` also a short run in the virtual device |
| `scripts/test-big-emu-bench.sh` | Tests of `scripts/big-emu-bench.sh` and its evaluation `big-emu-bench.py` without a device: the verdict limits, then the device half against a fake `/sys`, `runemu.sh` and emulator (about 1 min) |
| `scripts/rocknix-compare.sh run` | pocket-ui against stock ROCKNIX (EmulationStation + RetroArch) on a Mini V2, from outside: presses into the pad, the screen recorded from sway; cold start, list move, game start, pause menu, quit, switch, wake, CPU, battery power. Puts the device back and runs `verify-device.sh`. See [COMPARE-ROCKNIX.md](COMPARE-ROCKNIX.md) |
| `scripts/test-rocknix-compare.sh [--sim]` | Tests of the comparison: the Wayland recorder against a fake compositor, the analysis on made-up recordings and a synthetic video, the device script's restore in a made-up root (also after SIGTERM); `--sim` also both UIs through every scenario on a simulated Pocket and the side-by-side video cut from it, checked to line up within a frame (about 10 min) |
| `scripts/idle-cpu.sh [seconds [max %]]` | In the virtual device (after `vdev.sh deploy`): starts a game and measures pocket-ui's CPU and wake-ups per thread while it runs behind it; fails above 1 %. It should sleep while a game runs |

A unit test that writes a fake program for the code to run (a stand-in `curl`, `bluetoothctl`, ROCKNIX script ...)
writes it with `testutil::write_executable`, not `std::fs::write` + `set_permissions`: a file this multi-threaded test
process ever held open for writing can fail to start with "Text file busy" when another test forks at that moment.

A test that asserts *how long* something took measures the calling thread's CPU time
(`run_ahead::thread_cpu_ms()`), never the wall clock: the build machine runs other builds next to the gate, and a thread
that was paused for 95 ms says nothing about the code (2026-10-28: `timeline`, `control`, `power` had such assertions
and failed in gates). Where the point is that something did or did not *happen* — a lease given back, a hung helper cut
off, the menu's list written, a heartbeat that moves — wait for that event (a loop that ends as soon as it holds, with a
generous bound only as a guard against a hang) instead of sleeping first, and assert on the event (a marker file the
program would have written, the error kind, a counter the writer reports). Two things that bit us and are neither:
`flock` is released only when the last copy of the descriptor is gone, so a child another test thread just forked can
hold a lease for a moment (`Lease::take` waits for it, as the games do), and a helper's output is read until end-of-file
once its process group is gone (`proc::Pipe::take`), not for a fixed 100 ms.

A test that needs a folder takes it from `testutil::TestDir::new("label")` (unit tests; `tempfile::tempdir()` in `tests/`,
`tempfile` is a dev-dependency only, so it is not in the shipped binaries), never from `std::env::temp_dir().join(..)` +
`create_dir_all`: the folder is unique, it derefs to a `Path` (`dir.to_path_buf()` where an owned `PathBuf` is wanted),
and it is removed with everything in it when the value is dropped, also when the test panics. Keep the value alive for
as long as the test uses the folder (a helper that returns the path alone would delete it on return: return the
`TestDir` or keep it in the struct the test holds). Never `keep()` it or `mem::forget` it. Folders of fixed names in
/tmp piled up to tens of thousands on the build machine before this. The shell tests do the same with `mktemp -d` and
`trap 'rm -rf "$tmp"' EXIT`.

### Launch, Continue and update regressions

`tests/user_journeys.rs` drives the real app with buttons through confirmed failed starts, crashes after a first
picture, retries and a second explicit launch. Existing moments must keep their emulator route. The H700 GBA flow
checks bundled gpSP for a fresh game and mGBA for an existing mGBA Continue; it does not measure game speed.
The folder-correction flow follows Game check's remedy and discovers the moved game without restarting.
`libretro`'s clock regressions cover Continue, loading an old slot and Start over using an injected wall clock or
cartridge RTC memory; the game resets while its cartridge clock persists.

`tests/ota_previous_release.rs` runs the actual installer against layouts extracted from the public 0.2.5 and 0.3.0
ROCKNIX card assets, then starts the selected installed app twice and checks settings, games, Continue, slots and
battery/RTC files byte for byte. `scripts/public-update-layout.py ASSET.zip…` regenerates the small fixtures after
checking the adjacent published SHA256SUMS. ARM programs and other old payloads are marker files in this host test;
it proves layout/data compatibility, not execution of the old ARM release or the Settings update-button path.

Per release, also play GB/GBC, GBA, SNES, MD and PS1 for at least 60 seconds on each shipping device family with
the default core and settings. Keep measured fps, late frames, audio underruns, governor/frequency and temperature.
An empty measurement window is missing evidence. A core-routing host test cannot replace this hardware smoke.

### The navigation explorer

`tests/nav_explorer.rs` operates pocket-ui as a player does: buttons only, through the real binary (`--test-driver`), and
after every press it reads what the frame offered instead of looking at pixels. A frame's offer is recorded by
`src/nav_probe.rs` when `POCKETUI_NAV_PROBE` names a file: every string with its place, `‹nav:row›` where a row or control
a player can operate is drawn, `‹nav:focus›` where the focus mark is, `‹nav:hint›` for each hint the bar shows; the
binary adds where the App is (`@where`, `src/app_nav_probe.rs`: "settings:List Look", "setup:look (settings)",
"pause:Options", what covers it after " / "), the list it registered for touch (`@list`, `@hit`), a fingerprint of the
picture, `@busy` while a Wi-Fi list scan or cartridge lookup is pending, and `@end`. The explorer waits for both a
visually stable frame and a completed Wi-Fi list scan. Other loading screens remain observable, including the
remote-access regression that deliberately holds its reply until after inspecting the initial frame. Lookup journeys wait until the UI has applied the worker
result and the refreshed library has reached the shelf before continuing. Game-check journeys also wait for
the independently refreshed library before opening the corrected game list. The phone-upload page harness drains
its pending XHR and FileReader callbacks; a quiet upload counter does not mean those jobs completed. The FIFO command `nav` writes the record without a PNG.

The shared painters declare themselves (`kit::selection`, `kit::row_label`, `kit::shelf_row`, `retro::select`,
`features::row_back`, the first start's pill rows, both hint bars), and the rows a list registers for touch count as rows.
**A page that draws a row or its focus by hand calls `nav_probe::row` or `nav_probe::focus` there**, or the explorer
reports its rows as unreachable or its focus as invisible.

From each entry (START on the shelf; HOME; the power menu; SELECT + START; a game's pause menu; the newer-version card;
the welcome of a fresh card) it walks a page with Down and Up (and, where rows remain, every direction from every focus,
for grids such as Add games), then on each row presses A and every key the hints name, and once per page the keys they
don't name. A page of its own behind A (or behind another key: X "Style…", the first start's B "Continue offline") is
explored the same way, one level deeper, and B from it must come back to the page and row it was opened from. Keys that
change something are undone (B, the same key round, the opposite direction) or the UI starts again on a fresh copy of
the card and replays the buttons. A test fails on:

- a row drawn on a page that Up/Down (or the grid walk) never focuses;
- the list's selected row without a focus mark on it, or a selection that moves while the picture stays the same;
- B that doesn't lead back to the page and row a page was opened from (two presses for a dialog first), or a page B
  leaves whose bar doesn't name B;
- a hint whose key does nothing (anything on screen changing for a moment counts, "Looking…"), A that works without an A
  hint.

Keys other than A and B that work without a hint are listed in the report, not failed (the bar shows four hints at most).
Not pressed: rows that restart or quit pocket-ui, start a game or erase something (`NOT_PRESSED`). Test buttons takes
every button itself (`OWN_KEYS`); its hold to leave is a journey. The shelf is reached, not explored
(`tests/cabinet_navigation.rs`). Network programs and anything that could switch the computer off are stand-ins on PATH.
`POCKETUI_NAV_REPORT=<dir>` writes each part's pages, notes and failures there. A Brick build (`--features brick`) runs the
three parts on the Brick (`POCKETUI_DEVICE=brick`, its own card) and leaves the ROCKNIX sizes to main's; the merge gate runs the same
three on the Smart Pro (`POCKETUI_DEVICE=smartpro`, 1280x720, filter `on_the_smart_pro`, [TRIMUI-SMART-PRO.md](TRIMUI-SMART-PRO.md)).
`POCKETUI_NAV_REPORT=<dir>` writes each part's pages, notes and failures there.
`POCKETUI_NAV_BINARY=<path>` replays the same cases against a saved executable, for before/after regression proof.
When rechecking a key that seemed to work without a hint, the explorer uses the settled page's hint bar: an asynchronous
read (Remote access's service state) may have made the key available since the original frame. A barrier-controlled
remote-service regression covers that transition without depending on a wall-clock race. A Brick build adds its profile under
`cfg(feature = "brick")`.

The merge gate also runs short device regressions in `nav_explorer::rc03_*` on H700/640×480, Mini V2 and Odin 2 Mini:
saved-network menus during first start (with scripted NetworkManager output), Add games with a full popular-system
catalog, editable versus OS-owned/no-fan values, Starter pack directions, complete Retro help text and the active
colour of an available Load row. The profile selects the real device capabilities and logical canvas size; the
small-card fixture alone cannot reproduce a multi-row grid. A menu is named as a cover in `@where`, so returning
from it can be checked independently of returning from its page.

### Documentation checks

The user guide is also the manual on the device, and the features keep changing it, so `cargo test` checks the
documents against the code (`src/doc_checks.rs`, a test module of the `pocket-ui` binary; no network, milliseconds).
Each check compares names and structure, not wording:

Since 2026-11-04 only the checks that guard a player's way stay in the merge gate. The bookkeeping ones (every source
string has a German line, the German and Spanish snapshot matrix, every environment variable and source module
documented, the Settings paths in prose, each category's line naming its rows in order, the manual's page length) are
`#[ignore = "release check: ..."]` tests that `sh scripts/release-check.sh` runs before a release. When one is red,
`scripts/doc-missing.py` lists the undocumented environment variables and source modules, and
`scripts/i18n-extract.py --stub` the missing German lines; descriptions and translations are written by hand.

| Check | What has to match |
|---|---|
| Settings list | The table under "Open Settings" in `docs/user/settings.md` has one row per category of `SETTINGS_GROUPS` (`src/main.rs`), with its name in bold, in the same order, and names each of its entries; the first table under each category's own heading has one row per entry of that category, in bold, in its order; together they are `SETTINGS`. Settings is two pages (the manual's 30 000 px limit): `settings.md` holds Look, Sound & feel, Playing and Games, `settings-system.md` Connections, System and Help & tools, and the check reads both |
| Settings → Games → Curator, Performance & fan | The tables in `settings.md` list the rows of `app_curator::SETTINGS_ROWS` and `perf_fan_page::ROWS` in order, with their start values (`Prefs::default`) |
| Looks, Appearance, Motion, On start, Screen frames | The Looks table in `playing.md` lists `Theme::ALL` in order; `settings.md` names every choice of Appearance, Motion, On start and Screen frames in bold; no page says "four looks" when there are five |
| Pause menu | Every `MainItem` label is in bold in `docs/user/pause-menu.md`, and its table "Menu, top to bottom" gives exactly what `main_items` builds for pocket-core (signed in or not, with and without save states), PS2/GameCube/Wii/3DS, RetroArch and other emulators. A new `MainItem` has to be added to the check's list (a `match` there stops compiling) |
| Options | `docs/user/options.md` names every group and setting of `options::GROUPS` (in bold or as a heading) and every choice label; the Extras table marks each start value in bold; the Keep for choices (This game, This console) and Restore defaults are there |
| Lens | `curator.md` ("Lens") names every `curator::Facet` |
| Feature registry | Every entry of `features::FEATURES` (also the TODO ones) points at a page of `docs/user/` and a heading there (`guide`); the table in `tour.md` lists every switch card in order with what Simple and Enthusiast do with it, and the page names every card without a switch in bold |
| Manual pages | Every file of `docs/user/` is in `manual::PAGES` or `manual::DESKTOP_ONLY`, and the "Pages" list of `index.md` links the manual's pages in their order |
| Documentation index | `docs/README.md` links every document of `docs/` and `docs/user/` |
| Status ledger | Every feature row of [STATUS.md](STATUS.md) has one of its five statuses (device-measured, device smoke-checked, virtual device only, host tests only, not built) and says what is open; the file holds no IP or MAC address, mail address or tailnet name |
| Module list | "Repository layout" above names every file and folder of `src/` (`foo.rs` as itself, not as part of `app_foo.rs`), and the binaries table every `src/bin/*.rs` |
| Settings paths | No string of `src/` (comments aside) and neither side of `assets/i18n/de.tsv` writes a path in Settings with ">": it is "Settings › Playing › Battery" ([i18n-glossary.md](i18n-glossary.md)) |
| Environment variables | Every `"POCKET…"` string literal in `src/` that names a variable is in [Environment variables](#environment-variables) |
| Links | Every relative link in the Markdown of the root, `docs/` and `assets/` points at a file that exists, and every `#anchor` at a heading there (GitHub's anchor rules); `pocket-ui:<name>` (a picture the manual draws, `manual::PICTURES`) names one it knows |

When one fails, the message says which file and what is missing: change the document (or the code), not the check.

**The status ledger.** [STATUS.md](STATUS.md) is the one place that says how far each feature has been proven (on the
Mini V2, in the virtual device, in tests only). The rule: whoever changes a feature's verification state updates its
row in the same commit, and any other Status note in `docs/` or `docs/user/` that makes a claim about the device has to
agree with the row (or link to it). A new feature gets a row; a device run moves its row up and writes what was done
(date, commit, what exactly); a regression moves it down. A smoke check is not an endurance test: only call a row
device-verified when a note says it was run on the Mini V2. Keep private data (addresses, serials, tailnet names) out.
The manual's own tests in `src/manual.rs` add that every page parses cleanly, has a glyph for every character and
lays out within the screen's width and under 30 000 pixels (about 30 screens); split a page that grows past it.

Before you send a change, run `cargo test` (the debug profile: overflow checks on; it must be green too, not only
`--release`, review N6) and `scripts/test-linux.sh`, and `sh deploy/test-run-pocket-ui.sh` if
you touched the launcher. For a change to the shelf, the menus, game start or quit, also run
`scripts/latency.sh --compare` (about six minutes in the virtual device). `cargo fmt` uses `rustfmt.toml` (long lines are allowed).

Layouts can be checked without a device by rendering them to a PNG:

```sh
POCKETUI_MOCK=1 POCKETUI_DATA=/tmp/pocketui-data \
  cargo run --bin pocket-ui -- --snapshot main out.png
```

A snapshot opens no window, but winit's event loop wants a Wayland compositor first. On a Linux computer without one
(a build machine, a terminal over SSH), start `python3 -I scripts/wayland-stub.py /tmp/wayland-stub &` and set
`WAYLAND_DISPLAY=/tmp/wayland-stub`: the stub only answers the hello and shows nothing. `POCKETUI_LANG=de` paints a
snapshot in German ([Translations](#translations)), `POCKETUI_TEXT_SIZE=medium` or `large` with Medium or Large text
([DESIGN.md](DESIGN.md#text-size)). `POCKETUI_SNAPSHOT_SIZE=1280x960` paints it at another screen size
(the layouts are made for the Mini V2's 1240x1080; this only shows how they take another one).
`scripts/readme-shots.sh` renders the pictures of the README and `docs/media/` this way, with the made-up library.
`scripts/manual-pictures.py <binary>` renders the manual's screenshots (`docs/user/pictures/`, 640x480 WebP) with real
box art of official games; the device's manual leaves a picture on a line of its own out (`manual::parse`).

`--snapshot` takes a state name. They are parsed in `fn snapshot` (`src/main.rs`) and in the `*_snapshot` functions of
the `app_*.rs` files, whose comments give the details:

| States | Screen |
|---|---|
| `main`, `menu:<pocket-core\|ps2\|retroarch\|other\|scummvm\|ps2-starting\|xbox\|wiiu\|ps3\|psvita>[:<row>][:forget][:battery=<game\|system\|all\|measuring\|charging>][:disc=<in>/<count>][:save=<full\|unwritable>]`, `game-quick[:<row>][:own]` | The pause menu of that kind of emulator (`scummvm`: pocket-core without save states, `ps2-starting`: ARMSX2 before it named the game, `xbox`: hakuX while its core has no save states, each with its line instead of Save and Load; `:forget`: with Settings → Playing → Continue where I quit off; `:battery=`: the battery line at the title's right; `:disc=1/3`: a PlayStation game with its Disc row selected, `?/3` when the core names no disc, `app_disc.rs`; `:save=full`: the game's saves don't reach the card, the line under the title, `app_save_trouble.rs`); the quick settings opened from it (a pocket-core game; `:own`: it has its own Performance, Max, so the row says "this game: Max"; row 4 is Performance, 5 Fan, 6 Power…) |
| `quit-unsaved[:<row>][:unwritable]`, `game-save-warning[:unwritable] [<game.png>]` | The question before Quit while the game's saves fail (row 0 Keep playing, 1 Quit anyway); pocket-core's line over the game then (`overlay::paint_pill` at the bottom of the picture given, `app_save_trouble.rs`) |
| `slot-save`, `slot-load`, `slot-auto`, `slot-undo`, `slot-empty`, `slot-one`, `slot-earlier[:start\|:none\|:damaged]` | Save and Load with the slots on the game's save medium, Auto, Before load, an empty slot, the 3DS's one slot; Earlier selected (`app_earlier.rs`, `[picture.png]` as its picture): on the state from 12 min ago, on a session start, with none yet, on a damaged one |
| `save-card:<ps1\|ps2\|gamecube\|vmu\|xbox\|stick\|<cartridge system>>` | One memory card (or a cartridge) alone on grey, large and at the pause menu's size, to compare with a photo of the real one (`app_save_media.rs`) |
| `slot-<save\|load\|auto\|undo\|empty>:<system>[:nocover][:card]`, `info:…:saves[=none]`, `box:<system>:<name>:saves[=none][:end]` | The same slot page for a made-up game of that system, its slim row in the save medium's plastic (`:card`: the medium itself beside its blocks, as with Memory card in the pause menu on; `psx`, `ps2`, `gamecube`, `dreamcast`: memory card; `snes`, `gba`, `megadrive`, `nds`: cartridge; `ports`: a plain row), with a made-up cover written to the data folder (`:nocover`: the console's picture); a card's info page or box back with save slots 1, 3 and 6 two days old (`=none`: all empty; `:end`: the back scrolled to "Your game"; `app_save_media.rs`) |
| `options`, `picture[:<row>][:left\|:right]`, `picture-gba[:<row>][:left\|:right]`, `picture-snes[:<row>]`, `picture-psx[:<row>]`, `picture-custom[:<row>]` (a GBA game: the LCD grid's colours row; a SNES game, whose grid shows as Custom; a PlayStation game as it starts, Look Clean; one with the Look turned to Custom, its Sharpness and Effect open; `:left`, `:right`: Screen orientation turned, its row is the last; a raw game frame as the fourth argument stands in for pocket-core's preview, turned likewise), `shortcuts[:<row>]`, `extras[:<row>][:<off\|fps\|sensors\|pixel>]`, `emulator[:<row>]`, `category:<cat>:<row>`, `controls[:<row>]`, `cheats[:<row>]`, `cheats-more[:<row>]` (made-up cht files: cheats with codes and two entries pocket-core can't apply), `cheats-only` (only such entries) | The Options pages of a pocket-core game; `extras:3:sensors` has Over the game on FPS + sensors (any level's value works), `extras:2` is Earlier states |
| `ps2-options`, `gc-options`, `wii-options`, `3ds-options`, `gc-more`, `wii-more` | The Options of PS2, GameCube, Wii and 3DS |
| `notice:<several\|password\|packing\|checksum\|incomplete\|empty\|none\|bios\|bios-segacd>`, `notice:patch-<version\|damaged\|big\|kind>` | Home with the card a game that didn't start leaves: what pocket-core says about a zip it refuses (`archive::message`), about a patch next to the game it refuses (`softpatch::message`), or the safety net's BIOS line (`bios-segacd`: naming the BIOS files a Sega CD game lacks) |
| `achievements-menu`, `achievements-list:<row>`, `achievements[:in\|:user\|:password\|:signing\|:error\|:confirm]` | The pause menu with Achievements, its list, Settings → Playing → RetroAchievements |
| `pixel-path:…`, `pixel-path-menu[:full]` | Pixel Path (below) |
| `players:<case>[:<row>]`, `players-menu[:full]` | The Players page and the pause menu with Players (below) |
| `settings[:<category>]`, `settings:<category>:<row>`, `about`, `battery[:<range>][:samples]`, `sleep[:<row>][:screen][:never][:old]`, `manual[:<page>[:<pixels>]]` | Settings' categories with that one selected (0 = Look … 6 = Help & tools), a category's list at that row (from 0: `settings:2:6` is Playing → Battery); the rows counted are the shown ones, so Games has no Store row unless POCKETUI_DATA's settings.json has a `store_url`, About, Battery (`:samples` writes six made-up hours of history into POCKETUI_DATA first, with `POCKETUI_BATTERY` a battery), Settings → System → Sleep & power (`App::sleep_power_snapshot`: that row selected; `screen` with Short press on Screen off and Hold on Sleep, `never` with both times on Never, `old` with the 3 min of a file from before the page), a page of the manual (0 = the first) scrolled down |
| `updates:note`, `updates:refused` | Settings → System → Updates after "Get system": staged with a note of what the new system lacks, refused because pocket-ui would break ([UPDATES.md](UPDATES.md#os-what-pocket-ui-needs-from-the-new-system)) |
| `buttons[:<row>][:<presses>]`, `setup:look-systems` | Settings → Playing → Buttons (Button layout, Button symbols, the picture of the four buttons), and Systems on the shelf opened from its own row (B goes back to the Settings list) |
| `setup:extras[:<keys>]` | Settings → Games → Pictures & extras (Cover art, Pictures, PortMaster games; keys `d`, `u`, `a` as for the other `setup:` steps) |
| `gallery[:<what>][:<row>]` | Settings → Games → Screenshots with made-up screenshots (ten PNGs over five weeks, written to `<POCKETUI_DATA>/gallery-made-up`, today is 24 October 2026; `App::gallery_snapshot`): the list (`:<row>` selects another), `full` the picture alone, `delete` and `full-delete` the question X asks, `broken` and `broken-full` with two damaged files on top, `empty`, `loading` (the folder not read yet), `game` and `game-full` one game's own list (opened from its card), `card` the game's card on its "Screenshots (N)" row, `shelf` and `full-shelf` the list and the picture with what Y says |
| `gamelist[:<row>][:none\|:all\|:done]` | Settings → Games → From EmulationStation for the gamelists in the made-up library's `roms/gba/gamelist.xml` (`scripts/curator-fake-library.py`; or any `POCKETUI_ES_SYSTEMS` whose ROM folders hold one): `none` as if there were none, `all` after favorites and ratings were taken over, `done` after A on the row. `curator:card:gba:advance` is the card of a game the gamelist describes (`App::gamelist_snapshot`) |
| `controls-card[:<system>][:remap][:first][:off][:zoom]`, `controls-card-row:<system>:<game>`, `controls-card-pause` | The controls card ([controls-card.md](user/controls-card.md)): over the shelf for the first game of that console (`remap`: the Pocket's X presses A and Y nothing, as saved; `first`: the first start's hint; `off`: HOME + R1 switched off; `zoom`: HOME + X switched on), the game's card on its Controls row, and Options > Controls > Controls card of the pause menu (`App::controls_card_snapshot`) |
| `game-check[:<row>][:running\|:offer\|:green\|:none\|:failed]`, `game-check:group:<row>`, `game-check:detail:<row>`, `game-check:real[:<row>]` | Settings → Games → Game check with a made-up result: its groups, a running check, the offer after games were added, the green result, no games, a system list that can't be read; a group's games, a game's detail; `real` runs the check on the library of `POCKETUI_ES_SYSTEMS` (`tests/game_check.rs`) |
| `tools[:…]`, `pc-streaming[:…]`, `backup[:…]`, `setup:wifi[:<keys>]` (`setup:wifi-off`, `-switch-failed`, `-save-failed`, `-scan-failed`, `-empty`, `-switching`: the radio off, a switch or scan that failed, no network in reach, [SYSTEM-PAGES.md](SYSTEM-PAGES.md#snapshots); `setup:wifi-portal`: the same on a network that wants a sign-in, `net_check`; `setup:wifi-portal-sheet[:<keys>]` its sign-in sheet, `-nomac` without the Wi-Fi address, `setup:wifi-portal-signin=<what>[:<keys>]` its "Sign in…" with a made-up page or answer, [PORTAL-SIGNIN.md](PORTAL-SIGNIN.md)) | Settings → Help & tools → Tools, PC streaming, Backup & reset and Wi-Fi with made-up data ([SYSTEM-PAGES.md](SYSTEM-PAGES.md#snapshots)) |
| `storage[:<row>][:confirm\|:health-wait\|:health-errors\|:health-readonly\|:health-unknown\|:low\|:counting\|:updating\|:systems\|:hidden\|:hidden-empty\|:done\|:unreadable]`, `card-manage:<system>:<name>[:row\|:sheet\|:delete\|:armed\|:saves\|:working\|:error]`, `storage-notice` | Settings → System → Storage with a made-up 64 GB card (row 0 = Free on the card, 10 = Card check; `health-*`: the Card check found nothing yet, errors, read-only, nothing readable; `confirm`: the second-A question of that row; `low`: 812 MB free), its lists; a game's card scrolled to Hide or delete, its two choices, the delete sheet with made-up sizes (`saves`: saves and states too, with Sync on), before the sizes are in, for a game that is gone; Home with the low-space notice (`app_storage.rs`) |
| `leave[:<choice>]`, `about-es[:<choice>]` | The dialog before EmulationStation: over Home (SELECT + START held), over Settings → Help & tools → About (`app_rescue.rs`) |
| `report[:writing\|:saved\|:full\|:readonly\|:es]` | Settings → Help & tools → About with Report a problem selected (`:es`: Open EmulationStation… selected) and its sheet: while the report is written, saved, the SD card full or read-only (`app_report.rs`; nothing is written) |
| `--safe --snapshot rescue[:<row>][:reset\|:es[:<choice>]\|:done]` | Safe mode's rescue page with a made-up reason and crash, a row selected, the reset or EmulationStation dialog open, or after a reset (`app_rescue::snapshot`) |
| `conflicts:<card\|choose\|notice\|none>[:<system>][:<row>]`, `sync:conflicts[-list\|-card]` | A save with two versions, made up for the first game of `<system>` (`gba` by default; `ps2` is the memory card every PS2 game shares): the game's card with its row, the choice over it (that row selected), Home with the one-time notice, the list with nothing waiting; Settings → Sync with two saves waiting, their list, the memory card's versions opened from it (`App::conflicts_snapshot`, `App::sync_conflicts_snapshot`) |
| `sync:<off\|on\|busy\|waiting\|syncing\|open\|bios\|library>`, `bluetooth:<off\|empty\|search\|found\|pairing\|connected\|error\|forget>`, `tailscale[:off\|:loggedout\|:login\|:connected\|:busy\|:key\|:name\|:error\|:offline\|:offline-loggedout\|:offline-note]`, `web[:off\|:none\|:waiting\|:busy\|:pair\|:browsers\|:revoke][:<row>]`, `leds:<off\|colour\|colour-game\|battery>:<row>`, `night[:<off\|on\|scheduled>][:<row>][:<strength 1-5>]`, `perf[:<row>][:waiting]`, `store[:list\|:detail…]` | Settings pages with made-up data |
| `home[:<row>]` | Home in the look of `POCKETUI_DATA`'s settings with its last row selected, or that row (`home:0` the first, `home:-2` the second to last) |
| `switcher` | The game switcher on the shelf, with the recent games of `POCKETUI_DATA`'s `recents.json` (`POCKETUI_ES_SYSTEMS` for the library they are named from: two versions of a game show the system list's names) |
| `menu:port[:<row>]`, `menu:port-perf` | The pause menu of a running PortMaster port (Celeste Classic) and its Options page, Performance & fan, over the paused game (`App::port_perf_in_menu`); `setup:ports-update` is the PortMaster browser with two made-up updates |
| `port-perf[:<row>][:busy][:own]` | A port's Performance & fan (START on its card) for a made-up Deltarune: `busy` with the offer of Battery, `own` with a choice of its own (`app_port_perf.rs`) |
| `setup:<look\|wifi\|games\|first\|first-wifi\|first-games\|first-look\|play\|card=<id>\|ports\|ports-shared>[:<keys>]`, `port-files[…]` | The first start (`first`: its language page; `first-wifi` the Wi-Fi stage with made-up networks and the Time zone row from the guess, `first-games` "Your games", `first-look` the last stage, `first-look:dddr` its rows: Style, Colours, Appearance changed), Settings' setup pages ("How you like to play" is `play`, a card of Choose yourself `card=<feature id>`) and the PortMaster pages (below); keys `r` `l` `u` `d` `a` `b` `x` `y` `s` (START), `R` (R1) |
| `test-buttons[:<one\|bt\|two\|remap\|xbox\|hold\|unknown\|empty>]` | Settings → Help & tools → Test buttons with made-up controllers pressed through the real tester: the Pocket alone, a Pro 2 with its triggers as axes, two controllers, a player's remap with the explanation, the Xbox Button layout, HOME held for a second, an input pocket-ui doesn't know, no controller (`App::test_buttons_snapshot`) |
| `tour[:<basic id>[:<keys>]]`, `tour-new:<id>[,<id>…]` | Settings → Help & tools → Tour at that card of `features::BASICS` (`play`, `continue`, `saving`, `quick`, `add_games`, `settings`; keys `d` `u` `a` `b`, `r`/`l` for R1/L1), "New in this version" with those feature cards (`app_tour.rs`) |
| `sys:<quick\|quick-muted\|power\|power-off\|restart\|time\|…>` | Quick settings over the shelf (`quick:<row>`; 4 Performance, 5 Fan, 6 Power…; `quick-muted`: Volume with the output muted at 5 %; `quick-headphones`, `quick-bluetooth`, `quick-bluetooth-long`, `quick-unknown`: the sound note beside Volume), the power menu opened from their Power… row on Power off, Restart or Restart pocket-ui (`restart-ui`), the power menu alone (`power[:<row>]`, made-up battery 73 % and about 4 h 20 min left; `power-game`: opened in a game, `power-noboot`: without Restart to boot menu, `power-nobattery`, `power-loading`, `power-charging`), the time pages (`time:<row>`, 2 the Date and time row; `time-wrong`: the Time page with the clock at 2000, offline, `time-wrong-wifi` the same with Wi-Fi on, `time-unsynced`: Wi-Fi off and no server yet, `time-set[:<field>]`: Date and time's picker, `time-set-wrong[:<field>]`: the picker opened from the clock of 2000, `time-hand`: after a set by hand, offline; [SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#date-and-time-by-hand)) |
| `console:…`, `shelf:…`, `cabinet:<keys>[:<ms>[:<ms>]]` | Console pages, Home with a system selected, the Cabinet look (below); the last `<ms>` of `cabinet:` is the launch animation that long after A, as is `POCKETUI_SNAPSHOT_LAUNCH_MS` on `shelf:<system>:shelf:games:<row>` |
| `info:<look>:<system>:<name>[:<keys>]` | X on the game in that look (`minimal`, `cards`, `split`, `shelf`, `cabinet`): the info page (`game_info`) in the first three, the box in the others; `<keys>` as for `curator:card:` (`d` picks linked facts, then scrolls facts and description together; `y` the status picker) |
| `box:<system>:<name>[:<keys>]` | The game's card as its box (`game_box`): `r`/`l` turn it over, `d` scrolls the back; `POCKETUI_SNAPSHOT_FLIP_MS=<ms>` paints the last turn that long after it began, `POCKETUI_SNAPSHOT_OPEN_MS=<ms>` the game that long into sliding out of its case. Every other card state shows the page |
| `pictures:<box\|snap\|title\|fade\|off\|none\|card>[:<system>]` | Split's list of a system with the preview at a point of its cycle (`box`: before the first change, `snap`, `title`, `fade`: halfway from the box art to the screenshot, `off`: Motion Off, long after the dwell, `none`: a game without pictures, box art only) or that game's card with the pictures under its facts (`card`); needs `snaps/<system>/<game>.png` (and `titles/…`) in `POCKETUI_DATA` for the first game that has one (`App::pictures_snapshot`) |
| `emu:<system>[:<game file>][:<emulator>/<core>][:save][:<row>]`, `runs-on[:<row>]` | "Runs on" with a made-up system.cfg (a RetroArch choice made in EmulationStation for `gba`), over the shelf; the same page from a RetroArch game's pause menu (`App::emu_choice_snapshot`) |
| `curator:<home\|hub\|shelf\|lens\|save\|card\|echoes\|versions\|stickers\|settings\|options\|ready\|upnext\|cabinet\|cabinetlens\|surprise>[:…]` | Curator's screens and the game card, Up next on the shelf, Curator in the Cabinet look, Surprise me's card with a fixed seed (`App::curator_snapshot`; `home:<look>:<favorites\|shelf\|sticker\|nofavs>` is Home on that row with its covers, [CURATOR.md](CURATOR.md#curator-on-home-featcurator-forward-2026-10-26); the Lens's order sheet is `lens:S`, a smart shelf's `options:da`, the rating row of the picker `card:<system>:<name>:yddddrr`, the card after its rows were made again `card:<system>:<name>:f…` for a new Curator index or `:c…` for the sync conflicts' scan); `curator:card:<system>:<name>:battery=<game\|system\|all\|measuring\|charging>` is the card with its Battery line from made-up measurements (`App::battery_snapshot`), `curator:card:<system>:<name>:lookup=<working\|offline\|busy\|found\|named\|nothing\|unreachable>` the card with the line "Wrong pictures? Look up again" ends with (`App::look_up_snapshot`; the row itself is `card:<system>:<name>:ydddddddddd`), `curator:card:<system>:<name>:patch` the card with a patch next to its game (`App::patch_snapshot`), a port's card (`curator:card:ports:<name>`) shows the busy-core hint when `POCKETUI_DATA/port-cpu.json` says so |
| `curator:<history\|history-empty\|history-cutoff\|history-mixed\|history-hub\|history-home\|review>[:…]` | Curator → History and the Year in review with made-up history ([HISTORY.md](HISTORY.md#snapshots), `App::history_snapshot`) |
| `search:…`, `letters:…` | The game search and the letter jump (below) |
| `bench-settings` | No picture: prints `bench-settings first_us=… first_glyphs=… first_patterns=… steady_us=… pill_us=… transition_us=…` (microseconds on this computer; letters drawn and strings run through the German patterns in the first frame), the numbers `tests/settings_open.rs` checks (`App::bench_settings`, `app_settings_groups.rs`) |
| `bench-shelf[:<look>]` | No picture: prints `bench-shelf items=… moves=… p50_us=… p95_us=… max_us=… glyphs=… patterns=… worst=<entry>` for the first frame after each move down and up Home's list in that look (Split by default; thread CPU time on this computer), the numbers `tests/shelf_moves.rs` checks (`App::bench_shelf`, `app_console.rs`). Runs on a device with a copy of its data folder (`POCKETUI_DATA`) |
| `bench-playful:<system>:<launch\|pull\|lean>` | No picture: prints `bench-playful <system>:<what> playful=<on> frames=… p50_us=… p95_us=… max_us=… over_16ms=…` for every frame (16 ms apart) of the Shelf look's launch animation (five times), ten moves with their spring, turn and wiggle, or a held Right with the spines leaning, on that system's games (`App::bench_playful`); run on a device with a fixture copy, the extra on and off in its settings.json (docs/LATENCY.md "Playful animations") |
| any state, with `POCKETUI_SNAPSHOT_WIFI=1` or `=0` | The status corner at the top right with the Wi-Fi symbol (connected) or without it (off or not connected), whatever the computer has; without the variable it shows what the computer's `wlan0` (`POCKETUI_WIFI_IFACE`) says. The battery is read from `POCKETUI_BATTERY` (a folder with `capacity` and `status`, e.g. `Charging`) and the time is the computer's (`app_status_corner.rs`) |

The PortMaster browser is `setup:ports[:keys]` (`setup:ports-shared:…` with file sharing on, keys as for the other
`setup:` steps: `r` the second list, `d` down, `x` "Copy your game files"); it shows `POCKETUI_DATA/ports.json` if
there is one, else a small built-in catalog. `port-files[:on|:off][:<catalog name>[:<rows scrolled>]]` is the page a
start opens when a port's files are missing (`port-files:on:wargus.zip:6`). Console pages are
`console:<system>[:sources|:credits][:<presses of Down>]` (`console:dreamcast`, `console:nes:6`, `console:gb:credits:8`;
`credits` builds Curator's catalog first, as it is a moment after the start; `scripts/console-credits-shots.sh` takes
21 consoles dark and light with Curator's made-up library and a few Sega, NEC, Bandai and Atari games), and Home with a system selected
is `shelf:<system>[:<shelf|minimal|split|cards|cabinet>]` (Split by default; `:games[:<row>]` after the look opens that system's list of games with the row selected, `shelf:snes:shelf:games:1`; box art is read at once, so the Shelf look's cases show their covers' colours); both need a library that has the system
(`POCKETUI_ES_SYSTEMS` pointing at an es_systems.cfg whose ROM folders hold a file each). A snapshot is painted in the
appearance saved in `POCKETUI_DATA/settings.json` (`{"onboarded": true, "appearance": "light"}` for Light).

Pixel Path (`src/app_pixel_path.rs`): `pixel-path:<gba|snes|patched|n64|runahead|compromised|pal|bluetooth|reading>[:<stage>[:about]]`
is its page with a made-up report (`pixel_path::demo`), the stage selected and, with `about`, its explanation open;
`pixel-path:<armsx2|dolphin|azahar>[:<stage>[:about]]` is a standalone emulator's page from the sample settings files in
`testdata/pixel-path/<case>` (`pixel_standalone::demo`, the files its tests read); `menu:ps2[:<row>]:pixel=<armsx2|missing>`
is a PS2 game's pause menu with Pixel Path switched on and those settings readable (listed, selected) or missing (not
listed; the log says why);
`pixel-path-menu[:full]` is the pause menu with the item switched on (`full`: with Achievements too, the longest menu).
`pixel-path:<case>:<stage>:over` has Over the game on Pixel Path (the Y hint says "Hide from the game"). What Over the
game draws is drawn by pocket-core, not pocket-ui: `cargo run --release --example over_game_shots -- <off|fps|sensors|pixel>
out.png [case] [screenshot.png]` paints a level's piece with made-up numbers (the strip of `pixel_live::demo`'s case,
also `gb` and `nds` for the corner and the column) where the HUD puts it, over a `vdev.sh shot` of the game; in the
virtual device the real one shows with `"over_game": "pixel"` (or `fps`, `sensors`) in `settings.json` or in
`game-settings/<system>.json`. The settings: `extras:3[:<level>]` (Options → Extras with Over the game selected, on
that level) and `settings:2:3` (Settings → Playing with Over the game selected).

Players (`src/app_players.rs`): `players:<two|promoted|gap|four|five>[:<row>]` is the page with made-up controllers
(`two`: the Pocket and a DualSense, Player 2 selected; `promoted`: the DualSense as Player 1; `gap`: Player 2 without a
controller; `four`; `five`: one waiting for a free player), `players-menu[:full]` the pause menu with the item listed and
selected (`full`: with Achievements and Pixel Path too, the longest menu).

Zoom (`src/zoom.rs`, `src/app_zoom.rs`): `zoom[:<2|3>][:low][:<left|right|top|bottom|step>...]` is the page over the background
picture given as the third argument (`--snapshot zoom:3:left out.png game.png`), moved as the words say from the middle;
`zoom-menu` the pause menu with the row selected (`<state>:zoomrow` lists the row on any menu snapshot). The page has no file or game behind it: it enlarges `QuickMenu::frame`.
Where it is reached (`app_zoom.rs`): Options → Zoom always; the main page's row only while `features::reading_on` (Easy to read) or settings.json `zoom_in_pause` (Settings → Sound & feel → Zoom in the pause menu) is on (`QuickMenu::zoom_row`, read when the menu opens); HOME + X (`hk_zoom`, Options → Shortcuts, off by default, never in Kids Mode or Easy) is seen by pocket-ui's `in_game_input`, which opens the menu straight on the page (`zoom_key`; black until the picture arrives, `zoom_picture_came`). `QuickMenu::zoom_back` says where B goes: the main page, the Options row, or on into the game.

The starter pack (`src/app_starter.rs`, [STARTER-PACK.md](STARTER-PACK.md)): `starter-home[:busy|offline|wifi-off|around]` is Home on a card with no games (`around`: after B, the empty shelf)
(the hint; `busy`: a download under way), `starter[:<row>[:busy|there|offline|failed]]` the page with that row selected
and the state named (a download at 40 %, three games on the card, the offline message, a damaged download). The folders
in a snapshot are under `<data>/starter-roms`; nothing is downloaded or written.

The game search and the letter jump (`src/app_search.rs`) have their own states: `search:home[:<look>]` (Home with the
Search row selected), `search:continue[:<look>]` (Home as it opens, on Continue under Search), `search:fresh` (the
search opened with no recent searches), `search:empty` (with five made-up recent searches), `search:typing:<text>`,
`search:results:<text>[:<presses of Down>]` (the list, keyboard aside; "nario kart" shows the closest matches) and `search:none:<text>`; `letters:<system>:<R2
presses>[:<look>]` is a system's list after that many letter jumps, with the letter showing whole (a snapshot is one
picture: the jump's letter is pinned instead of fading with the clock, and its box covers the page under it on purpose,
which the text-size check knows). They read covers at once instead of on the reader thread. For a library the size of a real one, `scripts/fake-library.sh <dir> [games]
[data dir]` writes an `es_systems.cfg` and ROM folders of empty files named like real dumps (region tags, scene
numbers, accents, "The" at the end; 5,000 games by default) and, with a data folder, plain covers for about half
of them:

```sh
scripts/fake-library.sh /tmp/lib 5000 /tmp/pocketui-data
POCKETUI_MOCK=1 POCKETUI_ES_SYSTEMS=/tmp/lib/es_systems.cfg POCKETUI_DATA=/tmp/pocketui-data \
  cargo run --release --bin pocket-ui -- --snapshot "search:typing:mario kart" out.png
```

For the virtual device, point `VDEV_ROMS` at `<dir>/roms` (it has to be under your home folder for colima).

`cargo run --release --example port_files_check -- ports.json [--all]` shows what pocket-ui reads out of the
instructions of every port that needs the player's files (folder, file names); see below.

`cargo run --release --example cover_check < roms.txt` shows which cover each game would get without downloading
anything (`roms.txt`: one `system/file name` per line).

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

The gate also prints separate wall times for the Rust runner plus doctests, the Brick input corpus and UI checks, serial timing
checks, platform checks and shell suites (`gate: stages …`). The runner includes compilation; nextest's own Summary
and Cargo's Finished lines distinguish test execution from compilation.

Private builds run `--features brick --test brick_settings_walk` and the `text_size` group's
`textfit_brick_031_` cases alongside the Brick input corpus. The ordinary
integration tier excludes that walk and those functions through their feature guards; the explicit run checks the Brick's settings,
sleep explanations and complete hints with the actual Brick build. It uses host-rendered mock screens. A red walk
fails the gate without a retry; docs-only changes and public builds without the private feature keep their existing
tiers. Real in-game device-family smokes remain a separate release requirement.

The full gate (`scripts/gate.sh`) runs all Rust, Brick, serial timing, platform and shell tiers whenever a train
contains a source, test, script, Cargo, asset or unknown path. `GATE_FULL=1` forces that path. Only a strictly
allowlisted train takes the short path: unembedded root `*.md`, `docs/**/*.md` outside `docs/user/`, and
`assets/i18n/skip.txt` (extraction notes). It runs the same active `doc_checks::` link/index/document checks and
`i18n::` checks as the full gate, plus the private-data scan. The on-device manual, `LICENSE`, `NOTICE.md`,
runtime translation TSVs and documents embedded with `include_str!`/`include_bytes!` or read by other Rust
contracts retain the full gate.
Unknown dynamic embed expressions, symlinks, a missing change list and mixed changes also select the full gate.
A failed document, link/index, translation or private-data check cannot merge through the short path.

`sh scripts/test-gate-paths.sh` verifies this selection with temporary fixtures and a fake cargo command, without building;
the full gate always runs it before selecting suites. Missing or incomplete results of selected shell suites fail the
gate. An interrupted parallel coordinator fails without retrying beside a lane it may have left running.
After the tests (unless only documents changed) the gate runs `scripts/check-platforms.sh`: `cargo check --features x11`,
the Steam Deck build's own code ([STEAMDECK.md](STEAMDECK.md)), and `cargo check --target aarch64-linux-android` of the
two Android crates (`android/`, `android/player/`, and with them the shared `src/` under `cfg(target_os = "android")`;
[ANDROID-MVP.md](ANDROID-MVP.md)). A few seconds each once warm. A compile error there fails the gate (`GATE: platform
check FAILED`); the handheld build never compiles that code. The Android part needs the Rust target and an NDK and says
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
without a picture and writes `POCKETUI_NAV_PROBE`'s record ([The navigation explorer](#the-navigation-explorer)).
`<directory>/run-game` is its stand-in emulator; game process groups and exit notifications follow the ordinary path.
With `POCKETUI_TRACE=<file>`, each requested frame records the usual `frame` and `paint` events before capture
diagnostics, scaling and PNG encoding. Use `paint` for UI paint time; the FIFO-to-PNG response also includes that
capture work and filesystem latency.
Captures use the live appearance painter, including saved Light mode and appearance changes made during a journey.
Power commands and RTC paths must point at fixture files when exercising shutdown. A successful fake shutdown
leaves the UI waiting for the system to turn off; use a failing command to exercise recovery and continue a journey.
This is a host journey harness,
not proof of game rendering, audio, Wayland surfaces or physical hardware.

Agent rule: use the quick tier while editing; run the complete `gate.sh <worktree>` (the wrapper around `scripts/gate.sh`) in the
merge queue before merging. A quick pass is never merge approval. The full gate retains all shell, documentation,
translation and layout checks. The full gate uses `profile.gate`; quick uses `profile.quick` (our crate at opt-level 1,
256 codegen units, dependencies at opt-level 3/16 units). Both retain release's checks and unwinding, disable LTO and
compile incrementally. `GATE_PROFILE` overrides either tier. Shipping builds still use `profile.release`. Builds default
to four jobs and low priority.

Baseline timings (shared build machine, load 34–53 on 10 cores) and the slowest 40 individual tests are in
[test-suite-baseline.tsv](test-suite-baseline.tsv). The baseline cold compile took 19m 04s; test executables 216.5s,
then shell suites 132.7s. The components total 24m 53s; this is not a green gate: three fake shutdown tests timed out
in the host-wide `sync`, and the known Azahar header timing test failed. Individual measurements were made separately
with four processes; do not add those durations or compare them directly with the whole-binary run at ten threads.
No idle-machine sample was available.

After measurements (lower shared load, around 8; these are not controlled idle-machine comparisons) are in
[test-suite-after.tsv](test-suite-after.tsv), including every test binary and the slowest 40 nextest cases.

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
  flushes the file and parent directory, rather than all unrelated writes on the host, and reports flush failures.
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

## Adding a feature to the tour

Every optional feature has one entry in `features::FEATURES` (`src/features.rs`). The first start's "How do you like to
play?" and "New in this version" read only that list, so a new feature needs no code in either of them (Settings → Help & tools → Tour is not
this list: it shows the six explained basics of `features::BASICS`, then the Easy to read choice; other new features do not go there):

```rust
Feature {
    id: "night_light",                   // (an example) stable; kept in settings.json (`tour_later`), used by the snapshots
    title: "Night light",                // the card's title, sentence case
    plain: "warmer colours after sunset", // what it does in everyday words, lower case, no feature names: the first start's "On: …, …" lists these
    line: "Warmer colours after sunset.",  // one sentence, ends with a full stop
    place: "Settings › Look › Night light", // where it is changed later (the fonts have no →, the UI writes ›)
    guide: "settings.md#night-light",    // its section in docs/user/, checked by doc_checks
    since: "0.3.0",                      // the version it ships in: "New in this version" shows it after that update
    switch: Some(Switch { get: |s| s.night_light, set: |s, on| s.night_light = on }),
    simple: Pick::Off,                   // what the profiles do with it: On, Off or Keep
    enthusiast: Pick::On,
    preview: pv::night_light,            // fn(&mut Canvas, &mut Text) in feature_previews.rs
    todo: None,
    try_it: None,                        // Some("Try Night light"): after an update the card starts on On and A says this
},
```

`plain` is what the first start's profile page lists under "On:" and "Off:" instead of the title (a new user does not
know "Echoes" or "Pixel Path"): short, lower case, no feature names; `features::tests` checks it for every entry with a
switch, and its German line goes into `assets/i18n/de.tsv`. An entry without a switch repeats its title there.

1. **The switch** reads and writes `features::Switches`, a copy of what Settings keeps. When the feature's setting is
   not in `Switches` yet, add the field, fill it in `App::switches` and use and save it in `App::adopt_switches`
   (`app_tour.rs`) exactly as its Settings page does. A feature without a switch (`switch: None`, both picks `Keep`) is a
   card that says where it is; it is shown after an update and in "Choose for yourself", never in the Tour.
2. **The preview** paints the feature's screen at 1240 × 1080 in `pal()` with made-up data ("Sample game"), using the real
   model where there is one; the card scales it down. Things drawn over a game stay dark (`palette::scoped(0, …)`). No
   file, no network: a preview is painted when its card is first shown and must stay within a few milliseconds
   (`cargo run --release --example feature_cards` prints the times: 1 to 4 ms for a preview and 1.5 to 5 ms for a
   card's first paint with it on an M1 Max, 0.5 ms for each paint after that).
3. **The docs**: its section in `docs/user/` (the `guide` anchor), and a row in the table of `docs/user/tour.md` when
   it has a switch (in the registry's order), or its title in bold under the table when it has none. `cargo test`
   checks both.
4. **The order** of the entries is the order of the cards; switch cards come before the others.

`todo: Some("branch: what is missing")` keeps an entry out of every list until it is wired (none is now). To wire
one: give it its switch and picks, its preview, `since`, its own `guide` section, and set `todo: None`.

**"New in this version".** settings.json keeps `seen_version`, the version whose cards were shown (written by the first
start and, once per new version, after the first frame). `features::due` takes the entries with `since` newer than that
and not newer than the running version, at most three, newest first. A device set up before the registry has no
`seen_version` and counts as `features::BASELINE` (0.1.0, the version of the first entries), so an update never brings
cards for what was already there. Dev builds report Cargo's version; to see the cards, build with `POCKETUI_VERSION` newer
than an entry's `since`, use `--snapshot tour-new:<id>`, or (mock runs) `POCKETUI_NEW_IN`. A look worth trying has
`try_it` (Retro, 0.3.0): in "New in this version" only, while it is off, its card starts on On and A says "Try Retro", so
one press tries it and B leaves everything as it was.

## PortMaster ports that need the player's files

About half of PortMaster's arm64 ports are not ready to run (`attr.rtr: false`): they are engines for games the
player owns. `ports.rs` keeps them (`Port::needs_files`, `Port::instructions` = `attr.inst`); only experimental
ones are left out. The browser (`setup.rs`, `Step::Ports`) shows them as a second list, and `Step::PortFiles`
is the "Copy your game files" page (`port_files::FilesPage`, also used by the shelf in `app_ports.rs`).

PortMaster says where the files go only in prose. `port_files::expect` reads it:

- **Folders**: paths in the text that name the port's folder (`sprout/assets`, `/roms/ports/counter-strike/valve`),
  also under another spelling of it after `ports/` (`ports/alientdeathmob/assets` for `aliendeathmob`); failing
  that, phrases ("into the assets folder", "this port's gamedata folder", "the port folder"). What follows
  "optional", "you can also", "if you want" is left out.
- **Files**: names with a game-data extension (`data.win`, `Balatro.love`, `arco.exe`, `pak0.pak`) after a verb that
  puts them somewhere, not in an "alternatively" sentence. Names with spaces come out as their last word ("Left.exe"
  for "Pengu Never Left.exe"); everything that looks for them matches the end of a name.
- On the October 2026 catalog (511 such ports): 378 with one folder, 10 with several, 47 with named files only,
  76 where the text says neither (README only, "the port folder" without names). The unit tests in `port_files.rs`
  hold about 50 of the real texts.

**The start guard** (`App::port_needs_files`, before every start of a `ports` game): it finds the port by its script
in the `items` of `roms/ports/*/port.json` (harbourmaster writes that file last when it installs). Ports whose
port.json says `rtr: true`, or has no `rtr`, start as always. For the others `port_files::check` says "missing"
only when all of these hold, and the page opens instead of the start:

1. pocket-ui has never seen the port run for 60 s or more (`library_meta` sessions);
2. no file in the port's folder, other than `log.txt`/`*.log`, is more than two minutes newer than port.json: a port
   that prepared its data on a first start (patched `data.win`, unpacked an installer, emptied `gamedata` again)
   has written something, so it is left alone;
3. the first folder the text names holds only placeholders (empty files, dotfiles, `*.txt`, `*.md`, "place … here"),
   and none of the named files is anywhere in the port; or that folder is missing and the text named it with the
   port's own folder; or the text names the port folder itself and none of the named files is anywhere in it.

Anything else (no clear folder, any real file in it, a folder of more than 20 000 entries) starts the port.

**Where the files go.** Straight into the port's own folder: the page shows `roms/ports/<port>/<folder>` and its
address on the share (the port folder itself when the text names several folders or none), and makes the folder when
the text names it with the port's own folder (`port_files::make_place`), handed to the share user. The web companion
writes to the same place (`web::files::port_target`). pocket-ui moves nothing at a start. `roms/port-files`, the drop
folder up to 0.1.8, is no longer read or made; what a player left there stays, and `library::NOT_STRAYS` keeps it out
of the stray folders.

## Environment variables

These are meant for development and tests; normal use needs none of them. On a device, put them one per line in
`/storage/pocket-ui/dev.env`. Every variable the code reads is listed here (checked by `doc_checks`).

**pocket-ui**

| Variable | Effect |
|---|---|
| `POCKETUI_MOCK=1` | Use in-memory mocks for brightness, Wi-Fi, battery and so on; skip the hardware GPU stock-clock helper |
| `POCKETUI_MOCK_NOW=<Unix seconds>` | With `POCKETUI_MOCK`: freeze the wall clock used by Curator and relative play history; the synthetic library generator accepts the same value for timestamps and anniversaries. Snapshot fixtures pin both children to `tests/common/root.rs`’s `FIXTURE_NOW`; monotonic deadlines and cartridge RTC tests keep their own clocks. |
| `POCKETUI_MOCK_ZONE=<zone>` | With the mocks: the time zone the system starts with (`UTC` for a fresh ROCKNIX) ([VIRTUAL-DEVICE.md](VIRTUAL-DEVICE.md)) |
| `POCKETUI_DATA` | Use another data folder instead of `/storage/.config/pocket-ui` |
| `POCKETUI_ES_SYSTEMS` | Read another `es_systems.cfg` |
| `POCKETUI_REPORTS=<dir>` | Write Settings → Help & tools → About → Report a problem's zip into this folder instead of `/storage/roms/reports` |
| `POCKETUI_SCREENSHOTS=<dir>` | Save screenshots (HOME + A) and let the web companion list them from this folder instead of `/storage/roms/screenshots` |
| `POCKETUI_PORTS_JSON=<file>` | Tests only: `cargo test` also parses this downloaded PortMaster catalog (`ports::tests`) |
| `POCKETUI_MOONLIGHT=<program>` | Start this program for PC streaming and Settings → Games → PC streaming (pair, list, unpair) instead of ROCKNIX's `moonlight` (virtual device, tests) |
| `POCKETUI_WIFI_IFACE=<name>` | The network interface counted as Wi-Fi (`eth0` in the virtual device; `wlan0` by default) |
| `POCKETUI_BATTERY=<dir>` | Read the battery from another sysfs directory than `/sys/class/power_supply/battery` |
| `POCKETUI_SNAPSHOT_NATIVE=1` | Snapshots only: bypass the logical canvas for raw small-panel layout comparisons |
| `POCKETUI_FRAME_BENCH=1` | Snapshots (`home`, `settings`, `main`): report settled paint and paint-plus-reduction thread CPU medians and p95, excluding startup, PNG and presentation ([H700-SCALED-LAYOUT.md](H700-SCALED-LAYOUT.md)) |
| `POCKETUI_SNAPSHOT_WIFI=1\|0` | `--snapshot`: the status corner as if Wi-Fi were connected (`1`) or not (`0`) (the `--snapshot` table above) |
| `POCKETUI_SNAPSHOT_USB=1` | `--snapshot`: nothing now: USB file transfer is retired (`usb_transfer::RETIRED`), the row and tile are never shown |
| `POCKETUI_SNAPSHOT_GAMES_CARD=1` | `--snapshot`: Add games as with a games card in (`games_card`) |
| `POCKETUI_SNAPSHOT_IMAGE_CARD=1` | `--snapshot`: Home and Add games as with a flashed system image in the second slot (`card_health::image_card`) |
| `POCKETUI_SNAPSHOT_GAME=<system>\|<name>\|<file>` | `--snapshot`: the game behind the pause menu and the slot pages (default `gbc\|uCity\|uCity.gbc`); `scripts/readme-shots.sh` sets it |
| `POCKETUI_SNAPSHOT_FLIP_MS=<ms>` | `--snapshot box:…`: the box's last turn that long after it began, instead of done (`game_box::BoxView::snapshot_settle`) |
| `POCKETUI_SNAPSHOT_OPEN_MS=<ms>` | `--snapshot box:…`: the game that long into sliding out of its case, instead of out (`game_box::BoxView::snapshot_settle`) |
| `POCKETUI_SNAPSHOT_FROZEN=<n>` | `--snapshot`: the first n recent games of `POCKETUI_DATA` stand open, whatever is on the card (`App::frozen_snapshot`) |
| `POCKETUI_SNAPSHOT_SPIN_MS=<ms>` | `--snapshot box:…`: the disc beside its case that long into its slow turn (Playful animations, `game_box::BoxView::snapshot_settle`) |
| `POCKETUI_BENCH_FRAMES=<n>` | `--snapshot box:…`: after the picture, that many frames of the box 16 ms apart (its disc turning with Playful animations), their thread CPU time on stdout as `bench-box … p50_us= p95_us= max_us= over_16ms=` |
| `POCKETUI_SNAPSHOT_HELD=<steps>:<ms>` | `--snapshot shelf:…:shelf:…`: Right held for that many steps 70 ms apart, painted `ms` after the last one: the spines leaning or swinging back (Playful animations, `App::shelf_playful_snapshot`) |
| `POCKETUI_SNAPSHOT_MOVED_MS=<ms>` | `--snapshot shelf:…:shelf:…`: the selection that long after it moved: the spine springing up, the front turning and its wiggle (`App::shelf_playful_snapshot`) |
| `POCKETUI_TEST_STALL_CHILD=<dir>`, `POCKETUI_TEST_OPERATION=<phase>`, `POCKETUI_TEST_BLOCK_PHASE=<phase>` and `POCKETUI_TEST_ENTERED=<file>` | Set by `src/bin/pocket_core/quit_regression.rs` (`stalled_player_returns_to_shelf_and_preserves_continue_in_every_phase`) for its own child process: the child blocks the core in one phase (`load`, `run`, `size`, `resume`, `retry-run`, `unload`; `ENTERED` is the file it creates once it is stuck) and the parent checks that the watchdog sends the player back to the shelf and keeps the earlier Continue moment. Not for use by hand |
| `POCKETUI_TEST_CRASH_CHILD=<dir>` and `POCKETUI_TEST_ABORT_PHASE=<phase>` | Set by `src/bin/pocket_core/quit_regression.rs` (`a_moment_that_crashes_the_core_while_loading_is_not_tried_at_every_start`) for its own child process: the stub core aborts inside `retro_unserialize` twice; the third start must come up fresh and Load must still list the moment. Not for use by hand |
| `POCKETUI_TEST_QUIT_CHILD=<dir>` and `POCKETUI_TEST_BLOCK_SERIALIZE=<file>` | Set by `src/bin/pocket_core/quit_regression.rs` for its own child process: the child loads a fake core whose `serialize` never returns and must still leave within the deadline, keeping the earlier Continue moment. Not for use by hand |
| `POCKETUI_TEST_COVERS=<dir>` | `cargo test` (`media::tests::real_covers_lose_their_side_strip`): a folder with `gba/`, `gb/` and `snes/` box art PNGs (e.g. a library's thumbnails; not in the repository) to check that a system's printed side strip is cut off the cartridge labels; without it the test says SKIPPED |
| `POCKETUI_TEST_CRASH_AT=<step>` and `POCKETUI_TEST_CRASH_SPEC=<json>` | Set by `update::crash_tests` for its own child process: the child runs `install_app` and ends with SIGKILL at the named step (`downloaded` … `linked`, also `layout-copied-*`, `layout-synced`, `layout-current`, `layout-linked-*` during flat-install conversion). Covers existing same-version release folders as well. Not for use by hand |
| `POCKETUI_TEST_KILL_AT=<n>` and `POCKETUI_TEST_KILL_LOG=<file>` | Tests only (`fsutil::crash_point`): the process ends with SIGKILL at the n-th write step; the log lists every step reached. Set by `libretro`'s quit test for its own child. Not for use by hand |
| `POCKETUI_TEST_QUIT_DIR=<dir>` | Set by `libretro::tests::a_quit_cut_off_at_any_write_step_keeps_a_whole_moment_and_the_newest_battery_save` for its own child process (the session that quits). Not for use by hand |
| `POCKETUI_TEXT_FIT_OUTPUT=<dir>` | `cargo test` (`overlay` startup notes): also write each fitted note as a PNG into this folder, to look at them |
| `POCKETUI_ROOT`, `POCKETUI_SYSTEM_CFG`, `POCKETUI_ROCKNIX_FUNCTIONS`, `POCKETUI_ROCKNIX_PROFILE`, `POCKETUI_ES_SETTINGS`, `POCKETUI_ES_ONLY_FILE`, `POCKETUI_START_ES`, `POCKETUI_LOCALE_DIR`, `POCKETUI_TZ_CACHE`, `POCKETUI_QUANTUM`, `POCKETUI_SWAY_WAIT`, `POCKETUI_RESTART_DELAY`, `POCKETUI_UI_PID_FILE`, `POCKETUI_GAME_FILE`, `POCKETUI_DARK_FILE`, `POCKETUI_FAN_BEAT`, `POCKETUI_FAN_NOTE` | Read by the launcher `deploy/run-pocket-ui.sh`: its folders, ROCKNIX files and programs, waits and marker files moved into a test folder. `update::crash_tests` and `deploy/test-run-pocket-ui.sh` set them to start the real launcher on a computer. Not for use on a device |
| `POCKETUI_SNAPSHOT_LAUNCH_MS=<ms>` | `--snapshot shelf:…`: the launch animation that long after A (`App::launch_animation_snapshot`) |
| `POCKETUI_SNAPSHOT_SIZE=<w>x<h>` | `--snapshot`: paint at another screen size than the device's (the Mini V2's 1240x1080 unless `POCKETUI_DEVICE` or a `:device=` says otherwise; `1920x1080` the Pocket 5 and Flip 2, `1280x960` the Mini V1, [DEVICES.md](DEVICES.md)); a value that isn't a size from 320x240 to 4096x4096 is ignored |
| `POCKETUI_DEVICE=<name>` | Which handheld this is, instead of the device tree: `miniv2`, `mini`, `rp5`, `rp5-visionox`, `flip2`, `flip2-visionox`, `steamdeck` (a `--features brick` build also `brick` and `smartpro`, [TRIMUI-SMART-PRO.md](TRIMUI-SMART-PRO.md)) or a device tree `model` string (`src/device.rs`, [DEVICES.md](DEVICES.md)); a snapshot state may end in `:device=<name>` for the same |
| `POCKETUI_STATES` | Save-state directory instead of `/storage/roms/savestates`; the Deck launcher uses its data folder (handheld default unchanged) |
| `POCKETUI_GBA_CORE` | Development path to the bundled ARM64 gpSP library; enables H700 GBA core selection on a stand-in device too, retaining existing Continue ownership and saved choices ([HOST.md](HOST.md#h700-gba)) |
| `POCKETUI_TEST_TIME_FACTOR` | Tests only: an extra scheduling allowance from 1 to 64 for hang guards on a constrained runner; product deadlines and simulated clocks are unchanged (see the timing-test notes above) |
| `POCKETUI_HOST_BIN` | Path of `pocket-core` instead of `/storage/pocket-ui/pocket-core` (the Steam Deck app, [STEAMDECK.md](STEAMDECK.md)) |
| `POCKETUI_HW_ROOT=<dir>` | The hardware seam (`system_ctl::RealHw`) looks below this folder instead of `/` (sysfs files, stand-in programs in `usr/bin`); only in a `--features brick` build ([TRIMUI-BRICK.md](TRIMUI-BRICK.md)) |
| `POCKET_CORE_SAVE_DIR=<dir>` | pocket-core keeps a game's battery saves and memory cards in `<dir>/<system>` instead of next to the game; only in a `--features brick` build |
| `POCKET_CORE_SOFTWARE=1` | pocket-core without GL, the CPU copies the picture; only in a `--features brick` build ([TRIMUI-BRICK.md](TRIMUI-BRICK.md)) |
| `POCKETUI_HW_ROOT=<dir>` | Only in a `--features brick` build: `system_ctl::RealHw` looks for sysfs files and stand-in helpers below this folder instead of `/` ([TRIMUI-BRICK-STANDALONE.md](TRIMUI-BRICK-STANDALONE.md)) |
| `POCKETUI_BRICK_STANDALONE=1` | Only in a `--features brick` build: the Brick runs from its own card without NextUI, so the Wi-Fi page and Wi-Fi hints stay (`brick_profile::standalone`, [TRIMUI-BRICK-STANDALONE.md](TRIMUI-BRICK-STANDALONE.md)) |
| `POCKETUI_BRICK_BLUETOOTHCTL=<program>` | Brick builds: the `bluetoothctl` pocket-ui runs (the card's `hwroot/usr/bin/brick-bluetoothctl`, set by `deploy/brick/start.sh`; [TRIMUI-BRICK-BLUETOOTH.md](TRIMUI-BRICK-BLUETOOTH.md)) |
| `POCKETUI_UPDATE_URL=<url>` | Only in a `--features brick` build: the update channel when settings.json has no `update_url` (`update::default_url`); the Brick card's start script sets it from `app/update-url` ([TRIMUI-BRICK-REMOTE.md](TRIMUI-BRICK-REMOTE.md)) |
| `POCKETUI_BRICK_CARD=<dir>` | Only in a `--features brick` build: the card's pocket-ui folder: an app update replaces its program folder `app/` (`update::install_brick`) instead of a release under `/storage/pocket-ui`; set by the Brick card's start script ([TRIMUI-BRICK-REMOTE.md](TRIMUI-BRICK-REMOTE.md)) |
| `POCKETUI_TS_PROXY=<url>` | Only in a `--features brick` build: Tailscale's local proxy (`http://127.0.0.1:1055` on the Brick card): update lists and files on a `*.ts.net` address are fetched through it (`update::proxy_args`, [TRIMUI-BRICK-REMOTE.md](TRIMUI-BRICK-REMOTE.md)) |
| `POCKETUI_TAILSCALE=<file>` | Only in a `--features brick` build on the Brick's own card: the `tailscale` command Settings › Tailscale runs, instead of `/usr/sbin/tailscale` ([TRIMUI-BRICK-REMOTE.md](TRIMUI-BRICK-REMOTE.md)) |
| `POCKETUI_TAILSCALED_SOCKET=<file>` | tailscaled's LocalAPI socket the web companion asks (`/run/tailscale/tailscaled.sock` when unset); the Brick's own card sets its run folder's ([TRIMUI-BRICK-MORE.md](TRIMUI-BRICK-MORE.md), `src/web/net.rs`) |
| `POCKETUI_BRICK_SYS=<dir>` | Only in a `--features brick` build: the Brick's sysfs (motor pin, LED driver) is below this folder instead of `/`; a fake tree in tests ([TRIMUI-BRICK-HW.md](TRIMUI-BRICK-HW.md)) |
| `POCKETUI_MOTOR_VOLTAGE=<min>,<max>` | Only in a `--features brick` build: the range `/sys/class/motor/voltage` takes, to try one without a rebuild ([TRIMUI-BRICK-HW.md](TRIMUI-BRICK-HW.md)) |
| `POCKETUI_WINDOWING=x11` | Put pocket-ui and the game in front with X11 requests (`desk.rs`) instead of sway's; on by itself for the Steam Deck profile ([STEAMDECK.md](STEAMDECK.md)) |
| `POCKETUI_RUNEMU` | The program that starts standalone emulators, instead of `/usr/bin/runemu.sh` (Steam Deck: `deploy/steamdeck/runemu.sh`) |
| `POCKETUI_CORES=<dir>:<dir>` | Folders searched for `<core>_libretro.so` after `/storage/cores` (Steam Deck: bundled cores, RetroArch's Flatpak) |
| `POCKETUI_BACKLIGHT=<dir>` | The backlight sleep dims (the first one under `/sys/class/backlight` by default); the launcher reads it too |
| `POCKETUI_LOG=<file>` | The log the logger copies to the card (`/var/log/pocket-ui.log` by default); the launcher writes it |
| `POCKETUI_SNAPSHOT_SIZE=<w>x<h>` | `--snapshot`: paint at this size instead of the panel's 1240x1080, e.g. `1920x1080` for a TV ([DISPLAY-OUT.md](DISPLAY-OUT.md)) |
| `POCKETUI_TV_OUTPUT=<output>` | Treat this sway output as a TV too (besides `DP-*`, `HDMI-*`): a second headless output in a test sway ([DISPLAY-OUT.md](DISPLAY-OUT.md)) |
| `POCKETUI_SECOND_RESTORE_FILE=<file>` | The recovery marker the second screen writes before its first brightness or output change (`second_screen::note_restore`), so the launcher can restore both screens' power and brightness after an exit, abort or SIGKILL (`pocket-ui --restore-second-screen`); default `/tmp/pocket-ui-second.restore`. For tests and the virtual device ([DUAL-SCREEN.md](DUAL-SCREEN.md)) |
| `POCKETUI_SECOND_OUTPUT=<output>` | Explicitly opt into a second screen (the lower one) on a profile other than RG DS Plus: a second headless output in the virtual device; the shelf goes on another one, and Settings → System → Second screen shows ([DUAL-SCREEN.md](DUAL-SCREEN.md)) |
| `POCKETUI_DUAL_PANELS=<key=value ...>` | Corrects the RG DS Plus's panel table (`second_screen::Panels`: `top`, `bottom`, `top_light`, `bottom_light`, `touch`, `touch_turn`) on the device without a new build ([DUAL-SCREEN.md](DUAL-SCREEN.md)) |
| `POCKETUI_TOUCH_ROTATE=0\|90\|180\|270` | Turn the touch mapping if taps land in the wrong place |
| `POCKETUI_SUSPEND_CMD`, `POCKETUI_POWEROFF_CMD`, `POCKETUI_REBOOT_CMD`, `POCKETUI_BOOTLOADER_CMD` | Replace the commands sleep and the power menu run, for tests ([SLEEP.md](SLEEP.md)) |
| `POCKETUI_NEW_IN=<version>` | Under `POCKETUI_MOCK` only: the version "New in this version" compares with instead of the build's, so a journey sees the cards of an update (`src/app_tour.rs`; the Retro card's journey in `tests/user_journeys.rs`) |
| `POCKETUI_UPDATE_HINT=<version>` | Under `POCKETUI_MOCK` only: stands in for an update list naming that pocket-ui version, so the shelf's once-per-version notice shows at once without a network, and the Updates page offers that version with notes (`app_update_hint::mock_manifest`; `src/app_update_hint.rs`, the journeys in `tests/user_journeys.rs`) |
| `POCKETUI_BUILD_UNIX=<seconds>` | Set by `build.rs` at build time (not by hand): when this was built, so the clock check knows a clock before it is wrong (`src/clock_check.rs`, [SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#a-wrong-clock)) |
| `POCKETUI_SLEEP_MINUTE_SECS=<n>` | A "minute" of the sleep timers lasts n seconds, so sleep can be tested quickly ([SLEEP.md](SLEEP.md)) |
| `POCKETUI_SLEEP_DPMS=1` | Light sleep also switches the panel off through sway, not only the backlight |
| `POCKETUI_RTC_ALARM=<file>` | The RTC's wake alarm for switching off after sleeping, and the one disarmed before every power off and restart, instead of `/sys/class/rtc/rtc0/wakealarm` ([SLEEP.md](SLEEP.md#l3-switch-off-after-sleeping-2026-10-25)) |
| `POCKETUI_RTC_PROC=<file>` | The stand-in for `/proc/driver/rtc` (`alarm_IRQ`, `alrm_pending`) read back after the disarm; with `POCKETUI_RTC_ALARM` set and this not, the proc file is not read ([SLEEP.md](SLEEP.md#power-off-restart-and-the-rtc-alarm-2026-10-26-fixpower-off-rtc)) |
| `POCKETUI_NO_AUTOSUSPEND=1` | Light sleep never goes on to a suspend (as `no-autosuspend` in either `<data>` or `/storage/.config/pocket-ui` does); the low-battery guard still switches off |
| `POCKETUI_POWER_KEY=1` | Read the power key even without logind (the virtual device) |
| `POCKETUI_SAFE=1` | Start in safe mode, the rescue page, as `pocket-ui --safe` does ([ARCHITECTURE.md](ARCHITECTURE.md#rescue-path)) |
| `POCKETUI_SAFE_REASON=<sentence>` | Why safe mode started, shown on the rescue page; set by the launcher |
| `POCKETUI_FRAME_FILE=<file>` | Created once the first picture is on screen, for the launcher's black screen guard; set by the launcher (`/tmp/pocket-ui.frame`) |
| `POCKETUI_NO_BOOT_MENU=1` | The power menu as on a device that can't restart into the boot menu |
| `POCKETUI_CLOCK_SHIFT=<seconds>` | Move the clock that Automatic appearance and the night light's schedule follow, to see the change at once ([DESIGN.md](DESIGN.md), [SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#night-light)) |
| `POCKETUI_ROOM_TIME=HH:MM` | The time of day the Shelf look's and the Cabinet's light follows (Look → Cozy extras → Light by time of day), instead of the clock: snapshots, trying it without waiting ([DESIGN.md](DESIGN.md#the-room-light-and-decorations-room_light-room_deco-app_room_extrasrs)) |
| `POCKETUI_SLIDESHOW_AFTER_MS=<ms>` | With `POCKETUI_MOCK`: how long the shelf is left alone before the idle slideshow starts, instead of a minute (the journey tests; [SLEEP.md](SLEEP.md#idle-slideshow-2026-10-30-srcslideshowrs-srcapp_slideshowrs)) |
| `POCKETUI_SLIDESHOW_STATES=<dir>` | With `POCKETUI_MOCK`: the folder of save-state pictures the idle slideshow takes the player's own save pictures from, instead of `/storage/roms/savestates` (snapshots) |
| `POCKETUI_ES_HOME=<dir>` | Where pocket-ui looks for EmulationStation's and ES-DE's `gamelists` and `downloaded_media` folders (`<dir>/.config/emulationstation`, `<dir>/.emulationstation`, `<dir>/ES-DE`; `/storage` by default): the virtual device and snapshots. The ROM folders' own `gamelist.xml` need no variable (`gamelist.rs`) |
| `POCKETUI_TEXT_SIZE=large\|medium\|normal` | The text size whatever Settings → Look → Text size says, for snapshots and tests (`type_scale.rs`, [DESIGN.md](DESIGN.md#text-size)) |
| `POCKETUI_PROMPTS=nintendo\|xbox\|playstation` | The Button symbols whatever Settings → Look → Colours or Style says, for snapshots and tests (`button_prompts.rs`) |
| `POCKETUI_COLOURS=warm\|indigo-cube\|grey-brick\|green-x\|blue-station` | The Colours whatever Settings → Look → Colours or Style says, for snapshots and tests (`palette.rs`) |
| `POCKETUI_FONT=barlow\|pixel\|rounded\|retro` | The font style whatever settings.json's `"font"` says, for snapshots and tests (`font_style.rs`, [DESIGN.md](DESIGN.md#font-styles-srcfont_stylers)); `retro` is the whole Retro style |
| `POCKETUI_RETRO=cocoa\|pea\|off` | Settings → Look → Style → Retro in that palette (or off) whatever settings.json says, for snapshots and tests (`retro.rs`, [RETRO-STYLE.md](RETRO-STYLE.md)) |
| `POCKETUI_H700_VOLUME_OSD=1` | Set before start on an H700 handheld to keep the old display path for the volume pop-up instead of the lighter `mako_osd.rs` one; for A/B measurements only ([H700-VOLUME.md](H700-VOLUME.md)) |
| `POCKETUI_RETRO_DIGITS=lcd\|pixelify\|plain` | Retro's digits: the LCD set (default), Pixelify's own or Barlow's, for comparing snapshots (`retro::Digits`, [RETRO-STYLE.md](RETRO-STYLE.md)) |
| `POCKETUI_BLIP_DIR=<dir>` | The ignored test `retro_blip_table` writes the Retro menu blips as WAVs there and prints their level table (`ui_sound.rs`) |
| `POCKETUI_VISIBLE_TEXT=<file>` | A rendered snapshot writes the strings painted into its final canvas, after clipping and dialog coverage; excludes width measurements and off-screen strings; a running pocket-ui writes it for every frame it draws. Used by `tests/user_journeys.rs` and the Wi-Fi bench ([WIFI-BENCH.md](WIFI-BENCH.md)) to assert what the player can read. This is a text transcript, not OCR; images and text painted into a separately composited layer are outside it. |
| `POCKETUI_LAYOUT_AUDIT=<file>` | A rendered snapshot writes what the text-box record cannot see because nothing overlaps (`text::boxes::layout_audit`): `N` and a column of three or more one- or two-character strings, `S` and a string painted under 14 px. Read by `scripts/size-sweep.sh`. |
| `POCKETUI_NAV_PROBE=<file>` | Every frame's offer to the D-pad, for `tests/nav_explorer.rs`: each string and mark (`‹nav:row›`, `‹nav:focus›`, `‹nav:hint›`) with its place, then, under `--test-driver`, where the App is, its touch list and `@end` (`src/nav_probe.rs`, `src/app_nav_probe.rs`, [The navigation explorer](#the-navigation-explorer)). Unset: nothing recorded |
| `POCKETUI_SNAPSHOT_CACHE_DIR=<dir>` | Test-only fresh run directory for shared static layout renders (`tests/common/snapshot.rs`); never set by the player. Interactive and timing tests bypass it |
| `POCKETUI_SNAPSHOT_CACHE_DISABLE=1` | Execute every static request through the same staged fixture without reusing outputs, for controlled A/B measurements |
| `POCKETUI_NAV_REPORT=<dir>` | `tests/nav_explorer.rs` writes each part's pages, notes and failures to `<dir>/<profile>-<part>.txt` |
| `POCKETUI_TITLE_BLOCKS=<file>` | A snapshot writes the lines of the big game name it painted (`title_block`), one per line: size, `main` or `sub`, baseline, text. `tests/long_titles.rs` reads it to check that the name is whole |
| `POCKETUI_SPINE_LINES=<file>` | Every spine made (Shelf and Cabinet looks) appends a line to the file: title, size, its lines (two joined by a bar), the case (`media::note_spine`). `tests/spine_names.rs` reads it |
| `POCKETUI_TEXT_BOXES=<file>` | A snapshot writes to `<file>` where its text collides: `O` and two strings whose ink runs into each other (or into a hint bar, `‹hints›`; a page may record a row's band as `‹row›`, which only a hint bar may lie on), `T` and two strings on one line closer than 3 px, `E` and a string cut by the left or right edge of what it was painted into, `V` and a string the picture's own top or bottom edge cuts (a heading half off the screen). Empty when there is nothing; `tests/text_size.rs` reads it (`text::boxes`) |
| `POCKETUI_CJK_FONT=<file>\|off` | The font for the Japanese, Chinese and Korean letters Barlow lacks, instead of ROCKNIX's `/usr/share/fonts/truetype/noto-cjk/NotoSansCJKsc-Regular.otf`; `off` (or empty): none, a letter shows as "?" (`text_fallback.rs`, [DESIGN.md](DESIGN.md#japanese-chinese-and-korean-names)). For snapshots on a computer without it: the file of an unpacked ROCKNIX image |
| `POCKETUI_TEST_CJK_FONT=<file>` | The Noto Sans CJK the tests use (`src/text.rs`, `tests/cjk_snapshots.rs`), instead of `/tmp/rocknix-root-20261001/usr/share/fonts/truetype/noto-cjk/NotoSansCJKsc-Regular.otf`; without either the tests that need it print `SKIPPED` and pass |
| `POCKETUI_LANG=de\|en` | The interface in this language whatever Settings → System → Language says, for snapshots and tests ([Translations](#translations)) |
| `POCKETUI_I18N_MISSES=<file>` | Append to `<file>`, each once: every string painted without a German line (`M<TAB>…`), a part of a translation left in English (`P`), every translation made (`O`), and by its English a string `Text::fit` cut short (`C`) or drawn past the right edge (`X`); `tests/i18n_snapshots.rs` and `scripts/i18n-shots.sh` read it |
| `POCKETUI_FRAME_LOG=1` | Log "press to frame" latency for every button press and the paint time of animation frames by kind (`app_motion.rs`), and the Cabinet look's frame times |
| `POCKET_LAUNCH_HOLD_SKIP=<path>` | Set by pocket-ui for pocket-core: once the file is there the launch animation was skipped and the window stops waiting (`startup.rs`) |
| `POCKETUI_RIG_ASSETS=<dir>` | Read a launch rig from `<dir>/<system>/` instead of the compiled-in files, for working on one without a build (`insert_rig.rs`) |
| `POCKETUI_MOTION_SCALE=<n>` | Slow the menu animations down n times (`motion.rs`), for frame sequences in the virtual device |
| `POCKETUI_SOUND_DEBUG=1`, `POCKETUI_SOUND_QUANTUM=<frames>` | Menu sounds: log how long each took to start; PipeWire's quantum on the shelf (`0` keeps ROCKNIX's) ([HOST.md](HOST.md)) |
| `POCKETUI_H700_SPEAKER_MAX_RAW=<raw>` | Process-only lab ceiling, 1..the compiled speaker maximum; can only lower it. Raw 1 still needs an independently verified digital-zero source and running PCM clock for silent tests ([H700-VOLUME.md](H700-VOLUME.md)) |
| `POCKETUI_FAST_SWITCH=0` | No hand-over: the game switcher waits for the old game's process to end before it starts the next one, and Quit waits for it before the shelf comes back, as before ([ARCHITECTURE.md](ARCHITECTURE.md#switching-games)); for comparing switch and quit times |
| `POCKETUI_SWAY_IPC=0` | Start `swaymsg` for every focus change and window lookup instead of speaking sway's IPC socket (`SWAYSOCK`) directly (`control.rs`); for comparing on the device |
| `POCKETUI_FREE_BYTES=<n>` | With `POCKETUI_MOCK`: the free space on the card the low-space notice (`app_storage`) reads; without it a mocked run never warns, whatever the build machine's disk has left |
| `POCKETUI_NO_HISTORY=1` | Play history records nothing of this run (no writer thread, no migration; what is recorded stays). Set by `scripts/latency.py`, so measured game starts never land in the player's history ([HISTORY.md](HISTORY.md#recording)) |
| `POCKETUI_LIBRARY_CACHE=0` | No game list cache: every start reads es_systems.cfg and every ROM folder before the first frame, as before 2026-10-24 (`library_cache.rs`, [Start time](#start-time)); for comparing cold starts on the device |
| `POCKETUI_STARTUP_PROBE=1` | Load libwayland-client and libxkbcommon before the event loop as a start step of their own ("display libs" in the `first frame after` line), so their loading is told from the connection to sway ([LATENCY.md](LATENCY.md#cold-start)) |
| `POCKETUI_TRACE=<file>` | Append latency trace lines (input, frame and its paint time and size, game start and end, the steps of a Quit and of the pause menu opening) to `<file>`; pocket-core inherits it. Audio events include `audio-push` (input/output/queued frames), `audio-write` (remaining queue and writer phase), `audio-prime` (priming write duration), `audio-pause` and `audio-underrun` for cadence/resume diagnosis. Off when unset. Set by `scripts/latency.sh`, see [LATENCY.md](LATENCY.md) |
| `POCKETUI_SNAPSHOT_POCKET_CORE=1` | Snapshots describe games as started on pocket-core although it isn't installed (set by `--snapshot curator:…` itself) |
| `POCKET_UI_HOST_ALL=1` | Also route systems that are wired for pocket-core but not verified |
| `POCKET_HAPTIC_TICK=<ms>,<strength>` | The menu haptics' tick ([HOST.md](HOST.md)) |
| `POCKETUI_VERSION=<version>` | At build time: the version `pocket-ui --version` and the Updates page report (see [Building](#building)) |
| `POCKETUI_COMMIT=<sha>` | At build time: the commit the report's `system.txt` names (the release scripts set it; a build of your own says "not recorded") |
| `POCKETUI_FAN_ROOT=<dir>` | Smooth fan and the Performance & fan readout read and write the fan and thermal files below `<dir>/sys` instead of `/sys`: mock files for the virtual device ([FAN-CURVE.md](FAN-CURVE.md#the-virtual-device)); the launcher's hand-back honours it too |
| `POCKETUI_FAN_MIN_PWM=<0-255>` | Smooth fan's lowest running PWM for this fan, once measured on the device (40 by default; [FAN-CURVE.md](FAN-CURVE.md#on-the-device)) |

**pocket-core**

| Variable | Effect |
|---|---|
| `POCKET_TEST_SAVE_DELAY_MS=<ms>` | For a device test of a slow card: pocket-core's exit save takes this much longer (`quit::still_saving`: nothing stops a game while it writes). |
| `POCKET_TEST_CORE_STATE_LIMIT_S=<seconds>` | For a device test of a PSP game that hangs while saving: pocket-core waits this long for the exit save before it gives up and ends the game itself, instead of its built-in limit (`CORE_STATE_LIMIT`, `src/bin/pocket_core.rs`) |
| `POCKET_CORE_NO_SHORTCUTS=1` | Set by pocket-ui for a Kids Mode game: pocket-core's HOME shortcuts are all off, also after applying the game's settings ([KIDS-MODE.md](KIDS-MODE.md)) |
| `POCKET_CORE_NO_OVER_GAME=1` | Set by pocket-ui for a Easy game: no FPS, CPU or Pixel Path line over the game and no HOME + Y, whatever Over the game says; the setting itself is untouched ([BIG-SHELF.md](BIG-SHELF.md)) |
| `POCKET_CORE_TRACE=1` | Print every libretro environment call a core makes that pocket-core does not answer; a core's log has no 2 MB limit then |
| `POCKET_CORE_TEST=1` | The command FIFO also takes test input (`press A 150`, `axis lx 0.8`), `peek <offset>` and `achievement-demo` ([HOST.md](HOST.md)) |
| `POCKET_CORE_AUDIO=alsa\|pulse` | The sound output (ALSA through PipeWire's plugin first, PulseAudio's simple API as the fallback) |
| `POCKET_CORE_AUDIO_MS=<ms>`, `POCKET_CORE_ALSA_US=<µs>`, `POCKET_CORE_ALSA_DEVICE=<name>` | The sound queue's target, the ALSA buffer, the ALSA device (`pipewire`) |
| `POCKET_CORE_AUDIO_PW=1` | Sample PipeWire's per-node errors from the stream's open for `audio session`/`audio sample` (benches; otherwise the first `audio stats` starts it) |
| `POCKET_CORE_RESAMPLER=linear` | The old linear sample-rate conversion instead of the windowed sinc (`src/resample.rs`); emergency only |
| `POCKET_CORE_QUANTUM=<frames>` | PipeWire's quantum while a game runs (`0` keeps ROCKNIX's; default 1024 on H700, 256 elsewhere) ([HOST.md](HOST.md)) |
| `POCKET_CORE_QUANTUM_OWNER=pocket-ui` | Set by pocket-ui for the games it starts: pocket-core leaves putting ROCKNIX's quantum back at its end to pocket-ui (`ui_sound::game_ended`), which does it in order with the shelf's own and never under a sound ([HOST.md](HOST.md)) |
| `POCKET_CORE_PREFERRED_HW=<context type>` | The OpenGL context a hardware-rendered core is told to prefer, for experiments |
| `POCKET_CORE_GPU_TIMER=1` | GL timer queries around the picture pass (scaling, sharpness, effect): pocket-core logs `gpu picture: … ms mean, median, p99` every 600 pictures ([HOST.md](HOST.md#picture-effects-2026-10-24)) |
| `POCKET_REQUIRE_GL=1` | `tests/picture_shaders.rs` fails instead of skipping when there is no headless GLES 3 context |
| `POCKET_CORE_REWIND_CHECK=1` | Compare every rewound frame with the frame shown the first time ([HOST.md](HOST.md)) |
| `POCKETUI_FRAME_CACHE=<dir>\|off` | Where drawn screen frames are kept between game starts (`$XDG_RUNTIME_DIR/pocket-ui-frames`, else `/tmp/pocket-ui-frames-<uid>`, both in RAM); `off` draws every frame anew ([FRAMES.md](FRAMES.md#cache)) |
| `POCKET_CHEATS_DB=<dir>` | Read the cheat database from another folder |
| `POCKET_RUMBLE_DEVICE=/dev/input/eventN` | The device that rumbles, instead of the one found |

**Set by pocket-ui for pocket-core** (not for setting by hand)

| Variable | Effect |
|---|---|
| `POCKET_LAUNCHED_AT` | The moment the game was asked for (`CLOCK_MONOTONIC`, ms), so pocket-core logs its start against it ([Start time](#start-time)) |
| `POCKET_LAUNCH_HOLD_UNTIL` | Cozy animations and "Where was I?": the `CLOCK_MONOTONIC` ms until which pocket-core keeps its window back (draws nothing, silent) while the launch animation plays or the card is read; a button ends it, an animation deadline more than 2 s away is ignored; `HOLD_STILL=1` waits for a button (`startup::held`) |
| `POCKET_LAUNCH_HOLD_STILL=1` | Set by pocket-ui with "Where was I?"'s hold: the game runs no frame during the hold once its moment is loaded (`startup::stands_still`) |
| `POCKET_LAUNCH_HOLD_READY` | Optional reminder-ready file: pocket-ui creates it when the decoded launch picture arrives; if it is still missing when pocket-core starts, the game plays immediately rather than waiting for an invisible card |
| `POCKET_SKIP_RESUME` | Set by pocket-ui (`frozen::at_launch`): pocket-core starts fresh and moves the auto state aside for Load: it was made for another version of the file |
| `POCKET_START_NOTE` | Set by pocket-ui: a line pocket-core shows over the game's first moments (`frozen::at_launch`) |
| `POCKET_LAUNCH_CRT` | `1`: pocket-core plays the old-TV power-on (`crt_on`) over the game's first 300 ms after the hold; set by pocket-ui only for Cozy with Motion Full. pocket-core prints `crt-on` (and traces it) when it starts: the hook for the sound |
| `POCKET_LAUNCH_CRT_SILENT` | `1`: the old-TV power-on without its sound; set by pocket-ui beside `POCKET_LAUNCH_CRT` when Settings → Sound & feel → Case sounds is off (`crt_on::silent`) |
| `POCKET_HANDOFF_FD` | The pipe pocket-core writes `saved` into for the hand-over (fast switch, Quit to the shelf) and `paused` when it has taken `pause` ([ARCHITECTURE.md](ARCHITECTURE.md#switching-games)) |
| `POCKETUI_FAN_REQUEST` | Smooth fan drives the fan: pocket-core writes the game's fan choice into this file (`/tmp/pocket-ui-fan.game`) instead of changing the trips ([FAN-CURVE.md](FAN-CURVE.md)) |

**Scripts**

| Variable | Effect |
|---|---|
| `POCKETUI_HOST` | SSH target of the scripts (`root@<device-ip>`); alternatively `POCKETUI_HOST=...` in `scripts/local.env` |
| `VDEV_NAME`, `VDEV_ROMS` | The virtual device's container name and ROM folder ([VIRTUAL-DEVICE.md](VIRTUAL-DEVICE.md)) |
| `VDEV_DOCKER_ARGS` | More `docker run` arguments for `vdev.sh up` (the Wi-Fi bench's `--network none`) |
| `WIFI_BENCH_HOST`, `WIFI_BENCH_RESULTS` | Where `scripts/wifi-bench/run.sh` runs the Wi-Fi bench (an SSH host with colima, or `local`; also from `scripts/local.env`) and where its results go ([WIFI-BENCH.md](WIFI-BENCH.md)) |

## Repository layout

Every module of `src/` is named here (checked by `doc_checks`, see [Documentation checks](#documentation-checks)); the
first line of each file says what it is for.

| Path | Contents |
|---|---|
| `src/game_card_model.rs`, `rating.rs`, `play_time.rs` | Shared card/box data, status/rating painters, playing-time labels and the shared history/display clock, independent of the full game-card page |
| `src/untar/`, `src/update/` | Feature-only Brick streaming extraction and atomic card installer; standard implementations stay in `untar.rs` and `update.rs` |
| `src/platform/` | OS adapters at existing I/O boundaries; unchanged Linux curl transport and target-selected implementations |
| `src/linux_modules.rs`, `android_modules.rs`, `library_names.rs`, `look_settings.rs` | Target module composition and shared library-name/disc-sheet rules and look values; [ANDROID-MVP.md](ANDROID-MVP.md) |
| `src/android_02_tests.rs` | Test-only: compiles the Android app's `ui.rs` and `play_cards.rs` into the Linux library tests so the Android controller and painter code runs on a host ([Android 0.2 controller regressions](#android-02-controller-regressions)) |
| `src/main.rs` | The `pocket-ui` app: the event loop, Home and the game lists, Settings (`SETTINGS_GROUPS`: the categories and their rows; `SETTINGS`: the rows one after another), the pause menu (`MainItem`, `main_items`), snapshots (`fn snapshot`) |
| `src/app_achievements.rs`, `app_appearance.rs`, `app_bluetooth.rs`, `app_cabinet.rs`, `app_cabinet_tv.rs`, `app_room_extras.rs`, `app_case_stickers.rs`, `app_card.rs`, `app_controls_card.rs`, `app_cozy_extras.rs`, `app_console.rs`, `app_curator.rs`, `app_connections.rs`, `app_emu_choice.rs`, `app_this_game.rs`, `app_history.rs`, `app_fast_switch.rs`, `app_where_was_i.rs`, `app_kids.rs`, `app_leds.rs`, `app_menu_open.rs`, `app_look.rs`, `app_messages.rs`, `app_motion.rs`, `app_pictures.rs`, `app_slideshow.rs`, `app_night_light.rs`, `app_perf_fan.rs`, `app_quick_profiles.rs`, `app_pixel_path.rs`, `app_players.rs`, `app_zoom.rs`, `app_disc.rs`, `app_earlier.rs`, `app_save_media.rs`, `app_display_out.rs`, `app_second_screen.rs`, `app_column.rs`, `app_save_trouble.rs`, `app_sync_conflicts.rs`, `app_library_sync.rs`, `app_ports.rs`, `app_port_perf.rs`, `app_rescue.rs`, `app_report.rs`, `app_settings_groups.rs`, `app_tools.rs`, `app_moonlight.rs`, `app_backup.rs`, `app_storage.rs`, `app_gamelist.rs`, `app_gallery.rs`, `app_home_panel.rs`, `app_search.rs`, `app_starter.rs`, `app_status_corner.rs`, `app_switcher.rs`, `app_sync.rs`, `app_tailscale.rs`, `app_touch.rs`, `app_tour.rs`, `app_trace.rs`, `app_web.rs`, `store_page.rs` | The App's side of one feature each (part of the `pocket-ui` binary): its threads, its painting, its input, its snapshots. The logic they drive is in the library modules below |
| `src/lib.rs` | The library both programs use: one `pub mod` per module below |
| `src/app_controllers.rs` | The Controller setup page: device/player identity, button capture before the merged menu pad, swap feedback and background saves; opened from Buttons and the game's Controls page |
| `src/button_test.rs`, `src/app_test_buttons.rs` | Settings → Help & tools → Test buttons: the pure state (one view per controller, lights, trigger travel with the 0.55/0.40 marks, sticks with the dead zone, the last press explained through the player's layout and the Button layout, the HOME / B + START hold to leave) and the page that draws it; fed per controller from `merge_pad`, before the merge |
| `src/input_corpus_tests.rs` | Captured built-in and Bluetooth descriptors through the real decoder, Mux/Inputs, both menu layouts and hotkeys; [fixtures and provenance](../tests/fixtures/input/README.md). Test-only Linux module, also mandatory with the Brick feature in the full gate |
| `src/pad_linux_decode.rs` | Shared evdev decoder for menus and pocket-core: SDL Linux mappings, positional buttons and hats, normalised trigger axes with hysteretic button edges |
| `src/pad_bindings.rs` | Controller layouts keyed by device identity in `controllers.json`: occupied-button swaps, worker-side persistence, in-memory profiles and releases using the role captured at press time |
| `src/pad_bridge.rs` | Linux standalone SDL adapter (`--controller-bridge`): separate uinput player seats for Dolphin and ARMSX2 (`bridged`; Azahar binds by GUID and keeps its own), identity-bound layouts, hotplug and rumble forwarding; [controller notes](#identity-bound-linux-controller-layouts) |
| `src/shelf.rs`, `cabinet.rs`, `cabinet_art.rs`, `cabinet_tv.rs`, `cabinet_tv_art.rs`, `case_stickers.rs`, `media.rs`, `room_light.rs`, `room_deco.rs`, `room_props.rs`, `look_sample.rs`, `switcher.rs`, `lastframe.rs`, `launch_picture.rs`, `where_was_i.rs`, `favorites.rs`, `recents.rs`, `marquee.rs`, `systems_grid.rs`, `system_notes.rs`, `system_art.rs`, `ui_art.rs` | What Home lists, the five looks (the Cabinet and its art; the games' cases and what is inside them, their pixel-art templates in `assets/media` and `assets/media/game` drawn by `assets/media/tools/draw_media.py`, and the shelf props, shared by the Cabinet and the Shelf look; the stickers on the cases, `assets/media/stickers`; light by time of day and the decorations that grow with play; the
Cabinet's CRT TV with its own picture thread, `assets/media/tv`), the look step's previews, the game switcher and the last frame of each game, the picture a game starts from, favourites, recent games, scrolling titles, the "Add games" grid, the console pictures, the interface's pixel art (empty pages, handhelds, Settings icons, review stickers, the emblems on the fronts of boxes that are not a game; `assets/ui-art`) |
| `src/document_check.rs`, `android/src/check_page.rs` | Android's Settings → Game check: `game_check`'s shared checks behind its `Files` seam, on a document provider's listing, and what is Android's own (folder grants, the APK's cores and emulator apps, the in-app player's limits). Compiled on Linux as well, so `cargo test` runs it; see [ANDROID-MVP.md](ANDROID-MVP.md#game-check-parity-with-linuxs-settings--games--game-check) |
| `src/nav_probe.rs`, `app_nav_probe.rs` | The navigation explorer's record of a frame: rows, focus and hints from the shared painters, where the App is, under `POCKETUI_NAV_PROBE` only ([The navigation explorer](#the-navigation-explorer)) |
| `src/document_library.rs`, `touch_ui.rs`, `device_model.rs` | Provider document identity/disc-set rules, portable hit targets and device names for shared art; Android uses these without Linux input/detection services |
| `src/settings_values.rs` | The values at the right of the Settings rows: their words, the thread that reads the ones needing a process or a file (Wi-Fi's network, Remote access, time zone, paired PCs), cutting a long one with "…" (`app_settings_groups.rs`; [ARCHITECTURE.md](ARCHITECTURE.md#where-the-other-features-live)) |
| `src/wifi_activation.rs` | Numeric NetworkManager device-state guard shared by user and watchdog connects: join the target activation, disconnect another SSID before switching, boot/resume settling; fake clocks and scripted nmcli tests need no sleeps |
| `src/net_state.rs` | The shared picture of the Wi-Fi (`Snapshot`: the stored wish, the radio, the address, the saved networks, the last scan with its typed error), the one serialised worker for switching, scanning and connecting, the generation that drops stale results, the retries ([SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#quick-settings-home-on-the-shelf)); read by the quick settings' Wi-Fi row and Settings → Wi-Fi |
| `src/big_shelf.rs`, `src/app_big_shelf.rs` | The Easy look (docs/BIG-SHELF.md): the order and jumps, and the screen, which reuses Kids Mode's case row and pause screen without its closed doors |
| `src/kids.rs` | Kids Mode's model: the choices (`kids.json`), the day's count and its clock (`kids-time.json`), the time warning, the PIN's dials and tries, the grown-up's steps, the recovery file; the App's side is `app_kids.rs`, the door in front of safe mode `app_rescue.rs` ([KIDS-MODE.md](KIDS-MODE.md)) |
| `src/heap_trim.rs` | `malloc_trim(0)` on a thread every 20 s: the free pages between the live pictures of the Cabinet, covers and pages go back to the system (30 min of monkey test grew the process from 53 to 154 MB without it) |
| `src/kept.rs` | A value the UI shows but never reads itself (a process, a file, a syscall that can stall on the card): read on a thread, kept in memory, a frame when it changed (`Kept`); the About page's free space and address, Settings' Bluetooth row, `wifi::ip` |
| `src/status_corner.rs` | The status corner at the top right of Home and the pages over it (time, Wi-Fi symbol while connected, battery): its layout and Wi-Fi symbol, the pill Settings, the pause menu's pages and the quick settings show it in (`Pill`), the thread that reads Wi-Fi and the battery only while the corner is on screen (and works out the Wi-Fi symbol's pixels before the first frame needs them), the minute wake-up (`app_status_corner.rs`; [ARCHITECTURE.md](ARCHITECTURE.md#where-the-other-features-live)) |
| `src/retro.rs` | Settings → Look → Style → Retro ([RETRO-STYLE.md](RETRO-STYLE.md)): whether it is on, its palettes Cocoa and Pea, the pixel frames, the selection bar and cursor, key caps, pills and round buttons the painters ask for, and its PixelLab icons (`assets/ui-art/retro/`) |
| `src/palette.rs`, `kit.rs`, `appearance.rs`, `motion.rs`, `frame_pace.rs`, `text.rs`, `title_block.rs`, `text_fallback.rs`, `type_scale.rs`, `font_style.rs`, `i18n.rs`, `gfx.rs`, `image.rs`, `scaled_layout.rs` | Drawing (including small-panel logical canvases and cached area reduction): colours (dark and light, five sets: Theme → Colours), the interface's shared parts (`kit.rs`: the hint bar in its two places, a Settings-style row and its selection, the explanation beside the hint pill, panels, sheets, the scroll bar, an empty page's picture and words; [DESIGN.md](DESIGN.md#design-language)), Settings → Look → Look → Appearance and Motion, type (`title_block.rs`: the big name of the selected game as a main title and a smaller subtitle, up to three lines, [DESIGN.md](DESIGN.md#a-games-big-name-srctitle_blockrs); the glyph cache, drawn ahead of time by a thread once the first frame is on screen: `Text::prewarm`; a character the font lacks is painted as "?" or left out, never as a box; the snapshot checks' record of where text went, `text::boxes`; `text_fallback.rs`: the system's Noto Sans CJK for the letters Barlow lacks, memory-mapped and outlined one glyph at a time, [DESIGN.md](DESIGN.md#japanese-chinese-and-korean-names)), Settings → Look → Text size (`type_scale.rs`: the one scale every size, row and line step goes through, [DESIGN.md](DESIGN.md#text-size)), Settings → Look → Style (`font_style.rs`: Barlow, Pixel or Rounded type, a process-wide setting every `Text` follows by itself, the alternative faces fitted to Barlow's capitals, [DESIGN.md](DESIGN.md#font-styles-srcfont_stylers)), Settings → System → Language (the table the text painter translates every string through, [Translations](#translations)), the software canvas, PNG/JPEG decoding |
| `src/library.rs`, `library_cache.rs`, `app_library.rs`, `library_meta.rs`, `layouts.rs` (the folder names and picture folders of other systems' cards, read in place: [OTHER-LAYOUTS.md](OTHER-LAYOUTS.md)), `gamelist.rs` and `gamelist_page.rs` (the player's EmulationStation `gamelist.xml` files, read-only: a tolerant scanner, a cache of its own in `<data>/gamelist-cache.json`, the name rule, the card's facts, the pictures and the page that offers the favorites and ratings; `app_gamelist.rs` is the App's side: [ARCHITECTURE.md](ARCHITECTURE.md#where-the-other-features-live)), `archive.rs` (zip handling: the one game of a zip, refusals with advice for several games, passwords, other packings and a CRC-32 mismatch: [HOST.md](HOST.md#how-it-works); `rom_crc`, the game's CRC-32 from the zip's directory), `dsv.rs` (DraStic's `.dsv` saves taken over into the DS's `.sav` at game start: [HOST.md](HOST.md), "NDS saves"), `softpatch.rs` (IPS/BPS/UPS patches next to a game, applied by pocket-core as RetroArch does, refusals with advice: [HOST.md](HOST.md#how-it-works)), `gallery.rs` (the screenshots on the card, `screenshot.rs`'s files: the names read from the end, the in-memory index with a count per game, the page's state and keys, the delete question, the small picture cache with its cap, the copy to the shelf's picture; `app_gallery.rs` is the App's side: [ARCHITECTURE.md](ARCHITECTURE.md#where-the-other-features-live)), `rom_id.rs` and `app_identify.rs` (games recognised by their checksum against libretro-database's No-Intro lists, cached in `<data>/identify/`; the card's "Wrong pictures? Look up again": [CURATOR-DATA.md](CURATOR-DATA.md#identification-by-checksum)), `disc_serial.rs` (PlayStation 1/2 discs recognised by the serial in their `SYSTEM.CNF`, named by libretro-database's Redump lists: [CURATOR-DATA.md](CURATOR-DATA.md#playstation-discs-the-serial), [user/library.md](user/library.md#playstation-1-and-2-discs)), `covers.rs`, `auto_covers.rs`, `game_pictures.rs`, `picture_cycle.rs`, `picture_loader.rs`, `slideshow.rs` (the idle slideshow's pure part: when it starts, the picture list, a picture prepared for the screen, the timing and the camera, the bilinear paint: [SLEEP.md](SLEEP.md#idle-slideshow-2026-10-30-srcslideshowrs-srcapp_slideshowrs)), `auto_texts.rs`, `wiki_text.rs`, `game_card.rs`, `game_info.rs` (the one-screen info page, X on a game in Minimal, Cards and Split: [DESIGN.md](DESIGN.md#info-page-srcgame_infors)), `game_box.rs` (the card as the game's box, front and back, X on a game in Shelf and Cabinet: [DESIGN.md](DESIGN.md#cabinet-look-srccabinetrs-srccabinet_artrs-srcmediars-srcapp_cabinetrs); its frames in `assets/media/back`, made with `tools/` there) | Reading the game library (one directory pass with `DirEntry` types and explicit symlink fallback; names: `display_name`, `tell_apart` for two games of the same name; disc sheets and their files; for Add games, folders of no system and discs missing a file); the cache of the last scan the first frame comes from, and the scan after it (the App's side in `app_library.rs`, [Start time](#start-time)); play status, notes, sessions and stickers (`library.json`); zip handling; cover art, the games' screenshots and title screens (parallel `snaps/` and `titles/` folders, a made-up `Remote` in the tests; `app_pictures.rs` is Split's cycle, the card's pictures and the shared 16-entry fitted-cover cache; `picture_loader.rs` decodes and scales these on its worker, newest queued request first; arrivals redraw only when the last frame requested that key) and the game card's descriptions, by hand and loaded ahead; the game card |
| `src/gamedb.rs`, `curatordb.rs`, `romfacts.rs`, `assets/gamedb.bin`, `assets/curatordb.bin`, `assets/data/curator-tags.json` | Offline game facts (Wikidata), Curator's feel tags and ROM file-name facts; see [CURATOR-DATA.md](CURATOR-DATA.md) |
| `src/civil_date.rs` | Calendar arithmetic on day numbers (local day, civil date, day of a date), shared by the play history and by Rediscover (Android has no play log) |
| `src/connections.rs` | Connections and Rediscover: a game's developer, publisher, series and genre matched loosely across the library (company words, genre spellings), series from titles where the data has none, the lists' order; the games played a year ago today or not for a year ([CURATOR.md](CURATOR.md#connections-and-rediscover-featconnections-2026-10-30)) |
| `src/curator/justplay.rs` | "Just Play" on Home: `Index::just_play` picks (unfinished, starred unplayed, unplayed, the rest), the App starts it through `launch` ([CURATOR.md](CURATOR.md#just-play-featone-button-play-2026-11-02)) |
| `src/curator/`, `curator_page.rs`, `pixel_path.rs`, `pixel_path_page.rs`, `pixel_live.rs`, `pixel_standalone.rs` | Curator: the index, the Lens, smart shelves, stickers, Echoes, Versions and Settings → Games → Curator's switches (`curator/prefs.rs`), the sort orders of the Lens and smart shelves (`curator/sort.rs`) and "Surprise me" (`curator/surprise.rs`), what Home's Curator row offers today (`curator/today.rs`); its pages; Pixel Path's report, grades and page, its strip over the running game, and its stages for ARMSX2, Dolphin and Azahar read from their settings files ([CURATOR.md](CURATOR.md)) |
| `src/play_log.rs`, `play_history.rs`, `history_page.rs` | Play history: the session log (`<data>/play-log.bin`, written on its own thread), the numbers of a week, month or year and the Year in review (also `stats_json` for the web companion), the History and review pages ([HISTORY.md](HISTORY.md)) |
| `src/consoles.rs`, `console_page.rs`, `assets/data/consoles.json` | The console fact sheets, the console page (X on a system) and the short facts on the shelf |
| `src/console_credits.rs`, `credits_page.rs`, `assets/data/console-credits.json` | Credits & links (X on a console page): people, makers, chips, family, best-selling game, and the links into the library ([Console credits](#console-credits)) |
| `src/search.rs`, `search_page.rs`, `letters.rs` | Finding games: the search index, matching and ranking, the recent searches; the search screen; the L2/R2 letter jump |
| `src/home_panel.rs` | The picture side of Home's Curator, Favorites and pinned rows: what it shows and where each cover goes; `tests/home_panel_labels.rs` renders every console's artless narrow tile with a one-game library, independent of the daily pick ([DESIGN.md](DESIGN.md#home-panels-srchome_panelrs-srcapp_home_panelrs)) |
| `src/setup.rs`, `settings.rs`, `on_start.rs`, `button_layout.rs`, `button_prompts.rs`, `app_buttons.rs`, `wifi.rs`, `keyboard.rs`, `remote.rs`, `secret.rs`, `sockets.rs`, `timezone.rs` | First-start setup, `settings.json`, Settings → Playing → On start and Buttons (Button layout: which face button plays A, B, X, Y in the menus, and Button symbols: see below), Wi-Fi (and its watchdog), the gamepad keyboard, file sharing and SSH switches, time zones |
| `src/features.rs`, `feature_previews.rs` | The feature registry: every optional feature with its switch, the profiles, the version it came in, its card; the cards and the profiles' page; the cards' pictures ([Adding a feature to the tour](#adding-a-feature-to-the-tour)) |
| `src/library_sync.rs`, `library_sync/` | Settings → Sync → Library data (its two-device tests in `library_sync/tests.rs`): the per-device files of the Syncthing folder, finding this device's edits, the field-by-field merge with tombstones, the play history union, applying the changes to `LibraryMeta`, `Favorites` and Curator's shelves ([SYNC-CURATION.md](SYNC-CURATION.md)) |
| `src/sync_conflicts.rs`, `sync_conflict_page.rs` | Saves with two versions from Syncthing: finding the `.sync-conflict-` copies, whose game they are, keeping one version and moving the others into `/storage/roms/sync-backups/<date>`; the page that offers the choice ([user/sync.md](user/sync.md#two-versions-of-a-save)) |
| `src/sync.rs`, `sync_page.rs`, `tailscale.rs`, `tailscale_page.rs`, `bluetooth.rs`, `bluetooth_page.rs`, `moonlight.rs`, `moonlight_page.rs` | Settings → Connections → Sync (Syncthing), Tailscale and Bluetooth, each as the logic and the page's state; PC streaming and Settings → Games → PC streaming (pairing, the PCs, their games) |
| `src/web/`, `web_page.rs`, `examples/web_dev.rs`, `scripts/web-shots.mjs` | The web companion: its HTTP server on threads of its own (`server.rs`, `http.rs`), where it listens and tailscaled's `whois` (`net.rs`), pairing, sessions and CSRF (`auth.rs`), the JSON API (`api.rs`), uploads (`files.rs`), the page (`page/`, compiled in by `assets.rs`), its tests (`tests.rs`); Settings → Web companion's state; the development server and the page's screenshots ([WEB-COMPANION.md](WEB-COMPANION.md)) |
| `src/phone_upload.rs`, `phone_upload_page.rs`, `assets/phone-upload/` | "Send from your phone" (Add games, Y): the upload server that exists only while its page is open (Wi-Fi address only, a code per opening, the page and two upload routes, files through `web::files`), the Pocket's page with the QR code and the status, the phone's page ([PHONE-UPLOAD.md](PHONE-UPLOAD.md)) |
| `src/game_check.rs`, `game_check_page.rs` (`app_game_check.rs` is the App's side; `examples/game_check_bench.rs` measures it on a device) | Settings → Games → Game check: "will my games start?". Re-uses `hostsys`/`systems_grid` (what a system needs), `library` (discs, stray folders), `archive::refusal_of` (zips); adds `KNOWN_BIOS` (known-good BIOS files per system, size and CRC-32) and the look at files the shelf doesn't list. Low priority, stoppable, reads only; findings capped per group |
| `src/tools.rs`, `tools_page.rs`, `backup.rs`, `backup_saves.rs`, `backup_page.rs` | Settings → Help & tools → Tools (ROCKNIX's tools and the Store's) and Settings → System → Backup & reset, each as the logic and the page's state ([SYSTEM-PAGES.md](SYSTEM-PAGES.md)) |
| `src/games_card.rs`, `usb_transfer.rs`, `app_usb_transfer.rs` | Copying games without Wi-Fi: a games card coming or going (a thread asleep in `poll`), USB file transfer (ROCKNIX's MTP gadget; retired, not offered: `usb_transfer::RETIRED`) ([DEVICES.md](DEVICES.md)) |
| `src/storage.rs`, `storage_page.rs`, `card_health.rs`, `game_files.rs` | Settings → System → Storage (`card_health`: the read-only SD card check row): the walk of the card by kind, the clean-ups and what they may delete, the low-space rule (`storage`), the page's state (`storage_page`); the files of one game, its saves and states, and their delete for the card's Hide or delete (`game_files`) ([ARCHITECTURE.md](ARCHITECTURE.md#where-the-other-features-live)) |
| `src/ports.rs`, `port_files.rs`, `port_perf.rs`, `port_perf_page.rs` | PortMaster (the catalogue with each port's version, the installed ports' port.json index that finds a port's cover and the update note, `ports::update_available`), its ports that need the player's files, and a port's own performance and fan (applied while it runs, the busy-core look, the page START opens on its card and Options in its pause menu). A running port is a guest (`standalone::guest`, `Guest::Port`): Quit closes its window, then SIGTERM and SIGKILL to the whole process group and the helpers that left it (`quit_port`) |
| `src/bin/pocket_core.rs`, `libretro.rs`, `video.rs`, `picture_fx.rs`, `audio.rs` (bounded opening PCM delivery and cadence-aligned resume reserve), `resample.rs` (prepared polyphase windowed-sinc conversion, ARM64 NEON; [audio path](AUDIO-PATH.md)), `audio_diagnostics.rs` (backend errors, negotiated PCM and writer pacing), `audio_pw_stats.rs` (verified per-client PipeWire observations), `audio_route.rs`, `h700_volume.rs` (scoped H616 hardware gain/mute workaround; [H700-VOLUME.md](H700-VOLUME.md)), `hw_render.rs`, `vfs.rs`, `hostsys.rs`, `state_policy.rs` (shared raw-state size exceptions), `state_file.rs` (shared Linux/Android compressed Auto codec, raw compatibility and namespace-safe copies; [HOST.md](HOST.md#pause-menu-features)), `core_options.rs`, `exit_guard.rs` (whole-session stall and independent Pause/Quit/SIGTERM recovery clocks, plus the PSP serialization guard), `log_shim.c`, `shaders/` | pocket-core: the libretro host, picture (and its turn: Screen orientation and the core's SET_ROTATION, `video::turned_shape`), the screen effects' numbers and their CPU reference (`picture_fx`), sound and the PipeWire quantum for the speaker or a Bluetooth sink (`audio_route`, which also names where the sound goes for Quick settings' Volume row), OpenGL cores, the virtual file system, the systems it hosts, the cores' options, the cores' log callback (C, built by `build.rs`), GLSL |
| `src/headless_gl.rs` | A GLES 3 context without a window (EGL device platform, surfaceless) for `tests/picture_shaders.rs` and `examples/picture_shots.rs`; pocket-core doesn't use it |
| `src/achievements/`, `achievements_page.rs`, `rc_shim.c` | RetroAchievements: the session in pocket-core, the account, offline unlocks, the pages; the glue to rcheevos (`vendor/rcheevos`) ([HOST.md](HOST.md#retroachievements-srcachievements)) |
| `src/options.rs`, `look.rs`, `emu_menu.rs`, `controls.rs`, `controls_card.rs` (the controls card: model of which Pocket button is which console button, from the player's `Mapping`, the core's snapshot names or a table read from the cores' input descriptors, and its painter; docs/user/controls-card.md), `preview.rs`, `slots.rs`, `states.rs`, `timeline.rs`, `rewind.rs`, `hotkeys.rs`, `screenshot.rs`, `app_screenshot.rs`, `extras.rs`, `over_game.rs`, `run_ahead.rs`, `power.rs`, `cheats.rs`, `cheat_pack.rs` (the Brick card's cheat database as one file), `rumble.rs`, `overlay.rs`, `osd_sensors.rs`, `emu_settings.rs` | The pause menu and its Options pages: settings per console and game, the Emulator and Controls pages (and the Pocket's D-pad and sticks turned with Screen orientation, `controls::turn_buttons`), the live preview, save slots and states of every emulator, Earlier (pocket-core's rolling states: play time, the cost guard that never stalls a frame, the writer thread and its caps; [HOST.md](HOST.md#earlier-rolling-states-srctimeliners-2026-10-25)), rewind, shortcuts, screenshots (HOME + A: the file names, the atomic write, the thread that encodes; `app_screenshot.rs` is the standalone emulators' side), fast-forward, run-ahead, CPU and fan, cheats, rumble, messages over the game, the device's state in the FPS line (Show FPS → Sensors: a reader thread, twice a second); the Options of PS2, GameCube, Wii and 3DS |
| `src/brick_hw.rs` | The Brick's own hardware (`--features brick`, [TRIMUI-BRICK-HW.md](TRIMUI-BRICK-HW.md)): the rumble motor, the LED driver, F1/F2 choices, the mute slider choice, each light's choices |
| `src/app_brick_hw.rs` | The App's side of `brick_hw`: haptics strength, what F1/F2 do, the battery lights' upkeep |
| `src/app_brick_lights.rs` | Settings > Lights & buttons on the Brick: the lights, F1, F2, the mute slider, and Each light one level down |
| `src/brick_profile.rs` | The TrimUI Brick under NextUI (`--features brick`, [TRIMUI-BRICK.md](TRIMUI-BRICK.md)): name, screen, detection, pad and standalone-only additional core routes ([core builds](TRIMUI-BRICK-CORES.md)); `Board` tells the Brick from the Smart Pro on the same card ([TRIMUI-SMART-PRO.md](TRIMUI-SMART-PRO.md)) |
| `src/brick_gl.rs` | The Brick's prototype (`--features brick`, [TRIMUI-BRICK.md](TRIMUI-BRICK.md)): the GPU through the system's EGL, a picture through a texture onto the screen (`pocket-fb --gl`) |
| `src/brick_gpu.rs` | The Brick's prototype (`--features brick`, [TRIMUI-BRICK.md](TRIMUI-BRICK.md)): page transitions handed to `pocket-fb --gl` over the sway IPC |
| `src/brick_gles.rs` | The Brick's prototype (`--features brick`, [TRIMUI-BRICK.md](TRIMUI-BRICK.md)): a GLES context for 3D cores on the vendor's EGL, frames read back to the CPU picture |
| `src/brick_picture.rs` | The Brick's prototype (`--features brick`, [BRICK-SHADERS.md](BRICK-SHADERS.md)): `pocket_picture`/`pocket_shaders`, pocket-core's renderer (shaders, screen frames) inside `pocket-fb --gl` on the picture a game hands over at its own size |
| `src/soft_screen.rs` | The Brick's prototype (`--features brick`, [TRIMUI-BRICK.md](TRIMUI-BRICK.md)): pocket-core's picture copied by the CPU into a `wl_shm` window |
| `src/bezel.rs`, `frames.rs`, `frames_more.rs`, `frames_tv.rs`, `frame_led.rs`, `frame_cache.rs`, `ambient.rs` | Screen frames: Auto and frame packs, the built-in frames by family, the power light, the RAM cache of drawn frames, Ambient ([FRAMES.md](FRAMES.md)) |
| `src/save_media.rs` | Save slots beside the game's save medium of its era, as that console's memory-card screen showed its blocks: the PlayStation's, PS2's and GameCube's cards, the Dreamcast's VMU, the Xbox's memory unit, the Memory Stick Duo (PixelLab pictures in `assets/save_media`, from public-domain photos where there is one, [SOURCES.md](../assets/save_media/SOURCES.md)), the game's own cartridge (`media::game_pic`) beside its save label, the blocks alone for the rest |
| `src/disc.rs`, `pause_saves.rs` | The pause menu's honest rows: the Disc row of a multi-disc game on pocket-core (`/tmp/pocket-core.disc`, `disc <n>`; [HOST.md](HOST.md#how-it-works)), and Save and Load only for the emulators where they do something, else one line where saving is ([ARCHITECTURE.md](ARCHITECTURE.md#the-pause-menu)) |
| `src/standalone.rs`, `hakux.rs`, `fallback.rs`, `first_game.rs`, `control.rs`, `quit.rs`, `proc.rs`, `fast_switch.rs`, `notice.rs`, `idle.rs` | PlayStation 2, GameCube, Wii and 3DS integration, the RetroArch safety net, the line over the very first game that names HOME (and the same words beside the first start's Finish, `told_on_last_page`), process control, Quit, Xbox (hakuX in RetroArch: the core's folder, the appendconfig, the network port, [XBOX.md](XBOX.md)), helper programs with time limits, the hand-over between two games, the "couldn't start" card, staying asleep behind a game |
| `src/this_game_page.rs` | The "This game" page's state: the pause menu's Picture and Extras rows, Controls and the standalone emulators' rows for a game that is not running, written at once as the game's own settings ([options.md](user/options.md#this-game)) |
| `src/h700_gba.rs` | H700 GBA core default, bundled ARM64 gpSP and preservation of existing mGBA Continue moments ([HOST.md](HOST.md#h700-gba)) |
| `src/emu_choice.rs`, `emu_choice_page.rs` | Which emulator plays a game: es_systems.cfg's emulators and cores, the choices in ROCKNIX's `system.cfg` (as EmulationStation keeps them), resolving them for a launch, the RetroArch keys setsettings.sh reads, writing under ROCKNIX's lock; the "Runs on" page ([HOST.md](HOST.md#choosing-the-emulator-runs-on)) |
| `src/net_check.rs` | A Wi-Fi network that wants a sign-in (a hotel, a train): one plain request to NetworkManager's `nmcheck.gnome.org/check_network_status.txt` (200 and "NetworkManager is online" is online; a redirect or another page is a sign-in), keeping the redirect's address (`Check`); the update, store and PortMaster errors point to the Wi-Fi page instead of "Check Wi-Fi"; `recheck` asks again for the Wi-Fi page's 15 s re-check ([SECURITY-HARDENING.md](SECURITY-HARDENING.md#the-sign-in-check)) |
| `src/portal.rs` | The sign-in helper's requests: loads a sign-in page with curl (redirects one by one, a session cookie jar, every host looked up and pinned, never the Pocket itself), sends a form only to its own site, asks the check after ([PORTAL-SIGNIN.md](PORTAL-SIGNIN.md#security-rules-and-their-tests)) |
| `src/html_form.rs` | A small tolerant HTML reader for sign-in pages: title, heading, text, forms and fields, refresh and trivial script redirects; one pass with hard limits, never a panic; what a form sends and its encodings |
| `src/portal_form.rs` | "Sign in…": the sign-in page's form in pocket-ui's style (fields, ticks, choices, buttons, the keyboard, the ends) and the made-up pages of its snapshots |
| `src/portal_page.rs` | The Wi-Fi page's sheet for a network that wants a sign-in: what it wants, its sign-in page's host, the other ways online with the Pocket's Wi-Fi address ([PORTAL-SIGNIN.md](PORTAL-SIGNIN.md)) |
| `src/update_hint.rs`, `app_update_hint.rs` | The shelf's once-per-version hint that a newer pocket-ui is ready: the rules and `update-hint.json`, the look 60 s after the start with Wi-Fi up (then daily, idle-only, backoff; `update_hint::step`), the On/Off switch and the dot, the notice whose A opens Settings → System → Updates, the Install-now/Later question and its low-battery line ([UPDATES.md](UPDATES.md#the-hint-on-the-shelf)) |
| `src/clock_check.rs`, `app_clock.rs` | Is the clock clearly wrong (before 2026 or before the build day, `build.rs` sets `POCKETUI_BUILD_UNIX`): the rule, the Time page's line, and the one shelf notice 45 s after the start |
| `src/manual_time.rs` | Settings → System → Time → Date and time: the picker's fields (local time, month ends, leap years, the start at the build day for a wrong clock) and setting the system clock and the RTC (`clock_settime`, `RTC_SET_TIME`; only `system_ctl::RealHw` calls them, [SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#date-and-time-by-hand)) |
| `src/system_ctl.rs`, `quick.rs`, `quick_paint.rs`, `quick_profiles.rs`, `osd.rs`, `display.rs`, `gamma.rs`, `night_light.rs`, `display_out.rs`, `second_screen.rs`, `device.rs`, `desk.rs`, `fn_keys.rs`, `mako_osd.rs`, `ui_sound.rs`, `pad.rs`, `players.rs`, `players_page.rs`, `zoom.rs`, `touch.rs` | Which handheld this is (the Mini V2 or an experimental SM8250 sibling, from the device tree; [DEVICES.md](DEVICES.md)), the second screen (the RG DS Plus's lower panel, its table of outputs and backlights; [DUAL-SCREEN.md](DUAL-SCREEN.md)), window handling without sway on the Steam Deck (`desk.rs`, x11rb, [STEAMDECK.md](STEAMDECK.md)), system controls, quick settings (and their Performance and Fan rows), the on-screen display, colour temperature and the night light (the schedule, the fade, its thread; [SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md#night-light)), playing on a TV through USB-C (sway's outputs, [DISPLAY-OUT.md](DISPLAY-OUT.md)), the volume keys (and hiding ROCKNIX's own volume box, `mako_osd.rs`), menu sounds, the gamepads (each device read on its own and merged, `pad::Mux`; opened at the very start, their events kept in `pad::Early` until the event loop is there), the players of pocket-core games and the pause menu's Players page, the touch screen (opened first thing, then supervised: `touch::supervise_with` with its `Bus` fake in the tests, `touch::linux::supervise` on evdev; `touch::IgnoreLog` is the one log line per spell of dropped touches in `app_touch`; and the Flip 2's lid switch, read with the pads) |
| `src/auto_off.rs`, `app_auto_off.rs` | Switch off after sleeping (L3): the plan on CLOCK_BOOTTIME, the RTC alarm (and its verified disarm before every power off and restart, [SLEEP.md](SLEEP.md#power-off-restart-and-the-rtc-alarm-2026-10-26-fixpower-off-rtc)), the decision at the alarm's wake, the note the next start says once over the game or on the shelf ([SLEEP.md](SLEEP.md#l3-switch-off-after-sleeping-2026-10-25)) |
| `src/sleep_settings.rs`, `app_sleep_power.rs` | Settings → System → Sleep & power: Screen off after, Sleep after, Switch off after sleeping, the power button's short press and hold; `sleep.json` and its older form ([SLEEP.md](SLEEP.md#sleep--power-settings-2026-10-26)) |
| `src/sleep.rs`, `powerkey.rs`, `power_menu.rs`, `low_battery.rs`, `heat.rs`, `battery.rs`, `battery_history.rs`, `battery_estimate.rs`, `battery_games.rs`, `leds.rs`, `perf_fan.rs`, `perf_fan_page.rs`, `fan_curve.rs` | Sleep and the power key (Screen off with the game running), the power menu (its battery line, marks and lines, Restart pocket-ui), the low-battery guard, the heat guard (the TrimUI Brick's card), the battery page and history, the battery per game (the pause menu's time left, the card's rate; minute samples buffered for ten-minute writes, with final flushes at sleep/shutdown/exit), the stick LEDs, Settings → Playing → Performance & fan, Smooth fan (the curve, its engine and thread, the hand-back note, the thermal simulation; [FAN-CURVE.md](FAN-CURVE.md)) |
| `src/manual.rs` | Settings > System > Manual: `docs/user/*.md` bundled with `include_str!`, a Markdown subset parser, layout and the reader |
| `src/update.rs`, `update/` (the download tests, and an install cut off at every step with the real launcher after it), `os_compat.rs`, `store.rs`, `untar.rs`, `bin/store_tool.rs` | Signed updates (and `pocket-ui --check-os-update`), whether a new ROCKNIX system still has what pocket-ui needs ([UPDATES.md](UPDATES.md#os-what-pocket-ui-needs-from-the-new-system)), and the Store |
| `src/starter.rs`, `assets/starter-pack.json` | The starter pack: free games for an empty card. The list (author, licence in the author's words, file, SHA-256), the install (room, offline, hash, zip, never overwrite, one atomic write) and the card's lookup ([STARTER-PACK.md](STARTER-PACK.md)); the page and Home's hint are `app_starter.rs` |
| `src/crt_on.rs`, `examples/crt_on_shots.rs` | Cozy's old-TV power-on after the launch hold: `plan` gives the black bars, the line and the glow as rectangles for a moment in time (host-tested), `Overlay::draw_solid` draws them; `crt_on_shots` renders frames of it through the real renderer on a headless GL context |
| `src/cozy_extras.rs` | The cozy extras: one switch per little touch (`Extra`), each with one home (`Extra::home`: the page Settings → Look → Cozy extras, a row of Sound & feel, or Sleep & power), kept in settings.json's `"extras"` (only values that differ from the start are written), read anywhere with `cozy_extras::on` (one atomic load). Rows of features still being built stay off the page until `Extra::built` says so; the keys are listed in the module's `Extra::key` |
| `src/system_art.rs` | Embedded console pictures and the saved `GbaModel` choice under Look → Cozy extras. `picture_id` selects SP art everywhere while `canonical` preserves the GBA ROM system. Linux and Android both save and apply the choice; `app_cabinet::console_look_changed` clears fitted art, case and TV caches on a change |
| `src/insert_rig.rs` | A game going into its console's real mechanism in Cozy: the rigs (`assets/media/rig/<system>/`, from photos), the one timeline (`pose`), a lid's 3D map, rear-slot cartridges' label-away turn (`face_at`); the optional SP's opening screen and bottom slot also keep their geometry in Quick. Drawn by `insert_scene`, started by `app_cabinet::paint_rig` with `cabinet_art::blit_affine`; `POCKETUI_RIG_ASSETS` reads a rig from disk without a build |
| `src/insert_scene.rs` | A game's start as a picture and a schedule, shared by `app_cabinet` and `android/src/insert.rs`: `Scene::paint` (rig or plain), `cues` (its sounds), `hold`, `HandOver` |
| `src/phone_shelves.rs` | The shelves of a phone's Games page, in the Cabinet's order ([ANDROID-MVP.md](ANDROID-MVP.md#shelves-on-the-games-page)); on Linux only under `cfg(test)` |
| `src/frozen_phone.rs` | Frozen games on Android: its state and copy paths, the mark beside the state, what a start does before the core sees a state, moments set aside in a slot ([ANDROID-MVP.md](ANDROID-MVP.md#frozen-games-018)); on Linux pocket-core uses its set-aside (`set_aside_all`, `set_aside_refused`, `restore_kept`) for a moment it doesn't load |
| `src/frozen.rs` | Frozen games: which recent games stand open and what a start does about a moment it can't use (`scan`, `at_launch`, `Mark`), plain functions; `app_frozen.rs` runs them and `app_cabinet.rs` paints the open cases (docs/user/playing.md "Frozen games") |
| `src/playful.rs` | Playful animations (the Cozy extra Settings → Sound & feel → Playful cases): the easing, wiggle, seat and disc-turn curves, how each console takes its game in (`Loader`, `keys`) and the Shelf's leaning spines (`Lean`, a spring run in fixed steps from the scroll glide's segment, so a snapshot shows what a frame does); drawn by `app_cabinet.rs` and `game_box.rs` with `cabinet_art::blit_sheared`, `blit_scaled`, `blit_spun`, `tray` and `lid` |
| `src/splash.rs`, `boot_fade.rs`, `bin/pocket_splash.rs` | The boot screen: its picture, timing, the turn into the frame buffer and the loop (`splash.rs`, all host-tested), the program around it (frame buffer, console, signals), and pocket-ui's fade in from black at the first start after a boot ([BOOT-SPLASH.md](BOOT-SPLASH.md)) |
| `src/trace.rs`, `startup.rs`, `scripts/latency.{sh,py}` | The speed guard: latency trace and benchmark ([LATENCY.md](LATENCY.md)); where the start time goes |
| `src/crash.rs`, `frame.rs`, `fsutil.rs`, `save_writer.rs`, `save_trouble.rs`, `logs.rs`, `net_log.rs` (`net.log`: the network daemons' journal and the saved networks' names, secrets blanked), `shutdown_log.rs`, `report.rs` | Panic log, error values instead of panics, atomic file writes, crash-safe saves (the battery save and the cartridge clock, `.rtc`, written off the game thread, each write's outcome kept), saves that don't reach the card (the kinds, their words, the note pocket-core leaves for pocket-ui), the logs on the card (`shutdown_log.rs`: pocket-ui's own `shutdown.log` for every power off and restart, fsynced per entry), Settings → Help & tools → About → Report a problem (the zip of versions, logs, settings and emulators for a bug report, every file cleaned of private details first: `Redactor`, its tests on `testdata/report/private-log.txt`; `app_report.rs` is the App's side) |
| `src/rescue.rs`, `system_cfg.rs` | The rescue path: the launcher's exit codes, safe mode's rescue page, the dialog before EmulationStation ([ARCHITECTURE.md](ARCHITECTURE.md#rescue-path)); ROCKNIX's `system.cfg`, every write under its lock ([ARCHITECTURE.md](ARCHITECTURE.md#rocknixs-systemcfg)) |
| `src/doc_checks.rs`, `src/testutil.rs`, `src/test_clock.rs`, `src/fixtures/`, `src/testdata/` | Tests only: the documentation checks; helpers shared by the unit tests (`write_executable`, the scratch folder `TestDir`, scheduling allowances for bounded waits); recorded tool output (bluetoothctl, Dolphin profiles, Syncthing, Tailscale) and data (the Curator golden set, ROM file names in the shapes found on real cards) the unit tests read |
| `deploy/` | Files that go onto the device: the launcher `run-pocket-ui.sh`, the `essway` drop-in, the boot screen's unit (`pocket-splash.service`), SD boot config (`sd-efi/grub.cfg`) and the quiet start for a card set up before (`grub-quiet.sh`, its test `test-grub-quiet.sh`), the SD image's initramfs hooks and notes (`image/`), the install package's scripts and README (`package/`), RetroArch defaults, benchmark and PINE tools |
| `scripts/` | Dev loop (`dev.sh`, `vdev.sh`, `devlock.sh`), tests (`test-linux.sh`), release (`release-app.sh`, `release-rustflags.sh`, `check-binary-paths.sh`), the SD card image (`make-image.sh`, `check-image.sh`, `image/`), the install package for an existing ROCKNIX card (`make-install-zip.sh`) and the two files for an official one (`make-fat-install.sh`), the Xbox core's files (`hakux-files.sh`, both packages), boot files (`make-sd-efi.sh`, `make-test-bootimg.py`), the quiet start on an existing card (`boot-quiet.sh`), `autostart.sh`, `flash-sd.sh`, `android/` helper scripts for the Android side of the install, translations (`i18n-extract.py`, `i18n-shots.sh`), other screen sizes (`size-sweep.sh`), `wayland-stub.py` (snapshots on Linux without a compositor) |
| `docker/` | The virtual device: image build, boot scripts, virtual pad and touch screen |
| `store/test-catalog/` | A tiny signed catalog with one package of every kind, for testing the Store |
| `vendor/rcheevos/` | RetroAchievements' rcheevos library (MIT), built by `build.rs` |
| `assets/fonts/` | Barlow, embedded in the binary |
| `tests/gallery_snapshots.rs` | The screenshot gallery's pages with data missing, dark and light (above) |
| `tests/controls_card_snapshots.rs` | The controls card's states on the made-up library, at 640x480 and 1024x768 with normal and large text (`cargo test --test controls_card_snapshots`) |
| `tests/card_rows.rs` | The game card keeps every row when an open card's rows are made again (above) |
| `tests/user_journeys.rs` | Player-facing renders on a messy temporary card: fresh-card setup, game launch/quit, Quick settings/Wi-Fi agreement, exactly one disarm/power-off, both offline sign-ins, typo search, Large text and a missing BIOS filename; the Xbox guest quit, a game that never starts, the starter pack, delete and Bluetooth journeys; real FIFO/event loop or snapshot renders with painted-text assertions, without a device |
| `tests/escaped_bug_outcomes.rs` | Host regression journeys: the pause frame after provisional 800×600 → 640×480 sizing is admitted immediately and accepts navigation; every Cabinet shelf at native 1024×768 keeps its case, title, showcase and hints visible in dark and light. Uses the compositor greeting stub, painted text and the final touch/launch rectangles; `POCKETUI_ESCAPED_EVIDENCE=<directory>` keeps Cabinet pictures for review. Dreamcast save/restart/Continue marker coverage lives in `libretro::resume_tests`. |
| `src/bin/pocket_core/quit_regression.rs` | Host PSP exit recovery: a child runs the real exit watchdog against a fake core blocked forever inside serialize. It must exit within five seconds with a shortened watchdog limit, and the next Continue must restore the previous saved marker. This does not establish real PPSSPP/device compatibility. |
| `tests/first_day.rs` | What a screen shows where its data is missing, from a new player's first day on a messy card (above) |
| `tests/cjk_snapshots.rs` | Japanese game names on the shelf, the card and in search, with and without the system's CJK font (above) |
| `tests/shelf_end.rs` | The Shelf look at the ends of its list at 1240x1080, 1920x1080 and 1280x960, dark and light: with the last, second-to-last or first row selected the list reaches down to the hints, a short list shows whole (the rows measured in the pictures; [DESIGN.md](DESIGN.md#layout-12401080-left-aligned-72-px-margin)) |
| `tests/small_screen.rs` | Every snapshot state at 640x480: the pages laid out for 1240x1080 are cut off at the edge rather than slicing past the picture (above) |
| `tests/text_size.rs`, `tests/layout-risk-states.txt` | Settings → Text size against every snapshot state at Large/German and the risk list at Medium/Large × English/German: no text runs into other text, a hint bar or an edge in Medium and Large ([DESIGN.md](DESIGN.md#text-size)) |
| `tests/font_styles.rs` | Settings → Look → Style against the risk list at 640x480/Large/German for Pixel and Rounded, the setting reaching a start, a theme's fonts winning, a `Text` following the style ([DESIGN.md](DESIGN.md#font-styles-srcfont_stylers)) |
| `tests/release_channel.rs` | `scripts/release-channel.sh` with made-up artefacts and a throwaway key: the signed list, what it refuses, `--check` ([UPDATE-CHANNEL.md](UPDATE-CHANNEL.md#tests)) |
| `assets/i18n/`, `tests/i18n_snapshots.rs`, `tests/i18n-states.txt` | The German table (`de.tsv`), the names kept in English (`keep.txt`), the strings never painted on their own (`skip.txt`); the test of both and the states it renders ([Translations](#translations)) |
| `examples/` | `cover_check` (which cover each game would get), `port_files_check` (the same for PortMaster's instructions), `gamedb_match` (ROM names to gamedb QIDs for build scripts), `curator_check` (Curator on real data), `run_ahead_bench` (what run-ahead costs a core), `timeline_bench` (what Earlier costs a core: state size, serialize, the writer; [HOST.md](HOST.md#earlier-rolling-states-srctimeliners-2026-10-25)), `battery_fake` (a battery history for the Battery page), `over_game_shots` (what a level of Over the game draws, the FPS line or Pixel Path's strip, over a screenshot, where the HUD puts it), `picture_shots` (pocket-core's picture effects over made-up test frames at 1240×1080 on a headless GL, `…-right` turned by Screen orientation, and `--time` for what each costs, upright and turned; [HOST.md](HOST.md#picture-effects-2026-10-24)), `feature_cards` (every feature card, dark and light, to PNG files, with the time each took), `fan_curve_sim` (the Deltarune case simulated: ROCKNIX's trips against Smooth fan, a table, a chart and the samples; [FAN-CURVE.md](FAN-CURVE.md#simulation)), `web_dev` (the web companion on 127.0.0.1 with a made-up library; [WEB-COMPANION.md](WEB-COMPANION.md#working-on-it)), `i18n_cost` (what the German table costs the text painter per string, [Translations](#translations)), `audio_tone` (a GBA's 1 kHz tone through pocket-core's audio path, played and measured by `scripts/audio-bench/`; [HOST.md](HOST.md#bluetooth-audio-2026-10-25)), `ui_open_bench` (where opening the UI sounds' output spends its time: dlopen, `snd_pcm_open`, `set_params`, the first writes, after a quiet spell; [HOST.md](HOST.md#ui-sounds-srcui_soundrs)), `media_sounds_play` (the media sounds through the real UI mixer, alone and as the Cozy launch queues them, recorded by `scripts/ui-sounds/device-capture.sh`) |
| `docs/` | Documentation, indexed in [README.md](README.md) |

`src/app_loading.rs`: slow pocket-core launch feedback; the due tick paints before advancing its next wake.

## Releasing the app

For the public channel: `scripts/release-channel.sh --repo <owner>/pocket-ui --in release-out` makes and signs the list
from the release artefacts and writes `site-updates/` for the website, `--check` compares the uploaded files with it
([UPDATE-CHANNEL.md](UPDATE-CHANNEL.md#releasing-step-by-step)). For a channel of your own,
`scripts/release-app.sh <version> <out-dir> "notes"` builds with that version, packs a tarball and adds it to a
signed `update.json`. Signing needs the private key (default `~/.config/pocket-ui/store-signing.key`, never
committed). The public key is compiled into `src/store.rs`. See [UPDATES.md](UPDATES.md) and [STORE.md](STORE.md).

Before a release run `sh scripts/release-check.sh` (the bookkeeping tests the merge gate skips, see
[Documentation checks](#documentation-checks)).

The public channel is `https://pocketui.xyz/updates/stable/update.json` (`update::DEFAULT_URL`), with the files on
GitHub Releases; `scripts/release-channel.sh` makes and signs its list, nothing is published yet
([UPDATE-CHANNEL.md](UPDATE-CHANNEL.md)).

## Console fact sheets

`assets/data/consoles.json` holds one verified fact sheet per system (51 of them: 39 consoles, plus on 2026-11-03 the Amiga, C64, ZX Spectrum, Atari 5200, Atari 8-bit family, PS3, PS Vita and Wii U, and four that are no console: PICO-8 and TIC-80, `kind` "fantasy", ScummVM and MS-DOS (the `pc` folder), `kind` "software", released "WW" for worldwide), compiled into the binary by
`src/consoles.rs` and looked up by ROCKNIX system id; ids that are the same console (`genesis`, `famicom`, `sfc`,
`tg16`, the `*h` hack folders, `wiiware`) are mapped in `consoles::ALIASES`. `megacd` is the Sega CD's other folder. Systems without a sheet (the PC Engine CD, which has no
article or Wikidata facts of its own, the black-and-white Neo Geo Pocket) and later models with other
hardware (WonderSwan Color, New 3DS) are other consoles and get no page.

- **The facts are not ours to edit.** Every value comes from the sources listed with its console (Wikidata,
  Wikipedia, the makers' figures). Don't add, round, correct or "fix" a fact without a source for the change, and
  add that source to the console's `sources`. The code shows the values as they are; it only formats dates
  ("1998-11-27" as "27 November 1998"), names regions ("NA" as "North America") and orders releases by date.
- `lines.*` are ready-made English strings for the shelf's short facts; the other fields feed the console page.
  A field that is missing or `null` is left out of the UI, never shown as "None": keep it that way
  (`consoles::tests::no_row_and_no_page_text_ever_says_none_or_null`).
- The tests check that the file loads, that every console resolves by its id and aliases, and which shelf rows a
  console gets. A new console needs its id in `consoles::tests::IDS` and, ideally, a picture in `assets/systems`.

## Console credits

`assets/data/console-credits.json` holds Credits & links for every console of `consoles.json` (`src/console_credits.rs`
reads it, `src/credits_page.rs` shows it). Unlike `consoles.json` it is **built, not edited**:

```sh
python3 -I scripts/build-console-credits.py review          # fetch (cached), print every candidate fact per console
python3 -I scripts/build-console-credits.py build --strict  # check the chosen facts, write the JSON
```

- **Sources.** Wikidata (CC0) through its SPARQL endpoint, and each console's English Wikipedia article, first section
  only (the infobox and the lead), through the MediaWiki API. Politeness as `build-gamedb.py`: one request at a time,
  2 s apart, a User-Agent naming the tool, Retry-After honoured on 429/503; answers cached in `~/.cache/pocket-consoles`
  (`CONSOLES_CACHE`), so `build` runs offline after the first `review`. `--refresh` fetches again.
- **What ships is chosen by hand, checked by the script.** `scripts/console-credits.in.json` lists per console the
  people, makers, chips (with their maker), family members and best-selling games, each with its `source`:
  `wikidata:<P>` (the console's item has that statement pointing at the fact's `qid`), `wikidata:^<P>` (the fact's item
  points at the console), `wikidata:family` (the fact's item is in the console's family item, not its generation),
  `infobox:<field>` (the field, markup removed, contains the name; for a best-seller, the title and the figure) or
  `lead` (one sentence of the first section has all of the fact's `words`, a few short phrases, and one phrase has the
  name together with what was done, `ROLE_WORDS`: "team led by Hideki Sato", so a sentence that merely mentions
  someone else doesn't pass). A chip's maker must
  be the chip item's P176 (shown as "made by") or P178 ("designed by"). `build` drops every fact that fails its check
  and says why; `--strict` fails instead. A `game_makers` name must be one of the console's checked makers.
- **No chips for programs.** A system whose `kind` is "fantasy" or "software" (PICO-8, TIC-80, ScummVM, DOS) has `chips: []` in the
  input: it exists only as a program, and the test lets it have none.
- **No copyrighted prose ships.** Only the short proof phrases stay in the input file; the output has names, figures, our own
  short role texts ("Led the design team") and our two-sentence `summary` per console. Write those only from facts on
  the page (Hifiteam tone: calm, no marketing), and keep them true without the reader knowing more.
- **People.** Wikidata has almost no "designed by" (P287) or inventor (P61) statements for consoles (only the
  WonderSwan's Gunpei Yokoi, 2026-10-23), so people come from the article's lead with the phrases as proof. The infobox
  fields "designer" don't exist for these articles. Launch titles have no field in either source and are left out.
- **Links.** A family member becomes a link when its `qid` is a ROCKNIX system of its own (`SYSTEM_OF` in the script;
  a model such as the Game Boy Pocket plays its console's games and is no system) or the input names a `system`
  (Triforce). Both are held to the ROCKNIX ids of es_systems.cfg (`ROCKNIX` in the script,
  `console_credits::ROCKNIX_SYSTEMS` in the tests). Consoles with the same chip are found at run time by the chip's
  `qid`: the same Wikidata item, never a similar name (the SG-1000's NEC µPD780C is not the Z80 of the Master System).
  The App shows a link only when the library has games for that system (`console_credits::folder_plays`, aliases such
  as `genesis` and `megacd` included); "By the makers" uses Curator's catalog (the developers from gamedb).
- **Tests** (`console_credits::tests`, `credits_page::tests`): the file parses and covers every console page, every
  fact names a checkable source, links point at ROCKNIX systems and never at the console itself, the same-chip links,
  the game makers, and the page's selection, scrolling and touch rows.

## Game data (gamedb and Curator)

`assets/gamedb.bin` (Wikidata facts) and `assets/curatordb.bin` (Curator's feel tags) are built offline and
committed; the device never fetches them. Rebuild with `scripts/build-gamedb.py` (Wikidata, cached in
`~/.cache/pocket-gamedb`) and `scripts/build-curatordb.py label` then `write` (a batch labeller, cached in
`~/.cache/pocket-curator`). Both files keep one zlib stream per system, so a lookup unpacks only its system.
The vocabulary, the golden set, the quality numbers and the reader APIs are in [CURATOR-DATA.md](CURATOR-DATA.md).
Never commit a personal library list or file names from it.

## Translations

The interface is in English, German or Spanish (Settings → System → Language, `src/i18n.rs`). Nothing in the code is wrapped for
translation: the text painter (`Text::draw`, `width`, `fit`, `wrap`, `draw_right`, `draw_clipped`) looks every string
up by its exact English wording in the language's table (`assets/i18n/de.tsv`, `es.tsv`) and paints that line instead. A new page needs no code
for it, only lines in the tables. A new language is a `Lang` value, a line of `TRAITS`, and a table with every line (`i18n`'s test
`every_language_has_every_line_with_its_placeholders`). Words and tone: [i18n-glossary.md](i18n-glossary.md). pocket-core reads the
same setting, so its notes over the game are German too. The one line put together in code is FPS + sensors
over the game: its numbers change twice a second, so `osd_sensors` looks up each part with words ("Fan {n}%") itself,
on its reading thread. The manual (`docs/user/`) stays English: the painter is
switched off while its pages are drawn (`Text::set_translate`).

| File | What it holds |
|---|---|
| `assets/i18n/de.tsv` | `English<TAB>German`, one line each, compiled into both programs. Placeholders for strings with numbers or names: `{n}` `{m}` `{k}` `{n2}`… a number, `{s}` `{s2}`… text kept as it is (a game, a network), `{t}` `{t2}`… text looked up in the table in turn ("Set to {t}" makes "Set to Dark" "Auf Dunkel gestellt"). The German side may put them in any order. `\n` is a line break. A `!` in front of the English (`!{t} and {t2}`) uses a pattern only when every `{t}` is a line of the table, so a short one doesn't take a game's name apart ("Rock and Roll Racing"). A `{s}` never takes in " · " or the end of a sentence; a paragraph of sentences the table has is translated sentence by sentence. Lines that don't parse are left out (and printed); the unit tests refuse them |
| `assets/i18n/es.tsv` | `English<TAB>Spanish`, the same lines as `de.tsv` in the same order, the same placeholders |
| `assets/i18n/keep.txt` | Strings shown in English on purpose: names, brands, addresses; `re:` lines are patterns (`^` `$` `.*`, `\d` digits, `\` for a literal character) |
| `assets/i18n/skip.txt` | Strings the extraction script finds that are never painted on their own: log lines, keys of other programs' files, pieces of longer strings the table translates whole |
| `scripts/i18n-extract.py` | Lists the strings of `src/` that read like words for a person and the ones without a German line (`--stub`: as table lines to fill in; `--summary`: only the numbers; `--check`: exit 1 if one is missing; `--stale`: table lines whose English the source no longer has) |
| `tests/i18n_snapshots.rs`, `tests/i18n-states.txt`, `tests/spanish.rs` | The test (above) and the snapshot states it renders in each language; the Spanish specifics |
| `scripts/i18n-shots.sh` | The same states as contact sheets (`SHOT_LANG=es` for Spanish), dark and light; lists what is cut short or runs past the edge only in German |

### Adding or changing a string

1. Write the English string in the code as usual.
2. Run `scripts/i18n-extract.py`. It prints the numbers and every string without a German line, with its file and
   line; `scripts/i18n-extract.py --stub` prints them as table lines to fill in.
3. Translate in `assets/i18n/de.tsv` and `es.tsv`, in the section of its page, with the words of the
   [glossary](i18n-glossary.md). A format string comes out of the script with `{s}` for each hole: make it `{n}` for a
   number and `{t}` for a value the table translates, keep `{s}` for a name. Put a name into `keep.txt` and a log
   line or a key into `skip.txt` instead. Every string painted is looked up, game names and descriptions too: a line
   of only `{s}` holes and a short word (`{s} of {s2}`) turns "The Legend of Zelda" into "The Legend von Zelda".
   Anchor it with a number or more words (`{n} {s} of {n2} {s2}` for "41.2 GB of 63.9 GB"); `i18n`'s test
   `the_names_kept_in_english_stay_english` runs `keep.txt`'s names through the table. Game names have a structural
   guard besides: the places that make a game's name (`library::load_with`, `library_cache::read`,
   `library::shelf_name`, the play history) hand it to `i18n::learn_names`, and a learned name is painted as it is,
   whatever the table has for the same words ("Black" is the Look page's word, "Time to Play" fits "{t} to {t2}";
   `tests/game_names.rs`). A name made only to be compared (`library::display_name` for the picture catalogue's
   matching) teaches nothing, and a new place that paints names it makes itself should call `learn_names` or
   `shelf_name`. The price: a word of the interface that is also the name of a game on this Pocket stays English.
4. Run `cargo test --release --test i18n_snapshots`. It fails, naming the strings (and the states), when a string
   of `src/` has no line, or when a German snapshot of `tests/i18n-states.txt` paints a string without one. The
   second catches what the script can't see: strings put together at run time. Add the states of a new page to
   `tests/i18n-states.txt`.
5. Look at the page in German, dark and light: `scripts/i18n-shots.sh` writes contact sheets of every state to
   `screenshots/i18n-de/`, with `cut-de-only.txt` and `overflow-de-only.txt` (strings cut short with "…", or drawn
   past the right edge, in German but not in English). German runs about a third longer than English. The text has
   to fit where the English fits; cut short only where the English is cut short too. Shorten the German before you
   touch the layout. Overlaps (a long label running into its value) only show on the sheets.

A changed English string loses its German line until the table is changed with it; the test says so. After merging
a branch with new pages, do the same: `scripts/i18n-extract.py --stub`, translate, add the new pages' states, run the
test, look at the contact sheets.

### What it costs

English costs one atomic load per painted string (4-5 ns). In German a string painted in the frame before costs one
hash look-up and no allocation: 23-30 ns for a line of the table, 27-32 ns for one a pattern made, 45-50 ns for a
name that isn't in it (build computer, a shared x86 machine, `cargo run --release --example i18n_cost`; measuring the
same string's width takes 360-420 ns). Only the first sight of a string that is not a line of the table runs the
patterns, 6-9 µs, once per painter (`i18n::Cache`). The table is compiled in; when German is chosen it is read on a
thread (1.2-1.3 ms), so the first frame doesn't wait for it. The Mini V2 is slower; measure there before relying on
these numbers.

## Conventions

- Animation deadlines include painting in the refresh budget (`src/frame_pace.rs`, `main.rs`); still pages continue to wait for input or real deadlines. Guard: `cargo test --profile gate --lib frame_pace::` (14 ms paints keep 60 animation frames/s, 25 ms paints keep 40, fast paints remain capped).
- Panics end the process (`panic = "abort"`). Anything that can fail for a moment (a surface, a buffer, GL
  setup) returns an error value; see `src/frame.rs`. The panic hook writes `crash.log` and leaves a notice for the
  next screen (`src/crash.rs`).
- Files the user would be upset to lose are written through a temporary file and a rename (`src/fsutil.rs`).
- Pictures from files (covers, previews, screenshots, frames) are decoded only through `image::png_reader`,
  `image::load_png`/`load_jpeg`/`load_image` or `bezel::load_png_rgba`, which refuse a header larger than
  `image::MAX_EDGE` / `image::MAX_PIXELS` before allocating anything; a damaged or hostile file is then "no picture",
  not an out-of-memory abort (review N2).
- UI text uses sentence case and plain words. Buttons are called by the names printed on the Mini V2 (A is the
  right face button).
- Documents in `docs/user/` describe only what exists. Anything planned is marked "planned".
- Verification state lives in [STATUS.md](STATUS.md): change a feature's proof, change its row ([Documentation checks](#documentation-checks)).


SM8550 experimental device profiles (`device.rs`: Pocket 6/TOP-DPAD, Nova and the Odin 2 family, [DEVICES.md](DEVICES.md)); the Anbernic H700 handhelds (`device.rs`: one `Model::H700` with a row per model in `H700_BOARDS`, [DEVICES.md](DEVICES.md#anbernic-h700-handhelds-2026-10-27-installable-2026-10-29-experimental); `hostsys::route_on` keeps N64/PSP/Dreamcast/DS on ROCKNIX there; `os_compat` only logs the H700 image's missing time-zone helper and `quantum`; `has_fan`, `has_touch`, `has_sticks`, `has_wifi`; the fan page, the LEDs row and the Wi-Fi page read them; `system_ctl.rs` reports `Radio::Absent` instead of mocking a radio)
reuse the existing layouts. `sleep_settings` normalises their buttons/timers and `sleep` keeps every path at
screen off, with an optional dark-time power-off and no RTC alarm. `power::set_fan`, `fan_curve::Engine` and
`hand_back`, plus launcher recovery, leave SM8550 PWM and thermal trips to ROCKNIX. CPU policies are discovered.

### Wi-Fi activation collision regressions

`wifi_activation::tests` advances a fake clock through boot/resume, joining an activation (success, failure and
50-second timeout), and switching SSIDs after disconnect. `wifi::imp::activation_tests` uses `testutil::TestDir` and
`write_executable` for nmcli: numeric states with translated descriptions, SSID lookup by UUID, observing an
existing activation through to success, and disconnect-before-up ordering. A controlled wait changes the fake
state without a sleep. `setup::friendly::maps_nmcli_errors_to_plain_language` checks every error class and its
German translation. `tests/user_journeys.rs::wifi_timeout_explains_the_next_step_without_nmcli_diagnostics` renders
the ordinary failed-connect handler (`setup:wifi-timeout`), which is also in the German/layout matrices.
`activation_tests::nm_with_iwd` is an nmcli stand-in for NetworkManager with iwd: profiles as files, iwd's mirrored
`<SSID>.psk`, and iwd's endless retry of a refused key (the device stays in `config`); the fresh-card incident of
2026-10-27 runs through it (wrong then right password, wrong twice, an old profile overwritten, a double forget).
`tests/user_journeys.rs::first_wifi_wrong_password_then_the_right_one_then_forget` does the same first start with the
real binary and stand-ins for `nmcli`, `ip`, `iw` and `curl` on PATH (`wifi_programs`).
`net_state::tests::watchdog_queued_during_a_user_activation_is_discarded` and
`a_radio_switch_stops_the_watchdogs_scan_before_it_can_connect` use channel/gate barriers to verify the shared
lane and cancellation when the player takes over.

The review regressions also cover `a_typed_password_replaces_the_old_activation_and_is_tried_once`,
`a_join_failed_for_missing_secrets_reopens_the_password_prompt`, and
`a_join_can_finish_after_networkmanagers_dhcp_timeout`. The joined error carries `GENERAL.REASON` through to the
password prompt. `radio_off_cancels_a_running_watchdog_activation_within_one_second` gates a real fake nmcli
process on a Unix socket, switches the fake radio off and condition-waits for cancellation; it checks `-w 30`
and the shared lane without sleeps. Network-not-found and device-not-ready errors have separate wording.

Odin 2 Mini/Portal use Xbox menu defaults (saved choices win); safe mode follows the device labels.
`pad::linux` also reads InputPlumber keyboard HOME/BACK aliases; paddle support is not claimed with the plain ds5 target.

Long-title regression: `cargo test --profile gate --test text_size long_game_titles` checks the
selected search row, switcher and game card with long Pokémon and Super Mario metadata, plus Burnout
Revenge, at Normal/Large, English/German, dark/light and 1240×1080/1280×960. Set
`POCKETUI_TEXT_FIT_OUTPUT` to retain the rendered PNGs. `POCKETUI_TEXT_FIT=1 scripts/size-sweep.sh`
uses the same long-title fixture for a wider sweep. Marquee unit tests compare the first frame and
selection resets pixel for pixel with one ellipsized text draw. The switcher reduces picture height on
960-line screens to keep the title and metadata clear of the hints; its neighbouring cards intentionally
peek through the screen edge, while selected-card overflow and all text intersections remain checked.
The title audit also covers the existing `Text::fit` paths in the pause header, Curator lists,
settings/storage/history rows and device web pages; the game card keeps its two-line wrapped header.

### Measuring small-panel layouts

`POCKETUI_SNAPSHOT_SIZE=640x480` writes a physical 640×480 PNG after painting the supported logical canvas.
`POCKETUI_SNAPSHOT_NATIVE=1` bypasses scaling for snapshots only, to compare raw-layout findings; never set it for
a scaled-layout sweep. `POCKETUI_FRAME_BENCH=1` repeats the settled final snapshot page 105 times (four warmups),
printing median logical paint, median paint plus live reduction, and p95 frame thread CPU time in microseconds.
Use `home`, `settings` or `main`; branches that return an earlier snapshot do not run this benchmark.
For large game lists, `--snapshot bench-shelf:<look>:<system> ignored.png` measures the first frame after
120 forward and 120 backward moves (Cabinet Right/Left, other looks Down/Up), after two warming passes.
`scripts/perf-library.py OUT 5000` creates a disposable generated library and covers; set `POCKETUI_ES_SYSTEMS`
and `POCKETUI_DATA` to its printed paths. Use `scripts/latency.sh -- --nav-system gbc` for live cold-cover scrolling;
the snapshot bench intentionally warms images first. See [LATENCY.md](LATENCY.md). It excludes
startup, PNG compression and compositor presentation. See [H700-SCALED-LAYOUT.md](H700-SCALED-LAYOUT.md).
`tests/vdev_profiles.rs` evaluates only the H700 profile-selection fragment of `docker/vdev/entrypoint.sh` under
`sh` and checks its screen, model, compatible, SoC, lid and absent touch against Rust device profiles; no Docker boot.

## Android feasibility branch (2026-10-28)

`android/` is an independent `cdylib` package using winit/android-activity and softbuffer. It includes selected existing
source modules by path, so it never compiles or launches the ROCKNIX app/services. `src/text_size.rs` contains the
platform-independent TextSize value (re-exported by settings); Android supplies its locale over JNI, not system.cfg.
Build, emulator setup, scope and controls: [android/README.md](../android/README.md),
[user guide](user/android-spike.md).
This is a feasibility experiment, not a supported release target.

## Platform adapters

`src/platform/mod.rs` selects OS adapters at I/O boundaries. `platform/linux_http.rs` owns the existing curl
transport for shared covers, pictures and ROM identification, including compressed-listing fallback. Network work
stays on the existing workers; this move changes neither curl arguments nor cache/persistence behaviour.

## Android launcher and in-app player

`src/lib.rs` selects `linux_modules.rs` or `android_modules.rs`. The Android entry depends on the normal shared
library; `platform/` supplies Android SAF/JNI, HTTPS/system queries and AAudio. `library_names.rs` and
`look_settings.rs` hold shared pure rules/values, re-exported by Linux. `audio.rs` keeps the shared queue/rate control; `platform/` selects the output sink. The EGL Android player uses shared `libretro::Core` and save logic. The complete composition, supported subset,
intent table and build/emulator scripts are documented in [ANDROID-MVP.md](ANDROID-MVP.md) and
[the Android build guide](../android/README.md). The older feasibility section above records the original spike.

Android portable rendering: `src/cabinet_tv_art.rs` holds the shared TV body/glass painter (Linux worker reexports it); `game_card_model` and `play_time` also provide the portable problem label and relative-time text used by box/info pages. Android target composition selects these painters without Linux services. See [ANDROID-MVP.md](ANDROID-MVP.md#main-refresh-2026-10-30).

The private Brick build keeps its extractor and card installer in `src/untar/brick.rs` and
`src/update/brick.rs`, compiled only with `--features brick`. Standard embedded assets remain
in their original locations; Brick translations and manual variants are separate. See
[BRICK-CFG.md](BRICK-CFG.md) for the reference comparison and outstanding verification.

### Android 0.1.14 refresh modules

`src/state_policy.rs` is the portable raw-state size exception list, used by
`hostsys::core_states_vary` and `android/src/player.rs`. PPSSPP is deliberately absent.
`android/src/phone.rs` paints Easy and the Games Just Play action; `ui.rs` refreshes
Curator user history before the shared choice policy and launches through `Command::Launch`.
`onboarding.rs` uses separate action IDs for Animations/Appearance so adding a Look
cannot collide with those controls. See [ANDROID-MVP.md](ANDROID-MVP.md#android-refresh-for-0114-branch-build)
for supported behavior and the remaining picture/player differences.

The pause-menu shot reader (`preview.rs`) accepts older one-image shots and an optional 4×1 game rectangle before the picture (the initial picture-first layout is also accepted), ignores invalid crop bounds, retries incomplete files, and bounds decoding to eight nonempty images, 4096² pixels each and 128 MiB per file. Both preview and shot reads use one open file handle so an atomic rename cannot mix requests.

The controls card remembers each console in `<data>/controls-card-seen` using `fsutil::write_atomic` before showing it; a failed write skips the interruption. Existing auto-resume states (including taken/retry states) consume the first-start mark without showing a card. Its twelve-second deadline participates in the event loop wake-up schedule. The card consumes the dismissing press before HOME/START shortcut handling. `user_journeys` exercises dismissal, an idle timeout and existing resume states with a stand-in game. Overlay taps take priority over the search and game card underneath; Tour and slideshow remain behind the card.

## Standalone Brick Bluetooth

The existing Bluetooth page uses the card's `brick-bluetooth` service through its systemctl stand-in;
`brick-bluetoothctl` preserves pairings on the card. See [TRIMUI-BRICK-BLUETOOTH.md](TRIMUI-BRICK-BLUETOOTH.md)
for lifecycle, firmware dependencies, host stubs and the controller-only audio boundary.

Brick button labels use `i18n::device_text` after translation and before `Text` measures or wraps them (including raw manual prose).
Learned game names bypass it; other device profiles retain HOME. The slow-load note is the shared one
(`app_loading`, every Linux device): its timer requests the due frame before advancing, so it reaches pocket-fb after the launch animation.

### Android 0.2 controller regressions

`src/android_02_tests.rs` includes `android/src/ui.rs` and `play_cards.rs` in the Linux
library-test harness, so real navigation, preference reload, control names, crop boundaries
and preservation of rejected PSP moments are checked without a device. Run
`cargo test --profile gate --lib android_02_tests`; include the focused `type_scale`,
`controls::tests` and `frozen_phone` filters after changes to the shared rules. Compile both
Android libraries with `scripts/check-platforms.sh android`. The disposable AVD journeys
and shell-only pinch/uinput probes are documented in [the Android guide](../android/README.md#android-02-preview).

Android’s internal player holds the reminder until a button or touch; `android/src/player.rs` has no reminder deadline.

### Identity-bound Linux controller layouts

The [input corpus](../tests/fixtures/input/README.md) records real device identities, capability bitmaps and axis ranges.
`cargo test --profile gate --lib input_corpus_tests` runs every public descriptor through the production decoder,
controller merging, game input, both menu-role layouts and shortcuts. The full gate additionally runs
`--features brick --lib input_corpus_tests`, including the built-in Brick whose USB ID collides with Xbox 360.
Never replace captured data with a generic controller descriptor or weaken expected roles to fit a new decoder.
Refresh with `scripts/input-corpus-capture.py` (read-only, including already-open InputPlumber nodes) or
`scripts/input-corpus-capture.c` on devices without Python; neither reads gameplay events or grabs a device.


`src/pad_linux_decode.rs` normalises evdev using the selected SDL Linux records in `assets/controllers/`:
buttons and axes are capability-ordered indices, digital hats stay hats, trigger axes produce L2/R2 button edges
with 0.55/0.40 hysteresis. `src/pad_bindings.rs` saves device permutations to `controllers.json`; its reader runs
on the supervisor, never during painting. `src/app_controllers.rs` captures input before the merged menu pad,
undoing menu roles and the current device permutation. Held releases retain their original role. Game-specific
`ctl.*` remains a separate stage; interactive assignments in `emu_menu` swap conflicts while old files stay readable.

Each `pad::Supervisor` entry owns a reader stop signal. Retiring an entry (node removal/replacement, including
udev metadata changes) signals its old reader before opening the replacement; dropping the supervisor retires
all readers. Linux polls evdev and an eventfd together, with nonblocking evdev reads and retry on EINTR/EAGAIN.
Idle pads sleep until input or retirement; if eventfd creation fails, polling falls back to a 100-ms cancellation
check. Retirement releases held input and sends Disconnected before closing the evdev fd. No grab is taken.
The log distinguishes a node change from a reader ending. A source's late completion cannot retire a newer source.

`src/pad_bridge.rs` runs as `pocket-ui --controller-bridge <program> <args...>` for ROCKNIX SDL Dolphin and ARMSX2
(`bridged`). Not Azahar: its qt-config.ini binds each button to the built-in pad's SDL GUID, and a seat's GUID changes
with every start, so a 3DS game would have no buttons. When the seats can't be made (no uinput, no event node) the
emulator starts without them, with ROCKNIX's controller setup. Four uinput seats remain present until that game exits; SDL sees only that wrapper's reserved VID/PIDs.
A frozen game's seats cannot collide with another game's. Unchanged triggers retain analogue travel; remapped trigger buttons drive SDL trigger axes.
Sticks stay analogue. Force feedback forwards to the real controller through `rumble`, whose discovery excludes
bridge pads. Host-owned Deck input and external players outside these adapters retain their native input path.

Targeted verification: `cargo test --profile gate --lib pad::`, `players::`, `controls::`, `emu_menu::`, `hotkeys::`.
The ignored `uinput_modes_two_pads_and_hotplug_use_the_real_reader` test needs root and `/dev/uinput`; run the
compiled library test executable with that filter and `--ignored --test-threads=1`.
The ignored `retiring_a_silent_reader_releases_held_input_during_capture` test uses a real uinput reader and a
fake directory generation change while L2 is held: retirement must release L2 and finish without another press.
`tests/user_journeys.rs::controller_capture_survives_disconnect_and_reconnect` drives the ordinary UI loop,
including the handheld's B, the timeout, successful capture after reconnect and rendered frames. Its ignored
`uinput_disconnect_during_controller_capture_keeps_ui_alive` counterpart uses the saved hid-generic Lite 2 S-mode
key/axis capabilities (057e:2009, version 0001); events enter through the actual evdev pump, then the isolated
driver FIFO (`tdisconnect` carries Disconnected). Axis ranges are synthetic. Run that compiled integration test
as root with `/dev/uinput`; it creates/removes only its own test node. These tests cover userspace lifecycle,
not Bluetooth transport, the H700 kernel or compositor liveness during a hardware lockup.
`scripts/test-controller-bridge.py <pocket-ui-binary>` (root, SDL2) exercises two identical D-input pads through the
actual helper and SDL, including distinct maps, held input, disconnect, reconnect and right-stick isolation.
It creates only its own input nodes when udev is absent and removes them after the run.
Snapshot states: `controllers:0`, `controllers:7:capture`, `controllers:7:swapped`, `controllers:7:disconnected`.

Update transport (`src/update.rs`): signed alternate sources, per-source 30-second no-growth and five-minute total limits; strict full-file SHA-256 after every attempt. `src/update/download_tests.rs` exercises blocked, redirecting, stalled, truncated and corrupt servers with real curl. See [UPDATES.md](UPDATES.md#bounded-downloads-and-alternate-sources-03).

Standalone card assets: `src/update/brick_assets.rs` hashes immutable installed components, downloads every missing component into RAM before card writes, and reuses unchanged files with a durable rename/recovery manifest. `src/update/brick_io.rs` caps RAM downloads and shares a 2 MiB/s, 256 KiB/fsync bulk-write budget between update extraction, web/phone transfers and save backup/restore; their worker threads use idle I/O priority. The fixed Brick boot script recovers interrupted moves and rollback before launching an app. Before UI startup, `pocket-ui --brick-seed-data SOURCE DESTINATION [RELEASE]` (`src/update/brick_seed.rs`) compares bundled engine data by content and atomically replaces only changed files through the same write budget. It invalidates the BIOS release marker before writes, so rollback repairs partial new data; unrelated player BIOS files stay untouched. The optional release marker is published by the same descriptor-relative helper with an exclusive temporary and atomic rename, refusing symlinks. `scripts/brick-update-pack.py` prepares immutable packages; `scripts/test-brick-update-pack.py` covers stable reuse and hosting-size refusal.

**Test buttons** (`src/button_test.rs`, `src/app_test_buttons.rs`): `merge_pad` hands every pad event to `test_buttons_event`
before the controllers are merged (as the capture above), with the controller of its source. The events are the
App's: the reader has applied the controller's own layout and then the Button layout, so `Mapping::physical` undoes both
(the same inversion as the capture) and a held button keeps the physical button it was pressed as. While the page is open
`user_event_inner` stops every pad event but volume and power at its top (`test_buttons_swallows`), so quick settings, the
leave dialog and the other handlers never see B, START or HOME; a press from no controller (`usize::MAX`: the touch
screen's tap on the hint pill, the FIFO) leaves at once. The way out is a hold counted in `ButtonTest` and ended in
`about_to_wait` (`test_buttons_tick`). Tests: `cargo test --profile gate --lib button_test::` (virtual pads through the
real `pad::decode::Decoder`: a trigger-as-axis Pro 2 and two pads), `--test empty_views` (no controller found).
The test driver's command FIFO has three verbs for it (`--test-driver` only): `tconnect <source> <key> <name…>`, `tbtn <source> <button> <down|up>` and `taxis <source> <axis> <value>`, which send what a controller of its own would (`tests/user_journeys.rs`, `the_button_tester_takes_every_button_and_is_left_by_a_hold`). Snapshot states: `test-buttons`, `test-buttons:idle` (nothing pressed yet: the two hints), `test-buttons:bt`, `test-buttons:two`, `test-buttons:remap`, `test-buttons:xbox`,
`test-buttons:hold`, `test-buttons:unknown`, `test-buttons:empty`.

Cartridge-clock and start-failure regressions live in `libretro.rs` (`clock_regression_*`, `full_card_regression_*`), `fallback.rs`
(moment protection and legacy migration) and `fast_switch.rs` (first-frame evidence survives SIGKILL).
See [HOST.md](HOST.md#failed-starts-and-continue-safety-2026-11-07) for the pipe protocol and temporary retry.
