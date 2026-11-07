# Two screens: the RG DS Plus

The **Anbernic RG DS Plus** has two 1024x768 panels, one above the other, touch on the lower one only (ROCKNIX's RK3566
image, docs/DEVICES.md). Automatic activation is limited to that profile and requires both enabled outputs. The
imported Retroid add-on prototype requires the explicit `POCKETUI_SECOND_OUTPUT` testing override; existing devices
keep their single-window behavior without it.

| | Upper screen | Lower screen |
|---|---|---|
| Home | pocket-ui as on every handheld, in every look | the game Home rests on: its box with the disc or cartridge sliding out (what X shows), after 4 s the info page (facts, your play, the save slots) |
| A system or a row | as always | the console and its name, or "Choose a game: its box shows here." |
| A card, a menu, Settings | as always | stays as it was |
| A game | the game | off: `output <lower> power off` and its backlight's `bl_power` 4, back on after it |
| DS, 3DS | the emulator | left on for the emulator (DraStic or melonDS put the second DS screen there); our window goes to sway's scratchpad and comes back after |
| Settings → System → Second screen: Off | as always | off, outside DS games too |

N64, PSP, Dreamcast and DS stay on ROCKNIX's emulators on the RG DS Plus (`Model::small_chip`, as on the H700). Touch:
the RG DS Plus's lower-panel touches are dropped from startup, even before output detection or when the second
window is unavailable (`touch::Ignored::LowerScreen`), so a tap below
never reaches the shelf, quick settings or a menu above; pocket-ui uses no touch there yet. The lid sleeps through the
usual lid path (`SW_LID`, docs/SLEEP.md); the screen-off and the brightness row move the upper panel's backlight, and the
lower one follows its level (`second_screen::follow_light`).

On other profiles, without an explicit testing override, no second-screen thread, output query, window or hardware
command runs. Backlight selection and brightness mirroring are limited to the RG DS Plus profile.

## What the device must confirm: `second_screen::Panels`

Everything the RG DS Plus has to confirm is one table with best guesses from ROCKNIX 20261001's sources, logged once at
start, and changed on the device without a new build by `POCKETUI_DUAL_PANELS` (in `/storage/pocket-ui/dev.env`).

| Key | Best guess | From |
|---|---|---|
| `top` | `DSI-2` | `111-sway-init`: the last connected connector is the main output (`con`) |
| `bottom` | `DSI-1` | the other one (`second_con`): powered off for EmulationStation, on for DraStic |
| `top_light` | `backlight1` | `rk3568-anbernic-rg-ds-plus.dts`: dsi1 has `backlight1` |
| `bottom_light` | `backlight0` | dsi0 has `backlight0` |
| `touch` | `Goodix Capacitive TouchScreen` | one GT911, `touchscreen-size` 1024x768 (`1046:911:Goodix_Capacitive_TouchScreen` in sway) |
| `touch_turn` | `0` | landscape like the panel; only logged |

The log line, e.g. `second screen: panels top DSI-2 (light backlight1), bottom DSI-1 (light backlight0), touch "Goodix
Capacitive TouchScreen" turned 0; sway has [DSI-1 1024x768, DSI-2 1024x768], backlights [backlight0, backlight1]; all
found`, ends in `CHECK: ...` for what isn't there. With the outputs the wrong way round:
`POCKETUI_DUAL_PANELS=top=DSI-1 bottom=DSI-2 top_light=backlight0 bottom_light=backlight1`. The touch screen opened is
the `touch: <name>` line.

## How it works

`second_screen.rs` decides (the table, which outputs, lit or not, the sway commands; unit tests), `app_second_screen.rs`
has the window: a second winit window with the app_id `pocket-ui-second` and the title `pocket-ui game info` (none of
the words of ROCKNIX's rule for emulators' second windows), placed by a `for_window` rule set before it opens, with
`no_focus`, and the shelf's window moved to the upper output with the focus (ROCKNIX's rules name EmulationStation's
window). Every power change ends with `focus output <upper>`, so a game opens there. The box is built from the card data
(`App::build_card`) once the selection has rested 180 ms, so scrolling paints nothing below. The passive page uses
cached or gamelist descriptions and cached achievement counts. Browsing starts no Wikipedia/achievement lookups or
ROM hashing and does not build the interactive card's library-wide related-game lists; the card's hint bar is
left out there (`Card::bare`). The game rules in `control.rs` (`for_window`, `show_ui`/`show_game`) leave
`pocket-ui-second` alone.

One controller thread serializes placement and power changes, coalesces pending requests, and retries failed writes.
After any partial failure it reconciles the window as well as output power, including when the next request returns
to the previous state: quitting DS restores Game info, and going back to DS hides the info window again.
It checks enabled outputs every two seconds, drops the second window when either disappears, and creates a fresh
window on reconnect. Placement rules are installed once per output, so reconnects do not accumulate sway rules.
Failed surface presentations retry with the presenter's backoff; waking asks for a fresh frame. Sleep overrides
Game info and DS's two-screen request. Wake restores the upper output; the controller restores the lower output
according to the game and setting, so a one-screen game keeps its lower panel dark.

Before the first RG DS Plus power write, the worker creates `/tmp/pocket-ui-second.restore`. The launcher uses it
on exit, abort or SIGKILL: after ending the game, `pocket-ui --restore-second-screen` restores both `bl_power` values,
brightness (including the saved light-sleep level), and output power before restarting or returning to
EmulationStation. Other profiles do not create this note or write either panel's backlight.

## Trying it without the device

`--snapshot second:<box|info>:<system>:<name>` (or `second:system:<system>`, `second:none`) paints the lower screen,
`POCKETUI_DEVICE=rgdsplus POCKETUI_SNAPSHOT_SIZE=1024x768` the upper one at the device's size. In the virtual device: a
second headless output and `POCKETUI_SECOND_OUTPUT` names it (it wins over the table and shows the Settings row).

```sh
swaymsg create_output
swaymsg 'output HEADLESS-1 mode --custom 1024x768@60Hz position 0 0'
swaymsg 'output HEADLESS-2 mode --custom 1024x768@60Hz position 0 768'
echo POCKETUI_SECOND_OUTPUT=HEADLESS-2 >> /storage/pocket-ui/dev.env   # then vdev.sh deploy
grim -o HEADLESS-1 top.png; grim -o HEADLESS-2 bottom.png
```

Not done yet: anything touched below (the keyboard, the pause menu), dimming the lower screen on its own, German
Large-text info layout, and real-device acceptance/performance measurements. The initial box has a short animation;
after the info page settles there is no continuous lower-page render loop. The two-second output query runs off the
UI thread; card construction still includes local metadata/save checks and needs an A55/SD-card latency measurement.

The lower window keeps its own last-presentation time and image dependencies. Other windows' events cannot speed up
its animation, and a decoded cover requests a frame only on the window using it. Once the info page and bounded image
retries settle, no lower-window frame deadline remains. `POCKETUI_TRACE` records successful lower presentations as
`second-frame` for idle measurements; see [IDLE.md](IDLE.md).
