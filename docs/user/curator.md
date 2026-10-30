# Curator: Lens, smart shelves, stickers and play history

The second half of [Your library](library.md): what Curator does with a larger collection. Game cards, play status, ratings, notes and
where the facts come from are on [the first page](library.md).

## Curator

Curator helps you find your way around a larger collection, with facts that come with pocket-ui and work without a
network. It has seven parts; each can be switched off in Settings → Playing → [Curator](settings.md#curator), and none takes
up space until it has something to show. [Rediscover](#rediscover) has no switch: it is only there when it has a game.

When pocket-ui starts, Curator reads your library in the background, after the shelf is up and never while a game
runs. Until it is done (with a large library, a second or two), its rows say **Getting ready…**; nothing waits for it.

### Lens

**Curator** is the first row on Home, before the recent games (in the Cabinet look, a shelf: see
[Playing](playing.md#looks)). A opens it: the **Lens** first, then your smart shelves, **History** and **Year in
review** (once you have played something) and your stickers. The Lens combines
filters, for example "Game Boy Advance · Racing · Never played", or "PlayStation · Story-driven · Not known for
grinding · German":

- The left column lists what you can filter by: System, Family (racing, role-playing, platformer …), Sub-genre (kart
  racing, metroidvania …), Mechanics (drifting, time travel …), Players (solo, co-op on one screen, versus on one
  screen …), Length, Session (short or long sessions), Pace, Difficulty, Story, Grinding, Status, My rating, Played
  (never, this month, long ago), Reading, Language (from the file names, a fan translation counts), Controls (plays well on
  a D-pad, two screens …), Era, Setting (fantasy, sci-fi …), Mood (cosy, tense …), Stickers and Favourites. A facet
  with nothing to offer is left out.
- **Length**: **Under 5 hours**, **5–15 hours**, **15–40 hours**, **40+ hours** or **Endless** (score attack,
  sports seasons, versus). It is a rough guide, plus or minus one step, and there is nothing finer: for "under
  about 8 hours" pick Under 5 hours **and** 5–15 hours, which reads "under 15 hours" (two steps next to each other
  read as one range).
- **Story**: **Story-driven** (the story matters, or is the reason to play), **Some story**, **Cinematic** (many
  cutscenes), **Rich world and lore**, or **Little story**.
- **Grinding**: **Grinding** (repeating fights or races to level up or unlock) or **Not known for grinding**.
  Grinding is marked only where it stands out, so the second value means "not marked", not a promise.
- **Reading**: **Lots of reading** (hard to enjoy without reading the game's language well) or **Little
  reading**. Next to Language, it tells you which games work in a language you don't read well.
- **My rating**: **5 stars** down to **1 star**, and **Not rated**, from your own [ratings](library.md#my-rating). Before your
  first rating there is only Not rated, with a line saying how to give one. Picking 5 and 4 stars reads "rated 4 and
  up".
- Story, Grinding and Reading come from Curator's own descriptions of about 9,300 well-known games. A game it
  doesn't know well is in none of their values, so "little" never just means "unknown".
- The right column lists the values of the selected facet. The number next to each value is how many games you
  would see with it picked, so you never pick your way into an empty list by surprise.
- Within one facet the picks are alternatives ("SNES **or** GBA"); across facets they narrow down ("SNES or GBA,
  **and** Racing"). The header shows the result: "18 games".
- In the Curator list the Lens row says what it holds: with nothing picked, the games it looks through and the
  filters it offers ("332 games · 18 filters"), with picks the games they leave ("18 games").

**SELECT** in the Lens chooses the **order** of the games START shows:

| Order | First | Last, in name order |
|---|---|---|
| **Name** | A to Z (as every list, and the start) | |
| **Last played first** | the game you played last | games never played |
| **Most played first** | the most hours | games not played |
| **Best rated first** | your five-star games | games you have not rated |
| **Oldest first** | the earliest year | games without a year in the offline list |
| **Shortest first** | Under 5 hours, up to 40+ hours, then Endless | games Curator has no length for |

A game is never left out for want of a figure: it only goes to the end. An order other than Name is shown under the
Lens's title and after the list's name ("Lens: Racing · most played first"). A game owned on several systems counts
once, with the time of all its versions together and the latest of their last plays.

The buttons of the Lens are in [Controls](controls.md#in-the-lens). The Lens keeps its picks and its order until
pocket-ui restarts.

### Surprise me

Under the Lens in the Curator list: one game picked for you when you can't decide. A opens the picked game's
**card**, with one line over the facts that says why: "Never played · you like arcade racing games", "Started, not
played for 3 months", "Not played for 2 years · you gave it 5 stars". A plays it, **D-pad Right** picks another (the
hint bar says "RIGHT Another"), B goes back to the Curator list. Status, rating and note can be set on that card as on
any other.

How it picks: every game of your library can come up, each game owned on several systems once (its preferred
version). Games you haven't started and games you are playing or paused come up most; completed games about one time
in six as often, dropped ones rarely. A game you played today hardly ever, this week or this month less often, and an
unfinished game you left for more than a month a little more often. Games rated 4 or 5 stars come up a little more, 1
or 2 stars much less. Once Curator knows your taste (time played, finished and starred games, ratings), games like
the ones you play come up more, and the line says what you like about them. Without any of that, it is a fair draw
among your unplayed games and says "Never played". Right never shows a game again until every game has been shown since
you last opened Surprise me.

Its first pick is **today's pick**, the game the Curator row on top of Home names ([Home](playing.md#home)). The
entry sits in the Curator list: from Home, Up twice (Curator), A, Down, A. It is there while Settings → Playing → Curator → **Lens & smart shelves** is on. In its row,
"From 332 games" says what it picks from; "Getting ready…" until Curator has read your library (A then says so too),
and with no games, "No games yet".

### Smart shelves

X in the Lens saves it as a **smart shelf**. pocket-ui suggests a name made of your picks ("GBA · Racing"); change
it on the keyboard or press START to keep it. A new shelf is a row on Home, like Favourites, and keeps itself up to
date: a game you add, finish or star appears or leaves on its own.

In the Curator list, **Y** pins a shelf to Home or takes it off, and **X** opens its options: change the filters
(the Lens opens with them and the shelf's order; X there saves both over the shelf), **Order** (the same choices as
SELECT in the Lens, kept for that shelf), rename, show on Home, delete. A shelf in an order other than Name shows it
after its name and on its row ("Smart shelf, on Home. It updates itself · most played first"). Shelves are kept in
`/storage/.config/pocket-ui/curator.json`.

### Stickers

Stickers are your own labels for games, such as "Couch co-op" or "Before bed". On a game's card, move down to the
**Stickers** row and press A: a short list of your stickers, each with a tick box (after the five
[stickers on the case](library.md#stickers-on-the-case), which the Shelf and Cabinet looks show on the game's case). A puts a sticker on the game or
takes it off; the last entry, **New sticker…**, makes one (a name is suggested). B closes the list.

Stickers show in the Curator list and as a facet of the Lens. Pin one with **Y** in the Curator list to have it as a
row on Home. X there renames or deletes it; deleting a sticker takes it off every game and out of your smart
shelves. Stickers are kept in `library.json`, next to status and notes.

### Echoes

The card's **Echoes** row names the three games of your library most like this one. A on the row opens the full
list of twelve, each with its box and a short reason:

- "Same series · crash takedowns"
- "Futuristic racing · adrenaline rush"
- "Like Burnout 3 - Takedown, which you finished"

The words come from pocket-ui's own list of about 170 descriptions (how a game plays, how it feels, where it is
set), made for about 9,300 well-known games and checked against games labelled by hand. Lesser-known games have
their genre, series, developer and years to go on.

What counts: what kind of game it is and how it plays, the same series or developer, the same years, and your own
taste: games you finished, played for a long time or starred count for more, games you dropped count against
similar ones. A game you have not started yet is moved up a little, one you completed down a little; dropped games
are never suggested. One series fills at most three places, so the list stays varied.

When your taste changed the order of the list, the line under the title says so and names the games behind it:
"Why these? You finished Burnout 3 - Takedown". When it changed nothing, the line just counts the games.

On the Echoes page, A plays the selected game, X opens its card, B goes back. With Settings → Playing → Curator → **Games not
on this device in Echoes** on, the page also names a few games of the same series or developer that you don't have,
from the offline game list. Names only, no shop.

### Up next

When you quit a game you played for a minute or more, the shelf may show **Up next**: one game of yours like it,
with the reason, for example "Up next: Burnout Legends, PSP · Same series". It is the first of the game's Echoes
that you haven't finished or dropped, isn't another version of the same game, and wasn't played in the last day
(so two games don't take turns suggesting each other). When nothing is alike enough, nothing shows.

- **A** plays it; a tap on it too.
- **B** puts it away. Any other button puts it away and does what it always does.
- It goes by itself after a few seconds; after that A is the shelf's again.

Up next never starts a game by itself and costs nothing while you play: Curator works it out
after the game, once the shelf is back. It is off until you switch it on: Settings → Playing → Curator → **Up next after a game** (the first start's Enthusiast turns it on). A device that already had the setting keeps it.

### Versions

When you own a game on more than one system (Chrono Trigger on the SNES, the PlayStation and the DS), its card gets
a **Versions** row. Its page has one row per version: the system, the release and its languages as the file name
says (a fan translation is marked), what plays it and with which performance setting, and your save states, when you
last played it and for how long.

**Y** makes the selected version the **preferred** one; A plays it. Lists that mix systems (Favourites, smart
shelves, Echoes) show such a game once and start the preferred version. Without a choice, the version you played
longest is preferred. Settings → Playing → Curator → **Merge versions in mixed lists** switches this off.

A remake is a game of its own, not a version: Ocarina of Time 3D is not the Nintendo 64 game on another system. When
you own both, each card has a Versions row, and the page lists the other game at the end, marked **Related: remake**.
A plays it; it can't be the preferred version, and it is not listed under Echoes, since its Versions page already
names it.

### Rediscover

**Rediscover** in the Curator list (and on the Curator shelf in the Cabinet look) brings back games you have been
away from: first the ones you played **a year ago today**, then the ones you haven't played **for a year**, those you
played longest first. Each says why it is there ("played a year ago today", "last played 412 days ago"). It counts
pocket-ui's play history, the recent games and, when you came from EmulationStation, the "last played" in your
gamelists, so a card that moved over from ROCKNIX's menu has a past from the first day. The row is only there while
it has a game in it.

### Play history

pocket-ui keeps a small diary of what you play: when a game started and ended, on which system and emulator, for how
long. PortMaster ports and the big emulators (PS2, GameCube, Wii, 3DS) count like any other game; PC streaming and
the tools don't. It stays on this Pocket, in `/storage/.config/pocket-ui/play-log.bin`, and nothing sends it anywhere
unless you switch on [Library data](sync.md#library-data) in Settings → Sync, which shares it with your own other
devices; it is not part of a backup either.

Once you have played something, the Curator list has a **History** row (with the Lens off, History is its own row
on Home). Beside it: the time played this week ("40 min this week"), or "Nothing this week". It shows a week,
a month or a year:

- the time played, the sessions and the days you played on;
- a bar for each day (for each week in a year), the bar of today a little brighter;
- your most played games, with their system and time: A opens a game's card, and B on the card comes back here;
- beside them: your systems, what you leaned into ("Arcade racing · crash takedowns", from Curator's descriptions of
  the games, by time played), your longest streak of days in a row and the one running now, a game you came back
  to after two months or more, the games you marked Completed, and how many games you played for the first time.

L1 and R1 switch between week, month and year; Left and Right go to the one before or after, back to the one the
history began in. Y opens the review of the month or year you are looking at. The buttons are also in
[Controls](controls.md#in-history). The title changes at once; the numbers are counted in the background and
follow a moment later (on a very long history you may see **Counting…** for a moment).

What counts, so the numbers stay honest:

- A game's time is the time it was open, its pause menu included. Sleep is not counted: neither the dark screen
  (light sleep) nor the time the Pocket was asleep, so History can show a little less than the game card, which
  counts the dark screen. A session that runs past midnight is split between the two days, and one you put to sleep
  and went on with later counts each part on the day you played it.
- With **Merge versions in mixed lists** on, a game you played on two consoles is one game, but each console keeps
  the time you played on it.
- A start shorter than a minute adds its time but is no session (as on the game card). A game that did not start at
  all (a damaged zip, a patch for another version) counts for nothing.
- A game that never ended properly, because the battery died or pocket-ui stopped while it ran, counts as a session
  and a day played, but its time is unknown: History says **time not recorded** (for example "1 session, time not
  recorded"), or gives the time it knows and how many sessions have none ("2 h 05 min, 1 without time"). On the
  chart such a day has a small ring instead of a bar.
- When you first update to a version with the history, the time pocket-ui had already counted for your games is
  taken over. It counts in your totals and for "came back after a break", but not in a week, month or year: pocket-ui
  doesn't know when it was played. A period that began before then says "Counted since …".
- "Completed" is the day you set the status on the card. Games marked before the history began aren't listed.
- A session played while the Pocket's clock was wrong (not set yet, or ahead and put right later) is in no week,
  month or year.
- A week, month or year from before the history began says **Not recorded: History counts from …**, not "Nothing
  played".
- A game you deleted or renamed since keeps its name in History; A on it says **Not on this Pocket any more**.
- If the history file was damaged (a faulty SD card, a power cut while it was rewritten), History says so: **Not
  counted** for a few unreadable entries, or **Counted since …: an older history file could not be read** when the
  whole file had to be set aside (it stays on the SD card as `play-log.bin.damaged`). It goes on recording either way.

### Year in review

**Year in review** in the Curator list shows your year so far, one card at a time: the year in numbers, your most
played game and console, what you leaned into, your first game of the year, your longest session, the game you came
back to, the games you completed, your longest streak, and, with Echoes on, one game of yours you might like next.
A card only appears when there is something true to say. Milestones get a sticker.

Left and Right go from card to card, A on a card about a game opens its card, L1 and R1 go to the year before or
after, B goes back. From History, Y on a month opens the same cards for that month (on a year, that year's).

It never opens by itself and never appears on Home: only you open it, from the Curator list or with Y on History.
In January the Curator list's row opens the year that just ended.

Settings → Playing → Curator → **Play history** switches all of this off: History and the reviews go, and nothing new is
recorded. What was recorded stays on the Pocket and comes back when you switch it on again.
