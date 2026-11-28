# Retroid Pocket 5, Flip 2, 6, Nova and Classic

On these Retroid handhelds the easiest way is **pocket-ui for Android**: keep Android, install one app. The SD cards are
new and still need reports from real hardware.

- **pocket-ui as:** the [Android app](device-android.md) (recommended), or an SD card
- **Status:** Pocket 5, Flip 2, Pocket 6 and Nova cards: Beta, no confirmed boot on real hardware yet. Classic:
  Experimental, untested on hardware
- **Get it:** [Pocket 5 and Flip 2](https://pocketui.xyz/install.html#rp5) ·
  [Pocket 6 and Nova](https://pocketui.xyz/install.html#rp6) · [the Android app](https://pocketui.xyz/install.html#android)

## Pocket 5 and Flip 2

- Same chip family as the Mini V2 (Snapdragon 865), so almost everything works the same way.
- **Boot menu first:** hold Volume Down + Power. If it shows **Loader**, boot the card from there and nothing gets flashed.
  If it only shows **Android** and **UEFI**, the card needs OpenABL once, which replaces the bootloader and can brick the
  handheld if it goes wrong: read the risk and the backup steps on the install page first.
- **Original screens only:** 12 GB models with the Visionox screen stay black, so don't flash those.
- On the wider screens the shelf centres and Settings uses larger type; closing the Flip 2's lid puts it to sleep.
- Details: [Other Retroid devices](other-handhelds.md#other-retroid-devices).

## Pocket 6 and Nova

- Snapdragon 8 Gen 2 (SM8550). The Pocket 6 card comes in two versions: stick above the D-pad, or D-pad on top. Pocket 6
  runs at 1920×1080, Nova at 1280×960.
- **Try Retroid's UEFI first** (Volume Down + Power, Secure Boot off): nothing gets flashed. If it drops back to the boot
  menu, the card needs OpenABL once, with the same risk as above.
- **Sleep:** stock ROCKNIX has no deep sleep on these chips. Power or idling turns the screen off and pauses your game;
  Settings → System → Sleep & power → **Switch off after** saves and shuts down after a longer idle on battery.
- **Fan:** ROCKNIX runs it, so pocket-ui's fan rows don't apply.
- **Plays:** from NES to PS2, GameCube, Wii and 3DS, plus PS3, Wii U and PS Vita (selected games).
- Details: [Experimental SM8550 handhelds](other-handhelds.md#experimental-sm8550-handhelds).

## Retroid Pocket Classic

A 1240×1080 touchscreen, four face buttons and no sticks; ROCKNIX runs the fan. The beta card needs ROCKNIX 20261101's
OpenABL (v1.2) already installed and never flashes a bootloader itself. Six-button Classic models aren't supported.
Details: [Retroid Pocket Classic beta](other-handhelds.md#retroid-pocket-classic-beta).

## Known limits

- The cards have no confirmed boot on real hardware yet. Tell us whether menu and game audio work on yours.
- On Pocket 6 and Nova, touch, stick LEDs, rumble, standby drain and 120 Hz pacing still need reports from real hardware.
- On the Classic, sleep, audio, touch and inputs still need live testing.

## More

[Image or app?](https://pocketui.xyz/install.html#card-or-app) · [Report a problem](troubleshooting.md#report-a-problem) ·
testers welcome in #testers on [Discord](https://discord.gg/sDqUV4M49g)
