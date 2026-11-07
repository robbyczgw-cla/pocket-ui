# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/) (before 1.0, a minor version may change anything).

## [Unreleased]

## [0.3.0] - 2026-11-07

### Added

- **Retro style**: Settings → Look → Style → Retro turns every menu into an old handheld's: pixel letters everywhere,
  chunky pixel frames, a flat selection bar with a small cursor, little colour icons for the Settings categories and the
  pause menu, short blips as menu sounds, and two colour sets of its own, Cocoa and Pea (dark and light). The shelf, the
  box art and Continue stay as they are. Numbers use clear LCD-style digits and the pause menu, Save and Load, Options and
  game notes fit at every size. The Font row is now called Style. After the update a "New in pocket-ui 0.3.0"
  card offers it: A ("Try Retro") switches it on, B leaves the style as it was.
- **TrimUI Brick (Beta)**: pocket-ui runs on the TrimUI Brick from a microSD card of its own. Unzip the card download onto an
  empty card, put it in, switch on; the Brick's stock firmware is not touched, and with your usual card back the Brick is what
  it was. The shelf, pause menu and settings fit its 1024x768 screen; Wi-Fi, Bluetooth pads and headphones, Kids Mode, games
  from your phone, Tailscale and Sync are there, F1 and F2 are quick save and load, and the mute slider and lights work.
  Updates come over Wi-Fi and replace only pocket-ui's own folder: your games, saves and settings stay, a new version that
  doesn't start goes back to the one before, and a power cut during an update is recovered at the next start. It is a Beta:
  tested on one Brick so far, so tell us what you find.
- **A shorter first start**: your language, Wi-Fi, your games, how they stand. The time zone is guessed while the Wi-Fi
  page is up and only asked when there is no clear guess; Box art is one switch on "Your games", which says what it found;
  the look comes last, shown with your own games: the layouts to choose from, with Style (Barlow, Pixel, Rounded, Retro),
  Colours and Appearance under them and the previews changing as you go, Retro's Cocoa and Pea included. "How do you like to
  play?", the feature cards and the separate Extras page are no longer in the first start (Settings → Help & tools has
  them). "For a child…" is a row on the first page; Kids Mode's steps come after the games, never with an empty shelf. Help &
  tools has **First-start setup** to run it again. The time zone is asked once (Skip counts), Wi-Fi opened from the phone
  page leads back to it, the time zone search knows German and Spanish city names (wien, viena), games you send during the
  setup stand in the look, and the first shelf has no "new games" toast for them. "Your games" names each console.
- **Controller setup**: each controller gets its own layout, kept for its identity through reconnects and player changes
  (Settings → Playing → Buttons → Controller setup, also Pause → Options → Controls). It captures analogue triggers and
  swaps a button that is already in use. SN30 Pro, Pro 2, Xbox, DualSense and Switch Pro are recognised with their SDL
  layouts, including brake/gas trigger axes and right sticks on Z/RZ. Disconnected players keep their seats; ROCKNIX's SDL
  Dolphin and ARMSX2 get the same layouts through separate virtual player pads. Azahar keeps its own controller setup (its
  buttons are bound to the built-in pad), and a game starts even when the virtual pads can't be made.
- **Test buttons** (Settings → Help & tools): every button, trigger and stick of each connected controller lights up while
  pressed, with what the last press does in menus and in games after your layout; hold HOME for 2 seconds (or B and START)
  to leave. In Retro the hints before the first press stay inside their panel.
- **A newer pocket-ui is said once on the shelf**: "pocket-ui 0.3.0 is ready. Your games and settings stay." A opens
  Settings → System → Updates, B closes it. Once per version, a little after the start and only with Wi-Fi, never over a
  game, a page, the first start or in Kids Mode. It only reads the signed update list the Updates page reads;
  `{"off": true}` in `update-hint.json` stops it.
- Wi-Fi: a saved network has a menu with Connect and Forget.

### Changed

- **One control scheme everywhere** (`docs/UI-SCHEME.md`): Up/Down move through every row, Left/Right change the focused
  one, A opens or confirms, and B goes back one step and does nothing else. The first start follows it too: B goes back a
  stage (before, it skipped forward), and every way on is a row you can reach with the D-pad: Scan again, Continue or
  Continue offline, For a child, Looks right / Choose another zone. Shortcut keys (X, Y, L1, L2, START) are named when there
  is room and are never the only way to anything. START still skips the rest of the first start, L2 switches Easy to read.
- **Every menu is reachable with the D-pad.** A new test walks every Settings page and setup page with the D-pad and fails on
  a row it cannot reach. It found Settings → Look, where Up/Down only turned the layouts: Appearance, Motion and Animations
  under them are rows now, B cancels a look you were trying out, and the hint bar names the keys of the row in focus. The
  hint bars name only keys that do something there: no A where nothing is waiting or there is one choice, Left/Right on
  rows that take them, and no arrows on values you can only read.
- **Where was I?** waits for a button: the deadline of about four seconds is gone. The press that dismisses it starts play
  and does nothing else.
- Faster Quit and sleep saves in pocket-ui's own player: automatic moments are compressed before the durable write. Older
  raw saves still load; manual slots keep their format. Downgrading to 0.2.x starts from the in-game save without
  overwriting the compressed moment. Linux, Deck and Android share the format.
- Still menus sleep between visible changes. Long selected names and Split previews make one pass and settle; toasts,
  notices and the second-screen window stop requesting frames while still. Menu audio closes after browsing stops.
- A game still loading after three seconds shows "Loading the game…" with a spinner, on every Linux device.
- Update downloads can use signed alternate servers, stop after 30 seconds without progress, and show connection and
  verification phases. A slow but steady download (a system update, weak Wi-Fi) is no longer cut off after five minutes.
- Updates come from pocketui.xyz by default for every pocket-ui version, with GitHub as a signed backup.
- Runs on starts with a "Default (…)" entry, the way back to the default emulator for a game or a console.
- Settings → Look lists the layout first, then the style; Help & tools has First-start setup and Test buttons.

### Fixed

- **Sound at the start of a game**: the first moments of a game's sound are no longer cut off while the output connects,
  and a game restored from a Continue moment plays its opening sound whole. PAL games such as PlayStation titles keep their
  sound after Resume too, including after pausing again and again.
- Controllers: the built-in buttons of the supported handhelds keep their measured layout when a USB or Bluetooth pad
  reports the same identity; a controller that drops out during a game gets its player back.
- Switch off after sleeping only switches off with the game saved just before: a save that fails, or a game in RetroArch
  (no moment for Continue), keeps the Pocket asleep instead of closing the game. On the Odin 2 Mini the screen output comes
  on first, so the save can run.
- Games that stop answering Pause, Quit or SIGTERM close through an independent guard, including stalls during loading,
  Continue and teardown; the previous saved moment stays. A game waiting behind a switched-off screen is no longer taken
  for a stalled one. Jeanne d'Arc defaults to standalone PPSSPP on ROCKNIX to avoid its libretro build's opening-movie hang;
  explicit emulator choices still win.
- Saves are harder to lose. A Continue moment that the card left damaged gives way to the moment before it, with a note,
  instead of being thrown away together with it at the next Quit. A moment that doesn't fit on a full card is said over the
  game and on the shelf. An in-game save the card can't read at the start is never written over.
- A power cut right after an update can no longer leave empty programs behind: the new version is written to the SD card
  before pocket-ui switches to it, and checks that its game player starts before it keeps itself; otherwise the previous
  version comes back.
- Updates check the SD card first and say how much room is missing instead of failing with "Check Wi-Fi"; a download too
  slow for its time says so, a wrong clock is named as the reason a secure connection fails, and a part left by a power cut
  that turns out bad is loaded once more on its own.
- 3DS: Save & quit no longer waits 16 seconds after the save for Azahar to be forced shut. Its window comes to the front for
  the close, and the wait for a signal Azahar ignores is gone.
- First start and screens: the welcome and "Your games" keep clear of the hint bar on a 1024x768 screen (the Brick), the
  Spanish welcome fits small screens with large text, and Add games keeps the selected tile in view on short screens.
  "Report a problem" and search no longer fail on logs or game names with unusual letters. A saved Wi-Fi network that a
  fresh scan doesn't see no longer disconnects the current one.
- German and Spanish wording fixes from the 0.2 review.

## [0.2.5] - 2026-11-07

### Fixed

- Pokémon Ruby, Sapphire and Emerald on gpSP keep time while the game is closed, including after loading an older moment.
- Start over keeps the cartridge clock in Pokémon Gold, Silver and Crystal on Gambatte.
- A crash after a game has shown a picture keeps pocket-core and Continue. A failed start may try the system emulator once; the next start tries pocket-core again, and existing moments prevent an automatic switch.

## [0.2.4] - 2026-11-06

### Fixed

- Continue moments skipped after an emulator or game-file change, or refused by a core, stay in a free slot past the next Save & quit. With every slot full, they wait safely for a free one.

- H700 GBA games without a Continue moment keep using mGBA when their newest save slot belongs to mGBA, so existing slots still load.

- Ordinary games no longer launch periodic PipeWire diagnostic helpers; measurements start them only when requested.

## [0.2.3] - 2026-11-06

### Fixed

- On H700 with the affected ROCKNIX audio configuration, system volume also controls the speaker DAC,
  with a reduced maximum and physical mute at zero. Headphones keep their Line Out path; external mixer
  changes and final mute are preserved. Gain changes ramp without per-step processes; volume overlays and hidden
  shelf redraws are suppressed during H700 games. Battery retains normal game speed instead of halving the CPU
  limit. Hardware loudness and timing validation remains pending.

## [0.2.2] - 2026-11-06

### Fixed

- Game audio uses prepared high-rate filters with ARM64 NEON. When a frame is late, queued samples stay audible
  and only the missing tail is filled with silence. The first stream opens with the configured cycle buffer.
- Anbernic H700 audio uses a 1024-frame graph cycle, a 48 ms software queue and a 64 ms PCM buffer for more
  scheduling margin. Audio diagnostics distinguish queue waits, output errors and unavailable measurements.

## [0.2.1] - 2026-11-06

### Fixed

- Game Boy Advance on Anbernic H700 handhelds uses gpSP in pocket-ui's own player, keeping the pause menu, picture slots and Continue. Existing mGBA Continue moments and saved emulator choices stay with their core. Games use full CPU speed under Auto and restore the previous profile on exit.

## [0.2.0] - 2026-11-05

### Added

- **Zoom** (pocket-ui's own host): the frozen picture of the game 2x larger, 3x with A, moved with the D-pad, B goes back. It uses the picture the menu already shows behind it, so the game does no extra work, and it fits every screen size. It is a row of Pause → Options (next to Picture and Controls) and stays off the pause menu's first page unless Easy to read is on or Settings → Sound & feel → **Zoom in the pause menu** is. Options → Shortcuts → **Zoom** adds HOME + X, off by default: the picture larger at once, B back into the game.
- **Hold to toggle** on Options → Controls: pick a console button (run, for example) and one press holds it down for the game, the next lets go. Saved per console or per game like the other Controls lines, for every player; nothing stays held across a pause.
- **Easy to read**, the last card in the Tour (after its six basics): Large text and Reduced motion in one switch (the same two settings as Text size and Motion, which stay in their rows).
- **Where was I?** Back to a game after more than two weeks, a quiet card on the picture of your moment says when you played it last ("Last played 3 weeks ago"), how long you have played it, and your note from the game card if you wrote one, while the moment loads. It never asks anything. On pocket-ui's own host the game waits at your moment for about four seconds, silent and still, so you can read it and miss nothing; any button plays at once. Easy shows it plainly, Kids Mode not at all. Settings → Playing → **Remind me where I was** switches it off.
- **The controls card**: a drawing of the Pocket's buttons with what each one is on the console (SNES Y X B A, GBA A B L R, PlayStation Cross Circle Square Triangle with L1 L2 R1 R2, the Nintendo 64's A, B, Z and C buttons on the right stick), your own changes from Options → Controls marked, and the HOME keys (menu, save, load) under it. It comes once at the first start of a console (any button goes on to the game), as the **Controls** row on a game's card, and as the first line of Options → Controls in the pause menu. The names are the emulators' own (read from their button lists) or, for a console that ran before, from its first game; German and Spanish follow.

### Fixed

- PSP: system dialogs like saving now show on a fresh card. pocket-core copies PPSSPP's missing atlas, VFPU tables and fonts from ROCKNIX into `roms/bios/PPSSPP` before a PSP game starts, and never replaces a file you already have there. The replaceable files copy without waiting for an SD flush, avoiding a long first-start pause; later starts repair missing or short seeded files.
- Options: Restore defaults and Keep for this console wait for pending changes before deleting overrides, so a slow card cannot bring back the old game settings.
- Options → Controls describes the controls card in fewer words.
- Polish before 0.2: the pause menu's game name is a plain title, no longer a band that looked like a second selection; the save warning under it is one short line, so the rows keep their spacing at 1024x768; the controls card's shoulder buttons are small labelled pills instead of wide empty-looking fields, and the D-pad and left stick labels share one size; the Easy look names a game's genre in German and Spanish ("Action" instead of "Action game"); "Surprise me" fits on its case at 1024x768; the "Added to Favourites" toast on Home sits at the bottom instead of over the game's name; the Picture page's "Whole screen" caption is never cut; the empty Tools page says it plainly, without a folder path.
- Controls card: continuing an existing game starts immediately; the once-per-console mark survives interrupted writes, and the first-start card always continues after twelve seconds or a button press.
- The return reminder skips on an early press, releases a failed picture or resume immediately, and never holds a fresh start because an old preview remains. Pause, sleep and screen off end its launch hold; ordinary launches keep their original safety limit.
- Playing's category summary includes the reminder and correctly omits performance on Steam Deck.
- The Tour's cards show the way to a setting whole: on 1024x768 with Large text the German "Easy to read" card cut its two-setting path after three lines and ran into the hint bar. The type of the path now steps down (28, 24, 21 px) and, last, the choices get lower, before anything is cut; the Spanish description of that card fits its two lines.
- SD card images give their handheld a name of its own on the network (`pocket-mini-v2`, `pocket-rg35xx-plus`, `pocket-rg35xx-sp`, `pocket-rg28xx`, `pocket-odin2-mini`, ...) instead of one shared name, so two handhelds on one network or Tailscale can be told apart.
- Cabinet on 1024x768 and 1280x800 screens: the chosen case is shown at a proper size again, with the TV beside it. A tall shelf (SNES, PlayStation 2) had left a 40-line chip and no TV; now the shelf is drawn smaller on these two sizes and the title sits a little higher, so the case, the TV and the whole spine titles all fit.
- **PSP: Quit after Load no longer hangs.** PPSSPP could wait for a frame while pocket-core waited for it to pause, so the exit save never finished (Wild Arms XF, Crisis Core). pocket-core now watches the exit save: if the core has not answered after 20 seconds, the game closes using the newest moment already on the card, and a newer in-game save is still protected when you continue. Those two games also start with GPU read-backs skipped, which avoids the wait; a read-backs choice you made yourself is kept.

### Changed

- The Xbox core (hakuX) now comes from the [hakuX-Linux](https://github.com/robbyczgw-cla/hakuX-Linux) fork, branch `linux-1.3.0` (commit `035ec48`): hakuX 1.3.0 with the Linux/libretro port by portare-ch and our fixes. Source, build recipe and checksums: [docs/XBOX.md](docs/XBOX.md), `hakux/SOURCE.txt`.
- **The hint bar never shows a key without its word**: at most four hints (Easy three), and where they don't fit whole hints go, the least needed first (A, B, HOME and START last). Home shows HOME for the quick settings; Easy shows A, HOME and START; the pause menu keeps START (the way to the quick settings) and drops SELECT first on a narrow screen.
- **One rule for messages**: a short confirmation is a toast that stays 2 s at the least and longer with its length; what went wrong ("Could not save: is the SD card full?", a game that did not start) is a card that stays until A, B or a tap closes it ("Press A to close."), over every page of the shelf.
- **Pause menu → Options has no Save changes page any more.** A change applies to the game and is kept at once, as everywhere else. **Keep for** (this game, this console) says where; **Restore defaults** asks for a second A. Files you saved before keep working. Runs on keeps its own Save changes.
- **Options → Picture has a Look row** (second, after Screen scaling) instead of Screen sharpness and Screen effect: **Clean · Smooth · Soft · Bloom · LCD grid · Custom** on handhelds and **Clean · Smooth · Soft · Bloom · Scanlines · Old TV · Custom** on TV consoles, arcade and computers. Each Look sets sharpness and effect together; **Custom** opens **Sharpness** and **Effect** below it, with every effect. The effect **CRT** is now called **Old TV**, **Soft glow** **Bloom** and **Smooth pixels** **Smooth**. The line at the bottom explains the Look you chose, not the whole row. The same rows are on a game's card (This game).
- Nothing you saved changes: the Look is worked out from the saved sharpness and effect when the page opens (Soft with an effect, or a grid on a PlayStation, shows as Custom with its rows open), no new setting is written, no picture changes, and Restore defaults and every console's start are as before. Choosing the LCD grid Look on the Game Boy Advance also turns LCD colours on.
- Screen frames are unchanged. The manual's Picture chapter and the German and Spanish texts follow; Pixel Path names the effects the same way.
- Game sound is converted to 48 kHz with a windowed-sinc filter instead of linear interpolation: the mirror images of high notes that the old conversion folded back into the audible range are gone (strongest foreign line under -90 dB, was -5 to -60 dB at 32 kHz cores), and the highs stay level up to 0.45 of the core's rate. About 1 ms more delay, no audible CPU cost. `POCKET_CORE_RESAMPLER=linear` brings the old one back.
- First start: "Your N games are on the shelf." counts only games you put on the card, not the entries a handheld brings itself (the Steam list of the Odin 2 Mini).
- The starter pack, search and the Settings page say "already here", "your games folder" and "this handheld" instead of "your SD card", which was wrong on a handheld with internal storage.
- The pause menu says why a row is grey: "No save in slot 1 yet" on Load, "keeps a moment every 5 min of play" on Earlier. The battery line says "Full" on the charger with a full battery, not "Charging".
- The Tour (and Choose for yourself) starts with Continue where I quit, Continue your last game, Motion and the menu sounds; the Curator's cards come last.
- The first page of the manual no longer names a single device.
- On Home, one line says what the Curator and Just Play are the first time the highlight reaches them.
- One selection in every list: Settings, quick settings, Options and the other pages mark the selected row with one sheet of paper across the row (it was a paper pill running into a grey band). Over a game the system's colour is a short stripe at its start.
- The arrows in lists are drawn, not the letters ">" and "‹ ›". An entry that opens a page shows its arrow on every row (quieter when not selected), so pages and switches are told apart at a glance; the hint says **A Open** or **A Change** (was "Switch").
- Shelf look: the selected case rises higher out of the row and a strip of light marks where it stood, so it stands out among cases of one colour and on 640x480. The grey Super Nintendo boxes have their band in the system's lilac instead of dark grey and no longer look switched off.
- Page titles over a game (Options and its pages, Players, Pixel Path, "Couldn't save. Quit anyway?") and the game's name in the pause menu are in the same large title type as Settings. The pause menu's items start under its title instead of floating in the middle of the screen.
- Covers are found for 39 more systems (playlist names for the box-art look-up) and the start of a name matches too.
- Faster and calmer in the background: the library is scanned once and its folder types are reused, battery history is written in batches and flushed when the handheld sleeps or quits, and the picture you just asked for is drawn before older requests.
- The pause menu no longer waits when the screen size is only provisional at start on small panels.
- 1024x768 in Normal text: safe mode's choices, a Pixel Path stage's paragraph, the game card's columns and bare hint keys fit; the manual's tables break words that cannot stay whole, so "coming from" no longer runs past the edge.

## [0.1.14] - 2026-11-03

### Added

- Box art for many more systems (3DS, Wii, Wii U, PS3, Neo Geo, Amiga, C64, MSX, 3DO, Jaguar, Atari 5200/7800/800/ST, ColecoVision, Intellivision, Vectrex, ZX Spectrum, DOS, ScummVM and more, plus the Microsoft disc system), and a name that is the start of one game's title ("Top Spin" for "Top Spin Tennis") finds its box. Files ending in `.xiso.iso` match by their game name.

- Settings → Look → Screen colour: the screen's white from Cooler 10 to Warmer 10 as a row of its own, the same setting as SELECT + Vol. Left/Right change it at once, A goes back to Neutral.

- Two more **Screen effects** (Options → Picture): **Soft glow** puts a soft halo around bright parts and leaves the rest clear (no lines, no mask), and **Smooth pixels** rounds one-pixel stair-steps on slanted edges while flat pixels and pixel art stay sharp. Both are one pass in the existing picture shader, even at every size.

- Game facts (year, developer, genre, players, and the Wikipedia text once online) for 15 more systems: Neo Geo, Neo Geo CD, Amiga, C64, ZX Spectrum, Atari Jaguar, Atari 5200, Atari 800, 3DO, ColecoVision, Intellivision, Vectrex, PS3, PS Vita and Wii U.

- Console pages (X on a system, with Credits & links) for the Amiga, C64, ZX Spectrum, Atari 5200, Atari 8-bit family, PS3, PS Vita and Wii U, and short pages for PICO-8, TIC-80, ScummVM and DOS that say what they are: not consoles but programs. Each fact names its source (Wikipedia, Wikidata).

### Changed

- The game's full card says when a moment is waiting where you quit ("Continue: 2 h ago", under Save slots, or as its own row for games without slots), by the same checks as the open case on the shelf. Nothing is shown without one.
- Screen frames (Options → Picture → Screen frame) carry the names of the consoles they draw: Brick is **DMG**, Pocket slim **GB Pocket**, Shell **GBC**, Wide **GBA**, Clamshell **GBA SP**, Twin **DS**, Gloss **PSP**; their colour rows are renamed to match ("DMG colour", …). A frame shaped like one console is offered for that console's games only. Swan, Watch and Goggles are gone from the list (Auto still draws them for WonderSwan, Game & Watch and Virtual Boy; a game that had one chosen now uses Auto). Saved settings stay valid.
- Echoes (More like this), the Lens and Up next now also know the feel of about 840 games on one more system, which had no tags before.
- A game card without a description now says why: "No Wikipedia article for this game" (nothing to wait for), or "Offline: the description comes with Wi-Fi" (it comes with the next connection). English, German and Spanish; before, the card said nothing or an English error word.

### Fixed

- 1024x768 (TrimUI Brick) in Normal text: safe mode's five choices end above the hint bar, a Pixel Path stage's paragraph fits between the clock and the hints, the Pixel Path values and the first-start look notes use a slightly smaller type before they end in "…", the game card's "Your game" column has room for its rating and play time, "How do you like to play?" no longer runs its last row under the hints, and the pause menu's and first start's hint bars no longer end in a key without a word. A test keeps this size free of overlaps and cut text.
- Cabinet: on a short shelf the empty strip under the shelf is used. The case and the TV beside it are shown bigger, and where room is still left the top of the next shelf looks in from below.
- Game info finds more of your games: "Mega Man X 2" and "Mega Man X 3" are no longer read as "Mega Man 10 2", and a Game Boy Color game filed in the Game Boy folder is found by its exact name first ("Link's Awakening DX" no longer shows the plain Link's Awakening).
- Dreamcast comes back to its moment and loads from Save slots again. Flycast's state is smaller at Quit (36 MB) than the size it reports at the next start (53 MB), so the file was refused as "the wrong size" and the game started fresh. Such a state now goes to the core padded to the full size. A save that fails because the state grew meanwhile is tried once more.

## [0.1.13] - 2026-11-03

### Added

- The Easy look (Settings → Look → Look): only your games, one at a time, for someone with a handful of them. Left and Right choose, A plays or resumes, X opens the game's card, HOME opens the quick settings, and in a game gives Continue, Save & quit and More. The first start offers it as its own row. It is calm: the case, name and buttons are smaller than in Kids Mode, More is a row like Continue and Quit, nothing is drawn over the game (no FPS or CPU numbers, whatever Over the game says), and there is no game switcher. An open case says once what open means.
- **Just Play** (prototype): one row on Home, right above Continue, and A starts a game picked for you: one you started and
  haven't finished first, then a starred one you never played, then any unplayed one; the game you played last is left
  out, hidden games never come up, and games on systems this handheld runs slowly come up less. No card, no question. It
  goes with Settings → Games → Curator → Lens & smart shelves and is not in Kids Mode.

### Changed

- Just Play's case on the Cabinet and the Shelf shows a picture: a game cartridge with a play triangle and two dice, in the pixel-art style of the other cases, instead of a plain grey case with its name.
- Console pages on a 1024 px screen: the Launch price and Hardware columns had 250 px, and "US$99.99" ran one character a line down the page. The second column now starts where the console's picture does, a label column gives way to a long value, and a value that still doesn't fit goes under its label. A short word torn across lines is now also reported by the text-size sweep (`W`).
- The game that stands out of an open case reads as meant: it comes out further beside the front (about half its width, and the name starts clear of it), and the strip above a spine is a little taller. The first time an open case is on screen one line says what it means, "Open means your moment is waiting" (English, German, Spanish; once, never in Kids Mode).
- Up next after a game (off by default, on with Enthusiast): **X** plays the suggestion, no longer A. A stays with the game the shelf is on, so a press of A right after a game continues that game and no longer starts another one.
- Help in German and Spanish says it is English: the Manual row reads "Handbuch (Englisch)" and "Manual (en inglés)"; the way back from the manual is named Help & tools (the manual said System).
- The first start's profile **Simple** is **The shelf** ("Your games on a shelf, as it comes."): in the first start, in Settings → Help & tools → How you like to play and in the manual, en/de/es. From Help & tools the page now says honestly what it does later: it puts every extra back as pocket-ui ships it, also On start and Screen frames (it is not the same as skipping then).
- Pause menu: the **Quick settings** row is gone (START opens them from any row, as its hint says); Options pages have a title ("Options", "Options › Picture" …) so you know where you are after two levels; Settings → Look no longer shows a key without its word at the end of its hint bar (R1 and L1 still work there; their rows are in Sound & feel).
- "Stickers" meant two things. Your own tags for games (Couch co-op) are now **Labels** in Curator, the Lens, the tour, the Settings row, the phone page and the manual; **Stickers** are the ones on the case. The card's row that holds both reads **Stickers & labels**. Saved labels and smart shelves are unchanged.
- "Extras" meant four things: the first start's page is **Pictures & extras** like its Settings row, its first row **Box art** (it said Cover art, and Box art under it), the pictures note no longer names the libretro collection or counts KB, and the pause menu's Options group is **Speed, rumble, fan** (Extras). English, German, Spanish and the manual.
- The list of recent games has one name, **Recent games**: the shelf's SELECT hint, the screen's title (it said Recently played, and Switch game from a game), the pause menu's SELECT hint (it said Games) and the manual (it said game switcher).
- HOME in Settings goes back to Home (it did nothing). Pages with something under way (an update, a download, a sign-in, a pairing, a clean-up, a button being remapped) keep HOME to themselves. On Home and the shelf HOME still opens the quick settings, in a game the pause menu.
- Pause menu: the Quit row says **Save & quit** while it keeps the moment, with "Next time, A continues from this moment." beside it. With Continue where I quit off (or nothing to save) it stays **Quit** with its own line.
- Game switcher: A says **Continue** only where a moment is waiting (the same check as the shelf's open cases) and **Play** otherwise, instead of promising Resume for every game. X **Remove** is gone: it took the game off the list and with it the play time it had counted.
- One word for going back to where you left off: **Continue**. A on a game with a waiting moment says Continue (it said Resume), the setting is **Continue where I quit** (Remember where I quit), Load's first stop is **Where I quit** (Auto), and the tour card is "Continue your last game". English, German and Spanish; the manual says the same.
- Frozen games is gone as a switch and as a word: Settings → System has no Frozen games row, the word on the cases is **Continue**, and the manual page is now **Continue**. An open case always means a moment is waiting. An old setting that switched it off is ignored (treated as on); the save rules did not change. Android has no row either.
- Pause menu → Load has a last stop, **From the start**, on pocket-ui's own host: the game begins again and nothing else changes; its own saves and the moment on the card stay until the next Save & quit replaces it.
- Remember where I quit off: the older moment is no longer the next start (the game starts as it would without one); it stays under Load as Auto until a Quit saves a new one. Applies to pocket-ui's own host; PlayStation 2, GameCube, Wii and 3DS are unchanged.

### Fixed

- Cabinet shows the selected shelf’s position and up/down arrows, so systems outside the visible shelves are easier to find.
- Box art: a picture server that did not answer (offline, a timeout, an error page, a broken file) no longer marks games as "has no cover" for good; they are asked for again at the next start. A run gives up after three failed requests in a row instead of waiting a minute per game, and Settings → Extras → Cover art says "Couldn't reach the picture server. Try again later."
- On a 1024x768 screen the first start's language line no longer ends in "Settin…": the sentence goes under the row, whole.

## [0.1.12] - 2026-11-02

### Added

- Commodore 64, Amiga, ZX Spectrum, Atari 800 and 5200 have their own picture on the shelf card
  and front, and the game goes into the right place when it starts, instead of the crate of games.
- SD card image: Settings → Help & tools → About names the system, "pocket-ui OS 0.1.9 · built on ROCKNIX 20261001
  (SM8250)" (on a ROCKNIX card with pocket-ui added, the ROCKNIX version). The device is called "pocket-ui" on the
  network, in Bluetooth and Tailscale, and EmulationStation's ROCKNIX update check is off.

### Changed

- USB file transfer (MTP) is no longer offered: no tile on Add games, no row in Settings → Connections, and the manual's "With a USB cable" part is gone. Nobody had completed a real transfer with it, and a Mac depends on OpenMTP, which was never shown to see the device. One that was switched on before this version is switched off at the start. The code stays, unused. Add games still offers File sharing and the phone page, plus the second card and the GAMES drive where they exist.
- RG28XX card: the first start's games step says the GAMES drive appears after this first start (switch off, card in a computer), and the README and docs/user/games-drive.md say that a computer shows no GAMES drive right after flashing, so there is no reason to flash again.
- SD card images: the big ROCKNIX source bundles are no longer uploaded next to the image (ROCKNIX, Knulli and pocknix publish none either). The card's SOURCE-OFFER.txt points to ROCKNIX's repository at the exact tag and commit and to the pocket-ui source (`pocket-ui-<version>-source.tar.gz`), and offers the complete source of the GPL and LGPL software on the card for at least three years from the image's release, on request through GitHub issues, for no more than the cost of providing it.
- Home with no games has a second button, **Add games** (X; also with no network), next to the starter pack, and B (**Look around**) puts the hint away:
  the shelf in your look stays empty with "No games yet", A brings the hint back. The hint returns at the next start while the card is empty and
  is gone once games appear.
- First start: once a Wi-Fi connection works, the setup shows "Connected to …" for a moment and goes on to the time zone by
  itself (one press fewer). A failed connection stays on the page; B from the time zone comes back and stays; Settings is unchanged.
- Mini V2 card README and INSTALL-DUAL-BOOT.md: the steps are for a Mini V2 with Retroid's own bootloader (firmware RPMiniV2_V1.0.0.275_20260407); with ROCKNIX's bootloader the card is untried, install the package instead.
- Home with no games, offline: the one button reads "Connect to Wi-Fi" and A opens the Wi-Fi page (one press instead of twelve to find it);
  the starter pack waits on Home when B comes back.
- Kids Mode's "No games to pick yet" page offers the starter pack with X, and comes back to the picker with the games.
- Settings → Help & tools → How you like to play: the first start's profile question again in one page, instead of 27 Tour cards.
- First start on a handheld with no Wi-Fi chip (RG28XX, RG35XX 2024): the Wi-Fi and Extras pages, which had nothing to choose, are
  left out. Five steps instead of seven; B and a restart in the middle follow the shorter way.
- SD card image: ROCKNIX updates are no longer installed on the card. At every start the boot hook moves any system
  update pocket-ui did not check (from EmulationStation's menu, the network share or copied by hand) to
  `/storage/.update-refused` and starts as usual; nothing there is deleted ([docs/SD-IMAGE.md](docs/SD-IMAGE.md)).
- On-device Manual: a link to another page of the guide opens that page (A on the framed link, X for the next link, or a tap; B goes back to where it came from). Links to a web address already showed the address.

### Fixed

- Manual (pause menu): the one-time note that HOME opens the menu is said only for the first game on pocket-ui's own host, not in RetroArch, standalone emulators or Android; the page now says so.
- Game check: when a folder had more than 50,000 entries and the check stopped at the first 50,000, it no longer says "All good"; it says "First 50,000 files checked".
- First start, Extras: at 1280x800 with Large text the note under the rows no longer touches the hint bar (the rows stand a little closer). Large text at 1024x768 is still tight there.
- Manual: a link of several words is one stop for X (it moved word by word before).
- First start, "For a child" on a handheld without Wi-Fi: after Kids Mode's setup it goes on to the time zone like the other play styles (no Wi-Fi page, steps numbered 4 and 5 of 5).
- Wi-Fi password: after a refused password the keyboard keeps what was typed (B fixes a letter); L1 now empties the field in one press, so
  leaving for the network list is L1, B instead of one B per letter.
- Android: a GBA or NES game comes back to its moment after Quit. The state it wrote was refused as the wrong size ("430144 of 528448 bytes") and the next start began fresh; the player now reads mGBA's and Nestopia's shorter states, as the handhelds do.
- Starting a game on a C64, Amiga, ZX Spectrum, Atari 800, MSX, Atari ST, Amstrad CPC or an arcade system (MAME, FinalBurn Neo, CPS, NAOMI) no
  longer shows a disc going in (those games come on floppies, tape or boards, which have no picture here): the case fades and the machine lights up.
  Amiga OCS/ECS and AGA use the Amiga's picture.
- 1024x768 screens: the Cards look keeps its game name and facts above the hints (the cards are lower there), and the on-screen keyboard's rows and its last row fit the screen instead of running over the hints and off the right edge.
- Settings → System → Report a problem only opens About with the row selected; the file is written when A is pressed there (before, merely landing on the row wrote a file into the games share).
- Android (0.1.11-android.1): the shared fixes of 0.1.10 and 0.1.11 come along (inserts on the console's middle, Light's case shadow, wrapped sentences, German and Spanish wording, patch-aware Frozen marks); a test pins that the phone's marks and older ones still stand open.

- Quick launch animation: a handheld's cartridge sinks behind the console's own outline, so on a tilted picture its corners no
  longer overlap the top edge; the slot bar is the console's own surface darkened, not black (it showed as a black stripe on a PlayStation's lid).
- The first game on a card says how to get out: "HOME opens the menu: save, load, quit." Nothing did before.
- First start: the last page ("You're all set") says "In a game, HOME opens the menu: save, load, quit." too, in English, German and Spanish (not in Kids Mode); a first game in RetroArch or another emulator has no line over it.
- For a child, with no games on the card: the first start ends on the page that adds games, not on the child's "ask a
  grown-up", and the Ready page no longer says it can be handed over.
- A handheld the project has played on (Odin 2 Mini, RG35XX Plus and SP) is no longer called untested at the first start.
- A device without Wi-Fi: the first start no longer promises Wi-Fi or a clock that sets itself.
- Extras and "You're all set" count the games on the card: "no games yet" stood there with six games on it.
- Add games with more than two wrong folders says how many and where to look, not a line of names cut off before the advice.
- A game's name on two lines stays its name: "Sonic Adventure" was "Sonic Aventura" on its case in Spanish.
- "1 Spiele", "Alle 1 Spiele holen" and their English and Spanish twins.
- Add games and the phone page say a computer's browser works too.
- A refused Wi-Fi password stays in the field, so one wrong letter doesn't mean typing it all again.
- Large text: Pixel Path shows each stage's grade under its name, so the values have the whole line, and the last seconds of play lose whole figures from the end instead of half of one; the Look step's lines about Animations, Motion and Appearance fit in German and Spanish; the selected row on Sync keeps its pill at 480 lines.
- A copied game the shelf leaves out because its file type belongs to another system, or because it lies more than one folder deep, now counts like a disc missing a file: the "can't be read" note after a rescan and Home's "No games yet" mention it, and point to Game check.
- SD card images: a build without a model (Mini V2) no longer gets GAMES drive files.
- SD card image: a card over about 36 GB (the 64 GB card an RG35XX SP comes with, for one) no longer ends in "Filesystem
  corruption has been detected" and a black screen after the first start's resize. Cards flashed with an earlier image
  have to be flashed again.
- Android: marking a game as a favourite keeps it selected, so Play starts that game.
- Android: the Games page with several shelves opened scrolled down to the shelf of the alphabetically first title after a fresh start. It now opens at the top (Recent games first); a remembered game still opens on its own shelf.
- On Android, a moment set aside while all eight slots are full is now under Load, after the last slot.
- On Android, Home during a slow game copy stops the start: no game comes up in the background later.
- On Android, Play works right after Game check found a disc's missing track copied in.
- On Android, back from Cozy extras lands on its row in Settings, no longer on the first.
- Tour and first-start cards on the Steam Deck: none for Haptics, Sync, Tailscale, Tools or Performance & fan.
- Steam Deck: no idle timers, suspend, power-key hold or low-battery power-off of ours any more; SteamOS owns sleep and the
  power button (the Settings rows were hidden, but the timers still ran and would have suspended after 4 minutes).
- 1024x768 screens: About keeps its two rows and their help above the hints, the first start's look step fits its rows above the hints (smaller preview), and Settings → Curator's rows leave room for their explanation.
- 1024x768 screens, more pages: Extras, "How do you like to play?", the feature cards, Search (reason over the keys), Tailscale, Web companion, RetroAchievements, Time, Battery, per-port Performance & fan, Pair a PC and History keep their lines above the hints and off each other; the pause menu's slot name no longer runs under the preview.

## [0.1.11] - 2026-11-02

### Added

- A second card that still holds a flashed system image (its small STORAGE partition would be the whole shelf) is said plainly on
  Add games and on Home's "No games yet": take it out, or format it for games. Nothing is reformatted.
- PlayStation 3 and Wii U have their own console picture on their shelf card and front (public-domain photos by Evan Amos, pixel-art
  conversion, lettering painted over), instead of the bare title card.
- A copy the shelf can't read no longer vanishes without a word: when a rescan finds unread game files (a disc
  without its file, a folder no system reads) and no new game, a note points to Game check, and an empty Home's "No
  games yet" says the files are there.
- Kids Mode: the PIN page names where the recovery file goes from the first look; the screens that stop the child say
  "Grown-ups: hold SELECT and START."; the grown-ups' page says when picked games are no longer on the card.
- The Manual shows the address after an outside link, to type on a phone. The phone upload page shows refusals in
  German and Spanish. After a skipped time zone with nothing to go on, the last first-start page says the clock is on UTC.
- The page you open on your phone for "From your phone" speaks Spanish too (after German and English), by the phone's
  language like the device.
- SD card image: Settings → Help & tools → About names the system, "pocket-ui OS 0.1.9 · built on ROCKNIX 20261001
  (SM8250)" (on a ROCKNIX card with pocket-ui added, the ROCKNIX version). The device is called "pocket-ui" on the
  network, in Bluetooth and Tailscale, and EmulationStation's ROCKNIX update check is off.

### Changed

- Updates: with no update list published yet, the page says "No updates published yet. A new version installs like
  the first one; your games and settings stay." Sleep says what it costs ("a few percent of the battery an hour").
- Words: second card ("only its games show while it's in"), empty Home's ways to add games, the remote access
  sentence, the manual's H700 and "START skips a step" lines, getting started leads with the installs that exist.
- Release scripts: the Ports scripts are uploaded and listed in `SHA256SUMS` under the names GitHub gives them
  (`Install.pocket-ui.sh`), so `sha256sum -c SHA256SUMS` passes for them too.

### Fixed

- Quit no longer kills a game still writing its saved moment. On a busy card the save could pass the 7 seconds after which the game was stopped, and the moment was lost; now nothing stops it while it writes, and a wait over a second shows it is working.
- Starting a game: the Game Boy Advance's cartridge goes into the middle of the console (it stood 35 pixels to the left on
  a 1240x1080 screen) and the DS's card comes straight down behind its upper screen. Every other system's plain animation, and
  Quick everywhere, aims at where the console's slot is on its picture, not at the middle of a picture that also holds the pad,
  at a size that fits the console.
- Starting a game in Light: no dark block stays where the case stood.
- The consoles at a game's start have clean outlines: no white fringe or grey patches left from their photos, and no white
  blade under the GameCube's open lid.
- Starting a game on a system without a console picture (arcade, home computers): its game goes into the crate of games, not
  into an empty grey box. The Saturn has a console picture everywhere now, in Quick too.
- Uploading games from your phone no longer moves the selection to another game in the shelf: the same game stays
  selected, as when the list is read again.
- Settings: the Kids mode row shows ">" when selected, like every row that opens a page.
- Tailscale, Sync, Bluetooth, the web companion and RetroAchievements: the line about the selected row wraps next to the
  hint pill instead of ending in "…" (Spanish "…en un navegad…").
- Explanations at the foot of a page no longer end on a single word ("ist."); a per-game Extras explanation takes the
  free lines above it before it is cut. German: "Ganzes Bild" over the small preview, "Behalten" in the first start's
  hint bar (the last key lost its label at 640x480), a shorter fan line so the note under it shows whole.
- Performance & fan on a 640x480 screen: the readout's long values (the CPU's clocks, the fan's line) have the room the
  short ones (temperature, power draw) don't need, instead of ending in "…". Starter pack: the explanation under the
  list stands at the left like every page's, no longer centred. German: the picture page's second line fits.
- First start / Settings → Systems on the shelf: a long line under the heading (German, Spanish, 640x480) takes a second
  line instead of ending in "…". Sentences in the Kids Mode pages, the game page, search and the report page no longer
  end on a single word.
- Look step (first start, Settings → Look): A "Keep this look" and B "Back" come right after Left/Right, so at Large text
  the keys that lose their label are the shortcuts (X, L1), no longer A.
- Performance & fan: the note about saved choices is shorter and shows whole on a 640x480 screen. First start, "How do you
  like to play?": the "Off:" line names two features and "and more" instead of running off the page.
- A fresh ROCKNIX card no longer starts the shelf with an empty "Splore" game that doesn't start: ROCKNIX's own PICO-8
  entry is hidden everywhere (shelf, counts, Recent, Add games, Kids Mode) until the official PICO-8 is installed, and a
  card holding only it says "No games yet".
- The install package no longer says pocket-ui "hasn't been tested" on an Odin 2 Mini, an RG35XX Plus or an RG35XX SP: it says the
  project has played on that model; the others still say they are untested.
- Two games with the same file name in different folders of one system share save states, as they always did, but no longer
  silently: a game only continues on its own from a moment it wrote last, the start says when they share and that saving can
  overwrite the other's, and deleting one leaves the shared states and pictures on the card. Rename one to keep them apart.
- Back up saves now includes ScummVM's saves, so a backup before flashing keeps your adventures.
- Adding, replacing or removing a soft patch no longer resumes the moment from the game's previous version: the game
  starts fresh and the moment stays under Load until your next Quit. This also holds with Frozen games switched off.
- Kids Mode: the grown-ups' page no longer offers "30 more minutes today" and "Start today's time again" while the time
  per day is "No limit".

## [0.1.10] - 2026-11-01

### Added

- Settings → System → Backup & reset backs up your saves and states: battery saves, save states, PS2 and GameCube memory
  cards, Wii and 3DS save data, no games and no BIOS files. It shows the size and the free space first and keeps the page
  moving while it copies. A restore never overwrites what is newer on the card. Flashing a card erases the folder, so copy it
  to a computer first; a second card keeps it ([docs/user/settings-system.md](docs/user/settings-system.md#backup--reset)).
- **Kids mode** (Settings → System, or "For a child" at the first start): a big, simple shelf with the games you pick, behind your PIN. Blue or Pink, a
  time per day with a warning five minutes before, a loudest volume. PS2, GameCube, Wii, 3DS and Xbox games too, on handhelds that play them; the emulators' and
  ROCKNIX's own shortcuts are closed meanwhile ([docs/user/kids-mode.md](docs/user/kids-mode.md)).
- Add games: a system the handheld runs but not every game of it smoothly (heavy ones such as PS2 and GameCube on a Snapdragon 865, Nintendo 64, PSP, Dreamcast and DS on an H700) says so in one plain sentence under its Files and BIOS lines.
- Add games: **From your phone** (a tile in the row of ways, also Y, also on the first start). Scan the code with your
  phone's camera, pick game files on the page it opens, and they land in the right system's folder; the shelf has them
  when you leave. Disc images and anything several systems take (`.iso`, `.chd`, `.cue`, `.zip`) ask which system. The
  Pocket stays awake while the page is open; it closes by itself after 10 minutes idle or when the screen goes dark
  ([docs/user/phone-upload.md](docs/user/phone-upload.md)).

### Fixed

- Kids mode: "Pick the games" shows the cover of the game under the cursor (it stayed an empty box on a device), and the case on the child's shelf fills as soon as its cover is read.
- Kids mode: the child's shelf reads the covers that arrive after it is shown (a case could stay blank), and stops repainting once they are in.
- Kids mode: a game that can't start stops on a picture asking for a grown-up. It no longer falls back to RetroArch, whose menu leaves the mode.
- Kids mode: a game paused by the screen going dark no longer uses up the day's time.
- Kids mode: a keyboard's Escape with the power menu, and a keyboard on the crash-loop door, no longer get past the PIN.
- A restore keeps a save that arrived or got newer after the comparison, also during the copy; it counts as kept.
- From your phone: a disc's tracks (`.bin`, `.raw`, `.img`, `.wav`) can be sent for Dreamcast, Saturn, Sega CD, PC Engine CD and the others, and go where the `.cue` or `.gdi` in their own folder goes.
- From your phone: choosing a system for one file no longer sends other files there; a button offers the same system for the rest, if you want it.
- The first saves backup after an update no longer goes to your other devices: the sync's ignore list is brought up to date first.
- Kids mode: a game hosted by pocket-core fills the screen as large as fits in its shape (a Game Boy Advance game on a 640x480 handheld is 640x427, no longer 480x320), whatever picture settings the grown-up saved; they come back when Kids mode is off.
- A large saves backup or restore keeps the handheld awake until it is done, instead of going to sleep halfway.

## [0.1.9] - 2026-11-01

### Added

- File sharing: nothing under `roms` is hidden any more. BIOS files, save states, `ports` and `PortMaster` can be copied over Wi-Fi file sharing (a BIOS file no longer needs a computer that writes ext4). Anyone with the share password can then also change them, and put programs into `ports` that run as root when started. Game check says where to copy a missing BIOS on your model.
- File sharing stays on across restarts until you switch it off, and comes back by itself once the Pocket is on Wi-Fi.
- Add games (Settings → Games → Add games, and the first start's last step): a row of tiles under the grid for the ways your
  handheld can take games, **File sharing** and **USB file transfer**. Reach it with Down from the grid, choose with Left
  and Right, A switches it, or tap it. USB file transfer is the same switch as Settings → Connections and both show
  the same state. Only the ways your handheld has appear; X still switches file sharing.
- Spanish: pocket-ui now speaks Español, in neutral Spanish for Spain and Latin America. Choose it on the Welcome page or
  in Settings → System → Language; a system set to any Spanish locale starts in it. The manual's pages stay in English.
- A second card on Anbernic H700 handhelds: put a card with a `roms` folder in the second slot and the shelf shows its games within a few seconds, also while the handheld is on. While it's in, only its games are on the shelf; its saves and states stay on that card. Add games says when the games come from it, and the shelf says when it's been pulled.
- USB file transfer: Settings → Connections → USB file transfer lets a computer copy games over a USB cable (MTP, ROCKNIX's own). Off by default, it stays on across restarts until you turn it off. Switched on and off on a Mini V2 and an Odin 2 Mini; copying to a computer has not been tried yet, and a Mac needs the free app OpenMTP, also untried.

### Changed

- PortMaster games that need your own files: the `port-files` folder is gone. Copy the files straight into the game's folder, `roms/ports/<game>/<folder>`, over file sharing or the web companion; "Copy your game files" shows the address. Files left in `port-files` stay on the card and are no longer moved.
- Remote access (SSH) no longer turns itself off after 30 idle minutes: it stays on until you switch it off, and is off after a restart.
- Enthusiast leaves the line over the game off: it's a choice of its own (Settings → Playing → Over the game), not part of the profile.

### Fixed

- File sharing writes as root: turning it on is instant and no longer interrupts a running copy, saves and port folders stay writable over the share, and a root-mounted exFAT or NTFS games card can be written. Add games says so when the card is read-only, and follows the real state while it is open. Safe mode's reset switches file sharing off; its password stays.
- Game check reports a game that is zeros all through (a copy that stopped early) as an empty file, no longer as ready; disc images are left alone.
- A game pocket-core couldn't start and that went to RetroArch goes back to pocket-core when you copy a new file over it (or delete it); a choice made under Runs on stays.
- X and Y are the right way round on Anbernic H700 handhelds. If you had swapped X and Y yourself on an H700, set them back.
- Game check no longer tells you to rename a save like `Game.nds.sav` to `Game.nds`: saves are left alone.
- Scaling preview on small screens: on 640x480 the Picture page's preview keeps at least a quarter of the height.

## [0.1.8] - 2026-10-31

### Added

- **Game check:** Settings → Games → Game check answers "will my games start?" after you copied games. It says in plain
  words what is wrong and what to do: a missing, misplaced or wrong BIOS (by checksum), a disc without its track file,
  empty files, zips without a game, the same game twice (A hides the copy), a game in the wrong folder or with the wrong
  file type, a system this handheld can't play. "All good: 214 games ready" when there's nothing. Adding games offers it.
  Works offline; never deletes a game, downloads anything or touches saves.
- **Frozen games:** your last five games with a saved moment stand a little open on the shelf, and A goes straight back to
  that moment, with no question and no waiting. It uses the moment Quit and sleep already keep: nothing keeps running,
  and nothing extra goes on the card. Games that can't go back stand closed and say why when started. On at the start;
  Settings → System → Frozen games switches it off.
- **Consoles that really take their game (Cozy, Playful cases):** the PlayStation's, GameCube's, Dreamcast's and Saturn's lid
  opens, the disc comes down onto the spindle and the lid shuts; the PS2's and Xbox's tray slides out and takes the disc in; a
  SNES, N64 or Mega Drive cartridge is pushed down behind the slot's lip and clicks; Game Boy, GBA and DS games go in at the back;
  the Wii pulls its disc in through the slot. Other systems keep their animation.

### Changed

- Cozy's start is slower, so you can see it: about 1.6 s until the game (it was about 1.1 s). Any button skips it at once, now
  also before the game's window is ready; Quick is unchanged, and Motion Reduced or Off start straight away.

### Fixed

- A game stuck in an error loop can't fill the memory with its log any more: a core writes at most 2 MB per game.

## [0.1.7] - 2026-10-31

### Added

- A **Cozy extras** card in the Tour (Settings → Help & tools → Tour) points to the page.

### Changed

- **Clear settings:** Settings has seven categories (Look, Sound & feel, Playing, Games, Connections, System, Help &
  tools), one line each with an icon, all seven on one screen at any text size; the line at the bottom names what is in
  the selected one. Every category is a short list of at most eight entries. What used to hide behind a shoulder button has an entry of its own: Appearance, Colours, Font, Motion,
  Systems on the shelf. The Theme page behind R2 is gone; Colours and Font are entries in Look and change right in the
  list. "Extras" is now **Pictures & extras**, and Tools and About moved to Help & tools. German names for all of it.
- **Quieter lists:** only the selected entry shows its marks (**>** on a page, **‹ ›** on a switch); the others show just
  their value.
- **Cozy extras, each where you see or hear it:** the page moved to Look and holds the eight you see (TV on the cabinet,
  stickers, printed discs, the memory card in the pause menu, light by time of day, decorations, game connections,
  Rediscover). **Playful cases** (was Playful animations), **Old-TV switch-on** and **Case sounds** are entries of Sound &
  feel, beside Animations, Motion and Sounds. The idle slideshow is **Slideshow before screen off** in System → Sleep &
  power. Your settings stay as they were.
- **Buttons page** (Settings → Playing → Buttons): button layout and button symbols in one place, with a picture of the
  four buttons and which one confirms and which goes back.
- **Systems on the shelf** says how systems appear when the list is empty.
- Pictures in the README and the manual show official games.
- Documentation: a new device list in the README and [docs/DEVICES.md](docs/DEVICES.md), grouped by chip with what each plays, the README's feature list brings 0.1.4 to 0.1.6, and the manual and screenshots match 0.1.6.

### Fixed

- A `.cue` written in Latin-1 with an umlaut in a file name (Windows tools) no longer hides its game from the shelf.

- Settings → Connections shows a connected Wi-Fi's name as it is: a network called "Off" or "Unknown" was shown as "Aus" or
  "Unbekannt" in German.
- Back from Report a problem lands on that row again (it landed on About).
- Tour and the first start's cards show only what the device has, like Settings: no Stick lights on handhelds without sticks
  (RG35XX Plus/2024, RG28XX).
- Leaving the idle slideshow or waking from sleep with a trigger pull no longer moves the shelf.
- The game's full page (X on a game in a system's list) shows the save slots row, "Save slots: 2 of 8" or "All empty", as the info page and the box's back do.
- The info page shows the save slots on the cartridge for SNES, GBA, GB, GBC and NES (the row was missing because
  these systems were not in the list of systems with saves).
- Cabinet: a tap on a case only counts when the Cabinet is in front, no longer under Settings, a card or setup.
- Colours and Font make the menu click when you change them, like the other Settings entries.
- Search: the text you type and Wi-Fi network names are shown as typed, not translated.
- Settings: each category's line names the entries this device has (no Store, no LEDs without sticks), in English and
  German.
- With light by time of day off, decorations are no longer re-scanned on every frame.
- About and backups name the release version.
- The shelf says hakuX and PortMaster, not their raw ids.
- At Large text, the explanation on Remote access and the first start's offline note wrap instead of running off the screen.

### Developer

- The gate runs its tests with nextest (one retry, flaky tests listed); the time-budget and rendering benchmarks run alone.
- The info page and hand-off tests no longer depend on the network.

## [0.1.6] - 2026-10-30

### Added

- **Theme** (Settings → Look & feel → Look, then R2): five colour sets for every page (Warm, the default; Indigo Cube,
  Grey Brick, Green X, Blue Station, each with a dark and a light version), three fonts (Barlow, Pixel, Rounded) and the
  button symbols in the hints: Nintendo letters, Xbox colours or PlayStation shapes. A live preview shows the choice
  ([docs/user/theme.md](docs/user/theme.md)).
- **Cozy extras** (Settings → Look & feel): each little touch on or off on its own: case sounds, old-TV switch-on, TV on
  the cabinet, stickers, memory card in the pause menu, printed discs, game connections, Rediscover, idle slideshow,
  light by time of day, decorations and playful animations.
- **Light by time of day** and **decorations that grow with your play** (Shelf and Cabinet): the room is cool and bright
  in the morning, warm with the lamp on in the evening and dim at night, and a plant, a lamp or a cactus stands beside the
  games where a shelf has room. The plant grows with your hours, a trophy appears after your first completed game, a framed
  "100 %" after your first 100 % sticker, and a system you played for 10 hours gets a small figurine.
- **Idle slideshow** (off until you switch it on in Cozy extras): after a quiet minute on the shelf your games' pictures
  play, slowly zooming. Any button ends it and does nothing else.
- **Playful animations** (Cozy extras, on with Cozy animations): cases spring out of the shelf and wiggle once, each game
  goes into its console its own way (a cartridge seats with a squash and a click, a disc tray slides out and shuts, the
  GameCube's and PlayStation's lid flips open and closed), the disc in an open box turns slowly, and the Shelf's spines
  lean when you scroll fast.
- **A new logo**: a pixel cartridge with the four system stripes on the boot screen, the first start, About and the README.

### Changed

- **A calmer pause menu:** Save and Load show the save slots as one slim row of small blocks in the colour of the
  game's memory card or cartridge, and the preview picture is as large as before. The memory card or cartridge with
  its blocks stays on the game's info page and box; **Memory card in the pause menu** (Cozy extras, off at first)
  brings it back to the pause menu.
- Names on spines get smaller or wrap in the wider fonts before anything is shortened with "…".
- README: a table of the devices pocket-ui was tested on and those it should run on.

## [0.1.5] - 2026-10-30

### Changed

- **Printed discs:** every disc and UMD shows its game's cover art and title in its system's look (PS1, PS2 with its blue
  rim and clear hub, GameCube mini disc, Wii, Dreamcast, Saturn, Xbox, a plain CD for the rest, the UMD with its window
  ring); a game without a cover gets a plain print with the title.
- **New cartridges:** the Game Boy Advance cartridge in dark grey and the Nintendo 64 one in light grey, redrawn from
  public-domain photos. The N64 box's spine takes its cover's colour, with the red block.
- README: a table of the devices pocket-ui was tested on and those it should run on.

## [0.1.4] - 2026-10-30

### Added

- **Stickers on your cases** (Shelf and Cabinet): Completed, 100 %, Favourite, Playing now and Game of the year with
  the year you pick. Put them on from the game's Stickers row; they sit a little crooked on the case's front and the
  game box, and one small one tops the spine on the shelf. Synced and backed up with your other game notes.
- **A TV on the cabinet** (Cabinet look): a small old CRT on the top board shows the selected game's screenshot (or
  your latest picture of it, or its cover), with scanlines and a warm glow, and crossfades as you move.
- **Connections**: the developer, publisher, series and genre on the info page and on the box's back lead to your other
  games with the same one ("More from Criterion Games", "Burnout series, in release order", "More racing games"). A small
  arrow marks a value that leads somewhere; Down picks it, A opens the list, B comes back. A value no other game of yours
  shares stays plain text. Where the data names no series, two different titles with the same name start can make one.
- **Rediscover** in Curator: games you played a year ago today, then those you have not played for a year, longest played
  first, each with the reason ("played a year ago today"). It also counts the "last played" of an EmulationStation
  gamelist; the row is only there while it has a game.
- **Old-TV power-on** at the end of Cozy's launch animation: the game comes on dark, with a bright line in the middle that
  opens into the picture and a glow that settles (a third of a second; Cozy with Motion on Full only).
- **Sounds of the cases** (Shelf and Cabinet, with menu sounds on): X opens a game's case with its own sound, a plastic
  case snapping open or a cartridge's cardboard box. A Cozy start plays the whole thing: the case opens, the cartridge
  slides in and clicks into place (a disc drops in and spins up, a game card clicks into its slot), then the old TV
  comes on with a thunk and a crackle of static. Quick keeps just a quiet click. At the level of the other menu sounds.
- **Saves on the save media of their era**: in the pause menu, Save and Load show what the game saved to back then,
  with its blocks beside it as that console's memory-card screen showed them: the PlayStation's or PS2's memory card,
  the GameCube's, the Dreamcast's VMU, the Xbox's memory unit, the PSP's Memory Stick Duo, or the game's own cartridge
  with its save label. A block with a save shows the game's cover. The same pictures, slots and buttons as before.
  The info page and the game box's back show the slots too ("Save slots: 3 of 8").

### Changed

- **Every game stands in its case** on the Shelf and in the Cabinet: the cartridge systems' games as their cardboard
  boxes (Super Nintendo, Nintendo 64, NES, Game Boy, Game Boy Advance), the rest in their cases as before. The cartridge,
  game card or disc is inside: X opens the box and it slides half out beside it; at the start (Cozy) the case opens and
  the game slides out of it into the console, the same way for every system.
- **Real sizes:** cases, boxes and what is inside them are drawn from the real things' millimetres at one scale (a Game
  Boy cartridge small beside a SNES one, a DS card tiny, a PS2 case tall, a jewel case wider than tall).
- Every system without a case of its own gets pocket-ui's own case: the cover under a band in the system's colour with
  its console and name, one size each for cartridge, disc and digital systems (Ports, PICO-8, TIC-80…).
- Long titles on spines are no longer cut while there is room: a little smaller, on two lines across a thick box (the
  subtitle on a line of its own), without the subtitle, and only then with "…".
- The cartridge systems' boxes (and the cartridges, cards and discs) come from photos of the real things, turned into
  pixel art; one system's boxes are all one size, and the front of a box is the cover edge to edge.
- The first start says "Step n of 7" (German: "Schritt n von 7") and, after a restart or a flat battery, opens again at
  the step it had reached. In the Wi-Fi step, A on the network you are connected to answers with a tick and the way on, and
  a network one search missed stays listed until three searches in a row miss it. The time zone a fresh image ships with
  does not count as "already set".
- Save slots say the real time of the save ("today 14:02", "yesterday 21:10", "23 Oct 2026, 14:02") instead of "just now".
- Menu sounds start at once: while pocket-ui is in front with the screen on and menu sounds on, the sound output stays
  open (on the RG35XX Plus the first sound after a quiet spell came 105 to 183 ms late; now 3 to 13 ms).
- The media sounds (cases, boxes, cartridges, discs, cards, the old TV) are recorded foley chosen by measurement, at the
  level of the other menu sounds.
- Add games shows the `smb://` address only while file sharing is on, and otherwise says how to turn it on.
- With only a few games on a shelf, the Shelf look stands them in from the end and the Cabinet shows a lone case in its
  middle. Settings categories get a scroll bar when larger text or German push some off the page. In the search keyboard,
  Up and Down land on the key right above or below.

### Fixed

- Quitting a PortMaster port closes only that launch and its helpers (also helpers in another process group), so a port
  started next keeps its controls.
- Port pictures and update notes are read in the background after a port is installed, updated or removed, so the shelf
  stays responsive while the card is read.
- An empty game file cannot start any more by accident: the card, the info page and the box say **Problem** with what to
  copy again, no cover is looked up for it, and the start message is clear.
- A sleep block (an app asking the system to stay awake) is respected by automatic sleep, the Sleep menu and lid sleep, not
  only by the power key; the screen stays dark and a button press wakes it.
- The shelf shows the proper title from a game's checksum (typos, glued articles) and a misspelled name no longer gets a
  wrong cover; the first list no longer waits for the checksum file. A name recognised during an interrupted background check
  reaches the shelf when the check resumes.
- Bluetooth headphones connected or disconnected in the menus adjust the sound output within two seconds; muted menu audio
  stops running in the background.

## [0.1.3] - 2026-10-29

### Added

- **Info page** in the Minimal, Cards and Split looks: X on a game shows everything on one screen (cover, your
  status, play time and rating, the facts, the description that scrolls in its place, a port's update note); SELECT
  shows the full card. Shelf and Cabinet keep the game box.
- Anbernic H700 handhelds can be installed, as **experimental**: the installers accept exactly the 11 models in the
  per-model table in [docs/DEVICES.md](docs/DEVICES.md#anbernic-h700-handhelds-2026-10-27-installable-2026-10-29-experimental)
  (RG28XX, five RG35XX, RG40XX H and V, RG34XX, RG34XX-SP, RG-SP), on ROCKNIX's H700 image. Computer tests for all models;
  the RG35XX Plus was also tried on a handheld, where games from Game Boy to PlayStation 1 run smoothly (Crash Bandicoot 2
  among them). The RG CubeXX (720x720) is refused with a sentence saying so.
- **PortMaster ports** are guests of the pause menu: HOME opens Continue, Options (Performance & fan), Quick settings and
  Quit over a running port. Quit closes the port and everything its start script launched. A port saves by itself, so
  the menu has no Save or Load.
- Ports show their PortMaster cover (else the screenshot) as the game box on the shelf, and the spine takes its colour. A
  port with a newer version in PortMaster says **Update available** in Extras and on its card and box; nothing updates by itself.
- **PS Vita** (Vita3K) as a guest of the pause menu like Wii U and PS3, with the Vita's own picture, a small blue-topped case on
  the shelf and covers from libretro's "Sony - PlayStation Vita" thumbnails. SM8550 handhelds only, experimental; the firmware and the
  games are your own.

### Changed

- **Animations: Cozy or Quick** (Settings → Look & feel → Animations, and on the look page, L1; it replaces the Launch
  animation switch). Cozy plays the launch animation long enough to see (about a second; a pocket-core game shows itself
  once it is over, any button skips it) and makes pages, the game box and the menus a little slower and softer; Quick is
  straight into the game. Until you choose, Cozy goes with the Shelf and Cabinet looks and Quick with the others.
- The Xbox core (hakuX) is updated to the current head of PR #9 (`2de4e9c`, rebased onto hakuX's own v1.2.0), which includes
  the pacing fix for the frame pickup and our audio relaunch fix; the build no longer needs a patch of ours. Measured on the
  Mini V2 with the usual settings it shows 59.9 new pictures a second, and Jade Empire's sound comes back after the relaunch
  into the game ([docs/XBOX.md](docs/XBOX.md)).

### Fixed

- A port that ends within seconds is a failed start: pocket-ui says "<name> didn't start. It may need files or libraries
  that aren't installed", with where to look, instead of counting a play session. Quit is not a failed start.
- A port's game box shows its cover (the cover is read while the card is open).
- H700: ROCKNIX's own volume box stays hidden from the first start, so only pocket-ui's shows when you change the volume.
- The pause menu no longer stays black on a screen where sway sends no resize (the H700's 640x480).
- After a game ends, its case on the Shelf is no longer left blank.
- The first-start note reads "an Anbernic ..." (the article follows the name).

## [0.1.2] - 2026-10-29

### Added

- **Xbox games**, played by hakuX (rfandango's Xbox emulator, Linux/libretro port by portare-ch) in RetroArch. Xbox is a
  system of its own on the shelf, with console picture, case and fact sheet. pocket-ui is the pause menu: Continue,
  Options (Resolution, Scaling), Quick settings and Quit. There are no save states yet; games save themselves, in the game.
  About 60 new pictures a second (was 47), sound after a relaunch (our audio fix, sent upstream), Quit in a blink.
  Runs on the Mini V2 and the SM8550 handhelds (Odin 2 family, Pocket 6, Nova), at 2x resolution by default on SM8550 only.
  All three downloads carry the hakuX core with its licence and source notes. Experimental ([docs/XBOX.md](docs/XBOX.md)).
- **Wii U** (Cemu) and **PS3** (RPCS3) as guests of the pause menu, with a performance profile and a clean Quit. SM8550
  handhelds only, the ones ROCKNIX ships them for.
- A launch animation while a game loads, and the game card shown as the game's box.
- A **This game** page: the game's picture, extras, controls, shortcuts (Fast-forward, Rewind, Earlier, Rumble), and where
  it runs. Open it from the card or the pause menu.
- PlayStation 1 and 2 games are recognised by the serial on the disc, not only by file name: `SLES_556.73.Something.iso`
  shows as "Final Fantasy VII", with its box art and facts ([docs/user/library.md](docs/user/library.md#playstation-1-and-2-discs)).
- A very quiet fan profile (off until 60 °C, stops below 55 °C).
- Report a problem, from Settings or with SELECT+START.
- START in the pause menu opens quick settings. The menu wraps around: Quit is one press Up from Continue.
- In preparation, not installable yet: profiles for the Anbernic RG28XX and RG35XX SP (H700) and a scaled layout for their small screens.

### Changed

- One look across the app: shared parts, warm neutrals, rows that lift off the page in Light, one scroll bar, one header
  for Settings pages, matching toasts and status panels, new emblems for Curator, Surprise me, PC streaming and stickers.
- The Cabinet look: every game stands in its system's real case on a shelf, the selected one large with its cover. Nothing
  is cut off and long titles stay readable.
- Big libraries start faster: covers load off the UI thread, generated libraries are reused after a save, and free memory
  goes back to the system every 20 s.
- Menu sounds follow the audio buffer's real size, so they no longer change under a sound.

### Fixed

- Menu sounds are heard again after a game on a big emulator.
- Quit takes one press. Xbox Quit no longer takes 18 s when its window is covered.
- A system with only one emulator is no longer relaunched for ever when a game fails; the error card says how to report it.
- Store lists and detail pages are never blank without a catalogue.
- A small screen (640x480) no longer crashes the interface.
- Volume-down on the Mini V2 shows the volume box every time.
- Smaller fixes to text fitting, German wording and the search field's caret.

## [0.1.1] - 2026-10-28

### Added

- **Two-file install** on the official ROCKNIX card: copy `mount-storage.sh` and `pocket-ui-seed.tar.gz` to the card's
  `ROCKNIX` partition on a PC and start twice. ROCKNIX stays untouched, and a failed install leaves a log on the card
  (`scripts/make-fat-install.sh`, `deploy/fat/`). Tested against a fake card only.
- **New menu sounds**: warm hardware samples (wooden tick, cartridge click, drawer, knob) instead of synthetic beeps, at a
  default level of 70 % (was 30 %) to match a game's loudness (`assets/ui-sounds/`).
- **A calm start**: no kernel text, a boot screen from systemd until the shelf takes over, and the shelf fading in from
  black ([docs/BOOT-SPLASH.md](docs/BOOT-SPLASH.md)). For a card set up earlier: `scripts/boot-quiet.sh`, with backup and undo.
- Pixel art in the interface: pictures above the words of empty pages, icons for the Settings categories, the handheld on
  first start and About, the Mini V2 with named controls in the manual, and stickers for milestones in the Year in review
  ([assets/ui-art/SOURCES.md](assets/ui-art/SOURCES.md)).
- Pictures and fact sheets for the SuperGrafx and the Pokémon mini, drawn from text prompts in the style of the others,
  not exact pictures of the hardware ([assets/systems/SOURCES.md](assets/systems/SOURCES.md)).
- Experimental support for the AYN Odin 2 family (Odin 2 Mini, Odin 2, Odin 2 Portal; ROCKNIX SM8550). Only the Odin 2 Mini
  was tried on a real device, over the command pipe with no button pressed: install checks, first start, shelf, a GBA game
  with the pause menu, screen off and wake. What is still open: [docs/DEVICES.md](docs/DEVICES.md#ayn-odin-2-family-2026-10-27-experimental).
- Licence texts travel with the programs: the package and the SD card image carry `LICENSE`, `NOTICE.md`, the Rust crates'
  and fonts' licences in a `licences/` folder. Settings › System › About has a Licences row naming it.
- `net.log` in the logs folder and in Report a problem: what NetworkManager, iwd and Tailscale said since the start, and
  the names (never passwords) of saved Wi-Fi networks.
- A Wi-Fi test bench (`scripts/wifi-bench/`, [docs/WIFI-BENCH.md](docs/WIFI-BENCH.md)): ROCKNIX's own network stack against
  simulated radios in eight scenarios (roaming, a vanishing hotspot, a hotel sign-in, a wrong password and more). It is
  not a test on a device.
- The update channel's address: pocketui.xyz for the signed list, GitHub Releases for the files, https only
  ([docs/UPDATE-CHANNEL.md](docs/UPDATE-CHANNEL.md)). Nothing is published there yet.

### Changed

- A newer ROCKNIX no longer blocks the install. The installers accept the tested ROCKNIX 20261001 and anything newer, say
  "tested with 20261001; your ROCKNIX is newer" and leave the decision to pocket-ui's own compatibility check
  (`pocket-ui --check-os-update /`), which still blocks when needed. An older ROCKNIX is asked to update first.
- The install guides are honest about the start: ROCKNIX on the Mini V2 needs its own one-time bootloader setup, and
  README, getting started and the card zip point to ROCKNIX's guide.
- One device list everywhere: the Mini V2 is the target; Pocket 5, Flip 2, first Mini, Pocket 6, Nova and the AYN Odin 2
  family are experimental.
- A hint how to copy files into `roms/ports` over ROCKNIX's file sharing (Samba) from Windows and macOS.
- **Switch off after sleeping is Off by default** (was 3 h): a sleeping Pocket stays asleep. If yours still switches off,
  set Settings › System › Sleep & power once.
- Power off and Restart clear the clock alarm and log it (`logs/shutdown.log`); the start clears a leftover one, so a
  Pocket never powers itself back on after Power off.
- The manual's longest pages are split in two; nothing was removed.
- English wording checked and made consistent (British spelling, "45%", Settings paths with "›"). Nothing works differently.

### Fixed

- **Bluetooth audio** no longer sounds broken. Games on Bluetooth headphones were heavily distorted on the Mini V2; the
  speaker was fine. The audio buffer was shorter than the Bluetooth sink's cycle and the pitch control wobbled. Measured
  off the device with `scripts/audio-bench`: 373 glitches in 8 s before, 0 in 10 minutes after
  ([docs/HOST.md](docs/HOST.md#bluetooth-audio-2026-10-25)). Not yet heard on a device with real headphones.
- **Wi-Fi after a boot**: quick settings and Settings › Wi-Fi share one picture of the radio and no longer disagree. A
  blocked radio shows "Wi-Fi is off" with A to turn it on, never "No networks found"; a failed scan says so; and if Wi-Fi
  is on in your settings but the system blocked the radio at start, pocket-ui switches it on once.
- **Wi-Fi in first-start setup**: after a wrong password the page kept offering the network as Saved and later showed
  "Connected" while nothing was. Now the key is dropped, the keyboard opens again with "Wrong password. Try again.", and
  after 60 s the page says the network isn't letting the Pocket in.
- **A hotspot that goes away and comes back** (phone, travel router): back in 4 to 21 s instead of 95 s, and a saved network
  heard again is joined at once.
- Sign in… on a hotel or café network said "No answer" the first time; it now waits 12 s instead of 5.
- ROCKNIX's own volume and brightness box no longer sits over pocket-ui while it is in front. Uninstalling puts it back.
- A small screen no longer crashes the interface: at 640x480 (the H700 handhelds' size) 62 of 620 snapshot states aborted,
  among them first start and every game card.
- Settings: the line under each category was cut with "…" at Medium and Large text; it now wraps whole.
- The Bluetooth page says "devices", not only "controllers": headphones and speakers are listed there too.
- Loading a missing or empty save state said "There is no the auto state" or "The the auto state is empty".

## [0.1.0] - 2026-10-28

The first public release, an early alpha: developed on one Retroid Pocket Mini V2 with ROCKNIX 20261001, plus a virtual
device. [docs/STATUS.md](docs/STATUS.md) says, feature by feature, how far each one has been proven.

### Added

- The shelf: Home with Continue, recent games, favourites, Curator and systems; five looks, dark and light; search as you
  type; cover art from the libretro thumbnail collection; play time.
- pocket-core, a libretro host for the handheld systems up to Nintendo DS, PSP, Dreamcast and Saturn, with resume, 8 save
  slots, the pause menu on HOME, per-console and per-game options, run-ahead and rewind.
- The same pause menu for PlayStation 2, GameCube, Wii and 3DS through ARMSX2, Dolphin and Azahar.
- Curator: a card per game with offline facts from Wikidata, filters, smart shelves, "more like this", history.
- First-start setup, Wi-Fi, Bluetooth controllers, sleep, battery, fan profiles, Syncthing, Tailscale, Moonlight, file
  sharing, a web companion, signed app updates with rollback.
- The manual on the device, in English, with menus in English or German ([docs/user/](docs/user/index.md)).

### Not included yet

- A published SD card image. It builds and passes its offline checks, but a complete first start as a new player (card,
  resize, setup, first game without help) has not been repeated with the fixed build.
- A public update server and Store catalogue.
