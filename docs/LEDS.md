# LEDs: the lights around the sticks

Settings > Sound & feel > LEDs (`src/leds.rs` for the choice, the scripts and the first start; `src/app_leds.rs` for the page).
Read from the ROCKNIX 20261001 image (`~/.cache/pocket-vdev/rootfs.tar`, `docs/VIRTUAL-DEVICE.md`); **nothing here
was tried on the real Mini V2**. The checklist at the end is for a person at the device.

## The hardware

Eight RGB LEDs, four around each stick, as multicolour LED class devices:

| | |
|---|---|
| left stick | `/sys/devices/platform/multi-ledl{1..4}/leds/rgb:l{1..4}/` |
| right stick | `/sys/devices/platform/multi-ledr{1..4}/leds/rgb:r{1..4}/` |
| `brightness` | 0..255 (the scripts write 255 as "on") |
| `multi_intensity` | three numbers in the order **blue green red** (ROCKNIX's `analog_sticks_ledcontrol` swaps its red/green/blue arguments into this; `battery_led_status` writes `0 0 255` for red) |

## What ROCKNIX does (platform SM8250; the Mini V2 has no LED quirk of its own, `devices/Retroid Pocket Mini V2` is a link to `Retroid Pocket Mini`)

| piece | what it does |
|---|---|
| `/usr/bin/ledcontrol off` | brightness 0 and intensity `0 0 0` on all eight; stores `led.color=off` |
| `ledcontrol rgb` | runs `analog_sticks_ledcontrol $(get_setting analogsticks.led)`; stores `led.color=rgb` |
| `ledcontrol battery` | LEDs off, stores `led.color=battery`, `systemctl restart batteryledstatus.service` |
| `ledcontrol rainbow` | a one-second colour sweep, then back to the stored state (needs `bc`; not used here) |
| `analog_sticks_ledcontrol B R G B R G B` | brightness, right stick RGB, left stick RGB. With any other number of arguments it prints usage and exits 1 |
| `battery_led_status` (the service) | every 15 s: `led.color` still `battery`? Then green above 40 %, yellow to 40 %, orange to 30 %, red to 20 % (all at brightness 255), flashing red below 10 %. Charging: orange to 94 %, then green. Any other `led.color`: the loop ends |
| `500-enable-led` (boot) | `ledcontrol $led.color`; **with nothing stored it stores `battery` and runs it: full brightness green. That is what a fresh install shows** |
| `031-analog_leds` (boot) | `analog_sticks_ledcontrol $analogsticks.led` if the key is set |
| `sleep.d/pre/001-led`, `post/001-led` | copy `led.color` to `sleep.led.color`, `ledcontrol off`; after wake `ledcontrol $sleep.led.color` (systemd's `system-sleep` hook runs both) |
| EmulationStation | "LED colour" runs `ledcontrol <off\|rgb\|battery>` (`ledcontrol list` gives the choices); "Analog sticks LED colour" saves `analogsticks.led` and runs `analog_sticks_ledcontrol`. The "LED brightness" row (`ledcontrol brightness`) needs `DEVICE_LED_BRIGHTNESS`, which SM8250 does not set |
| START + Vol | ROCKNIX's `input_sense` calls `ledcontrol` with no argument (a no-op on SM8250); pocket-ui takes that combo over anyway (`docs/SYSTEM-CONTROLS.md`) |

system.cfg keys: `led.color` (`off`, `rgb`, `battery`), `analogsticks.led` (seven numbers), `sleep.led.color`.

## What pocket-ui does

The mode, colour and brightness live in `<data>/leds.json` (`/storage/.config/pocket-ui/leds.json`; the file existing
means "configured"). Every change goes through ROCKNIX's own scripts, on a worker thread, newest change first:

| choice | calls |
|---|---|
| Off | `systemctl stop batteryledstatus.service`, `ledcontrol off` |
| Colour | `analogsticks.led=B R G B R G B` in `system.cfg` (as `set_setting` writes it, under ROCKNIX's lock, `src/system_cfg.rs`), `systemctl stop batteryledstatus.service`, `ledcontrol rgb` |
| Battery | `ledcontrol battery` |
| Colour "The game's", game running | `analog_sticks_ledcontrol B R G B R G B` with the system's stripe colour, nothing stored; the menu colour (warm) when the game ends |

So the next boot applies the same through `500-enable-led` and `031-analog_leds`, and EmulationStation shows the
state the player chose here. The battery service is stopped first so its loop cannot write over the new state for up
to 15 s; `ledcontrol battery` starts it again. Brightness steps: 25, 50, 75, 100 % (63, 127, 191, 255 of 255).
Colours (red, green, blue): blue 30 90 255, warm 255 150 60, teal 0 200 170, green 50 220 80, violet 140 70 255, rose
255 60 130.

**First start:** if `leds.json` does not exist, pocket-ui runs the Off plan once (`default_off_if_unset`, the same
idea as `cap_gpu_if_unset`) and writes `leds.json` with Off. `led.color=off` in system.cfg makes the next boot start
dark. A saved choice is never touched. Without `/usr/bin/ledcontrol` (not ROCKNIX) or when the script fails, nothing
is saved and the next start tries again.

Limits: waking from sleep restores ROCKNIX's stored colour (the menu colour), not the game's; the battery colours
are always at brightness 255 (ROCKNIX's script); a later change made in EmulationStation is not read back into the page.

## Checks on the device

Read-only, before and after each step. Over SSH (or the on-device shell), as root:

```sh
for d in /sys/devices/platform/multi-led[lr][1-4]/leds/rgb:*; do echo "${d##*/}  brightness=$(cat $d/brightness) intensity=[$(cat $d/multi_intensity)] max=$(cat $d/max_brightness)"; done
grep -E '^(led\.color|analogsticks\.led|sleep\.led\.color)=' /storage/.config/system/configs/system.cfg
systemctl is-active batteryledstatus.service
cat /storage/.config/pocket-ui/leds.json 2>/dev/null
```

| step | expect |
|---|---|
| first start of this version (or `rm /storage/.config/pocket-ui/leds.json{,.bak}` and restart pocket-ui) | eight lines `brightness=0 intensity=[0 0 0]`; `led.color=off`; service `inactive` (after at most 15 s); `leds.json` has `"mode": "off"`; sticks dark |
| LEDs > Lights: Colour (blue, 50 %) | `brightness=127`, `intensity=[255 90 30]` (blue green red) on all eight; `led.color=rgb`; `analogsticks.led=127 30 90 255 30 90 255`; sticks blue |
| LEDs > Brightness 100 % | `brightness=255` |
| LEDs > Colour: rose | `intensity=[130 60 255]`; the sticks pink, not orange or white. If red and blue are swapped, the order assumption above is wrong: tell me |
| LEDs > Colour: The game's, start a game | `intensity` changes to the system's stripe colour (blue-green order), restored to warm (`[60 150 255]`) when the game ends |
| LEDs > Lights: Battery | `led.color=battery`, service `active`, all eight at brightness 255 and one colour by charge |
| LEDs > Lights: Off | all `brightness=0`; `led.color=off`; service `inactive` |
| restart the device | the same state as before, from early in the boot (ROCKNIX applies it, not only pocket-ui) |
| sleep, then wake (Colour) | dark while asleep (the LEDs keep the device awake otherwise), the colour is back after wake |
| pressing left/right ten times quickly | no stuck state: the last choice is what the sysfs lines show a second later |

Switch the lights off right now, from a shell on the device (stores `led.color=off`, so a restart stays dark):

```sh
systemctl stop batteryledstatus.service; /usr/bin/ledcontrol off
```

Without the service stop it also works, because the battery loop ends by itself within 15 s once `led.color` is no longer `battery`.
