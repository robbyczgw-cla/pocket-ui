# Android spike (experimental branch only)

The `spike/android` branch holds an early standalone Android prototype. The ROCKNIX release does not include an Android
app. Build and install the debug APK on a test emulator using [the Android instructions](../../android/README.md). It
reads a shared ROM folder through legacy Android storage access rather than Android's Storage Access Framework.

| Control | Android prototype action |
|---|---|
| D-pad Up / Down or keyboard arrows | Select a game |
| Android A button, D-pad centre, Enter or Space | Launch the selected game in RetroArch |
| Tap a game | Select; tap again to launch |
| START, keyboard F or tap the folder bar | Enter a shared ROM folder path; Read folder saves it and rescans |
| Keyboard L or tap Light / Dark | Switch Light/Dark mode for this session |

Install RetroArch (`com.retroarch`) separately and download the matching cores inside RetroArch first. The prototype
recognises GB/GBC, GBA, NES, SNES, Master System/Game Gear and Mega Drive file extensions; only a Master System test ROM
with Gearsystem was tested here. No games, BIOS files or cores are bundled.

This prototype has no pause menu, Continue, Earlier states, sound, analogue stick input or Android launcher role—RetroArch
handles gameplay, controls and saves. Only the folder path is saved; appearance and selection reset when you close it.
Because it targets API 28 to test legacy paths, use it on a test emulator only; it is not meant for store release.
