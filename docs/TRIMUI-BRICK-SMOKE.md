# TrimUI Brick: the smoke test after an installation (private, branch `feat/trimui-brick-smoke`)

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

What it cannot show: what the panel displays (a read of `/dev/fb0` proves the framebuffer, as in the first beta tests), and step 6 needs a pocket-ui that has the FIFO `play` command (this branch on): an older one prints FAIL `6-start-A` and says why.

Test on a computer: `scripts/test-brick-smoke.sh` runs the same scripts against a fake sysfs/proc tree with a small simulator of pocket-ui's FIFOs; a healthy fake Brick passes, and each bug above (lit LEDs, `frame_hex` in the dark, options without effect, clock too low in a game or dark, no route, busy while dark, no `play`) is caught.
