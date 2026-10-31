# Android spike (experimental branch only)

The `spike/android` branch contains a separate Android prototype. The ROCKNIX release does not include an Android app.
Install the debug APK built with [the Android instructions](../../android/README.md) on a test emulator. It reads a
shared ROM folder using legacy Android storage access. It does not yet use Android's Storage Access Framework.

| Control | Android prototype action |
|---|---|
| D-pad Up / Down or keyboard arrows | Select a game |
| Android A button, D-pad centre, Enter or Space | Launch the selected game in RetroArch |
| Tap a game | Select; tap the selected game again to launch |
| START, keyboard F or tap the folder bar | Enter a shared ROM folder path; Read folder saves it and scans again |
| Keyboard L or tap Light / Dark | Switch appearance for this session |

Install RetroArch (`com.retroarch`) separately and install the required core in RetroArch first. The prototype
supports GB/GBC, GBA, NES, SNES, Master System/Game Gear and Mega Drive file extensions. Only the Master System test ROM
with Gearsystem has been played here. Games, BIOS files and cores are not bundled with the prototype.

The prototype has no own pause menu, Continue, Earlier states, sound, stick input or Android home-launcher role.
RetroArch owns gameplay, its controls and its saves. The path selection persists; appearance and selection are session-only.
Use this on a test emulator: it targets API 28 to investigate legacy file paths and is not ready for store distribution.
