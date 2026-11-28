# Anbernic H700

The Anbernic handhelds with the H700 chip: **RG35XX** (Plus, SP, H, Pro, 2024), **RG28XX**, **RG40XX** (H, V),
**RG34XX**, **RG34XX-SP**, **RG CubeXX** and **RG-SP**. Each model has its own ready-to-flash card.

- **pocket-ui as:** SD card image, one per model
- **Status:** RG35XX Plus and SP: Played on. The other models aren't played on yet: same chip, with a layout tuned for
  their screen
- **Get it:** [install page: pick your Anbernic](https://pocketui.xyz/install.html#anbernic). Already on ROCKNIX? The
  [Ports installer](https://pocketui.xyz/install.html#way-1) adds pocket-ui to your card

## Good to know

- **Your stock card stays safe:** the pocket-ui card goes in the first slot (TF1). To go back, power off and swap your
  original card back in.
- **Garbled, shifted or black picture?** Some models have a second image beside the download, for a newer screen or
  DDR3 memory.
- **Models:** screen size, Wi-Fi, sticks, lid and lights of every model are in the table on
  [Anbernic H700 handhelds](other-handhelds.md#anbernic-h700-handhelds-experimental). All of them have 1 GB RAM, no
  touchscreen and no fan, so the fan rows read "No fan".
- **RG28XX and RG35XX 2024 have no Wi-Fi:** on a 16 GB card or larger the first boot makes a **GAMES** drive that your
  computer opens like a USB stick ([The GAMES drive](games-drive.md)). Updates come as one zip on the card
  ([No Wi-Fi? Update from the card](updates.md#no-wi-fi-update-from-the-card)), box art from a computer
  ([Without Wi-Fi](extras.md#without-wi-fi)).
- **A second card:** every H700 model has a second slot. A card with a `roms` folder in TF2 shows its games within
  seconds ([Second card](copy-games.md#with-a-second-sd-card)).
- **Plays:** from NES to PlayStation, N64, Dreamcast, DS and PSP. N64, PSP and Dreamcast run on ROCKNIX's emulators, DS on
  DSperate; PS2, GameCube, Wii and 3DS aren't on H700 ([Game Boy Advance on H700](systems.md#game-boy-advance-on-h700)).
- **Small screen?** Settings → Look → Text size → **Large** gives bigger hints on 640×480.
- **Lights:** the RG40XX and RG CubeXX RGB rings stay under ROCKNIX's control.

## Known limits

- The original RG35XX (2023, without "Plus" or "2024" on the back) isn't supported.
- Skip ROCKNIX's online updates; update through pocket-ui ([Updates](updates.md)).
- A few PortMaster games need symlinks, which the exFAT **GAMES** drive can't hold.

## More

[What works, what is hidden](other-handhelds.md#what-works-what-is-hidden) ·
[A second card for your games](https://pocketui.xyz/install.html#second-card) ·
[Coming from another system](coming-from.md) · [Report a problem](troubleshooting.md#report-a-problem)
