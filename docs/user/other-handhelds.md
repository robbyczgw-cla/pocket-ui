# Other handhelds

The quick two-file setup in [Getting started](getting-started.md) targets the Retroid Pocket Mini V2. This page covers
handhelds supported as **experimental**: other Retroid models, Snapdragon 8 Gen 2 (SM8550) devices, the Anbernic H700
lineup, TrimUI Brick Pro and Retroid Pocket Classic.

## Other Retroid devices

**Retroid Pocket 5**, **Retroid Pocket Flip 2** (Snapdragon 865) and the original **Retroid Pocket Mini** are supported
as **experimental**. They share the Mini V2's ROCKNIX base, so almost everything works identically, though we haven't
tested them directly on hardware yet.

- **Installing:** don't flash the Mini V2 SD image onto them. Set up ROCKNIX for your exact model first, boot it once,
  then add pocket-ui with the ROCKNIX install package
  ([install guide](../INSTALL-DUAL-BOOT.md#install-on-a-rocknix-card-you-already-use)). The installer detects your model
  and notes its experimental status.
- **Beta cards for Pocket 5 and Flip 2:** ready-to-flash images live in the
  [latest release](https://github.com/robbyczgw-cla/pocket-ui/releases/latest). Hold Volume Down + Power to check
  Retroid's boot menu: if it shows **Loader**, boot the card right there (nothing gets flashed). If it only shows
  **Android** and **UEFI**, install OpenABL once using the `openabl` folder from a ROCKNIX SM8250 card (20261101 or
  newer). It replaces `abl_a` and `abl_b`. Follow the backup steps in the install guide before flashing. Then boot with
  **Boot mode: Linux** and **Boot source: SD card**. Original screens only: 12 GB models with the Visionox screen stay
  black. Full steps and the risk: [install guide](../INSTALL-DUAL-BOOT.md#installing-openabl-once).
- **What's different:** first start shows a one-time untested-device note (unless already hardware-tested in
  [DEVICES](../DEVICES.md)), and Settings → Help & tools → About tags it "(experimental)". On the wider Pocket 5 and
  Flip 2 displays, Shelf centres cleanly and Settings stretches across the screen in larger type; closing the Flip 2's
  lid puts it straight to sleep.
- **Hit a snag?** Use Settings → Help & tools → Report a problem to bundle logs for a bug report.

### Experimental SM8550 handhelds

The **Retroid Pocket 6** (including TOP-DPAD), **Retroid Pocket Nova**, **AYN Odin 2 Mini**, **AYN Odin 2** and **AYN
Odin 2 Portal** are experimental. Only the Odin 2 Mini has been smoke-checked on real hardware so far (2026-10-27:
installer, first start, shelf, GBA game + pause menu, screen off/wake via command pipe, without live buttons, touch, Bluetooth
or sleep tested); the other four are virtual-tested only. Install ROCKNIX's **SM8550 aarch64 image** (20261001), boot it
once, then add pocket-ui via the install package. Never use the Mini V2 image on these devices.

**Beta cards for Pocket 6 and Nova** live in the
[latest release](https://github.com/robbyczgw-cla/pocket-ui/releases/latest). Try Retroid's **UEFI** entry (Volume Down +
Power) with **Secure Boot** off first; nothing gets flashed (it worked for one tester with another Linux card, not yet
with ours). If your unit drops back to the boot menu, install OpenABL once using the `openabl` folder from a ROCKNIX SM8550
card (20261101 or newer), as ROCKNIX's "Flashing OpenABL" section says
([Pocket 6](https://rocknix.org/devices/retroid/retroid-pocket-6/),
[Nova](https://rocknix.org/devices/retroid/retroid-pocket-nova/)). It replaces the bootloader. Follow the backup steps in
the install guide before flashing. Then boot with **Boot mode: Linux** and **Boot source: SD card**, and let us know how
it runs ([steps](../INSTALL-DUAL-BOOT.md#pocket-6-and-nova)).

Pocket 6 and the Odin 2 lineup run at 1920×1080; Nova runs at 1280×960. **Stock ROCKNIX disables deep sleep on SM8550
devices (checked 2026-10-27):** pressing Power or idling turns the screen off and pauses your game, and any button wakes
it. Settings → System → Sleep & power adds **Switch off after** (Off by default) to auto-save and shut down after longer
idle time on battery since screen-off still draws power. ROCKNIX manages the fan directly (pocket-ui's fan toggles don't
apply). Touch, stick LEDs, rumble, standby drain, 120 Hz pacing and Zink still need hardware feedback. Odin 2 Mini and
Portal default to Xbox menu layout (A bottom), while the original Odin 2 defaults to Nintendo (A right); your own choice
always wins. See [device notes](../DEVICES.md).

## Anbernic H700 handhelds (experimental)

pocket-ui supports the full Anbernic H700 lineup covered by ROCKNIX's **H700 image** as **experimental**, with per-model
profiles for screen size, inputs and hardware. The **RG35XX Plus** has been played on real hardware; the rest are
off-device tested so far. The **RG CubeXX** (square 720×720) gets a tailored square layout (performance measured on the
same-chip RG35XX Plus). Use Settings → Help & tools → Report a problem if anything acts up.

| Model | Screen | Wi-Fi | Sticks | Lid | Lights |
|---|---|---|---|---|---|
| RG28XX | 640×480 | none | no | no | |
| RG35XX 2024 | 640×480 | none | no | no | |
| RG35XX Plus | 640×480 | yes | no | no | |
| RG35XX H | 640×480 | yes | yes | no | |
| RG35XX Pro | 640×480 | yes | yes | no | |
| RG35XX SP | 640×480 | yes | no | yes | |
| RG40XX H | 640×480 | yes | yes | no | RGB |
| RG40XX V | 640×480 | yes | yes | no | RGB |
| RG34XX | 720×480 | yes | no (ROCKNIX lists none) | no | |
| RG34XX-SP | 720×480 | yes | yes | yes | |
| RG-SP | 720×480 | yes | no | yes | |
| RG CubeXX | 720×720 | yes | yes | no | RGB |

All H700 models have 1 GB RAM, no touchscreen and no fan. RG40XX RGB rings stay under ROCKNIX control.

### Install

1. **Use a spare microSD card and leave your stock card untouched.** H700 handhelds boot entirely from slot 1, so you
   can always swap back to stock by swapping cards. Back up your ROM card before inserting it.
2. Flash ROCKNIX's **H700 image** (20261001 or newer) from the
   [official ROCKNIX H700 install page](https://rocknix.org/configure/h700-installation/) using balenaEtcher or Raspberry
   Pi Imager. **Try the DDR4 image first;** if the screen stays black on boot, flash the **DDR3** image onto the same
   card instead. For the **RG-SP** and **2026 Metallic Blue RG34XX-SP** (1 GB RAM or less), start with **DDR3**.
3. Re-insert the card in your PC to mount the **ROCKNIX** drive, copy your model's `.dtb` from `device_trees` to the
   root of the drive, rename it **`dtb.img`** (all lowercase), and safely eject:

   | Model | Device tree file (in `device_trees`) |
   |---|---|
   | RG28XX | `sun50i-h700-anbernic-rg28xx.dtb` |
   | RG35XX 2024 | `sun50i-h700-anbernic-rg35xx-2024.dtb` (`...-2024-rev6-panel.dtb` if garbled) |
   | RG35XX Plus | `sun50i-h700-anbernic-rg35xx-plus.dtb` (`...-plus-rev6-panel.dtb`) |
   | RG35XX H | `sun50i-h700-anbernic-rg35xx-h.dtb` (`...-h-rev6-panel.dtb`) |
   | RG35XX Pro | `sun50i-h700-anbernic-rg35xx-pro.dtb` |
   | RG35XX SP | `sun50i-h700-anbernic-rg35xx-sp.dtb` (`...-sp-v2-panel.dtb`) |
   | RG40XX H | `sun50i-h700-anbernic-rg40xx-h.dtb` (`...-h-v2-panel.dtb`) |
   | RG40XX V | `sun50i-h700-anbernic-rg40xx-v.dtb` (`...-v-v2-panel.dtb`) |
   | RG34XX | `sun50i-h700-anbernic-rg34xx.dtb` |
   | RG34XX-SP | `sun50i-h700-anbernic-rg34xx-sp.dtb` (`...-sp-v2-panel.dtb` for 2026 Metallic Blue units) |
   | RG-SP | `sun50i-h700-anbernic-rg-sp.dtb` |

   Garbled or striped screen? Hold Power 10 seconds, re-flash and try the bracketed newer-panel revision `.dtb`.
4. Boot ROCKNIX once. It expands storage, reboots and walks through its first-boot setup until ROCKNIX's menu opens.
5. Add pocket-ui (pick whichever fits your setup):
   - **Ports installer (offline-friendly):** copy `Install pocket-ui.sh` into `roms/ports`, open **Ports** in ROCKNIX's
     menu and launch **Install pocket-ui**.
   - **Two-file boot install (offline-friendly):** copy `mount-storage.sh` and `pocket-ui-seed.tar.gz` to the root of
     the **ROCKNIX** drive and boot; it installs automatically and opens pocket-ui.
   - **Over Wi-Fi / SSH:** use the ROCKNIX install package
     ([install guide](../INSTALL-DUAL-BOOT.md#install-on-a-rocknix-card-you-already-use)).

   All three check your model first and refuse unknown H700 boards cleanly without modifying ROCKNIX.

### What works, what is hidden

- **Games:** every console ROCKNIX's H700 image ships cores for runs in pocket-ui's own player with its pause menu,
  states and shortcuts, including **Nintendo DS on DSperate**. **N64, PSP and Dreamcast stay on ROCKNIX's emulators**
  (too heavy for pocket-core on H700), and PS2/GameCube/Wii/3DS aren't included on H700. See
  [Game Boy Advance on H700](systems.md#game-boy-advance-on-h700) for GBA core details.
- **Display:** scales pocket-ui's full layout cleanly to 640×480 / 720×480 / 720×720. Want bigger hints on 640×480? Turn
  on Settings → Look → Text size → **Large**.
- **Cleanly tailored:** fan rows read "No fan", touch options stay hidden, and stickless models hide stick LEDs. On
  non-Wi-Fi models (RG28XX, RG35XX 2024), Wi-Fi and online features (Tailscale, Sync, Web companion, Remote access,
  RetroAchievements, Starter pack) explain offline mode clearly, and Box art explains copying covers from a PC
  ([Box art without Wi-Fi](extras.md#without-wi-fi)).
- **Time zone:** uses ROCKNIX's H700 system setting, still offered during first start.
- **Uninstalling:** swap back to your stock card, run `sh /storage/pocket-ui/uninstall.sh`, or launch *Uninstall
  pocket-ui* in Ports to return to ROCKNIX's menu. Skip ROCKNIX online updates until a pocket-ui release confirms
  compatibility.

## TrimUI Brick Pro (experimental)

Runs the same TrimUI card package as the Brick and Smart Pro, adding dual sticks (L3/R3 clicks never trigger accidental
quick-save/load; use MENU or F1/F2) and separate F1/F2 keys. Its 3.95" display uses standard font sizes; the side switch
and LED controls stay disabled until verified, so **Lights & buttons** exposes F1/F2 assignments only.

Untested on real hardware so far; try it on a spare card and back up saves first. Send feedback via
Settings → Help & tools → Report a problem (or grab `pocket-ui/logs` off the card if there's no picture).

## Retroid Pocket Classic beta

Experimental and untested on hardware. Features a 1240×1080 touchscreen, four face buttons and no sticks (stick LEDs
hidden; ROCKNIX manages the fan). Sleep, audio, touch and inputs still need live testing. Six-button Classic models and
other revisions aren't supported by this beta card.

Requires ROCKNIX 20261101's compatible OpenABL (v1.2) bootloader already installed on your Classic; never flash a
bootloader from this beta or follow Pocket 5 Loader steps. Use a spare microSD card only. Internal eMMC access is
disabled and no ABL payload is included, so Android stays safe on internal storage (pick Android in your bootloader to
return). Boot and safety checks have been verified offline only.
