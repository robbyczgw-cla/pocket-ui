# Xbox: hakuX in RetroArch (experimental)

Original Xbox games start from the shelf (`roms/xbox`, `.iso`) and are played by **hakuX**, rfandango's Xbox emulator, in
ROCKNIX's RetroArch. pocket-ui starts RetroArch itself (`src/hakux.rs`) and is its pause menu, as for PS2 and GameCube.
The spike that preceded this (PR #9 booted Jade Empire in RetroArch on the Mini V2) is in the private notes of 2026-10-28.

## What is in the box

- **Core**: `hakux_libretro.so` and `hakux_libretro.info` in `/storage/pocket-ui/hakux/` (not in ROCKNIX's libretro folder,
  not in a release folder: an update of pocket-ui leaves them alone), with `licenses/` and `SOURCE.txt`.
  Built from hakuX PR #9 at `2de4e9cd86a47e9ffaf928c36e240a995a279713`, unpatched (our audio relaunch fix, for Jade
  Empire's silence after its relaunch into the game, is part of the PR since `77a5871`), with the PR's own tools in an aarch64
  container (`scripts/build-hakux-core.sh`). Needs glibc 2.38 at most, Vulkan 1.3 (Turnip on the Mini V2).
- **Attribution**: *Xbox: hakuX by rfandango (GPL-3.0), Linux/libretro port by portare-ch (PortareOS), PR #9 commit
  2de4e9c (it includes an audio fix of ours).* In `NOTICE.md` (on the device in `/storage/pocket-ui/licences`, Settings → Help & tools → About), `docs/LICENCES.md`
  section 5.2, and the manual. The core is GPL-3.0-or-later and loaded by RetroArch, a separate GPL-3.0-or-later program;
  pocket-ui never loads it. Whoever ships the core ships `SOURCE.txt` (commit, link, checksums) and `licenses/`.

## How it starts

`hostsys::default_emulator("xbox")` returns `("hakux", "hakux")` once the core, its `.info` and `/usr/bin/retroarch` are there
(`hakux::installed`), so the shelf's Xbox games use it; the player can still pick xemu on the Runs on page.
`standalone::launch` calls `hakux::prepare`: the folders, hakuX's core options (written once) and the appendconfig
`<data>/hakux/pocket-ui.cfg`, rewritten at every start with a free UDP port for RetroArch's network commands. `main.rs`
`spawn_game` then runs

    /usr/bin/retroarch --config /storage/.config/retroarch/retroarch.cfg --appendconfig <data>/hakux/pocket-ui.cfg \
        -L /storage/pocket-ui/hakux/hakux_libretro.so <rom>

in its own process group, not through runemu.sh (which loads cores from `/tmp/cores` and writes the config itself).
CPU/GPU performance and the fan are pocket-ui's, as for the other big emulators (`standalone::set_performance` once the game
runs, `perf_fan`).

The appendconfig (`hakux::appendconfig`, tested): Vulkan on Wayland, fullscreen, vsync, no threaded video, no run-ahead,
no shader; **RetroArch's own 59.94 Hz clock instead of audio sync** (`audio_sync = false`, `vrr_runloop_enable = true`, see
"Frame pacing"); **all files of the game under `<data>/hakux/`** (`save/` holds the console's HDD image and EEPROM, `system/`,
`cache/`, `screenshots/`), never in `roms/xbox`; no auto-save/auto-load, no config written on exit; RetroArch's own menu and
exit/state/reset hotkeys set to `nul` (HOME belongs to pocket-ui); `network_cmd_*` on the session port.

## Pause menu

HOME opens pocket-ui's menu over the game, which is stopped (SIGSTOP) while it is up, as for ARMSX2 and Dolphin:
**Continue, Options, Quick settings, Quit.** Options is the standalone page of `emu_settings::XBOX`: Resolution (Original 480p / 2x 960p), Screen scaling (Aspect / Fullscreen / Integer, pocket-core's names) and Performance. Resolution and scaling are per-game system.cfg keys (`xbox["<file>"].pocket_resolution` / `pocket_scaling`; `xbox.<key>` for all Xbox games), written into the core's options and RetroArch's `aspect_ratio_index` / `video_scale_integer` at every start. The core reads its resolution once before the console boots, so both apply **at the next start** (`Live::NextStart`: a restart would lose the game, there are no states). Screen frames for Xbox would need RetroArch overlays: a later idea. While the game runs pocket-ui ignores the pad (no sounds, no shelf
navigation), like every standalone guest.

- **Quit** first brings the game's window to the front (`control::show_game`: while the menu covers it, RetroArch hangs presenting and ignores QUIT and SIGTERM, 18 s until SIGKILL), then sends RetroArch's `QUIT` over the network port, waits 10 s, then SIGTERM to RetroArch, then (8 s later) SIGKILL
  to the whole process group (`standalone::quit_plan(Emu::Hakux)`). The longest it takes is under the 35 s grace.
- **Save, Load, Earlier are not offered** while the core's `.info` says `savestate = "false"`: the core reports a
  serialize size of 0. The menu says "No save states for Xbox games yet: save in the game before you quit."
  (`hakux::NO_STATES`). **The Xbox's own in-game saves work** (they live in the HDD image in `<data>/hakux/save`).
- **Save states, wired but not testable yet.** Once a newer core's `.info` says `savestate = "true"` (RetroArch reads the same
  key), `Session::states` returns the 8 slots, Auto and Before load, and the pause menu shows Save and Load, Quit writes the
  auto state and the next start loads it (`SAVE_STATE_SLOT n` / `LOAD_STATE_SLOT n` over the network port; states in
  `roms/savestates/xbox/<game>.state1 … .state8`, `.state.auto`, `.state98`). Unit tests cover the gate and the names; no
  Xbox state has been written on a device, because no core can yet. A new core must be device-tested before its `.info`
  is shipped (a test fails on purpose when the shipped `.info` says `true`).

## Frame pacing

The guest ran at 59.5 to 59.9 fps, but RetroArch took only **~47.5 new pictures a second** from the core (Mini V2 at 1x and
2x, Odin 2 Mini 48.3 at 1x and 2x, GPU 10 to 57 % busy): every fifth picture was never shown and one was shown twice.

**Cause (measured on the Mini V2, 2026-10-29):** hakuX runs free (its own threads) and each `retro_run` hands over the newest
picture. ROCKNIX's RetroArch has `audio_sync = true`, so each `retro_run` blocks in its audio write until PipeWire takes the
next quantum, and ROCKNIX forces that to 1024 frames (`clock.force-quantum`, 21.33 ms at 48 kHz). `retro_run` then comes in
bursts at the quantum's rate, 46.9 times a second: the calls of one burst all find the same picture. The core's own statistics
said so: 60.2 `retro_run` a second, 16.5 ms each of which ~15.8 ms in the audio write, and a new picture on average every
**21.06 to 21.24 ms**, the quantum. The chip does not matter, so the Odin shows the same.

**Fix: RetroArch keeps the time itself.** Three runs of 70 to 80 s on the Mini V2's title screen (Jade Empire, 1x, RetroArch
started by hand with pocket-ui's appendconfig plus the variant, own test data, speaker muted, `HAKUX_DISP_STATS`,
`HAKUX_LR_STATS`, `HAKUX_APU_STATS`; 10-s windows while the guest ran at 59.9 to 60.0 fps):

| RetroArch | `retro_run`/s | new pictures/s | gap between new pictures | audio |
|---|---|---|---|---|
| as ROCKNIX sets it (`audio_sync` on) | 60.2 | **47.1 to 47.2** | 21.2 ms (max 64) | in step |
| `audio_sync` off | 61.7 to 64.6 | 58.8 to 59.3 | 17.0 ms (max 34) | the core makes it as fast as `retro_run` takes it: ~4 % ahead |
| `audio_sync` off + `vrr_runloop_enable` (shipped) | **59.94** | **59.9** | 16.68 ms (max 17.8, none over 25) | APU 1500 frames/s (real time), 0 short periods, 0 xruns |

With `audio_sync` off alone, Mesa's Wayland swapchain lets the loop run at about 62.5 Hz on the 60 Hz panel, and the core's
APU, which produces as fast as `retro_run` drains it below its low watermark, would run ahead of the sound card; RetroArch's
rate control (0.5 %) cannot take 4 %. `vrr_runloop_enable` makes RetroArch's runloop sleep to the core's 59.94 Hz itself
(its audio input rate is then the core's 48000 Hz), so the core's vblank locks to `retro_run` one to one again (a picture is
taken 8.25 ms after the guest's flip, half a frame, as the core's design wants). The recording of RetroArch's stream (25 s,
with `pw-record --target RetroArch`, the speaker muted) has signal in every second (-27 to -39 dBFS). No core change is
needed. Not yet measured: the Odin with the fix, 2x with the fix, and the pause menu's Continue (SIGSTOP/SIGCONT) with the
fix on a device.

**Core with the pickup timing fix (PR commit `7068703`, "time the frame pickup where the front frame is taken"), Mini V2,
2026-10-29:** Jade Empire from a fresh start into the first scenes, 1x, a copy of the data folder, 70 s per run, same appendconfig
except `audio_sync`. With our override (`audio_sync` off + `vrr_runloop_enable`): **59.9 new pictures a second**, gap 16.68 ms
(max 17.7 to 18.1, 0 over 25 ms) in every window once the guest ran at 59.9 fps. With ROCKNIX's `audio_sync` on and no
`vrr_runloop_enable`: **47.2 to 47.6 new pictures a second**, gap 21.0 to 21.2 ms, 85 to 104 gaps over 25 ms per 10 s. The core change
does not fix a frontend that blocks in audio, so the override stays. Sound is the same in both: APU 1500 frames/s, `short=0`,
no PipeWire xrun, the same 5.4 s without signal (the intro) and none after, so audio does not decide it. The Odin 2 Mini could
not be measured with this harness (the guest stays at its first frame there, with the shipped core as well): open.

**Core rebased onto hakuX master (PR commit `2de4e9c`, with upstream's v1.2.0), Mini V2, 2026-10-29:** the libretro part of the core
(`native/libretro/`) is byte-identical to `7068703`; the rest is upstream's newer HLE, kernel and network work (system link, XLink
Kai; no new libretro options, `retro_serialize` still returns 0, so the `.info` still says no save states). One run of 190 s each
of the old and the new core, Jade Empire from a fresh start through New Game and the relaunch into the game, a copy of the data
folder, our appendconfig (`audio_sync` off + `vrr_runloop_enable`), the speaker muted and the sink recorded
(`pw-record --target RetroArch`): both **59.9 new pictures a second**, gap 16.68 ms (max 17.6 to 18.0, 0 over 25 ms) in every
window once the guest ran at full speed; `retro_run` 59.94 a second; APU 1500 frames a second, `short=0` after the intro (the
same 3 to 4 short periods while the intro loads); the sound after the relaunch comes back after the same ~5 s of intro silence
(40 to 44 s) and carries signal to the end; no xrun; a RetroArch QUIT command (network) ends it in 2.0 s with either core. Not
measured: the Odin 2 Mini and the pause menu's Continue with this core.

## Shipping the core

`scripts/build-hakux-core.sh` builds the package from PR #9's commit plus any patches in `deploy/hakux/patches/` (none today; applied
with `git am`, committer and date fixed, so a patched commit has the same id everywhere) inside the PR's own Debian image on an
aarch64 Docker host (`HAKUX_BUILD_HOST`, the MacBook Air), with the PR's pinned dependencies cached in `~/hakux-core-deps`
there; `scripts/hakux-files.sh DIR DEST` copies everything that must travel with the core (both packages use it, so they cannot
drift):
`hakux_libretro.so`, the `.info` of `deploy/hakux/` (the one a test pins to "no save states"), `SOURCE.txt`, `BUILD-INFO`, our `patches/` and
the core's `licenses/`, with its own `SHA256SUMS`. `scripts/make-install-zip.sh --hakux DIR` puts that into the package as
`hakux/` for the SSH and ports install, `deploy/package/install.sh` checks the sums and installs it to `/storage/pocket-ui/hakux`
(tested in `deploy/package/test-package.sh`); `scripts/make-fat-install.sh --hakux DIR` puts the same files into the card seed as
`pocket-ui/hakux/`, which the card hook copies to the same place (tested in `deploy/fat/test-hook.sh`). Without `--hakux` a
package has no Xbox core and Xbox games stay with xemu. **The SD-card image build (`scripts/image/build.sh`) does not carry the
core yet.**

## Known limits and open points

- Experimental: one game (Jade Empire, the device owner's own XISO) has been run. Compatibility is the core's, not ours.
- **Pad input into the game was not verified through pocket-ui's start.** The tests that reached gameplay used the core's own
  `NC_PAD_SCRIPT` (START/A injected inside hakuX); a START sent through InputPlumber's virtual pad (`SendButtonChord`) did not
  reach the title screen. A real pad in RetroArch (autoconfig of the Mini V2's pad, hotkeys set to `nul`) needs a hands-on check.
- **Sound after the relaunch: fixed.** Jade Empire was silent from its relaunch into the game on (music, voices and effects:
  the core's APU state kept a stale DSP preference; our patch, now part of the PR as `77a5871`). Measured
  with the patched core (531ce8d + the patch; the PR's version of it is identical): 193 of 196 s after the relaunch carry signal (the 3 silent ones are the new XBE booting); before,
  52 of 52 s were digital zero.
- While a game runs for 120 s without input, the generic sleep logic stops it and darkens the screen like for any game; any
  input wakes it. A test that scripts the game has to keep sending input.
- **Resolution default:** 2x (960p) on the SM8550 handhelds (`hakux::default_scale`, `device::is_sm8550`), Original (480p) elsewhere. The Odin 2 Mini measured 2x at the same speed as 1x (below); the Mini V2 held 2x too but runs hotter: 2x (1280x960) gave the same pictures (47.4 a second against 47.5 at 1x, before the pacing fix), the GPU at its top clock (57 % busy at 585 MHz against 35 % at 343 MHz) and the hottest sensor 8 °C warmer (70 °C average, 80 °C top against 62 / 74 °C), so it stays at Original. The Odin reached 95 °C with its fan on its own (~3500 rpm) at 1x and 2x alike. A key the player set (`xbox.pocket_resolution`, per game or for all) always wins, and the Options row shows the device's default while nothing is set.
- Screen frames around the Xbox picture would need RetroArch overlays: a later idea.
- Bluetooth and headphones: RetroArch plays through ROCKNIX's pulse/PipeWire driver (`audio_driver` of the global `retroarch.cfg`,
  which our appendconfig leaves alone); the route pocket-ui sets for pocket-core games is not involved. Not tested.

## Device test (Retroid Pocket Mini V2, ROCKNIX 20261001, 2026-10-28)

All with pocket-ui from this branch (and, for the measurements, a RetroArch started by hand with a copy of the same appendconfig and
`HAKUX_DISP_STATS=1`), Jade Empire, speaker muted, battery 99 to 100 % and charging. Screenshots: `xbox-test/` next to the other web exports.

| Step | Result |
|---|---|
| Start from the shelf (search → Jade Empire → A) | RetroArch + hakuX start 31.6 ms after the press; title screen after ~15 s; the HDD image, EEPROM and caches appear under `<data>/hakux/save`, nothing in `roms/xbox` (the ISO's size and mtime are unchanged) |
| HOME | pocket-ui's menu over the game: Continue, Options, Quick settings, Quit, and the line "Xbox games keep their saves in the game. Save states aren't available yet." (the wording then; now "No save states for Xbox games yet: save in the game before you quit."; no Save, Load or Earlier) |
| Continue | the game runs on (SIGCONT), measured right after: 59.9 fps guest, 56 % of a core, 2 % GPU |
| Quit (first build) | RetroArch ignored the single `QUIT`: ROCKNIX's `quit_press_twice = true`. The plan fell back to SIGTERM after its 10 s wait; the process was gone after 12 s, "closed" after 18 s. **Fixed** in the appendconfig (`quit_press_twice = false`); a hand-started RetroArch with that setting stopped the core 106 ms after one `QUIT`. The fixed build was not run through the pause menu again |
| Title screen, 20 s in pocket-ui's start | 59.9 fps guest, CPU 74 % of one core (all threads), GPU 12.9 % busy at 587 MHz, 44.6 °C |
| 240 s with the core's pad script, 1x, twice (runs A and B; both 1x, see below) | in the village and the combat tutorial: guest 59.9 fps, **47.6 to 47.7 new images per second** taken by RetroArch (before the pacing fix; the spike's observation, reproduced), CPU 177 and 183 % of one core on average (181 to 203 % in the scenes), GPU 32 % busy on average (26 to 38 % per phase) with clocks 305 to 587 MHz, hottest sensor (thermal_zone6) 50.4 and 49.2 °C. The two runs agree within 1 to 4 % on every figure |

**Run B of this table ran at 1x, not 2x:** RetroArch reads `core_options_path` only with `global_core_options = "true"`, which
the appendconfig did not set then (it does now). 2x was measured that evening with the setting in place (see "Known limits").
Picture of the Xbox resolution at 1x for reference: `xbox-test/s4-game.jpg` (title), the combat tutorial in run A's `mid.jpg`.

## Device test: AYN Odin 2 Mini (SM8550, ROCKNIX 20261001, 2026-10-28)

Current main with the Xbox guest and the quit fix, core with the audio relaunch fix (hakuX PR #9 + `951124c7b4`), Jade Empire from the shelf
(FIFO: Xbox, A, A), the core's pad script for START/A, 240 s per run after 45 s, charging at 56 to 70 %, fan on its own, `HAKUX_DISP_STATS=1`,
Screen off and deep sleep set to Never meanwhile (without input pocket-ui stops a guest after 120 s). Runs interleaved:

| Run | Guest fps | New images/s (before the pacing fix) | CPU (cores) | GPU busy | Hottest CPU/GPU zone avg / max | Fan |
|---|---|---|---|---|---|---|
| 1x, first (device hot from earlier tests) | 35.6 | 35.2 | 1.36 | 7 % | 91 / 96 °C | 3820 rpm |
| 2x | 53.8 | 45.2 | 1.34 | 26 % | 80 / 95 °C | 3750 rpm |
| 1x, repeated | 58.8 | 48.4 | 1.31 | 10 % | 76 / 95 °C | 3510 rpm |
| 2x, repeated | 58.2 | 48.3 | 1.25 | 27 % | 78 / 95 °C | 3556 rpm |

The first 1x run started on a hot device and is the outlier; the two cooler runs agree: **2x holds 1x's speed on the Odin 2 Mini** (GPU 680 MHz,
CPU prime core 2.96 GHz in both). Sound after the core's relaunch: the speaker sink's monitor at 8 % showed music and effects (-35 to -31 dBFS RMS
5 s windows, peaks to 5700), then muted again. Quit through the pause menu: RetroArch gone 20 ms after the press, "Hakux closed after 211 to 303 ms"
(the fix of 134363b). Screenshots: `odin-xbox/` next to the other web exports.

## Device checks (Mini V2, 2026-10-28 evening and 2026-10-29)

Done: **Quit through the pause menu** ends RetroArch in 188 to 219 ms (three runs; it took 18 s before `control::show_game` came
before the `QUIT`); **2x** confirmed in the log (`render scale 2x`, 1280x960) and measured (above); **Screen scaling** Aspect,
Fullscreen (index 24 fills the panel) and Integer each seen on screen; **the HDD image is reused** on a second start (no new
format, the files bit for bit the same after start and kill); the spike's folders are gone (the owner's HDD image is in
`<data>/hakux/save/hdd`, a backup in `<data>/hakux/hdd-owner-backup-20261028`); **frame pacing** fixed (above).

Still open:
1. A real pad in the game (the Mini V2's buttons through RetroArch's autoconfig) and the Bluetooth headphones path: need hands.
2. The pacing fix on the Odin 2 Mini, at 2x, and across the pause menu's Continue, through pocket-ui's own start.
3. A title screen of this start once ran at ~9 fps for its first 30 s (the attract film); not looked into.
