# Your library: game cards, play status, ratings and notes

pocket-ui is made for a small set of games you actually play. Every game has a **card**: one calm
page with its box, what is known about it, how much you have played it, where you are with it, and
what else you own by the same people.

## Opening a card

Select a game on the shelf, in a system's list or in Favourites and press **X** (the top face button).
The hint bar shows "X Details" whenever a game is selected.

| Button | On the card |
|---|---|
| A | Play the game. On one of the rows under "Your game" (a save with two versions, Echoes, Versions, Stickers, Screenshots) or on Hide or delete: open it. |
| B | Back to the list, at the same game. |
| Y | Set the play status, or your [rating](#my-rating) (the row under the statuses). Under them: [Wrong game? Choose another](#wrong-game-choose-another) and [Wrong pictures? Look up again](#wrong-pictures-look-up-again). |
| X | Write or change your note. |
| START | Open [This game](options.md#this-game): the game's picture, frame, performance, controls and emulator, without starting it. On a PortMaster port: its [Performance & fan](performance.md#portmaster-ports). |
| D-pad Up / Down | Scroll the page. Down moves onto the rows (the save line, Curator's rows) once they are in view, Up from the first one goes back to scrolling. At the end of the card Down lands on **Hide or delete**. |
| L1 / R1 | Scroll a whole page. |
| D-pad Right | Only on a card opened by [Surprise me](curator.md#surprise-me): pick another game. |

## The game box

The card opens as the game's box, almost the whole screen, in the style of its case: a cardboard box for cartridges (as wide as the cover: a Super Nintendo box lies
on its side), the jewel case of a PlayStation or Saturn disc, the DVD case of a PS2, GameCube, Wii or Xbox game in
its colour, the Mega Drive's clamshell, the UMD case, the DS case. The front is the cover. A picture that has not been loaded yet appears when it is ready; you can keep moving through the list or opening the box while it loads.

| Button | On the box |
|---|---|
| D-pad Left / Right | Turn the box over. (On a card of [Surprise me](curator.md#surprise-me), Right picks another game and Left turns it.) |
| D-pad Up / Down, L1 / R1 | On the back: scroll it. |
| A, B, Y, X | As on the card below: play, back, status and rating, note. |
| START | [This game](options.md#this-game), as on the card. |
| SELECT | Show the card as a page, with its rows (Echoes, Screenshots, Hide or delete…). SELECT on the page brings the box back. pocket-ui keeps your choice: the next card opens the same way. |

The back is laid out like a real box's: the title on a band at the top, the screenshot and the title screen beside the
description, then the facts (year, developer, publisher, genre, players, reviews) and your game (status, play time,
rating, achievements), the system's name at the foot. Without a description from Wikipedia or your gamelist the
English card shows a sentence made of the facts; in German the back leaves it out, since the facts under it say the same. What isn't there yet gets a plain place of its own: a dark
screen with the console and "No picture yet", "No description yet", or the line that the game isn't in the offline
list.

## What the card shows

- **The label at the top**: the box art (when cover art was downloaded, see [Extras](extras.md)), the
  name, system, year and number of players, and your status with the time played.
- **Pictures**: under the facts, the game running and its title screen ("In the game", "Title screen"), when the
  free collection has them and Settings → Games → Extras → **Pictures** is Box art and screenshots ([Extras](extras.md#pictures)).
  A game without them has no such heading and no gap.
- **About the game**: developer, publisher, genre, series and review scores where they are known.
  A score reads, for example, "Metacritic 92 · via Wikidata". When the offline game list spells the game
  differently from your file, "Listed as" shows its name, so you can see what the card is about.
- **Your game**: status, your rating (five stars, or "None. Press Y" before you give one), time played and number
  of sessions, when you last played, your note, and how many of
  the game's achievements you unlocked when it has some on RetroAchievements ([Achievements](achievements.md)).
  **Battery** says how long the charge you have now lasts in this game and how much of it the game uses an hour
  ("About 3 h 20 min left, 18% an hour"), from the same measurements as the pause menu (see
  [Battery in this game](pause-menu.md#battery-in-this-game)): "Measuring this game…" until there is enough, the
  console's or all your games' time in brackets while only those are measured, and the rate alone on the charger.
  In the first moments after pocket-ui starts it may say "Loading…" while the measurements are read.
  A PortMaster port also has **Speed**, its performance and fan ("Auto, fan Auto"; "Loading…" in those first
  moments, when START says "Still reading the ports' settings…" instead of opening the page). When the port was seen keeping
  a processor core busy all the time, "Busy core? Press START" appears under it until you have opened that page; START opens
  [Performance & fan](performance.md#portmaster-ports), which offers Battery in one press.
- **Two versions of this game's save**: only when [Sync](sync.md) found that this game's save changed on two devices
  before they could sync. The row says "Two versions of this game's save. Choose one…" (or "… of a save state", or
  "… of the memory card" for a PS2 or GameCube card, which every game of that console shares). A on it opens the
  [choice](sync.md#two-versions-of-a-save). Without Syncthing, or with nothing to choose, there is no such row.
- **Echoes**, **Versions** and **Stickers**, three rows from [Curator](curator.md#curator) (each can be switched off in
  Settings → Playing → Curator).
- **Screenshots (3)**: only on a game you took screenshots of (HOME + A while you play). The row says how many, and when
  the newest was taken ("Newest: Today"). A opens a [gallery](playing.md#looking-at-your-screenshots) of this game's
  screenshots; B comes back to the card.
- **Description**: a few sentences from the game's English Wikipedia article (see below). Without a
  connection, a short line made of the facts above. A game with a description in your
  [EmulationStation gamelist](#your-emulationstation-gamelists) shows that one, unless Wikipedia's text is on the
  card already.
- **More by …** and **Other games in …**: games **from your own library** by the same developer or
  in the same series. The card never lists games you do not have; it is not a store.
- **Hide or delete**, the last row of every card: see [below](#hide-or-delete-a-game).

A console's [Credits & links](playing.md#credits--links) (X on a system, then X again) lists your games by the
makers of the console in the same way, and A there opens their cards.

Games that are not in the offline game list (homebrew, test ROMs, hacks with unusual names) still get
a card with your status, time and note.

## Play status

Press **Y** on the card and pick one:

| Status | Mark on the shelf |
|---|---|
| Not started | none |
| Playing | a dot |
| Paused | two bars |
| Completed | a tick |
| Dropped | a dash |

The mark sits at the right end of the game's spine on the shelf, and the pulled-out label starts its
line with the status ("Playing, last played 2 h ago ...").

After the **second session** of a game you have not given a status (a session is at least a minute of
play), the card suggests **Playing**: "Playing? Press Y". A game with at least a minute of play time from
before pocket-ui counted sessions (it shows in the recent games) gets the same suggestion right away. The picker then opens on Playing. It is only a
suggestion; nothing changes until you choose. Choosing any status, "Not started" included, ends the
suggestion for that game.

## My rating

Your own one-to-five stars for a game, next to its status: how much **you** liked it, whatever the reviews say.
Press **Y** on the card and go down to **My rating**, under Dropped. Left and Right give fewer or more stars, A keeps
them, B leaves them as they were. All the way left is "Not rated" again. The card shows the stars under "Your game";
"Rated 4 stars" shows for a moment when you set them.

Ratings are a [Lens](curator.md#lens) filter (**My rating**), an order for the Lens's games and your smart shelves (**Best
rated first**), and part of what [Surprise me](curator.md#surprise-me) and [Echoes](curator.md#echoes) know about your taste: five stars
count like a star for Favourites, four half that, three nothing, and one or two stars against games like it. A game
you have not rated counts as before.

## Wrong game? Choose another

If the card shows facts about a different game (or about none, although the game is well known), press
**Y** and pick the row under My rating, **Wrong game? Choose another**. pocket-ui lists the games of that system
whose names are closest to your file's, with system, year and developer, the one shown now marked "now",
and **None of these** at the end, which shows no facts. A picks. Your choice is kept for that file in
`library.json` and comes before the automatic match from then on; picking the game the name finds
anyway goes back to the automatic match. The box art stays as it was: covers are found by the game's checksum
or its file name in the libretro thumbnail collection, not through these facts; for a wrong cover, see the next
section.

The facts are matched the way the covers are: by the usual name of the release when pocket-ui recognised the game
by its checksum or, for a PlayStation 1 or 2 disc, by the serial on the disc ([below](#playstation-1-and-2-discs)),
else by the file name. A choice made here comes before both.

## Wrong pictures? Look up again

If a game shows the wrong box art, screenshot or title screen, or none although the collection should have them,
press **Y** on its card and pick the last row, **Wrong pictures? Look up again**. pocket-ui deletes the game's
pictures, reads the game file again to recognise it by its checksum (for a PlayStation 1 or 2 disc: by the serial on
the disc, [below](#playstation-1-and-2-discs)), and looks it up once more, by the name that gives first and by the
file name after it. The card says how it went, for example "Found 3 pictures for
Mega Man & Bass (USA)" or "No pictures found for this game"; when it found the game's usual name, the facts follow
it too.

- It needs Wi-Fi. Offline the card says "Connect to Wi-Fi first to look up pictures." and nothing is changed.
- While the automatic download runs, the card asks you to try again in a moment.
- When the collection can't be reached, the card says so; the game is looked up again with the next download.
- A picture of your own under the game's name is replaced too. Put it back afterwards if you want to keep it.
- The row is there for the consoles the collection has pictures for.

## Notes

Press **X** on the card to write a short note, for example "after the first boss". It uses the same
keyboard as the Wi-Fi password: A types, B deletes (B on an empty field leaves without saving), X is
Shift, Y switches to symbols, START saves. A note has up to 80 characters. To remove a note, delete its
text and press START.

## Hide or delete a game

At the very end of a game's card, under everything else, is the row **Hide or delete**. A on it offers two things:

- **Hide from the shelf**: the game stays on the SD card but leaves every list: Home's recent games, its system's list,
  Favourites, the search, Curator (Lens, smart shelves, stickers, Echoes, Versions, Up next), the game switcher, On start
  and the [web companion](web-companion.md). Good for BIOS test files, demos, the second disc of a game you start from
  its playlist, a version you don't want. The card closes and says where to bring it back: Settings → System → Storage →
  **Hidden games**, where A on a game shows it again. Hiding is kept in `library.json` (below), by the game's path.
  [Play history](curator.md#play-history) still counts the time you played it: it is a record of what happened.
- **Delete from the SD card…**: shows what it frees first ("Frees 2.3 GB: 3 files.") and the first file names, then asks
  twice: A on **Delete 2.3 GB**, then A again ("Press A again to delete. B keeps it."). Every file of the game goes: a
  `.cue` with its `.bin` tracks, a `.gdi` with its tracks, an `.m3u` with all its discs and their tracks, a game that is
  a folder with what is in it. A folder that held only that game goes when it is empty afterwards. Its box art and
  pictures go too.

  **Saves and states** stay unless you change the row above Delete to "Delete too" (A or left/right; it shows how much
  they take). Saves are the files beside the game with its name (`.srm`, `.sav`, a PS1 memory card …); states are the
  slots, the Continue state and Before load in the states folder. Shared memory cards (PS2, GameCube) are never a
  game's, and a PS2 game's states are named after its disc and stay. With [Sync](sync.md) sharing saves or states,
  the sheet says that they are deleted from your other devices too. A game in a folder of its own keeps the files in it that
  look like saves unless you choose "Delete too".

  Deleting is for good: there is no bin. The time played, status and note stay in `library.json` in case the game comes
  back.

## Where it is kept

Status, ratings, notes and session counts are in one file, `/storage/.config/pocket-ui/library.json`, next to
your favourites. It is written so that a power cut cannot leave it empty or half-written, and the last
good copy is kept as `library.json.bak`; pocket-ui uses the copy if the file is ever damaged. Games are
remembered by their file path: renaming or moving a ROM starts it fresh, as with favourites.

Descriptions loaded from Wikipedia are kept in `/storage/.config/pocket-ui/wikipedia/`, so each one is
loaded only once. Delete the folder to load them again.

While Wi-Fi is on and no game runs, pocket-ui loads the missing descriptions on its own, one after the
other with a pause between them, the way it downloads missing cover art; the shelf shows
"Descriptions 7 of 30" at the top right while it does. Starting a game stops it after the current one, and it
goes on after the game. Only games with an entry in the offline game list and a Wikipedia article are
asked for.

## Where the facts come from

- **Wikidata** (CC0, public domain): name, year, developer, publisher, genre, series, game mode,
  number of players and review scores. pocket-ui ships a compact copy of these facts for about 30
  systems, so the card works offline. Review scores are the ones Wikidata records, with the reviewer
  named; pocket-ui does not take anything from Metacritic, OpenCritic or HowLongToBeat itself.
- **Wikipedia** (CC BY-SA 4.0): the description. It is not shipped with pocket-ui. When you open a card
  with Wi-Fi on, pocket-ui asks Wikipedia for the lead of the game's article, shows it with
  "Text: Wikipedia, “<article>”, CC BY-SA 4.0" underneath, and keeps it on the SD card. With Wi-Fi on it
  also loads the descriptions of your other games in advance (see above). This means Wikipedia sees which
  games are in your library, as it would if you looked them up in a browser. pocket-ui introduces itself
  as `pocket-ui-wiki/0.1` and sends nothing else about you.

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
  would make the shelf stutter. Box art that pocket-ui downloads comes first.
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
short boot file in the disc's file system (`SYSTEM.CNF`, whose `BOOT` line says "SLES_556.73") — a few kilobytes at
the start of the image, never the whole image — and looks that serial up in the list of PlayStation 1 and 2 releases
(Redump, through libretro-database), the same way a cartridge is recognised by its checksum ([Extras](extras.md#cover-art)).
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
notes go by the file, as before. Your recent games (Continue, the list on SELECT and the game switcher), the games
in Play history and the web companion show the same names as the system list.

Curator (Lens, Surprise me, smart shelves, stickers, Echoes, Up next, Versions, play history and the Year in review) has its own page:
[Curator](curator.md).
