# TrimUI Smart Pro (experimental)

The Smart Pro is the Brick's sibling: the same chip (Allwinner A133P, PowerVR GE8300), the same TrimUI firmware family (TinaLinux,
kernel 4.9.191, glibc 2.33 from firmware 1.0.4 on), the same way in from a card (`runtrimui.sh` runs the card's `trimui/app/MainUI`).
One TrimUI card for both ([TRIMUI-BRICK-STANDALONE.md](TRIMUI-BRICK-STANDALONE.md); `pocket-ui-trimui-card-<version>.zip`), one update
channel (the Brick's, `updates/brick/update.json`: existing Bricks ask for that address); what differs is one profile.
Nobody on the project has held a Smart Pro. Everything below comes from public sources (other projects' code read for facts,
TrimUI's firmware notes); About says "experimental", and the project asks owners for feedback.

## Which one it is

`brick_profile::Board` (`Model::Brick(Board)`, the H700 boards' pattern): `Brick` or `SmartPro`. Everything the two share is
`device::brick()`; every difference is a method of `Board`.

| | Brick | Smart Pro |
|---|---|---|
| Found by (start.sh `board`, Rust `board_at`) | fb0 1024 wide, or MainUI says `Trimui Brick`, or nothing says Smart Pro | fb0 1280 wide, else `Trimui Smart Pro` inside `/usr/trimui/bin/MainUI` |
| `POCKETUI_DEVICE` (start.sh) | `brick` | `smartpro` |
| Screen | 1024 × 768, 3.2", 400 ppi | 1280 × 720, 4.96", about 296 ppi |
| Type (`type_scale::brick_base`) | small sizes a step larger | the design's sizes (Body 28 is 2.4 mm there) |
| Sticks | none | left ABS_X/Y, right ABS_RX/RY, about ±32767, centre 0, up negative; not clickable |
| 317 / 318 | F1 / F2 | not keys of its own (the pad's Xbox look announces them); pocket-ui makes them L3 / R3 |
| Mute slider (gpio243) | followed by `brick-hw slider-watch` | none: no follower, `BRICK_NO_SLIDER=1` for `volume restore` (gpio243 is something else there; a function switch arrives as EV_SW on the pad) |
| Lights (`/sys/class/led_anim`) | zones `lr m f1 f2`, Settings › Sound & feel › Lights & buttons | rings `l r m`, one `max_scale`: written 0 at start, no lights page until someone with a Smart Pro has mapped them |
| Stick power (gpio110, gpio114) | — | set high at start (every other project does) |
| Motor | gpio227 | gpio227, and a voltage first (`POCKETUI_MOTOR_VOLTAGE=500000,2200000`, µV) |
| What the display shows | fb0's page (`pocket-fb` reads it back) | the GPU's layer, not fb0, by other projects' findings: still pictures go through the GPU too (`pocket-fb --picture/--message --gl`) and the bridge trusts the GPU without the read-back (`--gl-trust`) |
| About / first start | played on | "(experimental)", called untested |

The Smart Pro S (TG5050: Allwinner A523, Mali-G57, kernel 5.15) is another system. start.sh knows it by `/etc/model`, the
device tree or `/proc/cpuinfo` (`TG5050`, `sun55i`), says on screen that pocket-ui does not run on it yet and switches off after
ten minutes; nothing is written to its hardware.

## Firmware

Smart Pro firmware 1.0.0 to 1.0.3 has glibc 2.29: none of our programs load, not even the one that paints the failure pictures.
start.sh asks `pocket-ui --version` and, when the loader names GLIBC, writes `pocket-ui/logs/FIRMWARE-TOO-OLD.txt` and stops. The
card's `pocket-ui/README.txt` says it: firmware 1.1.1 (1.0.4 at least); the site should too.

## Feedback: Report a problem

No mode of its own: Settings › System › About › Report a problem, as everywhere. On a board the project has not played on (the
Smart Pro) its `system.txt` ends with `[input_devices]` (`/proc/bus/input/devices`: the pad's name and ids) and `[boot_log]` (the
last 60 lines of the card's `boot.log`: the board start.sh found, the GPU's answer). A Brick's report is as it was
(`brick_profile::report_section`).

## Tests

- `brick_profile` unit tests: detection (fb0, then MainUI, else the Brick), the profile, the report's section; the Brick's answers
  unchanged.
- `scripts/test-brick-card.sh` 5s, 5u, 5v: the card on a Smart Pro (device, no slider, stick rails), by MainUI's name, a Smart Pro
  S (the note, no hardware), firmware too old.
- `tests/brick_settings_walk.rs` at 1280 × 720 as `smartpro`: its name, its sticks and none of the Brick's keys, and text that
  fits: no cut line or overlap the same page at 1280 × 960 doesn't have too (what the two share, a long Spanish explanation at
  Large or "Game Boy Adv…" in a tile, is the 1280-wide layout's own, on main's Mini V1 the same).
- `tests/nav_explorer.rs` `*_on_the_smart_pro` (in the merge gate, `scripts/gate.sh`): every row reachable with the D-pad, B goes
  back, honest hints, in Settings, the overlays and the first start.
- The Brick's renders are byte-identical to main's (every state of `tests/i18n-states.txt` in two variants, SMART-PRO.md).

## Open until a Smart Pro has booted the card

Whether our GPU way shows a picture there; the real stick ranges (and the right stick's axis order, which one source gives the
other way round); the ring zones; what the function switch should do; the motor's feel; battery, mixer and the backlight's lowest
useful value (other projects use 4 of 255 there, the Brick 1).
