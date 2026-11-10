# Options: Cheats

The **Cheats** entry of [Options](options.md), on a page of its own.

## Cheats

Games on pocket-ui's own host get a **Cheats** page in the Options menu when the libretro cheat database has cheats for
them. On TrimUI the database is an optional download: while it is missing, **Options → Cheats** shows a **Get · 30 MB**
row (the same as Settings → Games → [Extras](extras.md#cheats)); opening the page alone downloads nothing. A on it downloads
the database; the row shows how much has arrived. Without a connection it says **Needs Wi-Fi**; connect Wi-Fi and press A
again. B stops a download safely, and the next try continues from its saved checkpoint. After that cheats work offline,
including after app updates. Cards that already have the database keep using it without another download. The page says "No cheats found for
this game" when the installed database has none. ROCKNIX keeps using its included database.

Each cheat is On or Off: A turns it on or off, L1 and R1 skip five lines, "All off" at the end switches them
all off. What is on is remembered for that game and takes effect at once. The game is found by its file name without the
bits in brackets. Cheats are not part of Keep for.

Some cheat files (mostly Atari, PC Engine and a few NES games) name a place in memory and a value instead of giving codes;
pocket-core can't apply those yet; the last line of the page says how many there are, and a game with only such cheats
still gets the page with that line alone.
