# Curator: Lens, smart shelves and play history

The second half of [Your library](library.md): tools that make big retro collections a joy to explore, powered by offline
metadata built straight into pocket-ui. For game cards, statuses, ratings, notes and metadata sources, see
[Your library](library.md).

## Curator

Curator turns a huge ROM collection into something browsable without internet. Its seven features toggle independently
under Settings → Games → [Curator](settings-playing.md#curator), and nothing clutters your menus until it has something
to show. [Rediscover](#rediscover) has its own switch under Settings → Look → [Cozy extras](settings.md#cozy-extras)
and only appears when it has a game to suggest.

After pocket-ui boots, Curator indexes your library quietly in the background (never while a game is running). For a
second or two on huge collections, Curator rows stay quiet and opened pages show **Getting ready…**; nothing ever blocks
your shelf.

The first time you highlight Curator or Just Play on Home, a one-time hint introduces it ("Curator: pick by mood,
length, or what's new to you" / "Just Play: one press, the game you'd like now"; never shown in Kids Mode).

### Lens

*(Labels created in older versions still work in your library and existing smart shelves, though new labels can't be
added or edited.)*

**Curator** is geared toward big libraries and starts enabled in the Enthusiast profile (fresh defaults keep Lens,
Echoes and Versions off while leaving **Just Play** ready for one-tap picks; upgraded Pockets keep their previous
choices). Turn on **Lens & smart shelves** under Settings → Games → Curator and Curator sits at the top of Home above
recent games (or as its own shelf in Cabinet; see [Playing](playing.md#looks)). Press A to open **Lens**, your smart
shelves, **History** and **Year in review** (once you've logged play time).

Lens lets you stack filters effortlessly, like "Game Boy Advance · Racing · Never played", or "PlayStation · Story-driven
· Not known for grinding · German":

- **Left column (facets):** System, Family (racing, role-playing, platformer …), Sub-genre (kart racing, metroidvania …),
  Mechanics (drifting, time travel …), Players (solo, co-op on one screen, versus on one screen …), Length, Session
  (short or long sessions), Pace, Difficulty, Story, Grinding, Status, My rating, Played (never, this month, long ago),
  Reading, Language (read from filenames, including fan translations), Controls (plays well on a D-pad, two screens …),
  Era, Setting (fantasy, sci-fi …), Mood (cosy, tense …) and Favourites. Empty facets stay hidden automatically.
- **Length:** **Under 5 hours**, **5–15 hours**, **15–40 hours**, **40+ hours** or **Endless** (high-score chasers,
  sports, versus). Rough buckets (give or take one step); picking adjacent ranges merges cleanly: selecting Under 5 hours **and** 5–15
  hours reads "under 15 hours".
- **Story:** **Story-driven**, **Some story**, **Cinematic** (cutscene-heavy), **Rich world and lore** or **Little
  story**.
- **Grinding:** **Grinding** (heavy levelling/farming) or **Not known for grinding** (meaning not flagged, not a
  guarantee).
- **Reading:** **Lots of reading** or **Little reading**, great alongside Language for finding import games you can enjoy
  without reading Japanese.
- **My rating:** **5 stars** down to **1 star**, plus **Not rated** ([My rating](library.md#my-rating)). Until you rate
  a game, only Not rated appears with a quick tip; picking 5 and 4 stars reads "rated 4 and up".
- Story, Grinding and Reading draw from Curator's curated tags covering ~9,300 well-known games. Untagged games don't get
  lumped into "little".
- **Right column (values):** every value shows a live game count so you never filter yourself into a dead end.
- Picks inside one facet act as **OR** ("SNES or GBA"); picks across facets act as **AND** ("SNES or GBA, and Racing"),
  with a live total in the header ("18 games").
- On the Curator menu, the Lens row previews either your full scope ("332 games · 18 filters") or active matches ("18
  games").

Press **SELECT** inside Lens to sort results when you hit START:

| Order | First | Last, in name order |
|---|---|---|
| **Name** | A to Z (default) | |
| **Last played first** | Most recently played | Never played |
| **Most played first** | Most hours logged | Never played |
| **Best rated first** | Your 5-star games | Unrated games |
| **Oldest first** | Earliest release year | Games without a year in the offline database |
| **Shortest first** | Under 5 hours up to 40+ hours, then Endless | Games with unknown length |

Games missing a stat are never dropped; they just sort to the end. Non-alphabetical sorts appear in the title ("Lens:
Racing · most played first"). Multi-console games count once with combined play time and their most recent session.

See [Controls](controls.md#in-the-lens) for Lens shortcuts. Your picks and sort order stay active until pocket-ui
restarts.

### Just Play

Sitting on Home right above Continue (or as the first case on Cabinet's Continue shelf): **press A to launch a game
immediately**, with no browsing and no prompts. *"Less browsing. More playing."* Want to peek first? Press **X** to open its
game card with a quick reason at the top ("Never played · you like arcade racing games", "Started, not played for 3
months", "Not played for 2 years · you gave it 5 stars"). Press **A** to jump in, **D-pad Right** for another suggestion
("RIGHT Another"), or **B** to return. You can set status, rating and notes right on that card.

Launching via Just Play works just like pressing A on a recent game: saved moments resume right where you stopped, using
your chosen emulator and start animation, with the title briefly flashing on screen.

How it picks (first match wins):

1. Unfinished games you've started (Playing, Paused, or Not started with logged time), favouring ones you haven't touched
   in a day or two over something you just closed.
2. Starred favourites you haven't played yet.
3. Unplayed games in your collection.
4. Completed or Dropped games, only if nothing else remains (play time alone never marks a game finished).

It skips the very last game you played while other candidates exist in tiers 1–3, merges multi-console copies to your
preferred version, and never picks hidden games. Unplayed games on demanding consoles ("not every game runs smoothly",
like PS2/GameCube/Wii) appear one-fifth as often so a casual press won't drop you into a slow boot, though games you've
already started or starred aren't throttled. Missing files or unavailable emulators are skipped cleanly without changing
your emulator picks.

Enabled whenever Settings → Games → Curator → **Just Play on Home** is On (hidden in Kids Mode and Easy, where A already
starts the big featured game). Pressing Right cycles through every eligible game before repeating.

### Smart shelves

Press **X** in Lens to save your current filters and sort order as a **smart shelf**. pocket-ui suggests a tidy name
("GBA · Racing") that you can edit or accept with START. Smart shelves sit on Home just like Favourites and update
themselves automatically as you add, finish or star games.

In the Curator menu, press **Y** to pin/unpin a smart shelf on Home, or **X** to edit its filters (reopens Lens; press X
there to overwrite), change **Order**, rename, toggle Home visibility or delete. Custom sort orders display right in the
subtitle ("Smart shelf, on Home. It updates itself · most played first"). Stored in
`/storage/.config/pocket-ui/curator.json`.

### Echoes

Every game card's **Echoes** row previews the three closest matches in your library. Press A to open the full top-12
list with box art and a crisp reason for each:

- "Same series · crash takedowns"
- "Futuristic racing · adrenaline rush"
- "Like Burnout 3 - Takedown, which you finished"

Powered by ~170 gameplay, vibe and setting tags crafted across ~9,300 well-known titles (falling back to genre, series,
developer and era for deeper cuts). Scoring blends gameplay similarity, series/developer, era and your personal taste:
finished, heavily played and starred games pull similar games upward, dropped games push them down, unplayed games get a
gentle boost, and any single series caps at three slots to keep variety fresh.

When your taste influenced the ranking, the subtitle says why ("Why these? You finished Burnout 3 - Takedown"). Press A
to launch, X for the card, B to go back. Enable Settings → Games → Curator → **Games not on this device in Echoes** to
also see a few matching series/developer picks you don't own yet (names only, no store).

### Versions

Own the same game on multiple systems (like Chrono Trigger on SNES, PS1 and DS)? Its card gains a **Versions** row
comparing every copy: console, region/language (flagging fan translations), emulator and performance setting, save
states, last played and play time.

Press **Y** to set your **preferred** version, or **A** to play any copy. Mixed lists (Favourites, smart shelves, Echoes)
collapse duplicates into one entry that launches your preferred version (defaulting to your most-played copy). Turn off
merging anytime under Settings → Games → Curator → **Merge versions in mixed lists**.

Remakes count as separate games rather than versions (like Ocarina of Time vs Ocarina of Time 3D): when you own both,
each appears at the bottom of the other's Versions page tagged **Related: remake** (playable with A, never merged or
duplicated in Echoes).

### Rediscover

Found in Curator (and on Cabinet's Curator shelf), **Rediscover** surfaces nostalgia picks: games you played **a year
ago today** first, then long-neglected favourites untouched **for over a year** (most-played first), each with its
reason ("played a year ago today", "last played 412 days ago"). Reads pocket-ui's play log, recent games and existing
gamelist dates so imported cards have history on day one. Stays hidden when empty; toggle off under Settings → Look →
Cozy extras → **Rediscover**.

### Play history

pocket-ui keeps a private play diary with start/end times, console, emulator and duration across all games, ports and big
emulators (excluding PC streaming and tools). Everything stays locally in `/storage/.config/pocket-ui/play-log.bin` and
never leaves your device unless you enable [Library data](sync.md#library-data) under Settings → Connections → Sync
(excluded from backups too).

Once you've played a game, **History** appears inside Curator (or directly on Home when Lens is off) showing this week's
total ("40 min this week" or "Nothing this week"). Browse by week, month or year:

- Total time, session count and active days
- Daily (or weekly) bar chart with today highlighted
- Top games with console and time (A opens the game card, B returns)
- Top consoles, play-style tastes ("Arcade racing · crash takedowns"), longest and current streaks, comeback games
  (returned after 2+ months), Completed titles and first-time plays

Switch periods with L1/R1 (week/month/year), step back/forward with Left/Right, and press Y for that month or year's
review ([Controls](controls.md#in-history)). Stats calculate in the background (briefly showing **Counting…** on massive
logs).

How time is counted honestly:

- Counts active play plus pause menu time, excluding light sleep and deep sleep (so History may read slightly lower than
  the game card, which includes screen-off pauses). Midnight-crossing and sleep-resumed sessions split accurately across
  each calendar day.
- With **Merge versions in mixed lists** On, multi-console copies merge as one title while preserving per-console hours.
- Sub-minute launches add their seconds without counting as a session; failed launches count zero.
- Unclean exits (dead battery, crash) log the session and day as **time not recorded** (drawn as a small ring on the
  chart; mixed totals read e.g. "2 h 05 min, 1 without time").
- Pre-history totals import into lifetime stats and comeback detection, while earlier periods show "Counted since …" (and
  periods before logging started read **Not recorded: History counts from …**).
- Completed dates match when you set the status on the card; sessions played with an unset/wrong clock stay out of
  calendar periods.
- Deleted or renamed games keep their History entry (pressing A says **Not on this Pocket any more**).
- If a power cut damages `play-log.bin`, History flags unreadable entries (**Not counted**) or archives a broken file as
  `play-log.bin.damaged` ("Counted since …: an older history file could not be read") and keeps recording cleanly.

### Year in review

Open **Year in review** in Curator (or press Y on a month/year in History) to flip through clean recap cards: yearly
totals, top game and console, your genre taste, first game of the year, longest session, comeback pick, completed games,
longest streak and (with Echoes on) a suggestion for what to play next. Cards only appear when there's real data, and
milestones earn a sticker.

Flip cards with Left/Right, open a game card with A, change years with L1/R1, exit with B. It never pops up uninvited
(in January, the Curator row opens the year that just wrapped).

Toggle Settings → Games → Curator → **Play history** Off anytime to hide History and reviews and pause recording. Your
saved log stays intact if you turn it back on later.
