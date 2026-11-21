# Options: Cheats

A closer look at the **Cheats** menu inside [Options](options.md).

## Cheats

Games running in pocket-ui's own player get a **Cheats** entry in Options whenever the libretro cheat database has codes
for them. On TrimUI handhelds, the cheat database is an optional download: until you grab it, **Options → Cheats** shows
a **Get · 30 MB** row (the same one in Settings → Games → [Box art & downloads](extras.md#cheats)) and never downloads
anything just by opening the page. Press A to download it and watch the progress right in the row. If you're offline, it
shows **Needs Wi-Fi**—connect and press A again, or copy the cheats file onto your card and pick **Look on the card** in
[Box art & downloads](extras.md#cheats). Pressing B cancels cleanly, and trying again resumes where it left off. Once
installed, cheats work completely offline and survive app updates. Cards that already have the database use it right
away. If the database has no codes for a title, the page says "No cheats found for this game". On ROCKNIX, the cheat
database is already built in.

Press A to flip any cheat On or Off, use L1/R1 to jump five lines at a time, or pick **All off** at the bottom to clear
everything. Active cheats apply immediately and are saved per game (matched by filename without region tags in brackets).
Cheats are always per game and aren't affected by **Keep for**.

### Your own cheat file

You can also drop in your own RetroArch-style `.cht` files. Give the file the same name as your game with a `.cht`
extension (for example, `Metroid Fusion (USA).cht` for `Metroid Fusion (USA).gba`; upper/lower case doesn't matter).
pocket-ui checks these spots in order and uses the first match:

1. Right beside the game: `roms/gba/Metroid Fusion (USA).cht`.
2. Inside a `cheats` subfolder there: `roms/gba/cheats/Metroid Fusion (USA).cht`.
3. RetroArch's layout—a `cheats` folder beside your console folders with one subfolder per console:
   `roms/cheats/gba/Metroid Fusion (USA).cht`. That subfolder can use the ROM folder name (`gba`), the RetroArch core
   name (`mGBA`) or the database console name (`Nintendo - Game Boy Advance`).

In **Options → Cheats**, your custom cheats appear at the top tagged **[Your file]**, followed by the database cheats.
They toggle and save per game just like built-in cheats, and any duplicate code from the database is listed only once as
yours. Even if a game has no database entry at all, your `.cht` file still unlocks the Cheats page—and on a TrimUI card
without the database yet, your cheats still show up right above the **Get · 30 MB** row.

Custom cheat files can be up to 256 KB. If a file is larger, unreadable or has no valid cheat lines, the top of the page
tells you why and still shows the database cheats—nothing fails silently. `.cht` files beside your ROMs are ignored by
Game check and your library so they never clutter the shelf.

Some cheat files (mostly Atari, PC Engine and a few NES titles) patch raw memory addresses directly instead of using
cheat codes. pocket-core can't apply those yet; the bottom line shows how many were skipped, and games with only those
cheats still open the page to let you know.
