# Other handhelds

The two-file card install in [Getting started](getting-started.md) is for the Retroid Pocket Mini V2. This page is for the
handhelds that run pocket-ui as **experimental**: other Retroid devices, SM8550 handhelds and the Anbernic H700 family.

## Other Retroid devices

pocket-ui also runs on the **Retroid Pocket 5**, the **Retroid Pocket Flip 2** (the Snapdragon 865 one) and the first
**Retroid Pocket Mini**, as **experimental**: they use the same ROCKNIX system as the Mini V2, so most things should
work, but nobody has tried pocket-ui on one yet.

- **Installing.** Don't use the Mini V2's SD card image on them. Install ROCKNIX for that device as ROCKNIX's own guide
  says, start it once, then add pocket-ui with the install package for an existing ROCKNIX card (the
  [install guide](../INSTALL-DUAL-BOOT.md#install-on-a-rocknix-card-you-already-use) has the steps). It recognises
  these devices and says they are experimental.
- **What you see.** The first start says once that pocket-ui hasn't been tested on your device, unless the project has
  played on it ([DEVICES](../DEVICES.md)); Settings → Help & tools →
  About shows the device with "(experimental)". Everything else is the same as on the Mini V2. On the Pocket 5's and
  Flip 2's wider screen, Settings and the Shelf look sit in a column in the middle; on the Flip 2, closing the lid
  puts it to sleep.
- **If something doesn't work**, Settings → Help & tools → About → Report a problem saves a file to attach to a report.



### Experimental SM8550 handhelds

The **Retroid Pocket 6** (including TOP-DPAD), **Retroid Pocket Nova**, **AYN Odin 2 Mini**,
**AYN Odin 2** and **AYN Odin 2 Portal** are experimental. Only the Odin 2 Mini has been tried on a real device
(2026-10-27: the install package, the first-start screens, the shelf, a GBA game with the pause menu, screen off and
wake, driven over the command pipe; no button or touch pressed, no Bluetooth, no sleep); the other four are computer
tests only, with no hardware verification. Use ROCKNIX's own **SM8550 aarch64 image**
(20261001), set up following ROCKNIX's guide for your device, boot it once, then add pocket-ui
with the install package. The Mini V2 SD image must not be used on these devices.

Pocket 6 and the Odin 2 family use 1920×1080; Nova uses 1280×960. First start says once that the device is untested;
About marks it experimental. **Stock ROCKNIX still disables sleep on these devices (checked 2026-10-27):** the power button
turns the screen off, and idle turns it off with the game paused. Any button brings it back.
Settings → System → Sleep & power offers **Switch off after** (Off by default) to save and power
off after a longer dark spell on battery. Screen off still uses battery. ROCKNIX controls the fan;
pocket-ui's fan choices have no effect here. Touch, LEDs, vibration, battery drain, 120 Hz pacing
and Zink on the real GPU still need a tester. Mini and Portal menus start with Xbox buttons
(A bottom); the original Odin 2 uses Nintendo buttons (A right). Saved layout choices win. See [device notes](../DEVICES.md).

## Anbernic H700 handhelds (experimental)

pocket-ui runs on the Anbernic handhelds that ROCKNIX's **H700 image** supports. All of them are **experimental**:
pocket-ui knows each model (screen, buttons, what it has). The **RG35XX Plus** has been played on a real handheld; the other
models have only been tested on a computer so far. If something doesn't work, Settings → Help & tools → About → Report a problem saves a file to attach to a
report. The **RG CubeXX** (square 720×720 screen) is **not supported yet**: pocket-ui draws that screen too slowly on
this processor for now, and the installers say so and change nothing.

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
| RG CubeXX | 720×720 | yes | yes | no | RGB (**not yet**) |

All of them have 1 GB of memory, no touch screen and no fan. The RGB lights of the RG40XX are ROCKNIX's; pocket-ui
doesn't change them.

### Install

1. **Use a spare microSD card for ROCKNIX and keep your stock card as it is.** The H700 handhelds have no internal
   storage: the stock system lives on the card in slot 1, so ROCKNIX goes on a different card and you can switch back
   by swapping cards. Back up the card with your games before you put it in a ROCKNIX boot.
2. Download ROCKNIX's **H700 image** from the [official ROCKNIX H700 install page](https://rocknix.org/configure/h700-installation/)
   (version 20261001 or newer) and flash it with balenaEtcher or Raspberry Pi Imager. There is one image per memory
   type: **try the DDR4 image first. If the handheld doesn't start from it** (the screen stays black), flash the **DDR3**
   image onto the same card instead; only the card is at risk.
3. Take the card out and in again so your computer shows the **ROCKNIX** drive. Copy your model's device tree file
   from its `device_trees` folder to the top folder of that drive and name the copy **`dtb.img`** (all lower case).
   Eject the card properly: skipping this step can leave the card unable to start.

   | Model | Device tree file (in `device_trees`) |
   |---|---|
   | RG28XX | `sun50i-h700-anbernic-rg28xx.dtb` |
   | RG35XX 2024 | `sun50i-h700-anbernic-rg35xx-2024.dtb` (`...-2024-rev6-panel.dtb` if the picture is garbled) |
   | RG35XX Plus | `sun50i-h700-anbernic-rg35xx-plus.dtb` (`...-plus-rev6-panel.dtb`) |
   | RG35XX H | `sun50i-h700-anbernic-rg35xx-h.dtb` (`...-h-rev6-panel.dtb`) |
   | RG35XX Pro | `sun50i-h700-anbernic-rg35xx-pro.dtb` |
   | RG35XX SP | `sun50i-h700-anbernic-rg35xx-sp.dtb` (`...-sp-v2-panel.dtb`) |
   | RG40XX H | `sun50i-h700-anbernic-rg40xx-h.dtb` (`...-h-v2-panel.dtb`) |
   | RG40XX V | `sun50i-h700-anbernic-rg40xx-v.dtb` (`...-v-v2-panel.dtb`) |
   | RG34XX | `sun50i-h700-anbernic-rg34xx.dtb` |
   | RG34XX-SP | `sun50i-h700-anbernic-rg34xx-sp.dtb` (`...-sp-v2-panel.dtb`) |
   | RG-SP | `sun50i-h700-anbernic-rg-sp.dtb` |

   The name in brackets is for a newer panel revision: if the first file shows a garbled or striped picture, hold
   the power button for 10 seconds, flash again and try that one.
4. Start ROCKNIX from the card. The first start grows its storage and restarts by itself; then ROCKNIX asks its own
   first questions. Let it come up to EmulationStation once.
5. Add pocket-ui. Without a network (the RG28XX and RG35XX 2024 have no Wi-Fi), use one of these:
   - **The Ports installer:** copy the single file *Install pocket-ui.sh* into the `roms/ports` folder of the card
     (the card in your computer, or ROCKNIX's file sharing), start EmulationStation again, open **Ports** and start
     **Install pocket-ui**. The screen shows the checks and the install, then pocket-ui starts.
   - **Two files on the card:** copy `mount-storage.sh` and `pocket-ui-seed.tar.gz` to the top folder of the
     **ROCKNIX** drive, put the card in and start the handheld. It installs during that start and opens pocket-ui.
   With Wi-Fi and SSH, the install package for an existing ROCKNIX card works too (see the
   [install guide](../INSTALL-DUAL-BOOT.md#install-on-a-rocknix-card-you-already-use)).

   All three check the device first: only the models in the table are accepted, and an H700 handheld that isn't in
   it is refused with a sentence and nothing is changed. ROCKNIX keeps starting as before.

### What works, what is hidden

- **Games.** The consoles ROCKNIX's H700 image ships cores for play in pocket-ui's own player, with its pause menu,
  states and shortcuts. **N64, PSP, Dreamcast and Nintendo DS stay on ROCKNIX's own emulators** (they ask more than
  these processors give). PS2, GameCube, Wii and 3DS are not in ROCKNIX's H700 image, so those systems don't appear.
  See [Game Boy Advance on H700](systems.md#game-boy-advance-on-h700) for GBA cores and existing Continue moments.
- **Screen.** pocket-ui draws its normal layout and reduces it to the small screen, so it looks like on the Mini V2,
  only smaller. Small hint text can be hard to read on the 640×480 handhelds; Settings → Look → Text size
  makes it larger.
- **Hidden or reduced.** Fan settings say "No fan"; there is no touch setup. The RG28XX and RG35XX 2024 say
  "This device has no Wi-Fi" on the Wi-Fi page and offer nothing to switch on there: everything else works offline,
  while online features (Store, covers, updates, RetroAchievements, file sharing) wait for a network. On the
  handhelds without sticks, no stick lights are offered.
- **Time zone** is set the H700 way (ROCKNIX's system setting); the first-start step is there as on the other devices.
- **To go back:** take the card out and put your stock card in, or run `sh /storage/pocket-ui/uninstall.sh`, or
  start *Uninstall pocket-ui* in Ports: EmulationStation is back at the next start. Don't run ROCKNIX's online
  update on a card with pocket-ui until a pocket-ui release says that version was checked.

## TrimUI Brick Pro (experimental)

The Brick Pro uses the same TrimUI card as the Brick and Smart Pro. It has two sticks with L3/R3 clicks and separate F1/F2
buttons. Clicking a stick never quick-saves or quick-loads; use MENU and the pause menu for saves, or assign the separate F keys.
Its 3.95-inch screen uses the standard type sizes. The side switch has no assigned function and the lights stay off while their
behaviour is untested. Lights & buttons offers only the F1/F2 assignments.

Nobody on the project has tested a Brick Pro. Use a spare card and back up saves before trying it. For feedback, open
Settings → Help & tools → Report a problem. If there is no picture, send the card's `pocket-ui/logs` folder.
