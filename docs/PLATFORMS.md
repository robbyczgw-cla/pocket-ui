# What each platform has

One table of what a player can use, taken from the code and the docs (0.2.0). "Not seen" means the code has it but nobody has
watched it on that platform's real device. ROCKNIX handhelds are the reference: every feature below was seen there, unless its row says otherwise.
Android was seen on the emulator only; the Steam Deck on one SteamOS session over SSH (docs/STEAMDECK.md).

| Feature | ROCKNIX handhelds | Android | Steam Deck |
|---|---|---|---|
| Insert animations (the console takes the game) | yes | yes, at the phone's pixels (emulator) | in the code (shared with the handhelds), not seen |
| Continue (open cases: A goes straight back to the saved moment) | yes | yes (emulator) | no: the host owns the emulators and their states |
| Kids mode | yes | no: not in scope, the Android app has no such mode | no: Steam's own button can't be closed, so the row and the profile are gone |
| Game check | yes | yes (emulator); no "hide a twin" fix | in the code, not seen |
| Second card (copying games without Wi-Fi) | yes, where the handheld has a slot; USB file transfer (MTP) is retired, see [DEVICES.md](DEVICES.md) | no | no |
| "Sign in…" for a Wi-Fi that wants a sign-in page | yes | no | no |
| PSP ISOs of any size in the built-in player | yes | yes (emulator), a first start copies the game into the app and needs that much room; other systems' games up to 512 MB | no: the host owns the emulators |
| Cabinet and TV | yes, with multi-shelf navigation | yes (emulator), with shelves for recent games, favourites and each system in the Shelf, Cabinet and Cards looks; TV beside the cases in landscape | in the code, not seen |
| Case stickers | yes, editable | yes (emulator), the five case stickers; no Curator stickers of your own | in the code, not seen |
| Memory card (pause menu) | yes | yes (emulator) | in the code, not seen |
| Game connections | yes | yes (emulator) | in the code, not seen |
| Pause menu | yes | yes, touch and controller looks | yes; full for PCSX2, Continue and Quit over other emulator apps |
| Achievements | yes | no | in the code, not seen |
| Screen frames | yes, all built-in frames | only the small Console and CRT TV surrounds | in the code, not seen |
| Spanish | yes | in the code, not seen | in the code, not seen |
| Second card with games | H700 handhelds (their second slot), from ROCKNIX's automount and device trees | no | no |
| USB file transfer | retired (not offered; the code stays) | no | no |
| File sharing and its row on Add games | yes | no | no: the host owns it |
