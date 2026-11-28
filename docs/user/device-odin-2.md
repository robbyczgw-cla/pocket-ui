# AYN Odin 2 family

The **AYN Odin 2**, **Odin 2 Mini** and **Odin 2 Portal** run pocket-ui OS from an SD card, or pocket-ui for Android as an
app.

- **pocket-ui as:** SD card image (one per model), or the [Android app](device-android.md)
- **Status:** Experimental; the Odin 2 Mini card is played on
- **Get it:** [install page: AYN Odin 2](https://pocketui.xyz/install.html#odin), with notes beside each download

## Good to know

- **Boot helper, once:** already boots ROCKNIX from SD? Skip this. Otherwise install ROCKNIX's boot helper (ROCKNIX ABL)
  once, from the `rocknix_abl` folder of the ROCKNIX SM8550 20261001 image. It replaces the bootloader (`abl_a` and
  `abl_b`): run `backup_abl.sh` and copy the backup to your computer first.
- **Buttons:** the Odin 2 Mini and Portal start with the Xbox menu layout (A bottom), the Odin 2 with Nintendo (A right).
  Your own choice always wins ([Button layout](controls.md#button-layout)). In the Android app, menus follow Odin's
  Controller style setting ([Android](android-mvp.md)).
- **Screen:** 1920×1080.
- **Sleep:** stock ROCKNIX has no deep sleep on these chips: Power or idling turns the screen off and pauses your game.
  Settings → System → Sleep & power → **Switch off after** saves and shuts down after a longer idle on battery.
- **Fan:** ROCKNIX runs it, so pocket-ui's fan rows don't apply.
- **Plays:** from NES to PS2, GameCube, Wii and 3DS, plus PS3, Wii U and PS Vita (selected games).

## Known limits

- Newer ROCKNIX images from 20261101 on carry OpenABL instead of the boot helper; that is untested with this card.
- Touch, stick LEDs, rumble, standby drain, 120 Hz pacing and Zink still need reports from real hardware
  ([Experimental SM8550 handhelds](other-handhelds.md#experimental-sm8550-handhelds)).

## More

[Image or app?](https://pocketui.xyz/install.html#card-or-app) · [Report a problem](troubleshooting.md#report-a-problem) ·
testers welcome in #testers on [Discord](https://discord.gg/sDqUV4M49g)
