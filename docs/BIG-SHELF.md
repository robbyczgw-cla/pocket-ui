# Easy

The look for someone with 10 to 30 games who just wants to play: one big case, Left and Right, A. Its name in the code is "Big" (`Theme::Big`, `Profile::JustPlay`, `app_big_shelf`, `big_shelf`).

**Renamed:** it was "Just Play" in the first prototype; players now see **Easy** everywhere (the Look list, the first start's row, the manual, English, German and Spanish; its line is "One big game at a time, simple menus."). The code names stay. "Just Play" is the name of a different feature (a one-button action) and is not used for this look. In the first start's list the rows read The shelf, Easy, Enthusiast, Choose for yourself and For a child; the old profile name "Simple" is gone from the screen.

It is the second of the looks
(Settings → Look → **Layout**, and the first start's look step, the same list); going back is picking another look. Player's page: [user/playing.md](user/easy.md).

## What it shares with Kids Mode, and what it doesn't

Shared (`app_kids`): the case row (`kids_case_row`), the name (`kids_shelf_name`), the big buttons of the pause screen (`kids_paint_pause`). The insert, the fade, the toast and the hints are `paint_home`'s.

Not shared: everything that closes a door. No PIN, no time, no list, no `standalone::set_kids`, no `POCKET_CORE_NO_SHORTCUTS`, no closed ROCKNIX keys, no volume ceiling, no blocked routes: `kids_live()` stays false. Kids Mode
sets the Shelf look for itself; its words, its shelf and its pause are as they were.

It began as a switch: `Settings::load` turns a saved `big_shelf` into `"theme": "big"`, and the next save drops it. Phones keep their five looks (`Theme::PHONE`).

## The shelf

- One stable order (`big_shelf::arrange`): the favourites first, then each system with all its games. Playing moves nothing; the shelf opens on the game played last. A favourite stands twice: in front, where the top line says "Favourites", and in its place on its system; not when every game is a favourite. A star or a new list leaves the selection in the part of the list it was in.
- Top line: "3 of 9 · Super Nintendo", counting the systems in their order; "Favourites · Super Nintendo" in front; one game has only the system. Under the name (`big_info`, made once per game): "system · year · players"; a line or two of what the game is (the player's gamelist description, else the sentence made of the facts; in German and Spanish only the genre, as the sentence is English), cut after a whole sentence or with an ellipsis; "Played 2 h 03 min · 2 days ago". A line with nothing known is left out with no gap, and the open-case line stays below them.
- **A** plays. The word is **Continue** exactly when the next start would resume (`frozen::moments`, on a thread, over all the games: no limit of five, whatever the Continue setting says; not a moment two games of one file name share and this one can't be shown to own), with the case's ajar picture well out (`paint_ajar_game`: the same share of the case's width for a cartridge, a card and a disc) and the word on the case itself, else **Play**. The first time an open case is on screen, one line under the facts says "Open means your moment is waiting" (`Settings::open_case_told`): it stays until the player moves on or presses A, is then kept as told and never comes again; not in Kids Mode (`big_on` is false there).
- **X**: the normal game card of the other looks (`show_card`, the info page: facts, description, time played, notes, Wikipedia text), **B** back to the shelf. The separate short card is gone (`CardData::plain` is only the Connections page's now). **Y** stars, with the star after the name. **HOME** opens the quick settings, hinted so; **B** does nothing on the shelf, as on Home in the other looks (`quick::Sys` owns HOME). The hint bar shows three hints, each with its word: A, HOME (Quick settings) and START (`kit::top_hints`, `EASY_HINTS`); X, Y and Up/Down work as before and are in the tour and the manual, so no key stands without its word. **START**: Settings.
- **Up/Down** between the groups whenever there is more than one system. A held Left or Right repeats and stops at the ends; a press goes round.
- A failed start: the notice becomes a page in plain words (`big_shelf::failure`, `big_note_failure`) with the file's name small under it, A for the one thing that helps (Add games for a BIOS, else Game check) and B Back. It names no emulator; the notice itself goes to the log. Other notices are the normal banner. Only in this look; the normal shelf keeps its 8-second banner.
- An empty card: `big_live()` is false, so Home's normal path shows "No games yet" and the starter pack. A missing cover: the drawn front with the console and the name. Off Home (a Curator list from Settings) it paints the plain list (`paint_minimal`, the Minimal look until 0.4).
- A card with more than 30 games says "Best with a handful of games" on the look step, and nothing else is there for it. It still works.

## Calmer than Kids Mode

The case, the name, the facts and the pause rows are smaller than a child's and stand further apart (`SHELF_ROOM`, `NAME_SCALE`, `PAUSE_ROW` in `app_big_shelf`; `kids_shelf_name_at` takes the scale, Kids Mode calls it with 1.0 and is unchanged). The test pictures at 640x480, 1240x1080 and 1920x1080 and at Large text keep the name, the facts, the open line and the hints readable.

## No numbers over the game, no game switcher

- **Over the game** is forced off in a Easy game: pocket-ui puts `POCKET_CORE_NO_OVER_GAME` in the game's environment (`extras::NO_OVER_GAME`) and pocket-core reads every level, the Options key and HOME + Y as Off. The setting in settings.json and the Options files is never written, so another look has it back at the next start of a game.
- **No game switcher**, on purpose: SELECT on the shelf (`select_alone`) and HOME + SELECT or SELECT in the pause screen (`switcher_in_game_input`) do nothing, and no hint names them. HOME held for SELECT is no tap that opens the pause screen. The other looks keep both ways in; `just_play_has_no_game_switcher_and_the_other_looks_keep_theirs` presses them in both.

## A slow Quit

An exit save's time is its fsyncs: 0.08 to 0.7 s on the Odin's card when nothing else writes, seconds when something does (ext4 commits what else is waiting with it). pocket-core marks the save (`fast_switch::saving`) and nothing stops it while the mark stands, up to `quit::SAVE_LIMIT`; a wait over a second shows a spinner in every look (`paint_saving_wait`). `POCKET_TEST_SAVE_DELAY_MS` slows the save for a device test.

## The pause screen

Continue, then **Save & quit** where the moment is kept or **Quit** where it isn't (`big_keeps_moment`: the route's saves and `quit_leaves_out_resume`, so Continue where I quit counts), and **More: saves, brightness, options** as a third row of the same kind (selected like the others, a normal button on every size) that opens the normal pause menu (B comes back). START opens the quick settings. Up and Down move, Left and Right don't. START is hinted. A game that saves only in the game asks once, "Have you saved in the game?"; with Continue where I quit off the line says "Doesn't save where you are. In-game saves are kept." and there is no extra press. The line never names an emulator. A
card that refuses saves gets the normal page.

## Seen on a device

Mini V2, 2026-11-02, through the command FIFO, on temporary copies of test ROMs and a stub port: opens on the game played last, order unchanged after a game, Y's star, B's panel, the short card with Y, START and SELECT doing nothing, Up/Down
between systems; a pocket-core game (Left moves nothing, START's panel, Save & quit wrote its state, the case stands open with Resume); a port (Quit, the line, the question, then it quits); a port that exits at once (the page stays, A closes it and starts nothing).
RetroArch's route was seen in the earlier round; this round's wording for it is a snapshot's. Not seen: a phone upload live, a real held button, an RG28XX (640x480 is snapshots), touch.

## Later

Not decided: a Pick your games step. The first start offers this look as its own row, under The shelf (`Profile::JustPlay`).
