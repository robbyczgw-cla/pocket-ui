# TrimUI Brick: its own hardware (private prototype, branch `feat/trimui-brick-hw`)

Rumble, lights, the two extra keys and the mute slider, for pocket-ui on the Brick's own card
([TRIMUI-BRICK-STANDALONE.md](TRIMUI-BRICK-STANDALONE.md)). Brick hardware and `Button::F1/F2` are behind `--features brick`. The shared input corpus also
protects the measured H700 and native Retroid/Odin layouts in ordinary Linux builds.

Hardware facts below name their source. Volume mixer ranges were read on the real Brick; the new volume sequence is covered by a stateful mixer trace (device validation is recorded separately).

## What is built

| Part | What it does | Where |
|---|---|---|
| Game rumble | a core's rumble (libretro `GET_RUMBLE_INTERFACE`) switches the motor through pocket-core's rumble worker; the per-game strength (pause menu > Options > Extras > Rumble: Off/Low/Medium/High) applies | `src/rumble.rs`, `src/brick_hw.rs` `Motor` |
| Menu haptics | Settings > Sound & feel > Haptics: Off / Low / Medium / High (a tick shows it); the tick is 12 / 22 / 38 ms | `rumble::set_menu_step`, `app_brick_hw.rs` |
| Lights & buttons | Settings > Sound & feel > **Lights & buttons** is one page (`app_brick_lights.rs`). The row has the page's name (`app_brick_lights::TITLE`, shown through `app_settings_groups::row_name`; the table and the ROCKNIX build keep "LEDs", the lights around the sticks), its value is **Lights off** while they are dark, else Battery or the colour, and the category line says "lights & buttons" too (`group_line`). **Lights**: Off (default), Colour, or Battery & charging (dark while all is well, orange breathing while charging, green when full, red breathing at 15 % or less); Colour has a hue or **The game's** (main colour of the game's cover; the system's stripe colour when there is none) and brightness 25/50/75/100 %. **Each light** (one level down, colour mode): left grip, right grip, middle, F1, F2, each with its own colour (like the rest, a preset, or **Own colour**: hue in 15° steps and saturation 25/50/75/100 %), effect (like the rest, steady, breathing, fading, pulsing, **colour cycle**, **battery & charging** for that light alone, off), **speed** for an effect that moves (slow 2000 ms, normal 1000 ms, fast 500 ms: the driver's `effect_duration_<z>`; the colour cycle goes round in 16/8/3 s, turned by pocket-ui ten times a second while the screen is on) and brightness; the default follows the page. A dark screen puts the lights out but for what shows the battery; power off puts them all out. Out means effects 0, `max_scale` 0, `effect_enable` 0 **and the driver's raw picture `frame_hex` black** (23 × `000000`, `anim_frames_enable` 0): on 03.11. a dark Brick glowed with every effect at 0 because `frame_hex` still held `373737` for several LEDs. Start (`start.sh`), Off, a dark screen, `systemctl poweroff` and `brick-hw led-off` all write it; the log says `lights: the screen is dark, ...`. **F1 / F2 button**: Quick save, Quick load, Screenshot, Recent games, Pause menu, Nothing (default F1 quick save, F2 quick load). In a game pocket-core does save/load/screenshot (`hotkeys`, the Shortcuts switches and Kids Mode still apply); pocket-ui opens the game switcher or the pause menu. **Mute slider**: Mutes (default) or Does nothing | `src/leds.rs`, `src/brick_hw.rs`, `app_brick_lights.rs`, `app_brick_hw.rs`, `pad.rs`, `hotkeys.rs`, `bin/pocket_core.rs` |
| Mute slider | `brick-hw slider-watch` follows `gpio243/value` and runs `volume mute` / `volume unmute` (the level stays; keys while muted are ignored, as in MinUI/NextUI; the signal ramps down before the speaker and jack switches change). Started by `start.sh` after the volume is restored. "Does nothing" (`<data>/brick-slider` = `nothing`) un-mutes at once and ignores the slider | `deploy/brick/brick-hw.c`, `deploy/brick/start.sh` |

## What was read where

| Fact | Source |
|---|---|
| 23 RGB LEDs `sunxi_led<N>{r,g,b}`, driver `/sys/class/led_anim` with zones `l r lr m f1 f2`, files `effect_<z>`, `effect_rgb_hex_<z>`, `effect_cycles_<z>`, `max_scale`, `effect_enable` | the running Brick, listing only (03.11., BRICK-BETA-PLAN) |
| Values: `max_scale` 0..60, each colour channel 0..60 as `rrggbb` hex, cycles `-1` = for ever, effect 0 off / 2 breath / 4 static | Knulli's `knulli-rgb-led` (GPL, read for the numbers; the same driver on the Smart Pro). The Brick's `help` file in the driver has not been read yet |
| F1 = 317, F2 = 318, Menu = 316 on `TRIMUI Player1`; KEY_F1/F2 aliases also advertised | read on the Brick with its owner (03.11.); 317/318 are `BTN_THUMBL/THUMBR` in evdev, the Brick has no sticks, so `pad.rs` maps them to `F1/F2` for that pad name only. The measured USB ID is the Xbox 360 ID; `pad_linux_decode.rs` keeps this built-in mapping ahead of generic SDL/Xbox heuristics. Captured descriptors and end-to-end role tests: [input corpus](../tests/fixtures/input/README.md) |
| Mute slider is `gpio243/value` (input) | the strings of NextUI's `keymon` (GetMute/SetMute), read on the Brick |
| Motor is `gpio227` (output) with `/sys/class/motor/voltage` | the same strings and the pin set-up in the firmware's start script (`start.sh` already sets the pin to 0) |

## Open: only the device answers (in this order)

1. The mute slider: **down (gpio243 = 1) is muted**, read on the real Brick with its owner (03.11.); `BRICK_SLIDER_MUTED` (default 1) in `start.sh`'s environment is the one number to change.
2. `brick-hw motor-info`: the range `/sys/class/motor/voltage` takes. Until it is known (`POCKETUI_MOTOR_VOLTAGE=min,max`, then a constant in `brick_hw.rs`) the **strength of a tick is its length** and sustained game rumble is on or off (every setting above Off is the full motor); the pin alone is safe.
3. Each light: do the `l` and `r` zones light separately next to `lr` (they are written after `lr`), do the effects 1 and 3 look like "fading" and "pulsing"? `brick-hw led-set ff0000 50`, `led-off`, then the three LED modes in Settings: does the driver take `effect 4` (static) and `max_scale` this way on the Brick, which zones light, are 50 % comfortable, does Off go dark.
4. F1/F2 with a game: quick save and load, the screenshot, Recent games, the pause menu; the Menu key stays the pause menu.
5. Motor off after a game ends or crashes (`Motor` drops to 0; `start.sh` also writes 0 at every start).
6. LEDs and sleep: a dark screen puts them out (but the battery lights), waking brings them back, power off puts them out (`systemctl poweroff`); to see on the device.

LED tests are light: not while the owner sleeps.

## Tests (host)

- `cargo test --features brick --lib brick_hw leds hotkeys rumble`: the motor on a fake tree (on, off, drop, voltage range, no pin), the LED writes on a fake `led_anim` (static, breath, off, a missing zone file, no driver), the battery look (charging, full, low, at the limit, no reading), the cover colour made calm, F-key choices (default, saved, unknown value, junk file), F1/F2 in `Hotkeys` (press edge, Shortcuts and Kids Mode off).
- `scripts/test-brick-hw.sh`: the real `brick-hw` C helper against a fake sysfs: `led-set/-off`, `motor-pulse/-info`, `slider-get/-watch` (both polarities, one call per change), `volume mute/unmute`.
- `scripts/test-brick-card.sh`: `start.sh` starts the slider follower.
- Default build unchanged: see the commit message for the byte comparison.

## Volume curve and mute

The card's `volume`, `pactl` and `brick-volume-profile` share curve 3. Below 50%, a 5% key
step is one or two codec attenuation ticks (1.16 or 2.32 dB); from 50% upward it remains two
(2.32 dB). For 1..50%, first calculate the equivalent curve-2 percentage
`q = 20 + floor(3*percent/5)`, then `raw = round((100-q)*40/100)`. Above 50%, `q = percent`.
0% additionally sets DAC 0 and disables both output paths. 100% keeps raw 0 / DAC 160.
Headphone gain stays as found. Relative to the previous curve, 10% is now -34.80 dB
(previously -41.76), 20% -31.32 (-37.12), and 30% -29.00 (-32.48). 50..100% is unchanged.
Bluetooth uses `255-round((100-q)*232/100)` on the existing -51..0 dB softvol control;
zero mutes. Encoding through `q` lets a curve-2 OTA predecessor restore both control values.
This introduces no gain above the previous maximum; it does not certify the analogue speaker's
undistorted SPL. The vendor DAC TLV calls raw 160 +0.75 dB, while vendor firmware uses it as unity;
we preserve that value instead of interpreting it as permission to boost.

Important vendor metadata error: `digital volume` advertises an ascending TLV (-74.24 dB +
raw*1.16 dB), but the real Brick and firmware peers establish that raw 0 is loudest. Use raw*1.16 dB
as *relative attenuation from the existing maximum*, not the driver's absolute dB display. These
are control-derived estimates, not acoustic calibration. Stock `Soft Volume Master` is bypassed
by this card's `pipewire -> pocket_dmix -> hw:audiocodec,0` route.

A slider mute freezes `brick-volume`; volume keys show **Muted** and cannot alter the hidden
restore level. `pactl` and the controls worker read the global `brick-muted` marker.
The separate `/sys/class/speaker/mute` amplifier override is speaker-only; it must not freeze
headphone/Bluetooth keys or label those outputs globally muted. Slider and key commands
share a `flock`, so their hardware writes cannot interleave. Before turning either output path off,
volume ramps digital attenuation in single ticks to 63, then sets DAC 0. Before enabling paths it
sets attenuation 63 and DAC 0, enables them, waits, sets DAC 160, then ramps to the saved target.
Normal nonzero steps leave DAC and switches alone. The default ramp delay is 4 ms per raw tick;
scheduler/ALSA overhead adds to it. `amixer --stdin` keeps one ALSA handle for the whole fade, avoiding a process per tick; the endpoint is checked afterwards. Bluetooth ramps at most five softvol ticks (1 dB) per write.
The first restore reads the physical slider before enabling outputs. A repeated unmute does no writes.

The vendor kernel resets DAC volume on PCM open; DAC 0 alone is therefore insufficient to enforce
mute across games. Both output switches remain off while muted. This also stops the codec clock
and can block a pending PCM write until unmute (measured before this change); it is not an audible
underrun measurement. The sequence removes switching at the old live gain, but a control trace
cannot prove absence of an intrinsic analogue amplifier/DC transient. The hardware ramp is discrete,
not a sample-continuous software fade. Do not describe a silent digital recording as proof of no pop.

Regression: `python3 scripts/test-brick-volume.py` (also run by `scripts/test-brick-hw.sh`) checks
all key steps, exact mute restore including keys during mute, zero recovery, and every switch/write
in the complete mute/unmute sequence. `brick_hwroot` checks the real hardware seam; `quick` checks
that a muted key shows the existing Muted OSD without changing the saved level.
`test-brick-bluetooth.py` also connects a fake A2DP sink through the real service and
volume helper: its `pocket_bt` softvol (`BT Volume`) must reach 125 at 40%, mute to zero,
and restore exactly. The host mixer records every ramp write; no radio or sound is used.
The HW mute check sets the current level through `volume` and compares the mixer gain
and displayed percentage, rather than treating the rollback encoding as a percentage.

Restore converts stored percentages once to an attainable level that cannot raise **either**
the codec or Bluetooth gain. Levels below the new audible floor start at 0%. The exact target
lives in `brick-volume-curve3`; `brick-volume-curve2` retains a safe curve-2 value for older
helpers. `brick-volume` matches the actual `app.previous`: `compat=old` for the original helper,
`new` for curve 2, `low` for a package declaring `VOLUME_CURVE=3`. The last imported value is
kept in `brick-volume-before-curve3`. Empty curve-2 markers shipped by early betas are accepted.
An absent or damaged compatibility tag uses the quietest historical curve; ambiguous state may become quieter or silent, never louder. An unchanged tag is not rewritten; tag repair and format changes first write legacy zero.
For a curve-2 predecessor, the local and BT values are both preserved. With the original helper,
whose wired and BT curves differed, rollback may be quieter to keep both outputs safe.
Restore imports a predecessor's later changes, preserves the exact current percentage when
its compatibility encoding is unchanged, and passes through legacy zero when changing formats.
An interrupted format change therefore starts silent. Back up all volume state files together,
removing files absent from an older backup. The mixer regression checks all 101 percentages of
both older formats, repeated restores, subsequent predecessor changes and interrupted conversions.
No physical Bluetooth or acoustic level measurement is claimed.

Volume-key feedback reads the slider on the existing controls worker, rather than trusting the last
open settings panel. That worker reads the paired helper's target/mute state files directly in
`system_ctl::RealHw`; it neither forks pactl/amixer for polling on each held key nor mistakes an
intermediate fade tick for a new user setting. Mixer mutations remain in the serialized helper.
