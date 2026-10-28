# Display out: playing on a TV through USB-C

Status (2026-10-25, branch `feat/display-out-plan`): **host tests only**. Nothing has been plugged into the Mini V2
yet. This page is the research, the design ("TV mode"), what is built, and the test plan for the first plug-in.
The row in [STATUS.md](STATUS.md) is "Display out (USB-C)".

## What the hardware and ROCKNIX give us

Read from ROCKNIX's source (`next` at `651150fc88`, 2026-10-24) and checked against the unpacked 20261001 root the
device runs. Device facts from the main session, 2026-10-25: `/sys/class/drm/card0-DP-1` exists (disconnected),
`msm_dpu` binds the DisplayPort controller, ALSA has a "DisplayPort Playback" PCM (ASoC error -19 while nothing is
plugged in), sway has one output, `DSI-1`.

**The link.** The Mini V2 has no HDMI port. Its USB-C port carries DisplayPort alt mode (`pm8150b_typec` with a
`displayport` altmode, VDO `0x1c46`: source, pin assignments C, D and E). The device tree
(`sm8250-retroidpocket-common.dtsi`, shared by RP5, Flip 2, Mini and Mini V2) gives the DP controller **two lanes**
(`mdss_dp_out { data-lanes = <0 1> }`) and no `link-frequencies`, so mainline's default ceiling of HBR2 applies.
Two lanes at HBR2 carry 8.64 Gbit/s of picture: 1080p up to about 120 Hz, 1440p at 60 Hz, **4K at 30 Hz at most**.
The other two lanes stay USB 3 for a dock (pin assignment D). Retroid's own spec for the Mini says "1080p60 over
USB-C". An adapter that only offers pin assignment C (four DP lanes, USB 2 only) should still train at two lanes;
nobody has tried that on SM8250.

**ROCKNIX's fixes (all in 20261001).** Jacob Cook's work of September 2026 (`e4461cfea5`, `1dc63e1531`,
`5e52df2b9f`) is what made DP work on the RP5 at all: a phantom retimer removed and the SBU mux on GPIO 8/9 described
(DP never worked in either plug orientation before), the VBUS supply wired to the connector (PR #3368: without it PD
never completed and DP never connected), the power-role swap fixed, and the udev rule that calls `hdmi_sense` made to
match SM8250's `platform-ae01000.display-controller` and `DP-*`. Measured there: RP5 with the official dock, both
orientations, USB 3 enumerates, PD negotiates, the TV runs at 1920x1080p60, 53 EDID modes, pin assignment D, so
"1080p60 is the ceiling while USB 3 is up". The Mini V2 was only compile-tested. Patch 0019 stops the DP audio PCM
from stalling 3 s per open while nothing is plugged in (the -19 the device logs is that refusal, as designed).

**Power.** The connector is dual-role with `try-power-role = "sink"`, and both source and sink PDOs are only
5 V / 3 A (`op-sink-microwatt` 15 W). So a dock with a charger charges the Pocket at 15 W at most (less than its
own charger), and an adapter without power is fed from the Pocket's battery (5 V out, up to 3 A).

**What ROCKNIX does when a screen appears**, with or without pocket-ui (20261001, `111-sway-init`,
`/usr/bin/output_monitor`, `hdmi_sense`, `handle-hdmi-hotplug`):

| Piece | When | What it does |
|---|---|---|
| sway config (written at every boot) | at boot | `workspace 1 output DP-1 DSI-1`: workspace 1 (pocket-ui and its games) moves to `DP-1` when that appears and back to `DSI-1` when it goes. If the TV is plugged in **at boot**, `DP-1` becomes the main output: the config gets `output DSI-1 disable`, no transform for the panel, and `WLR_CON=DP-1` |
| sway itself | on plug | enables the new output at its preferred mode, placed to the right of the panel (extend, not mirror) |
| `output_monitor` (exec'd by sway, a shell loop on `swaymsg -t subscribe '["output"]'`) | 0.3 s after an output event | external lit → `wlr-randr --output DSI-1 --off` (the panel **disabled**), `--mode 1920x1080@60` on the TV, `systemctl stop usbgadget`, sound card profile **"HDMI"** (`wpctl set-profile`; if that profile is missing it **restarts wireplumber** and waits up to 10 s). Unplugged → panel `--preferred --on`, profile **"HiFi"** |
| `hdmi_sense` (udev, DRM change) | on plug and unplug | sets PipeWire's default sink: the first sink with `hdmi` in its name, else the first without. The Mini's DP sink is named after the UCM device `HDMI` (capitals), so it probably picks the first other sink, which can take the default away from Bluetooth headphones |
| `handle-hdmi-hotplug` | after `hdmi_sense` | only acts while EmulationStation runs (`pgrep -x emulationstation`); under pocket-ui it exits |

Sound: the Mini V2's UCM (`retroidpocket-RetroidPocketMiniV2.conf`) has two verbs, **HiFi** (speakers, headphones,
mic) and **HDMI** (`hw:RetroidPocket,3`, "DP0 Jack", S16LE 48 kHz, no mmap per `52-retroid-pocket-5-hdmi.conf`).
They are two card profiles: in "HDMI" the speaker and headphone sinks do not exist, so sound goes to the TV even with
headphones plugged in.

Known trouble elsewhere (ROCKNIX PR #3080, AYN Odin 2, SM8550, open): the hotplug is "often missed" on USB-C (sway
never sees the output although DRM says connected), the internal panel comes back sideways after a boot with the TV
attached, and suspend with an external screen attached misbehaved. KNULLI has no SM8250 display-out notes; the
public sources about the RP5/Mini only confirm 1080p60 over USB-C.

## How pocket-ui already behaves on a 1920x1080 window

- **The shelf and the menus** lay out from the window's size; the type and rows are sized for 1080 lines. At
  1920x1080 the rows simply get wider (snapshots below). No page has a fixed 1240 width that breaks.
- **pocket-core** draws at its window's size every frame (`draw` reads `inner_size`, `video::dest_rect`), so a game
  that moves to the TV is rescaled at once, by whole numbers: GB 7x, GBA 6x (1440x960), NES/SNES/Mega
  Drive/PlayStation 4x, 480-line games 2x (`video::tests::on_a_tv_the_picture_scales_by_whole_numbers_…`). Screen
  frames are generated for the output size (`bezel::generate`).
- **Standalone emulators and RetroArch** open fullscreen on the focused workspace (pocket-ui's `for_window` rules),
  i.e. on the TV, at its size. Their internal resolution stays their own setting.
- **Touch** maps the panel's coordinates onto the window: on a TV that would tap blindly.
- **Cost:** pocket-ui paints in software. 1920x1080 is 1.55 times the panel's pixels; a full-screen page transition
  measured 1.5 times as long on the development machine (1047 → 1564 µs, 1175 → 1790 µs, `bench-settings`).

## TV mode (the design)

On plug-in:
1. **Picture on the TV, panel dark.** ROCKNIX already does both (workspace rule, `output_monitor`). pocket-ui waits
   `SETTLE` (2.5 s), then repairs only what is missing: `move workspace to output DP-1`, `output DSI-1 power off`.
2. **1920x1080 at 60 Hz** (`display_out::choose_mode`). The design is 1080 lines high, so pages are drawn 1:1; a 4K TV
   doubles it exactly (its own scaler); games get whole-number scales to 1080; 60 Hz matches the consoles. 4K is no
   option on two lanes (30 Hz) and would cost four times the painting. Without a 1080p mode: the largest landscape
   mode up to 1080 lines at ~60 Hz (a 1366x768 monitor), then 50 Hz.
3. **A short note on the shelf**: "Showing on the TV" (and "Back on the built-in screen" on unplug). Not over a game.
4. **Touch off** while on the TV.
5. **Sound to the TV**, except: headphones in the jack, Bluetooth headphones connected, or a monitor without speakers
   (its ELD has no audio) keep it on the Pocket (`display_out::sound_route`). Today ROCKNIX's `output_monitor` switches
   to "HDMI" unconditionally; see phase 2.
6. **Controllers:** the Pocket itself, Bluetooth pads, or USB pads on a dock (pin assignment D keeps USB 3; pocket-ui
   reads every pad on its own, `pad::Mux`). A USB pad needs a dock or a hub, never the plain adapter.

On unplug, also mid-game:
1. The panel comes back on, turned as ROCKNIX turns it at boot (`output DSI-1 enable power on transform <fbcon>`) if
   ROCKNIX has not done it within a moment.
2. sway moves workspace 1 back; the game keeps running and is resized to the panel. **No game is lost**: while no
   output is lit (ROCKNIX disabled the panel, the TV is gone) pocket-core waits in its vsync until the panel is back,
   so the game pauses for that moment and its sound stops briefly.
3. Sound back to "HiFi" (ROCKNIX).

**The panel during TV mode.** Dark, as ROCKNIX does it. A calm "Playing on the TV" picture on the panel would need a
second pocket-ui window on `DSI-1` kept lit (power, battery, and an AMOLED showing a static picture for hours); not
planned. The status of the TV is better on the TV.

**16:9 layouts.** The shelf, lists and menus need nothing for 1920x1080 (snapshots). `display_out::ui_fit` says how
other sizes would fit: 1080 lines and more → the largest whole scale (3840x2160 → 1920x1080 doubled, 2560x1440 → 1:1
with more room); fewer lines (720p) → the 1080-line design scaled down (canvas 1920x1080 at 2/3). Only the 1:1 case is
wired: TV mode asks for 1080p, which nearly every TV and monitor takes.

**Sleep.** Today light sleep turns down the panel's backlight, which does nothing to a TV: the paused game stays on
the TV. Deep sleep suspends with the DP link up, which nobody has tried on SM8250 (PR #3080 saw trouble on the Odin 2).
Phase 2: light sleep switches the TV off too (`output DP-1 power off`), and deep sleep waits while a TV is attached
(only the power key suspends). Test first (below).

**Brightness keys and the night light** act on the panel. On the TV the Soft look (pocket-ui's frames, pocket-core's
tint) still applies; the compositor's gamma ramps are per output and only the panel has them.

**Standalone emulators (ARMSX2, Dolphin, Azahar, PPSSPP, Flycast) and RetroArch** run on the TV at its size like any
window. Azahar's two screens are laid out in a 16:9 window. Their own scaling settings decide sharpness. Nothing of
theirs is changed by TV mode.

### Phases

| Phase | What | State |
|---|---|---|
| 1 (this branch) | The watcher (`display_out::watch`): sway output events, the UI told, ROCKNIX first, then the repairs above; touch off on the TV; the notes; integer scaling checked for 1080p and 4K; snapshot size (`POCKETUI_SNAPSHOT_SIZE`) | built, host tests only |
| 2 | Sound by `sound_route` (headphones and Bluetooth win, speakerless monitors stay on the Pocket), replacing `output_monitor`'s unconditional switch and its wireplumber restart; sleep as above; the DS bottom screen on the panel as a touchpad (pocket-core); light sleep → TV off | design only, after the first device run |
| 3 | Taking over from `output_monitor` entirely (stop it at pocket-ui's start), if phase 1's run shows it fights pocket-ui or misses hotplugs; other sizes through `ui_fit` (720p, 4K at 2x without a 1080p mode) | only if needed |

## What is built (phase 1)

| Piece | Where | Notes |
|---|---|---|
| Outputs, the TV, the panel | `src/display_out.rs` (`parse_outputs`, `Screens`, `is_external`, `is_panel`) | `DP-*`, `HDMI-*`, `DisplayPort-*` are external; Retroid's dual-screen add-on (Lontium bridge) is not a TV |
| Mode choice | `choose_mode` | 1080p 60 Hz first (60.000 over 59.94), never above 1080 lines unless nothing else is listed |
| The repairs | `plan`, `Action` | Empty unless a TV is or was there: a panel-only Pocket never gets a command (a dark panel from sleep included) |
| The watcher | `display_out::watch` (thread `display-out`), started after the first frame (`app_display_out.rs`) | One `GET_OUTPUTS` at the start, then asleep on sway's socket (`SUBSCRIBE ["output"]`); a burst of events is one change (`DEBOUNCE` 250 ms); a new TV gets `SETTLE` (2.5 s) for ROCKNIX; reconnects with a growing pause if sway goes away. Nothing on the UI thread but storing `App::tv` |
| Touch off on the TV | `App::touch_event` (`app_touch.rs`) | |
| Notes on the shelf | `App::display_changed`, `change_message` | German in `assets/i18n/de.tsv` |
| Sound decision | `sound_route`, `profile_change` | Tested, **not wired** (phase 2) |
| Layout for other sizes | `ui_fit` | Tested, only the 1:1 case is in use. On a 1080p TV the pages of rows and text (Settings, the Shelf look, search) are painted in a 1440 px column in the middle, as on the Pocket 5 ([DEVICES.md](DEVICES.md#layouts)) |
| Snapshot at TV size | `POCKETUI_SNAPSHOT_SIZE=1920x1080 pocket-ui --snapshot …` | |
| A TV in a test sway | `POCKETUI_TV_OUTPUT=HEADLESS-2` treats that output as external (with `swaymsg create_output` in a headless sway) | |

Tests: `display_out::tests` (outputs, plan, modes, `ui_fit`, sound) and `display_out::tests::watcher` (the whole
watcher against a fake sway socket: a panel-only Pocket is asked for its outputs and nothing else and the UI is told
nothing; a TV plugged in is told at once and repaired only after `SETTLE`; what ROCKNIX set up is left alone; unplugged
brings the panel back and tells the UI), `video::tests::on_a_tv_the_picture_scales_by_whole_numbers_to_1080_and_2160_lines`.

Snapshots at 1920x1080 (dark and light; the pause menu is always dark): Home in Shelf, Split and Cards, Settings, the
pause menu. Taken with `POCKETUI_SNAPSHOT_SIZE=1920x1080`, kept in the worktree's `screenshots/`, not committed.

## Test plan for the owner (first plug-in)

What you need: a USB-C to HDMI adapter (or a dock with HDMI, USB and power in), a TV or monitor, ideally a USB pad.
The main session runs the commands over SSH while you plug. Set up once per SSH session:

```sh
export XDG_RUNTIME_DIR=/var/run/0-runtime-dir WAYLAND_DISPLAY=wayland-1 SWAYSOCK=/var/run/0-runtime-dir/sway-ipc.0.sock
```

**0. Before plugging in (the baseline):**

```sh
cat /sys/class/drm/card0-DP-1/status                 # disconnected
swaymsg -t get_outputs | grep -E '"name"|"active"|"power"|"transform"|"width"|"height"|"refresh"' | head -20
pgrep -af output_monitor                             # ROCKNIX's monitor runs (one bash loop)
wpctl status | sed -n '/Audio/,/Video/p'              # sinks; note the default (*)
pactl list cards short; pactl list short sinks
grep -E '^display:' /var/log/pocket-ui.log           # nothing: a panel-only start logs nothing
ls /sys/class/typec/ 2>/dev/null; cat /sys/class/typec/port0/data_role /sys/class/typec/port0/power_role 2>/dev/null
```

**1. Plug in the adapter (TV on), on the shelf.** Then:

```sh
dmesg | tail -40 | grep -iE 'typec|altmode|displayport|dp|tcpm|link|edid'
cat /sys/class/drm/card0-DP-1/status /sys/class/drm/card0-DP-1/enabled; head -5 /sys/class/drm/card0-DP-1/modes
ls /sys/class/typec/; cat /sys/class/typec/port0-partner/*/svid 2>/dev/null   # ff01 = DisplayPort
swaymsg -t get_outputs | grep -E '"name"|"make"|"model"|"active"|"power"|"current_mode"' -A0
swaymsg -t get_workspaces | grep -E '"name"|"output"|"focused"'
modetest -M msm -c | grep -E 'DSI|DP'                 # connector lines: status, size, mode count
grep -E '^display:' /var/log/pocket-ui.log           # "display: on DP-1 1920x1080 60.00 Hz (...)", any repairs
cat /run/hdmi-status.last; journalctl -b --no-pager | grep -iE 'hdmi|output_monitor|wireplumber' | tail -20
wpctl status | sed -n '/Audio/,/Video/p'; pactl list cards | grep -E 'Active Profile|Profile'
amixer -c 0 contents | grep -A2 "DP0 Jack"
```

Look at: does the TV show the shelf at 1920x1080 and the panel go dark? "Showing on the TV" for a moment? Does sound
of a menu click come from the TV? Does the touch screen do nothing?

**2. Start games on the TV:** a GBA game (should be 6x, 1440x960, centred), a SNES game, a PlayStation game, then a
PS2 game (ARMSX2) and a RetroArch system. Open the pause menu (HOME) on each, Save, Load, Quit. Watch for frame drops:

```sh
grep -E 'first frame|launch:|display:' /var/log/pocket-ui.log | tail -20
```

**3. Unplug mid-game** (GBA on pocket-core, then once with ARMSX2). Expected: within about a second the panel is back
the right way round, the game goes on where it was, sound comes back on the speakers.

```sh
grep -E '^display:' /var/log/pocket-ui.log | tail -5
swaymsg -t get_outputs | grep -E '"name"|"active"|"power"|"transform"'
pactl list cards | grep 'Active Profile'
```

**4. Plug in again mid-game.** The game should continue on the TV.

**5. Headphones and Bluetooth:** with the TV on, plug headphones in. Expected today: sound stays on the TV (ROCKNIX's
profile switch; phase 2 changes that). Note what happens to a Bluetooth headset's default sink after plug/unplug
(`wpctl status`).

**6. Sleep on the TV:** let light sleep start (2 min, or set Settings → System → Sleep & power → Screen off after to 30 s). Expected today: the game
pauses, the TV keeps the picture. Then press the power key (deep sleep) **only after saving**, wake, and check the TV
comes back (`cat /sys/class/drm/card0-DP-1/status`, `dmesg | tail -30`).

**7. Boot with the TV plugged in** (or restart pocket-ui with `systemctl restart essway`): does pocket-ui start on the
TV, and after unplugging, is the panel the right way round?

```sh
cat /storage/.config/sway/config | grep -E 'output|workspace'; grep WLR_CON /storage/.config/profile.d/095-sway
```

**8. Dock with power and a USB pad** (if there is one): does the Pocket charge (`cat /sys/class/power_supply/*/status`),
does the pad work in pocket-ui and in a game, does USB 3 enumerate (`lsusb -t`)?

**9. After the run:** `scripts/verify-device.sh` as after every deploy, and send the `display:` lines, the `dmesg`
excerpts and photos of the TV.

## Risks and open questions

- **DP may not come up at all on the Mini V2.** All of ROCKNIX's fixes were measured on an RP5 with Retroid's dock;
  the Mini V2 shares the device tree but was only compile-tested. A plain adapter that offers only pin assignment C has
  not been tried on SM8250.
- **Missed hotplugs** (PR #3080 on the Odin 2): sway may not see the output although DRM says connected. Then neither
  ROCKNIX nor pocket-ui reacts. Check `card0-DP-1/status` against `swaymsg -t get_outputs` in step 1.
- **Unplug with the panel disabled** leaves sway with no lit output for a moment: pocket-core waits in vsync (the game
  pauses, sound stops briefly). A standalone emulator might not like zero outputs; step 3 with ARMSX2.
- **Sound:** headphones are overridden by ROCKNIX's switch to "HDMI"; if the "HDMI" profile is missing,
  `output_monitor` restarts wireplumber, which cuts a running game's sound until its stream is linked again;
  `hdmi_sense` may take the default sink away from Bluetooth headphones on every plug and unplug.
- **Battery:** an unpowered adapter runs off the Pocket's battery; a dock charges at 15 W at most.
- **Boot with the TV attached** changes ROCKNIX's sway config for that boot (`DP-1` main, panel disabled without a
  transform). pocket-ui's panel-on repair adds the transform; the launcher's pre-rotation (ROCKNIX main, not
  20261001) would be worked out for `DP-1`.
- **`usbgadget` is stopped** by `output_monitor` on plug-in (USB networking, if used for SSH, drops).
- **Speed on the TV:** pocket-ui's frames cost about 1.5 times as much at 1920x1080; the latency baseline was taken on
  the panel only. Measure with `scripts/latency.sh` while on the TV if it feels slower.
- **The Nintendo DS** still reads the panel's touches in pocket-core, mapped onto the picture on the TV (playing
  blind). Phase 2.
