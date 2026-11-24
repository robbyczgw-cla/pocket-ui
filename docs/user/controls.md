# Controls

Button names match the Retroid Pocket Mini V2's printed layout: **A is the right face button, B is the bottom one.**
pocket-ui uses A to confirm and B to go back, and the hint bar at the bottom of every screen shows what's active right
now (tightening letter spacing slightly if needed so labels like **X Details** stay visible alongside Quick settings,
Settings and Continue).

![The Mini V2's controls: the D-pad and the left stick on the left, A, B, X and Y and the right stick on the right, SELECT and START at the top corners, HOME and BACK at the bottom corners, L1, L2, R1 and R2 on the shoulders.](pocket-ui:controls)

**Prefer Xbox button layout?** Set Settings → Controls → Buttons → [**Button layout**](settings-sound.md#button-layout)
to **Xbox** to put A at the bottom and B on the right across every pocket-ui menu while keeping in-game controls
untouched ([Button layout](#button-layout)).

<!-- TODO: confirm on the device that X is the top face button and Y the left one (Nintendo
     layout; the code uses BTN_NORTH for X and BTN_WEST for Y). -->

## On the shelf

| Button | What it does |
|---|---|
| D-pad Up / Down | Move through the list (hold to scroll; stops at the ends). |
| D-pad Left / Right | Same as Up / Down. |
| L1 / R1 | Jump five entries up or down. |
| L2 / R2 | Jump to the previous / next starting letter (on Home, jumps to the first console then steps alphabetically; hold to keep jumping). |
| Up on Continue | Highlight **Search** right above Continue, and press A to open [search](playing.md#finding-a-game). |
| A | Launch the selected game, or open a console, Favourites, Search, Curator, smart shelf or label. |
| B | Return from a console list, Favourites or Curator list. |
| Y | Star/unstar the selected game for Favourites (or pin/unpin a smart shelf or label on Home). |
| X | Open a game's [card](library.md) (facts, status, notes, related games), a system's [console page](console-pages.md), [Credits & links](console-pages.md#credits--links) from a console page, or options on a smart shelf/label. |
| SELECT (tap) | Open [Recent games](playing.md#recent-games). |
| START (tap) | Open [Settings](controls-settings.md#in-settings) from Home, any console or Favourites (L1/R1 switch categories; B returns where you left off). Holding START with SELECT or Volume triggers a combo instead. |
| SELECT + START, held for 1 second | Opens recovery dialog: **Cancel**, **Restart pocket-ui**, **Report a problem** or **Open EmulationStation** (ROCKNIX's menu, with warning; see [Safe mode](safe-mode.md#opening-rocknixs-own-menu)). Escape on a keyboard does the same. |

In the **Cabinet** layout, Left/Right move along a shelf and Up/Down switch shelves (landing on the nearest case).
Both L1/R1 and L2/R2 jump by starting letter. Pressing Up from the top shelf opens [search](playing.md#finding-a-game).
B stays quiet since every shelf is already on screen (except returning from a Curator or Lens list).

## In the search

| Button | What it does |
|---|---|
| D-pad | Navigate the keyboard (Up/Down jump straight to the key above or below even across wider keys). |
| A | Type the highlighted key (or jump to results when on **Results**). |
| B | Backspace, or close search when empty. |
| START | Jump to matching games (or recent searches if nothing is typed). |

Inside search results:

| Button | What it does |
|---|---|
| D-pad Up / Down, L1 / R1 | Move one or five rows at a time. |
| A | Launch the game (or re-run a recent search). |
| Y | Star or unstar the game. |
| X | Open the game's [card](library.md) (B returns to search). |
| B or START | Return to the keyboard with your query intact. |

## In the Lens

[Curator](curator.md#curator) → Lens puts filter facets on the left and values on the right:

| Button | What it does |
|---|---|
| D-pad Up / Down | Move within the active column (L1 / R1 jump six rows). |
| D-pad Right, or A on a filter | Enter its values (D-pad Left returns to filters). |
| A on a value | Toggle that filter value on or off. |
| Y | Clear all picks in the highlighted filter. |
| START | Show matching games (B returns to Lens). |
| X | Save current filters and sort order as a smart shelf. |
| SELECT | Cycle sort order: Name, Last played first, Most played first, Best rated first, Oldest first, Shortest first ([Lens](curator.md#lens)). |
| B | Go back (Lens remembers your picks and order until pocket-ui restarts). |

On a game card, Down highlights Curator rows (Echoes, Versions, Stickers) and A opens them (touch: tap to select, tap
again to open). Inside Echoes, A plays and X opens a card; inside Versions, A plays and Y sets your preferred copy.
See [Your library](library.md#opening-a-card) for other card actions (Y sets status and
[My rating](library.md#my-rating)). On a card opened with X from [Just Play](curator.md#just-play), D-pad Right rolls
another pick.

## In History

[Curator](curator.md#play-history) → History and Year/Month in review:

| Button | History | Year in review |
|---|---|---|
| D-pad Up / Down | Browse most-played games | - |
| D-pad Left / Right | Previous / next week, month or year | Previous / next card |
| L1 / R1 | Switch Week / Month / Year | Previous / next year (or month) |
| A | Open selected game card (B returns) | Open the featured game's card |
| Y | Open review for this month or year | - |
| B | Go back | Go back |

On touchscreens: tap a game to highlight and tap again to open its card; tap left/right on the chart or review card to
step; tap Week, Month or Year directly.

## Button layout

Settings → Controls → Buttons → [**Button layout**](settings-sound.md#button-layout) swaps menu confirmation buttons
everywhere in pocket-ui:

| Layout | A (confirm) | B (back) | X | Y |
|---|---|---|---|---|
| **Nintendo** (default) | right | bottom | top | left |
| **Xbox** | bottom | right | left | top |

Applies immediately across all pocket-ui screens (including pause menu and Quick settings) for both built-in buttons
and external controllers, updating hint bars, touch pills and this manual.

What stays unchanged:

- **In-game controls:** games always use the console's layout (remap per game in Options → Controls), and HOME shortcuts
  always follow the physical printed buttons. *(GameCube has its own **Button layout (GameCube)** in
  [Options](options-big.md).)*
- **Safe mode:** always uses the handheld's printed buttons (Xbox on Odin 2 Mini/Portal, Nintendo elsewhere;
  [Safe mode](safe-mode.md#safe-mode)). Switched by mistake? Left/Right on Button layout flips right back (or use
  **Reset pocket-ui settings…** in safe mode).
- D-pad, shoulders, START, SELECT, HOME and touchscreen taps.

## With a Bluetooth controller

Paired pads (Settings → Connections → [Bluetooth](bluetooth.md)) work in menus the instant they connect. pocket-ui maps
by physical button position rather than printed legends: in Nintendo layout, the **right** button confirms and the
**bottom** button goes back (B/A on Xbox, Circle/Cross on PlayStation); switch [Button layout](settings-sound.md#button-layout)
to **Xbox** and the bottom button confirms everywhere (Xbox A / PlayStation Cross). D-pad, sticks, shoulders, START and
SELECT work just like on the handheld.

Multiple pads can be active in menus at once without fighting over held inputs, and disconnecting one cleanly releases
only its own buttons. In-game multiplayer details are below.

## Playing together

pocket-ui's own player supports up to four players: built-in controls are Player 1, the first wireless/USB pad becomes
Player 2, and so on. Connecting or dropping a controller mid-game pops up a quick toast ("DualSense connected, Player
2"), and **Players** in the pause menu lets you reassign any controller, even making a wireless pad Player 1
([Players](pause-menu.md#players)).

- HOME, shortcuts and the pause menu work from every controller. To quit with SELECT + START, hold both on the same pad.
- Per-game button maps from Options → Controls apply to all players.
- Rumble routes to each player's own controller.

RetroArch assigns players in discovery order (change under RetroArch's menu, Settings, then Input); standalone emulators (PS2,
GameCube, Wii, 3DS, …) keep ROCKNIX's controller settings.

## In a game

**Menu Button layout never alters gameplay.** Game buttons, HOME shortcuts and Options → Controls always follow the
physical printed layout (A on the right). To swap A and B inside a game, remap it under Options → Controls.

| Button | What it does |
|---|---|
| HOME, tapped | Pause and open the pause menu. |
| HOME + SELECT | Pause and open [Recent games](playing.md#recent-games). |
| HOME + A | Capture a [screenshot](playing.md#screenshots) without opening the pause menu. |
| HOME + another button | Trigger a shortcut below: all of them in pocket-core, and matching RetroArch keys where supported ([Shortcuts in RetroArch games](options.md#shortcuts-in-retroarch-games)). |
| SELECT + START, held for 1 second | Quit a pocket-core game (always saves your moment regardless of Settings → Playing → [Continue where I quit](settings-playing.md#continue-where-i-quit)). |
| All other buttons | Sent straight to the game (customisable in pocket-core via Options → Controls). |

Shortcuts in pocket-ui's own player (toggle each off under Options → Shortcuts):

| Buttons | What it does |
|---|---|
| HOME + R1 | Quick-save to the active slot. |
| HOME + L1 | Quick-load the active slot. |
| HOME + Left / Right | Switch previous / next save slot. |
| HOME + R2 | Toggle fast-forward on/off. |
| Hold HOME + L2 | Rewind while held (requires Rewind On under Speed, rumble, fan). |
| HOME + Y | Cycle [Over the game](pause-menu.md#over-the-game) (Off → FPS → FPS + sensors → Pixel Path) for this session. |
| HOME + X | Open [Zoom](big-text.md#zoom) on the paused frame (B returns to play; Off until enabled under Options → Shortcuts → Zoom). |
| HOME + A | Save a [screenshot](playing.md#screenshots) ("Screenshot saved"; always physical A regardless of [Button layout](settings-sound.md#button-layout)). |

HOME + A also captures screenshots in standalone emulators (PS2, GameCube, Wii, 3DS) and PortMaster ports (always on,
silent without an overlay; see [Screenshots](playing.md#screenshots)). In RetroArch games it works the same way and can
be toggled off via Options → Shortcuts → Screenshot in any pocket-core game for that console.

In RetroArch games, HOME + R1 saves, HOME + L1 loads, HOME + R2 toggles fast-forward, holding HOME + L2 rewinds (when
Rewind is On under Runs on), HOME + Y toggles RetroArch's FPS counter, HOME + X opens RetroArch's menu and pressing
HOME + START twice quits. HOME + Left/Right, Zoom and SELECT + START quit aren't available in RetroArch; use the pause
menu's Quit or double-tap HOME + START. Check Pause menu → Options → **Shortcuts** inside any RetroArch game for the
full list.

While HOME is held, inputs never leak into the game. See [Options](options.md#shortcuts).

## In the pause menu

See Options → Controls to inspect or remap every game button ([Controls](options.md#controls)).

| Button | What it does |
|---|---|
| D-pad Up / Down | Move through the menu (wraps from Continue to Quit). |
| D-pad Left / Right | Pick a save slot on Save or Load (single state only on RetroArch). |
| A | Confirm (on **Quit**, exits while saving or skipping auto-state per Settings → Playing → [Continue where I quit](settings-playing.md#continue-where-i-quit)). |
| B or HOME | Resume the game (or return to the pause menu from Quick settings). |
| START | Open [Quick settings](quick-settings.md) (brightness, volume, fan); B returns here. |
| SELECT | Open Recent games; B returns here. |

On **Achievements**, Up/Down scroll and L1/R1 jump five (B returns). On **Pixel Path**, Up/Down pick stages, A explains
the highlighted stage, Y toggles the in-game overlay and B returns. On **Players**, Up/Down pick a player slot and A
assigns a controller.

## In Recent games

| Button | What it does |
|---|---|
| D-pad Left / Right | Step through recent games (Up / Down do the same). |
| L1 / R1 | Jump to first / last recent game. |
| A | Resume or launch the selected game (in-game: resume current or switch straight to another). |
| Y | Star or unstar the selected game. |
| B, SELECT or HOME | Close the switcher. |

## In Options

Inside Pause menu → **Options** and its subpages:

| Button | What it does |
|---|---|
| D-pad Up / Down | Move (wraps at ends). |
| D-pad Left / Right | Adjust the highlighted value. |
| L1 / R1 | Skip five rows on long Emulator option lists. |
| A | Open a submenu or cycle to the next value. |
| B | Go back one level. |

For Settings, first-start setup, touchscreens, power and volume keys, continue to
[Controls in Settings, setup and on the touch screen](controls-settings.md).

On the experimental **AYN Odin 2 Mini** and **Odin 2 Portal**, menus default to **Xbox** (A bottom, B right, X left, Y
top) to match their printed letters, while the original Odin 2 defaults to Nintendo (saved choices win; safe mode uses
printed buttons; games stay positional, so screenshots are HOME + the right face button regardless of menu layout).

## On Android

System **Back** (gesture or button) opens the pause sheet during gameplay and returns to it from subpages without
quitting (use **Save & quit** to exit). Disconnecting a gamepad pauses the game and brings back touch controls. Android
face buttons follow physical positions.

Pause-sheet **Zoom** freezes the frame: pinch to zoom, drag to pan, or use D-pad + A for 2x/3x (Back closes).
**Options → Controls → Hold to toggle** latches any single game button (tap to hold, tap again to release; clears on
pause, load, backgrounding or controller changes; saved per game). **Haptics** in Android Settings starts Off and
respects Android's system haptics toggle. **Text size** uses whichever is larger between the app setting and Android's
font scale (up to 200%).
