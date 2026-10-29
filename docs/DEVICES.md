# Devices: Mini V2 and experimental handhelds

> **Status (2026-10-26; ledger: [STATUS.md](STATUS.md)):** pocket-ui is made and tested on the **Retroid Pocket
> Mini V2**. The **Retroid Pocket 5**, **Retroid Pocket Flip 2** (the Snapdragon 865 one) and the first **Retroid
> Pocket Mini** ("Mini V1") run the same ROCKNIX SM8250 image and are offered as **experimental**: built and tested
> on a computer only (unit tests, snapshots at their screen sizes, a first day in the virtual device for each). Nobody
> on the project has one; nothing here has run on their hardware. Reports are welcome: Settings › System › About ›
> Report a problem.

Research behind this: `RESEARCH-NOVA-RP6-2026-10-26.md` (outside the repository) and
[NOVA-FEASIBILITY.md](NOVA-FEASIBILITY.md). The SM8550 devices (Pocket 6, Nova and the AYN Odin 2 family) are experimental too; their different platform restrictions are below.

## The four devices

| | Mini V2 | Mini V1 | Pocket 5 | Flip 2 (SD865) |
|---|---|---|---|---|
| pocket-ui | **tested** | experimental | experimental | experimental |
| Device tree `compatible` (first) | `retroidpocket,rpminiv2` | `retroidpocket,rpmini` | `retroidpocket,rp5` | `retroidpocket,rpflip2` |
| Device tree `model` | `Retroid Pocket Mini V2` | `Retroid Pocket Mini` | `Retroid Pocket 5`, `Retroid Pocket 5 Visionox` | `Retroid Pocket Flip2`, `Retroid Pocket Flip2 Visionox` |
| Name pocket-ui shows | Retroid Pocket Mini V2 | Retroid Pocket Mini V1 | Retroid Pocket 5 | Retroid Pocket Flip 2 |
| Screen as pocket-ui sees it | 1240x1080 | 1280x960 (4:3) | 1920x1080 | 1920x1080 |
| Panel, device tree rotation | 1080x1240, `rotation = <90>` | 960x1280, `<90>` | 1080x1920, `<270>` | as Pocket 5 |
| Touch in the device tree | 1080x1240 | 960x1280 | 1080x1920, inverted x and y (Visionox: y only) | as Pocket 5 |
| Lid | no | no | no | yes, `SW_LID` (hall sensor, `gpio-keys`) |
| Fan, LEDs, pad, haptics, battery gauge | the shared `sm8250-retroidpocket-common.dtsi` | same | same | same |

Strings from ROCKNIX's `projects/ROCKNIX/devices/SM8250/linux/dts/qcom/sm8250-retroidpocket-*.dts` (`next`,
2026-10-26). The Visionox variants keep their board's `compatible` and differ in `model` and panel. `HW_DEVICE` in
os-release is the platform, `SM8250`, on all four. ROCKNIX keys its device quirks by the `model`
(`QUIRK_DEVICE`, `quirks/devices/<model>/`; the Flip 2 and Visionox folders are links to the Pocket 5's, the Mini V2's
to the Mini's).

The Mini V1's panel is sold as 1280x960, but it scales internally (928 lines shown as 960, the 2025 screen
controversy that led to the V2). ROCKNIX drives it at 960x1280; pocket-ui lays out for 1280x960 and can't do anything
about the panel's own scaling.

## How pocket-ui tells them apart

`src/device.rs`, read once (two small sysfs files) on the first use:
`/sys/firmware/devicetree/base/compatible` decides, `model` alone when there is no `compatible`.
Anything else (another SM8250 handheld on the same image, a computer, the virtual device without a device option)
is **unknown**: everything behaves as it did before devices existed (the Mini V2's numbers), and nothing is said about
the device. The log's first lines say which it found (`device: Retroid Pocket 5 ("Retroid Pocket 5"), experimental`).

For tests and the virtual device: `POCKETUI_DEVICE=miniv2|mini|rp5|rp5-visionox|flip2|flip2-visionox` (or a
`model` string), and a snapshot state may end in `:device=<name>` (`about:device=flip2`), which also makes the
snapshot the device's screen size unless `POCKETUI_SNAPSHOT_SIZE` says otherwise.

## What each device gets

The same pocket-ui: every feature, the same ROCKNIX scripts, quirks, fan driver, sleep path, pad map and cores.
What differs:

| | Mini V2 | Mini V1 | Pocket 5 | Flip 2 |
|---|---|---|---|---|
| First start | as before | one line: "pocket-ui hasn't been tested on a Retroid Pocket Mini V1 yet. Most things should work; if something doesn't, Settings › System › About › Report a problem." | the same line | the same line |
| Settings › System › About | as before | a Device row: "Retroid Pocket Mini V1 (experimental)" | "Retroid Pocket 5 (experimental)" | "Retroid Pocket Flip 2 (experimental)" |
| Layouts | as before (pixel for pixel) | the 960-line fixes below | the wide-screen column below | as Pocket 5 |
| Smooth fan's lowest running speed | 40 (measured) | 51 | 51 | 51 |
| Lid | — | — | — | closing it is the power button's short press ([SLEEP.md](SLEEP.md#lid-the-flip-2)) |
| Integer scaling (Native) | GB 7x, GBA 5x, NES/PS1 3x | GB 6x, GBA 5x, NES/PS1 4x (the whole screen) | GB 7x, GBA 6x, NES/PS1 4x, PSP 3x | as Pocket 5 |

Nothing else asks the user anything; no reminders after the first start.

## Layouts

Every state of `tests/i18n-states.txt` was rendered at 1920x1080 and 1280x960, dark and light, Normal, Medium and
Large text, English and German, with `scripts/size-sweep.sh` (the painter's own records: text running into text or
the hints, text cut by an edge, strings cut short), and every page was looked at in Normal (2026-10-26).

- **1920x1080 (Pocket 5, Flip 2).** Nothing collided; the problem was width: a list row put its name and its value
  1,880 px apart and a paragraph ran to 200 characters. On a screen at least 1600 px wide the pages of rows and text
  (Settings and the manual, the Shelf look, the search, the first start) are painted into a **column of 1440 px in the
  middle** (`src/app_column.rs`: the page paints into a buffer of the column's size, which is copied into the frame;
  touch areas and the text records move with it; one copy per frame, only on such a screen). The looks made for the
  whole width keep it: Split shows all six facts beside the shelf, Cards shows five whole cards, Minimal and Cabinet
  fill the screen, as do the game card, the console page and the pause menu over the game. The look step's
  previews are painted at the screen's size (960x540 at 1920x1080). The same column applies on a 1080p TV
  ([DISPLAY-OUT.md](DISPLAY-OUT.md)).
- **1280x960 (Mini V1).** 120 lines fewer. Fixed: the Cabinet's shelves and the search keyboard are measured from the
  bottom edge (they ran under the hints); Split's picture panel is 120 px shorter, so all six facts show beside the
  shelf again (three did); Add games shows two rows of tiles instead of three; Performance & fan's
  readout goes two by two when four lines would reach the hints; Pixel Path's seven rows share the height (with larger
  text only the selected stage keeps its second line); History's chart is shorter so the five most played games stay
  above the hints; "How do you like to play?" stops its summary above the hints (cut with "…" at larger text); the look
  step's previews are 560x420; About's facts move closer together when the Device row would push its help into the
  hints. Left as it is: the letter jump's big letter covers a row for a moment, as it does on purpose.
- **The Mini V2's 1240x1080:** every state, both looks, three text sizes, compared picture for picture with the code
  before this work: unchanged (only the clock and fades differ between two runs of the same program).

### Small panels (H700)

`src/scaled_layout.rs` chooses a logical canvas for outputs at most 720 px tall. The physical window size wins;
before a window exists, the device profile supplies it. Thus an H700 on a 1080p TV uses the native large layout.

| Panel | Logical canvas | Final reduction |
|---|---|---|
| 640×480 | 1280×960 | 2:1 box average |
| 720×480 | 1440×960 | 2:1 box average |
| 720×720 | 1280×1280 | exact area coverage (16:9 reduction on each axis) |

Other small outputs preserve their aspect ratio with at least 1280×960 logical pixels. Transient windows smaller
than 64 px on either axis or with extreme aspect ratios (outside 1:2–4:1) keep native painting to bound allocation.
Shelf, Cabinet, game cards, Settings, pause menus, quick settings, first start and safe mode share this path.
Normalised touch positions map directly into the logical canvas, including quick settings; game touch is unchanged.
The canvas and filter weights are reused; the scaler adds no timers or idle redraws. Existing animation and redraw
rules still apply. Device speed, real-panel readability, rotation and suspend still need hardware checks.

See [H700-SCALED-LAYOUT.md](H700-SCALED-LAYOUT.md) for snapshot findings, CPU measurements and their limits.

`scripts/size-sweep.sh` is the tool for the next change: `SIZES="1920x1080 1280x960" scripts/size-sweep.sh` writes one
`.problems` and one `.md5` file per size, look, text size and language; `KEEP=1` keeps the pictures.

## Touch

pocket-ui maps touch itself (`src/touch.rs`), from the panel's coordinates to the turned screen. The panel's
inversion (`touchscreen-inverted-x/y` in the device tree) is applied by the kernel's driver (`edt-ft5x06`), so
pocket-ui only turns: by `/sys/class/graphics/fbcon/rotate`, which the kernel sets from the panel's `rotation`
(90 → 1 → `Deg270`, as measured on the Mini V2; 270 → 3 → `Deg90` on the Pocket 5 and Flip 2). Tests:
`touch::tests::the_pocket_5_flip_2_and_mini_v1_panels_map_to_their_screens`. Whether a finger lands where it should
on a Pocket 5 is for a device to show.

## Safety on untested hardware

- **Fan.** The same `pwm-fan` with the same `cooling-levels = <0 51 77 102 128 153 179 204 255>`, mapped into the same
  zones by the same `sm8250-pwm-fan-cooling.dtsi` (included by the common device tree; none of the four device trees
  overrides fan, trips or cooling maps). So Smooth fan's mechanism ([FAN-CURVE.md](FAN-CURVE.md)) is the same: it moves
  only the six `active` trips of the fan's zone, never `hot` or `critical`, never the CPU/GPU cooling devices; the
  guard (full speed from 78 °C) and the kernel's backstop (83.5 °C) are the device-independent safety. What differs is
  the fan itself and the case: so on the experimental devices the curve's lowest running speed is the kernel's own
  first level, **51**, the speed ROCKNIX runs these fans at, instead of the Mini V2's measured 40
  (`device::Device::fan_min_run`). Any trouble reading the fan or its zone hands it back to ROCKNIX at once, as on the
  Mini V2.
- **LEDs.** Same HTR3212 driver and sysfs names (`multi-ledl1..4`, `multi-ledr1..4`, `rgb:l1`…); each device tree
  maps the physical LEDs to the same names. pocket-ui looks for `multi-ledl1` first and says "No lights around the
  sticks found" without it.
- **Battery.** The Pocket 5 and Flip 2 report the common device tree's 3,850 mAh design for a 5,000 mAh cell.
  pocket-ui never uses the design capacity: the estimate is the slope of `capacity` in percent over time
  ([battery_estimate.rs](../src/battery_estimate.rs)), so it is not thrown off.
- **Performance.** The CPU policies and the GPU's frequencies are read from cpufreq and devfreq; the GPU cap uses
  ROCKNIX's SM8250 platform script, the same on all four. Nothing is device-hardcoded.

## Installing

- **The pocket-ui SD card image is the Mini V2's only.** It boots through GRUB with the Mini V2's device tree
  (`miniv2-noufs.dtb`, [SD-IMAGE.md](SD-IMAGE.md)); on another device that tree describes the wrong panel, touch
  and regulators. Do not use it on the others.
- **On the experimental devices:** install ROCKNIX's own SM8250 image as ROCKNIX's guide for that device says (its
  bootloader's "Set device model" picks the device tree, including the Visionox panels), start it once, then use the
  install package (`pocket-ui-<version>-rocknix-install.zip`, [INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md)), or without SSH
  the single file `Install pocket-ui.sh` in `roms/ports` (same checks, shown on the screen). Its
  `install.sh` accepts the four devices by their device tree `model`; on the three experimental ones it says so.
  Any other device still needs `--allow-other-device`.

## A first day in the virtual device (2026-10-26)

`vdev.sh up --device rp5|flip2|mini1` ([VIRTUAL-DEVICE.md](VIRTUAL-DEVICE.md#other-devices)) on the MacBook Air, the
real ROCKNIX 20261001 userland, one container each, removed afterwards:

- **Pocket 5:** the log's `device: Retroid Pocket 5 ("Retroid Pocket 5"), experimental`; the welcome step with the
  line; the look step's previews in 16:9; Home in all five looks; Settings in the column; a Game Boy Color game at 7x
  in the middle; the pause menu; About with "Retroid Pocket 5 (experimental)"; Report a problem (its `system.txt`
  said `device = unknown` there, the container having no `/proc/device-tree`: it now also says `device_profile =
  Retroid Pocket 5, experimental`).
- **Flip 2:** the lid device found (`pad: found gpio-keys-lid`); with the power key handled
  (`POCKETUI_POWER_KEY=1`) and a stand-in suspend, closing the lid in a game logged `sleep: lid closed`, saved the
  state and suspended; the game went on after the wake.
- **Mini V1:** the welcome and look steps at 1280x960; the Cabinet look above the hints; a Game Boy Color game at 6x;
  About with its Device row (its help was cut to one line there, now two).

## What only a device can show

Touch accuracy and rotation on the Pocket 5/Flip 2 panels; the layouts in the hand; fan noise and temperatures with
the 51 start; sleep and wake on each device; the Flip 2's lid (logind inhibitor, wake by opening); battery drain and
the estimate; LED order and brightness; vibration strength; the Visionox panels' refresh rate (ROCKNIX has a 120 Hz
mode table for them; pocket-ui's frame pacing assumes 60 Hz); ROCKNIX's install path per device. The STATUS rows
say "host tests only" until somebody reports from one.


## Experimental SM8550: Pocket 6 and Nova

Host tests only; nobody on the project has either device. These profiles use ROCKNIX's **SM8550** image,
not SM8250. Install ROCKNIX for the device first, then the install package; never use the Mini V2 SD image.

| | Pocket 6 (including TOP-DPAD) | Nova |
|---|---|---|
| DT model | `Retroid Pocket 6`, `Retroid Pocket 6 TOP-DPAD` | `Retroid Pocket Nova` |
| First compatible | `retroidpocket,rp6` (both variants) | `retroidpocket,rpnova` |
| Remaining compatibles | `qcom,qcs8550`, `qcom,sm8550` | same |
| Screen | 1920×1080 | 1280×960 |
| Panel | 1080×1920, rotation 270 | 1280×960 landscape, no rotation |
| Touch | 1080×1920, kernel inverts y; pocket-ui turns with fbcon | 1280×960, no inversion or turn |
| Sleep | screen off only | screen off only |
| Fan | ROCKNIX userspace fancontrol | same |

Source: ROCKNIX `origin/next`, `9af9639cc0`,
`projects/ROCKNIX/devices/SM8550/linux/dts/qcom/qcs8550-retroidpocket-{rp6,rp6-top-dpad,rpnova}.dts`.
`POCKETUI_DEVICE=rp6|nova` selects them for tests. The same first-start line and experimental About row apply.

**Power safety:** ROCKNIX disables suspend on this platform (`030-suspend_mode`). pocket-ui never starts a real
suspend, even through quick settings, a held power button, an old settings file or an automatic timer.
Short press uses Screen off (a playing game can continue); automatic screen-off pauses the game.
The Sleep & power page explains this and offers Switch off after (Off by default): from the beginning of
the dark spell, on battery only, save and power off. No RTC alarm is needed. Any button wakes the screen.

**Fan safety:** the platform runs a userspace loop writing `pwm1`; pocket-ui never writes PWM or thermal trips,
including recovery of stale notes. Fan settings show ROCKNIX and cannot change cooling. CPU policies are discovered
from sysfs (including policy0/3/7), frequencies read rather than invented. GPU cap scripts for SM8250 are skipped.
The launcher's inherited `MESA_LOADER_DRIVER_OVERRIDE=zink` stays intact for both programs. LEDs are offered only
when their sysfs files exist. Battery uses `/sys/class/power_supply/battery`; time estimates use percent slope only.

The existing 1920×1080 and 1280×960 layouts are reused. Real touch, brightness, 120 Hz pacing, Zink on Adreno,
fan operation, LED order, vibration, battery drain and the install path remain unverified. Back paddles have no
new mapping. AYN Odin 2 profiles are outside this change.


### SM8550 first-day evidence (2026-10-27; virtual devices only)

MacBook Air, own containers `vdev-sm8550-rp6` and `vdev-sm8550-nova`, **SM8250 ROCKNIX 20261001 userland**:
welcome notice, look previews, About's experimental row, Sleep & power and fan controls in dark and light;
GBC uCity launched through pocket-ui, the pause menu, power-key Screen off and wake. Old settings asking
for suspend and hold-to-sleep were loaded; a sentinel suspend command was never invoked. RP6 touch reported
1080×1920 / Deg90, Nova 1280×960 / Deg0. These are virtual inputs, not measurements of the real panels.

Zink probe: the default vdev cannot start Zink (no Vulkan device). Temporary Debian Mesa lavapipe libraries
inside the test containers supplied a software Vulkan device; with `MESA_LOADER_DRIVER_OVERRIDE=zink`,
pocket-core started and rendered a short GBC run on each profile. UI software snapshots also started with
that variable inherited. The launcher test independently checks preservation of it. This establishes the
startup/environment path only, **not ROCKNIX SM8550's Adreno/Zink stack or performance**. The temporary
helper container was removed; no GPU environment defaults changed.

`scripts/size-sweep.sh`: 596 states × dark/light × Normal/Medium/Large × English/German = **7,152 Nova
1280×960 snapshots**, all rendered. The changed contexts (welcome, About, Sleep & power, fan page) were
inspected in both appearances and at large German text; no new clipping. Existing diagnostics include deliberate
ellipsis, the letter-jump overlay and animation-dependent preview text. The previous 960-line fixes are reused.


### Final verification (2026-10-27)

- **Full gate: exit 0**, profile `gate` (release optimisation, no LTO), **2,264 library tests passed,
  3 ignored; 2,371 Rust tests passed in total**, all launcher, verify-device, licence and package checks passed.
  Command: `RUST_TEST_THREADS=4 CARGO_BUILD_JOBS=4 nice -n 10 gate.sh
  <worktree>` on this branch's worktree (one line). The first run found a missing directory in the new
  stale-note fixture and a settings manual page just over its length budget; both were fixed without relaxing
  checks. At the default high test concurrency, six shutdown tests also waited on Store's shared
  "install in progress" counter and timed out. The complete final gate used four test workers; all passed.
- **Release cross-build:** both `pocket-ui` and `pocket-core`,
  `aarch64-unknown-linux-gnu.2.38`, on the MacBook Air, `CARGO_BUILD_JOBS=4 nice -n 10 cargo zigbuild --release`.
  Native all-target checking and Clippy pass; Clippy retains existing warnings elsewhere.
- **Mini V2:** 588 existing states × two appearances × three text sizes = **3,528 matching frames**,
  exact parent `abc42ef` versus release `8135951`. The comparison fixed the clock, animation time, free space
  and fixture paths. 3,525 matched directly. Three differed only in the Load slot digit: concurrent snapshots
  share `std::env::temp_dir()/pocketui-snapshot` (`main.rs`). Re-rendering those three cases sequentially with
  a separate `TMPDIR` per state made them identical in both appearances and every text size. No Mini V2
  rendering change was needed. The fixed-clock harness ends a render after its complete PNG is written;
  it does not test worker shutdown with a frozen clock.
- **Artifacts:** `screenshots/sm8550-contact.jpg` (1280×1245 JPEG, gitignored); checksums, Nova diagnostics
  and the comparison summary in `screenshots/sm8550-validation/` (also gitignored).
- **Cleanup:** both own virtual containers were removed. No real handheld was contacted, and nothing was merged.

## Anbernic H700 handhelds: RG28XX and RG35XX SP (2026-10-27, experimental)

ROCKNIX's **H700 image** (Allwinner H700, 4× Cortex-A53, Mali-G31, 1 GB, no swap). Computer tests only; research and
sources are in `RESEARCH-H700-2026-10-27.md` (outside the repo). Both screens are **640×480** landscape for pocket-ui
(the RG28XX's panel is 480×640, turned by sway). The UI uses the scaled logical layouts below. These profiles
remain experimental until checked on the owner's devices.

| Device / override | DT model / first compatible | What it has | What it lacks |
|---|---|---|---|
| `rg28xx` | `Anbernic RG28XX` / `anbernic,rg28xx` | D-pad, A/B/X/Y, L1/R1/L2/R2, Start, Select, Menu | Wi-Fi (rocknix.org lists none; the DTB enables a chip anyway, a chip that answers still wins), fan, touch, sticks, lid, stick lights |
| `rg35xx-sp` | `Anbernic RG35XX SP` / `anbernic,rg35xx-sp` (the `-v2-panel` DTB names are accepted too) | the same buttons, **a lid** (`gpio-keys-lid`, `SW_LID`: the Flip 2's lid code applies), Wi-Fi | fan, touch, sticks, stick lights |

Menu labels are Nintendo (A right; the pad is ROCKNIX's `H700 Gamepad`, A is `BTN_EAST`). About says
"Anbernic RG28XX (experimental)" / "Anbernic RG35XX SP (experimental)".

What pocket-ui does about what is missing (`Device::has_fan`, `has_touch`, `has_sticks`, `has_wifi`, `ram_mb`):

- **Fan:** Settings › Playing › Performance & fan keeps the processor-speed row; the three fan rows read "No fan" and
  their help line says "This device has no fan." The fan curve thread, the trip profiles and every fan sysfs write are
  not started. Quick settings already hide the Fan row when the probe finds no fan.
- **Stick lights:** the LEDs row is not in Settings (Look & feel).
- **Touch:** nothing to hide; no touch device means no touch events. The vdev has no touch panel for these two.
- **Tools:** the gamepad-calibration helper's `inputplumber` restart is skipped where the service file is absent (already
  so); the H700 image has no InputPlumber.
- **Wi-Fi (RG28XX without a chip):** the Wi-Fi page (first start and Settings) says "This device has no Wi-Fi. Everything
  else works without it." and offers nothing to choose; the Settings row reads "None on this device"; Quick settings
  show no Wi-Fi row; the radio is "absent" instead of the mock's pretend networks. Pages that need the network (Store,
  downloads, updates) keep their own offline states.
- **Time zone:** the H700 image has no `steamos-set-timezone`. Without it pocket-ui stores the zone with ROCKNIX's
  `set_setting system.timezone`, restarts `tz-data.service` and calls `timedatectl set-timezone` (the helper's own
  steps). The service restart is unverified on hardware.
- **`/usr/sbin/quantum`** missing is tolerated already (`audio::restore_quantum` falls back to `pw-metadata`).
- **Not done here:** a per-device core allowlist (N64/PSP/DC/NDS stay on ROCKNIX's emulators), the
  install package's H700 acceptance, `unsquashfs` for OS updates.

Hardware plan (needs the owner's devices): flash ROCKNIX's H700 image with the right DDR type, copy the model's DTB to
`dtb.img`, check the SP's lid and sleep, the RG28XX's Wi-Fi (an `iw dev` and `ls /sys/class/net`), the battery source and
the sway rotation of the RG28XX.

## AYN Odin 2 family (2026-10-27, experimental)

All three use ROCKNIX's SM8550 aarch64 image and the existing 1920×1080 layout. Only computer tests have
run; the Mini V2 is still the sole tested hardware. The install package accepts these exact model strings,
first start shows the usual untested-device line once, and About marks the device experimental.

| Device / override | DT model / first compatible | Native panel / rotation | Kernel touch properties | Printed menu layout |
|---|---|---|---|---|
| `odin2mini` | `AYN Odin 2 Mini` / `ayn,odin2mini` | BOE XM91080G, 1080×1920, 90°, 60 Hz | cst340, 1920×1080, swapped axes, inverted y → portrait evdev | Xbox: A bottom, B right, X left, Y top |
| `odin2` | `AYN Odin 2` / `ayn,odin2` | Synaptics TD4328, 1080×1920, 90°, 60 Hz | rmi4, clips 1080×1920, inverted x | Nintendo: A right |
| `odin2portal` | `AYN Odin 2 Portal` / `ayn,odin2portal` | Chipone ICNA3512, 1080×1920, 270°, 60/120 Hz | ft5426, 1080×1920, no inversion | Xbox: A bottom |

All compatible lists end in `qcom,qcs8550`, `qcom,sm8550`. Geometry comes from ROCKNIX `next`
`c378e634a5` (refreshed 2026-10-27): `projects/ROCKNIX/devices/SM8550/linux/dts/qcom/qcs8550-ayn-odin2*.dts`
and panel patches `0055`, `0056`, `0057`. `0058_AYN-Odin2-Mini--backlight.patch` supplies the Mini LED
backlight driver; pocket-ui uses the discovered backlight's maximum rather than an assumed brightness range.
The kernel applies touch swaps/inversions, then pocket-ui follows fbcon rotation: 1 → Deg270 on Mini/Odin 2,
3 → Deg90 on Portal. Unit tests cover the resulting corners; actual touch alignment awaits hardware.
The printed layouts are visible in AYN's [Mini product image](https://www.ayntec.com/cdn/shop/files/1_6dd30630-56ff-42e9-9f9a-f007bba23264.jpg?v=1715943106)
and [Portal product image](https://www.ayntec.com/cdn/shop/files/odin2-portal_1.jpg?v=1737092883).
Unspecified menu layout follows those labels, including safe mode; explicit settings are preserved.
Games retain the existing positional controls: HOME + right face button takes a screenshot (printed B on Mini/Portal).

**Wii U and PS3 (2026-10-28, experimental).** ROCKNIX's SM8550 image ships Cemu (`start_cemu.sh`) and RPCS3
(`rpcs3-sa`, `start_rpcs3.sh`); they are guests of the pause menu on these devices (`standalone::guest`,
docs/BIG-EMULATORS.md "Wii U and PS3"): HOME → Continue/Quit, performance profile as for the other big emulators,
no Save/Load (neither emulator has a control channel for states).

**Sleep decision.** As of 2026-10-27, [PR #3126](https://github.com/ROCKNIX/distribution/pull/3126) is open,
unmerged. Its tested-device list describes the proposed branch, not stock `next`.
The effective [SM8550 suspend quirk](https://github.com/ROCKNIX/distribution/blob/c378e634a5/projects/ROCKNIX/packages/hardware/quirks/platforms/SM8550/030-suspend_mode)
still runs `/usr/bin/suspendmode off` and ignores logind power/suspend keys. There is no Odin device quirk enabling it
(the Mini has no device-quirk directory). Thus all three deliberately use the same screen-off-only path as RP6/Nova:
short power press keeps a running game going with the screen off; idle screen off pauses it. Any button wakes it.
Sleep & power shows only working choices and the calm no-sleep line; **Switch off after** is Off by default,
opt-in for a longer dark spell on battery. No wake alarm is armed for screen off. Existing verified RTC disarm
before power-off/restart remains in force. A future ROCKNIX suspend enablement needs a fresh review of this profile.

**Input.** ROCKNIX's `02-ayn-controller.yaml` matches all three and uses `ayn_mcu.yaml`, the same as RP6.
South/east pass through; north/west are normalised by InputPlumber. START/SELECT and L3/R3 stay distinct,
BTN_MODE becomes Guide/HOME, BTN_BACK becomes keyboard F1/BACK, and KEY_F24 becomes keyboard Home.
pocket-ui accepts that keyboard's HOME and BACK aliases alongside the pad. BACK is read as a distinct button, not SELECT or START; pocket-ui currently assigns it no action. The Mini has no rear paddles (AYN's [manual](https://m.media-amazon.com/images/I/A1ssuWVZjDL.pdf)
shows the rear and front controls). Odin 2 has a separate gpio-keys-paddles node (BTN_Z/BTN_C → Left/RightPaddle2);
Portal has physical rear buttons but no matching GPIO node in its DTS. The current target is **plain `ds5`**,
despite comments mentioning Edge, so paddle events are not promised or mapped to menu actions.

**Safety.** ROCKNIX keeps its userspace fan loop: no PWM/trip changes or stale SM8250 recovery. CPU policy0/3/7
frequencies are discovered, GPU clocks and the image's Zink environment are left alone. LEDs are used only if
found, and time estimates learn from battery percentage rather than assuming an Odin battery capacity.

### First hardware check for the owner

1. Boot the device's own ROCKNIX SM8550 image once, then install the package. Confirm first-start name and
   About “AYN Odin 2 Mini (experimental)”, 1920×1080, and A bottom/B right in menus. Try X/Y, START/SELECT,
   HOME/BACK, both sticks and L3/R3; confirm no duplicate key actions.
2. Tap all four corners, scroll a list and tap the hint bar. Check brightness from minimum to maximum and
   both appearances, including large German text. Confirm the whole Mini LED panel goes dark on power.
3. Start a hosted GBC/GBA game, hear sound, press HOME, save, quit and resume; then test a favourite external
   emulator. Check vibration and the image with stock Zink, without changing graphics variables.
4. From Home and in a game, power off the screen and wake with one button. Let idle screen off pause a game.
   Confirm no real suspend, no new RTC alarm, and the no-sleep line. Opt into a short Switch off after interval:
   verify the game is saved, the device stays off, and cold start resumes it; inspect the shutdown log's RTC clear.
   Repeat on the charger (no idle power-off). Restore the interval to Off afterwards.
5. Check ROCKNIX still controls fan speed under a demanding game and CPU profile changes restore on quit.
   Check battery percentage/charging, Wi-Fi, headphones, and display-out. Record failures and logs through About.


### Odin 2 Mini on a real device (2026-10-27, night)

The Odin 2 Mini (ROCKNIX 20261001, kernel 7.2.0) was installed with the normal install package from `a126f44`
(`sh install.sh --check`, then `sh install.sh`; `VERIFY: ok`) and smoke-checked over the command pipe, with nobody
pressing its buttons or touching its screen. Seen: the "experimental" note at install, About "AYN Odin 2 Mini
(experimental)", 1920×1080 with nothing cut off, the seven first-start steps (including the Wi-Fi list), the shelf with
the 115 games found, Settings pages, screen off after the set time and wake, EasyRPG and a GBA game through pocket-core
with the pause menu, Quit back to the shelf, no panic in the log. Saves before and after were identical apart from the
`.srm` the game wrote itself (removed again). Findings, none blocking (About and Storage disagree on free space, the
Extras step counts lookups twice, the wizard switches file sharing on and shows the password): the report in the
author's notes (`ODIN2MINI-TEST-2026-10-27/REPORT.md`). **Not tried**: buttons, sticks, triggers, touch, volume keys,
power button, Bluetooth, sleep, the light theme, German, the big emulators, LEDs, fan.

### Odin verification (2026-10-27)

Implementation `1696264`. Full gate exit **0** (profile gate, four build/test workers, nice 10):
**2,454 library tests passed, 2,574 Rust tests in total, three ignored**, all launcher/install/verification shell
checks passed. Both aarch64 release programs cross-built; clippy passed with existing warnings only, none in
changed code. No timing-test retry. An initial gate caught a manual page edited during compilation (two targets
had different bundled text); the final run used stable inputs. An automatic remote attempt was stopped when
its separate worker default proved to be twelve; the successful final gate ran locally with four.

`size-sweep.sh`: 7,404 pictures, 617 states × dark/light × Normal/Medium/Large × English/German,
1920×1080 with `DEVICE=odin2mini`. No new device-specific cut or overlap; the remaining reports are the existing
intentional ellipses, letter overlay and long paths. No layout changes were needed. The gitignored
`screenshots/odin2-contact.jpg` is 1280×1128: selected snapshots and actual virtual-device captures, reviewed.

The dedicated `odin2-mini` virtual container checked exact DT strings, first start, bottom face button confirming,
touch selecting the expected Settings row, restricted Sleep & power, hosted uCity on Gambatte, HOME via a simulated
InputPlumber keyboard opening pause, slot save, quit, resume and screen off/wake while playing. BACK/F1 is read
but has no action. Safe mode confirmed with the bottom face button and exited normally. Startup RTC disarm
was logged as verified; a temporary fake alarm stayed `0`, DPMS became false, and a sentinel replacing suspend
was never called. The container was removed. No physical device was accessed.

This is **host evidence only**: the vdev uses SM8250 userland with a synthetic DT/pad/touch and software GL.
The InputPlumber keyboard was a uinput stand-in, not the SM8550 daemon. Kernel touch transformations, real
suspend, Mini LED backlight, real GPU/Zink, LEDs, vibration, fan, battery and device installation remain untested.
