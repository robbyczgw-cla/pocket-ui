# Your library: your files, gamelists and discs

The sections of [Your library](library.md) about the files on your card.

## Your EmulationStation gamelists

If you come from EmulationStation (ROCKNIX's own menu, KNULLI, Batocera or ES-DE), your games may have a
`gamelist.xml` with names, descriptions, ratings, release dates and favourites you or a scraper made. pocket-ui reads
those files, **never changes them**, and uses them where its own data has nothing:

- **Where it looks**: `gamelist.xml` in each system's ROM folder (`roms/gba/gamelist.xml`), in ES's own
  `gamelists` folders, and in ES-DE's `ES-DE/gamelists/<system>/` folder. ES-DE keeps its pictures in
  `ES-DE/downloaded_media/<system>/`, which is read too. A game that several files list is taken from the first.
- **Names**: a file name that says little ("sf2ce.zip", "kof98", "1943") shows the name the gamelist gives it
  ("Street Fighter II' Champion Edition"). A file name that already reads as a name keeps pocket-ui's own, cleaned
  name ("Tetris", "Super Mario Land"), even when the gamelist spells it with a region or a version.
- **The card**: when the game's own facts are missing, the card takes the developer, publisher, genre, number of
  players and year from the gamelist, shows its rating as "4 of 5 in EmulationStation", and shows its description
  under **Description**, "From your EmulationStation gamelist". Wikipedia's text, once it is on the SD card, comes
  first, and so do the facts from Wikidata; the card says under the facts where they are from.
- **Box art**: a game without box art of its own shows the picture the gamelist names (a box, else a screenshot),
  where it is. Nothing is copied, and a picture of more than 1 MB is passed over, because drawing it while you scroll
  would make the shelf stutter. A game that has a box on your card is not looked up online at all; one that has only
  a screenshot still gets the collection's box, which then comes first.
- **Favourites and ratings**: nothing is taken over by itself. Settings → Games → **From EmulationStation** counts
  them ("Your EmulationStation favourites: 42 games") and A adds the favourites to Favourites, or gives the games
  their stars as **My rating** (ES's 0.1 to 1.0 becomes one to five stars). It only adds: games already in Favourites
  stay, and a game you rated yourself keeps your rating. The page works any time; without a gamelist it says so
  and where to put one.

A damaged, cut-off or very large (over 32 MB) gamelist never breaks the library: the games before the damage still
count, a file in another encoding (Latin-1, UTF-16) is read, and a file that can't be read at all is skipped.
What pocket-ui read is kept in `/storage/.config/pocket-ui/gamelist-cache.json`, so a start does not read the XML
again; a changed file is read again, and deleting the cache file does no harm.

## PlayStation 1 and 2 discs

A PlayStation disc is recognised by the **serial on the disc itself**, not by its file name. pocket-ui reads the
short boot file in the disc's file system (`SYSTEM.CNF`, whose `BOOT` line says "SLES_556.73"; a few kilobytes at
the start of the image, never the whole image), and looks that serial up in the list of PlayStation 1 and 2 releases
(Redump, through libretro-database), the same way a cartridge is recognised by its checksum ([Box art & downloads](extras.md#box-art)).
A file called `SLES_556.73.Something.iso` or `SLUS_209.46.iso` then shows the game's name on the shelf ("Final Fantasy
VII", "Gran Turismo 4"), gets that game's facts and its box art and pictures. The list is downloaded once for the
console you have games for, when the Pocket is online (about 4 MB per console), and kept on the SD card.

- A name you gave the game yourself comes first: a name from your
  [EmulationStation gamelist](#your-emulationstation-gamelists) stays as it is.
- A serial no list knows keeps the file name, with a serial in front left out when a title follows it
  ("SLES_556.73.Something" is "Something", "SLUS_209.46" stays as it is).
- Read are `.iso`, `.bin` and the `.cue` or `.m3u` that names them, including a PlayStation disc's own sector format
  (MODE2/2352, with a header in front of the data). A `.chd`, `.cso` or `.pbp` is not opened: such a game keeps its
  file name.
- The two discs of a multi-disc game show "(Disc 2)" after the name, so they tell apart.
- Reading a disc takes a few milliseconds and runs in the background, at the lowest priority and never while a game
  runs; each image is read once, and again only when the file changes.

The same goes for the game card's facts, which then match the release the serial names, and for Curator. The two
consoles are PlayStation (PS1) and PlayStation 2. Other discs (Saturn, Dreamcast, PSP, GameCube, …) are still matched
by their file names.

## Matching your files

pocket-ui finds the entry for a file by its name, the way it finds box art: region and version tags
such as "(USA, Europe) (Rev 2)" and dump tags in square brackets are ignored, a scene release number
("0902 - ") and a version at the end ("Pipes.v1.02") are dropped, "Legend of Zelda, The" reads as "The Legend of
Zelda", accents and "&"/"and" do
not matter, and "III" is the same as "3". Names in the No-Intro or Redump style match best. A different
sequel number never matches: "Crash Bandicoot 3" is not taken for "Crash Bandicoot 2".

Two games of one system that would show the same name keep what tells them apart ("Tobu Tobu Girl (Europe)" and
"Tobu Tobu Girl (World)"). That changes only the name in the lists: the card, box art, play time, favourites and
notes go by the file, as before. Your recent games (Continue and the list on SELECT), the games
in Play history and the web companion show the same names as the system list.

Curator (Lens, smart shelves, Echoes, Versions, play history and the Year in review) has its own page:
[Curator](curator.md).

## A card from another handheld

Cards from other systems work as they are; nothing has to be renamed, moved or scraped again. pocket-ui reads the games
where they lie and never changes your folders.

- **Folders**: next to `gba`, `snes` and the other folders, it reads a folder whose name means a system: `Game Boy Advance (GBA)`
  (the word in brackets decides, so `(MGBA)` and `(SFC)` work), `GBA`, `FC`, `MD`, `PS`, `genesis`, `Sega Genesis`,
  `Nintendo Game Boy Advance`. Two folders of one system are one list. A folder it does not know is named by
  Settings → Games → Game check.
- **Box art that is already there** is used before anything is downloaded: `images`, `Imgs`, `.media`, `covers`, ES-DE's
  `downloaded_media` and the `box` folders of a catalogue. A picture of the box comes first, then a mix, then a screenshot;
  logos, marquees and fan art never. Only when a game has no box of its own does pocket-ui fetch the collection's.
