# TrimUI Brick: the smoke test after an installation

Host tests were green for several Brick updates that then ran wrong on the device (lit LED raw picture in the dark, picture options and the FPS line
without effect, the route lost, the clock too low in a game). `scripts/brick-smoke.sh` runs on the dev machine against the real Brick after every installation and looks at exactly those things.

```
scripts/brick-smoke.sh [--owner-ok] [--expect-version V] [--host root@<brick>] [--rom FILE] [--log-dir DIR]
```

The Brick's ssh address comes from `--host`, `BRICK_HOST` or `~/.config/pocket-ui/brick-host` (kept out of the repository).

It takes `/tmp/brick-device.lock` for the whole run (10 minutes at most), copies `scripts/brick/smoke-device.sh` to the Brick's `/tmp` and runs it there
(BusyBox sh; the Brick has no bash, python, timeout or sftp). The pictures are compared on the dev machine. Output: a table and `~/brick-smoke-logs/smoke-<time>.log`. Exit 0 when nothing failed.
No reboot, no suspend, no `pkill`.

**Sound and light (owner's rule, 04.11.):** step 0 runs first: the speaker and jack switches must be off and `DAC volume` 0,0, else
`FAIL 0-mute` and nothing else runs. A watcher on the Brick puts them back off every second for the whole run; every time it had to is
`FAIL 9-sound-stayed-off`. With `--owner-ok` the panel is kept dim (`brick-hw bl-set 3`) while pocket-ui has it on; the picture checks
read the framebuffer. A Brick left dark is woken for 5 and 6. At the end the Brick is put into its dark state over the FIFO and checked:
`9-end-muted`, `9-end-dark` and `9-end-still` (the motor pin 0; the test games run with rumble off, nothing writes the motor) must pass. Options B and C are judged by what they do, since a game moves: Fullscreen makes the picture's area at least 5 % larger, Scanlines make neighbouring rows differ at least 1.5 times as much, each against the same measure's own noise (A against A2). Every other session on the Brick (an install, a restart of pocket-ui) follows the same rule:
mute first, keep it muted, darken right after pocket-ui is up, end dark and muted.

| # | Check | PASS when | Needs `--owner-ok` |
|---|---|---|---|
| 1 | Version | `app/RELEASE` is there (and equals `--expect-version`), the last update was not rolled back, `update-pending` is gone | |
| 2 | Services | `pocket-ui`, `pocket-fb`, `brick-hw watch`, `brick-net watch` run (`tailscaled` when Tailscale is on), a default route exists | |
| 3 | Clock in the menu | no cluster on `powersave`, `scaling_max_freq` is the top speed | |
| 4 | Lights, Lights=Off | every `effect_<zone>`, `effect_enable`, `frame_hex`, `anim_frames_enable`, every `sunxi_led*/brightness` is 0 (SKIP when Lights is not Off) | |
| 5 | Dark screen | `power` over pocket-ui's FIFO (`/tmp/pocket-ui.in`, the short press that pocket-ui takes itself; the Brick card never suspends, a dark screen is the screen off) makes the backlight 0; the clock rests (the dark menu: no `performance`, not pinned at the top; a game waiting in the dark: `powersave` at the lowest speed, 408 MHz); the lights stay 0; `pocket-ui`, `pocket-fb`, `pocket-core` use at most 5 % of a CPU over 6 s; the second `power` wakes it, the clock and the lights are right again | yes |
| 6 | Test game | a free homebrew (`--rom`, `BRICK_SMOKE_ROM` or `~/.config/pocket-ui/brick-smoke.gb`, at least 16 KB: Libbet and the Magic Floor, zlib; a copy under `.pocket-ui-test/smoke`) is started through pocket-ui (FIFO `play <system> <file>`, only files below a `.pocket-ui-test` folder): clock `performance` at the top speed; `/dev/fb0` (read only, the shown page) is compared for four runs with per-game settings: A integer scaling, B fullscreen, C scanlines, D FPS line on. B and C must change at least 5 % of the pixels (and four times the picture's own noise), D at least 1 % of the top-left corner. The game ends with `quit nosave`; the clock must be as before | yes |
| 7 | Owner's files | `Saves/` (md5) and the names and sizes of `Roms/` (not `.media`) are the same before and after | |

Without `--owner-ok` only 1-4 and 7 run (5 and 6 print SKIP). Use `--owner-ok` only when the owner is not using the Brick. The clean-up leaves the screen dark; if it is still on, the table says "the screen is STILL ON".
The test game runs with the card's sound as it is; keep the Brick muted (slider) while it runs.

What it cannot show: what the panel displays (a read of `/dev/fb0` proves the framebuffer, as in the first beta tests), and step 6 needs a pocket-ui that has the FIFO `play` command (this version on): an older one prints FAIL `6-start-A` and says why.

Test on a computer: `scripts/test-brick-smoke.sh` runs the same scripts against a fake sysfs/proc tree with a small simulator of pocket-ui's FIFOs; a healthy fake Brick passes, and each bug above (lit LEDs, `frame_hex` in the dark, options without effect, clock too low in a game or dark, no route, busy while dark, no `play`) is caught.


## Release journey and timing on the Brick

`scripts/device-check.sh --journey` adds a scratch-data release journey to the
existing device-check tooling. Run it from the host when nobody is playing:

```sh
scripts/device-check.sh --journey --label public-0.3.0.1 --expect-version 0.3.0.1 --out /tmp/brick-public-journey
```

It uses the installed UI and core. For a comparison, pass local ARM executables
with `--binary /path/to/pocket-ui --core /path/to/pocket-core`; they run from RAM
and never replace the installed application. The host needs Python 3, sshpass,
OpenSSH, ImageMagick's `convert`, a host in `~/.config/pocket-ui/brick-host`, the
password file `~/.config/pocket-brick/remote-password`, and a small legal GB ROM
in `~/.config/pocket-ui/brick-smoke.gb` (or `--rom`). Keep these files private.

The seven phases acquire the shared host `/tmp/brick-device.lock` separately with
nonblocking `flock -n` (a busy device fails before mutation),
release it between phases, and reserve cleanup time within a nine-minute limit.
`--phase first-start|perf|game|controls|settings|network|idle` reruns one phase.
`--phase perf-idle` combines the two measurements in one scratch UI, returns to
the Cabinet system shelf (or Home in list looks) after Details, and avoids repeating an ARM upload in a short device slot.
The output directory must be new. Each step writes PASS/FAIL and a JPEG;
`result.json`, `restore.json`, native timing traces and logs explain failures.
A foreign running game returns BLOCKED before changing files. A failed or
blocked phase makes the command exit nonzero. Inspect every phase result before
accepting a release; later steps continuing does not cancel an earlier failure.

**Known release blocker:** a real WLAN rfkill off/on cycle unexpectedly reset the
Brick firmware kernel. Its exact cause is unresolved; do not treat unchanged pstore
as proof that no reset occurred. The default network phase leaves the radio alone,
records BLOCKED (so the overall smoke stays nonzero), and only checks OTA.
`--wifi-cycle` is an explicit hazardous opt-in for a separately coordinated
investigation, not the normal release command. No repeat is needed to validate
UI frame pacing. Every input checks the original kernel boot ID in the same shell
command before writing the FIFO. A changed boot prevents later keys from reaching
the replacement UI; cleanup saves dmesg/pstore on the host, reports failure and
requires recovery from `config-before.tar`, because RAM backups disappeared.
Initial kernel/pstore snapshots are already saved on the host before the UI starts.

The journey covers scratch onboarding; D-pad selection and A on the scratch
cartridge (including the first-console controls card); game → pause → Save → Quit →
Continue; real F1/F2 input events; the installed slider watcher and volume helper;
D-pad reachability of Settings; a brief WLAN radio off/on cycle; an automatic
check against the Brick update channel without pressing Install; and new app,
kernel and pstore crash checks. The Settings explorer compares painted rows with
D-pad focus, enters only rows whose A hint says Open, and never activates service
switches. Both D-pad directions are explored, including rows above the initial
selection; all D-pad and A/B transitions wait for their native frame before checking the page.
Read-only or empty pages can have no selected row; B must still return to the original parent row. The remote slider check replays its GPIO input into the real watcher;
it cannot prove that the mechanical switch or its wiring works. Volume changes
are checked against the real codec while a wrapper and guard keep physical sound
off. The opt-in radio script restores WLAN autonomously if SSH drops; it cannot
recover from a kernel reset.

The installed UI is paused by its exact PID while a scratch process uses private
settings, ROM, SRAM, moments and home directory. Its FIFO and pre-existing core
files are parked and restored by rename. The script backs up original config
files, compares them afterwards, verifies installed UI/core/audio-file hashes,
and checks that its bounded guard ended. Config hashes are compared before the
original UI resumes, so its normal battery history writer cannot race the check. Unknown original files are never deleted.
CPU governors and frequency limits are recorded and restored after a game.
ALSA route and gain controls are compared too; the DAC stays zero for the night mute.
Backups can contain private settings: keep the output outside the repository.
Sessions end muted, dimmed to dark, with no test game or guard left running.

`perf` uses the installed library and look with a scratch library cache. Existing
identity, picture and description caches are copied with timestamps preserved, so
a steady shelf sample does not trigger a fresh checksum scan of every ROM. The automatic
update notice is disabled in the scratch data, so release and development builds
both stay on the shelf during an idle sample. The manual OTA check remains real.
The phase reports the UI process's first rendered frame, held-D-pad frame intervals and paint times,
and native input-to-frame menu timings. These are application frames, not a
panel refresh measurement. `--cold` drops filesystem caches before process start;
it is not a firmware cold boot. Measure a real boot separately from kernel
start ticks and the boot log, retaining the evidence and clock assumptions.
`idle` uses `scripts/baseline-device.sh`; `--idle-seconds` accepts 5–360 seconds.
The sampler pins the exact scratch PID, so a stopped owner with the same name
cannot be selected after kernel PID wrap. CPU time comes from the process group counters, including finished workers, with
the kernel's `AT_CLKTCK` clock rate. After 15 seconds to settle, an optional private
ftrace instance counts scheduler wakeups for the UI threads during the idle sample.
Its filter covers threads alive at sample start; later-created workers are excluded.
If unavailable, wakeups stay null with a reason; the baseline helper's context
switches remain a separately labelled proxy. Pre-existing trace settings and buffers
are untouched. Any debugfs mount belongs below the session's scratch directory;
cleanup verifies that both the trace instance and mount have gone. Battery discharge is left unmeasured
while charging; a short sample with integer battery percentages cannot establish
an accurate hourly discharge rate. Night runs keep the screen at 3/255 while on.

Host regression checks: `python3 scripts/test-brick-journey.py`, plus the existing
`scripts/test-device-check.sh`. They guard exclusion with shell flock, child
survival without BusyBox nohup, restoration inputs, service-row refusal, real
input encoding, D-pad reachability and held-input timing boundaries.
