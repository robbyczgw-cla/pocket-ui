# Options: Cheats

The **Cheats** entry of [Options](options.md), on a page of its own.

## Cheats

Games on pocket-ui's own host get a **Cheats** page in the Options menu when the libretro cheat database has cheats for
them. On TrimUI the database is an optional download: while it is missing, **Options → Cheats** shows a **Get · 30 MB**
row (the same as Settings → Games → [Box art & downloads](extras.md#cheats)); opening the page alone downloads nothing. A on it downloads
the database; the row shows how much has arrived. Without a connection it says **Needs Wi-Fi**; connect Wi-Fi and press A
again, or put the cheats file on the card and choose **Look on the card** in [Box art & downloads](extras.md#cheats). B stops a download safely, and the next try continues from its saved checkpoint. After that cheats work offline,
including after app updates. Cards that already have the database keep using it without another download. The page says "No cheats found for
this game" when the installed database has none. ROCKNIX keeps using its included database.

Each cheat is On or Off: A turns it on or off, L1 and R1 skip five lines, "All off" at the end switches them
all off. What is on is remembered for that game and takes effect at once. The game is found by its file name without the
bits in brackets. Cheats are not part of Keep for.

### Your own cheat file

A cheat file of your own (a `.cht` file, the kind RetroArch uses) works too. Put it where pocket-ui finds it, with the
game's name and the ending `.cht`, for example `Metroid Fusion (USA).cht` for `Metroid Fusion (USA).gba`; capital
letters don't matter. These places are looked at, the first one that has the file wins:

1. In the game's own folder, next to the game: `roms/gba/Metroid Fusion (USA).cht`.
2. In a `cheats` folder inside it: `roms/gba/cheats/Metroid Fusion (USA).cht`.
3. RetroArch's way, a `cheats` folder next to the system folders with one folder per system:
   `roms/cheats/gba/Metroid Fusion (USA).cht`. The folder can be named like the system folder (`gba`), like the
   RetroArch core (`mGBA`) or like the database's console (`Nintendo - Game Boy Advance`).

On **Options → Cheats** the cheats of your file come first, each marked **[Your file]**, then the database's. They
switch on and off and are remembered for the game exactly like the others, and the last line counts the entries that name
a place in memory, as above. A cheat of the database with the same code as one of yours is listed once, as yours. A
game needs no database entry for this: a file of yours gives it the Cheats page.
On a TrimUI card that has no database yet your cheats are listed all the same, and the **Get · 30 MB** row stays below them.

A file can be up to 256 KB. If it is bigger, can't be opened, or has no cheat lines in it, the first line of the page says
so and the database's cheats are listed as usual: nothing disappears quietly. Cheat files beside your games are not
games: the Game check and the library skip them.

Some cheat files (mostly Atari, PC Engine and a few NES games) name a place in memory and a value instead of giving codes;
pocket-core can't apply those yet; the last line of the page says how many there are, and a game with only such cheats
still gets the page with that line alone.
