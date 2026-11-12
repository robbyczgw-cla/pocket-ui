# Real device and virtual device

*Part of [DEVELOPING.md](../DEVELOPING.md). Read this when you deploy to or test on a handheld, or use the virtual device (vdev).*

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

`send` accepts: `up down left right a b x y l1 r1 l2 r2 select start home quit leave wake`, and reads where the UI is
before each key (`--at <page>`, `--launch`, `dev.sh expect <page>`: [DEVICE-TESTS.md](../DEVICE-TESTS.md#where-the-ui-is-tmppocket-uistatus)).
`wake` turns a dark screen on and is no button. The letters are the menus' roles
(a confirms, b goes back, x is the top button's action, y the left one's), the same in both Settings → Playing → Button
layouts: the FIFO is not a physical pad (`button_layout`). A real pad's buttons, and `vdev.sh pad press A`, which is
one, follow the layout. `quit` ends pocket-ui with
exit 0, which the launcher answers with a fresh start (not EmulationStation); `leave` opens the dialog SELECT + START
held opens. A running game reads the real pad, not this FIFO.

`deploy` overwrites `releases/dev` in place. If the launcher had marked it as the last known good version
(`known-good`, [ARCHITECTURE.md](../ARCHITECTURE.md#rescue-path)), it is copied to `releases/dev-good` first, so a dev
build that keeps crashing still has a working version to go back to.
Binary-only deploys also copy the active release's bundled `cores/` into the dev version. This preserves the H700
GBA default (gpSP) beside the new programs. A redeploy keeps an existing dev bundle; when an older deployment
already lost it, the previous release's bundle repairs it. Devices without bundled cores still use their OS cores.

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
| `env` | No `/storage/pocket-ui/dev.env` exists. Every pocket-ui and pocket-core has normal data/library paths, no test hardware, power, clock or snapshot overrides, no `POCKETUI_TRACE`, and `POCKETUI_NO_HISTORY` is unset, empty or `0` (the latter two do not hold recording off, [HISTORY.md](../HISTORY.md)). Explicit normal defaults and the launcher frame file are accepted |
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
[INSTALL-DUAL-BOOT.md](../INSTALL-DUAL-BOOT.md) phase 6), `deploy` restarts the `essway` unit instead of starting its
own, and `stop` leaves a marker so that EmulationStation takes over until the next `deploy` or restart. The launcher
itself no longer opens EmulationStation after crashes; see [Rescue path](../ARCHITECTURE.md#rescue-path).

Other scripts that work against the device:

| Script | Purpose |
|---|---|
| `scripts/verify-device.sh [--no-local] [host]` | Is the device in its normal state: the unit active, exactly one pocket-ui in `essway.service`, normal paths and no test overrides or dev.env, the deployed binary running ([above](#after-a-deploy-or-a-measurement-scriptsverify-devicesh)). Run by `dev.sh deploy` and `latency.sh` |
| `scripts/devlock.sh <who> <command...>` | Takes a lock so that several sessions do not use the one device at the same time. Wrap short runs: `scripts/devlock.sh me scripts/dev.sh deploy`. A lock older than 20 minutes counts as stale |
| `scripts/syscheck.sh` | Plays a game on pocket-core with test input, takes screenshots, saves and loads, and measures CPU against RetroArch. See the header of the script |
| `scripts/hwplay.sh` | Scripted play with the virtual pad and screenshots on the way |
| Idle menu regression | `cargo test --profile gate --lib idle_shelf_requests_no_redraw`; real-binary measurement recipe and audit: [IDLE.md](../IDLE.md) |
| `scripts/big-emu-bench.sh [--minutes M] [--rumble] ROM[@FPS]...` | PS2 (ARMSX2), GameCube/Wii (Dolphin) and 3DS (Azahar) games on the Mini V2: started like pocket-ui, muted and dimmed, then frame rate (average, 1 % low, long frames), audio underruns, clocks, temperature, fan and battery, and a verdict per game; stops when the fan starts, puts the emulators' folders back ([BIG-EMULATORS.md](../BIG-EMULATORS.md)) |
| `scripts/pixelify-glyphs.py [FONTS_DIR]` | Patches the bundled Pixelify Sans (`assets/fonts/PixelifySans-*.ttf`, needs fonttools): C and c open (they read as O and o), a diagonal Z, a flat-topped 5; advances and bearings stay, running it twice changes nothing. An OFL modification, noted in `assets/fonts/OFL.txt`; the tests `retro_letters_are_crisp_and_digits_tell_apart_from_letters` and `pixel_font_look_alikes_tell_apart_at_every_size` (`src/text.rs`) check it from 21 to 240 px |
| `scripts/ui-sounds/prep_media_sounds.py RAW OUT KIND FROM TO`, `media-sounds.tsv`, `loudness.py FILE…` | The media sounds (case, box, cartridge, disc, card, CRT): ElevenLabs output cut, filtered and levelled into `assets/ui-sounds/` (the table: prompt and cut of each); the loudness (M max, LUFS, also as a small speaker plays it), true peak, DC and edges of WAV files ([HOST.md](../HOST.md#ui-sounds-srcui_soundrs)) |
| `scripts/ui-sounds/device-capture.sh BIN [STEP] [OUT]`, `device-ui.sh POCKET_UI [OUT]` | On a handheld (`POCKETUI_HOST`, under the device lock; `MS_SINK=null` with the speaker muted): the media sounds recorded off the sink, through the mixer (`examples/media_sounds_play`) or from the buttons in a pocket-ui with its own data and two test ROMs; `analyze_capture.py` gives per sound how late, M max, true peak, D+N against the file, gaps and clipped samples |
| `scripts/audio-bench/bench.sh [seconds]` | pocket-core's sound path measured off the device: ROCKNIX's PipeWire in a container on `AUDIO_BENCH_HOST` (scripts/local.env), a 1 kHz tone on the speaker's and the Bluetooth graph, dropouts, clicks, THD+N and pitch wobble against limits, exit 1 when one is broken ([its README](../../scripts/audio-bench/README.md)) |
| `scripts/autostart.sh` | Installs or removes pocket-ui as the boot UI. Dry run by default |
| `scripts/harden-ssh.sh` | Key-only SSH for development devices. Dry run by default ([SECURITY-HARDENING.md](../SECURITY-HARDENING.md)) |
| `deploy/bench.sh`, `deploy/frametime*`, `deploy/thread-cpu.sh` | Benchmark helpers ([HOST.md](../HOST.md)) |

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
battery, Wi-Fi, fan, suspend) are in [VIRTUAL-DEVICE.md](../VIRTUAL-DEVICE.md). Do not take performance numbers from
it.
