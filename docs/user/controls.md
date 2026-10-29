# Controls

Button names follow what is printed on the Mini V2: **A is the right face button, B the bottom
one.** pocket-ui uses A to confirm and B to go back. The hints at the bottom of each screen
show the buttons that screen uses.

![The Mini V2's controls: the D-pad and the left stick on the left, A, B, X and Y and the right stick on the right, SELECT and START at the top corners, HOME and BACK at the bottom corners, L1, L2, R1 and R2 on the shoulders.](pocket-ui:controls)

**Used to an Xbox controller?** Settings → Playing → [**Button layout**](settings.md#button-layout) set to **Xbox**
puts A at the bottom and B on the right in every menu of pocket-ui; games keep their own ([Button layout](#button-layout)).

<!-- TODO: confirm on the device that X is the top face button and Y the left one (Nintendo
     layout; the code uses BTN_NORTH for X and BTN_WEST for Y). -->

## On the shelf

| Button | What it does |
|---|---|
| D-pad Up / Down | Move through the list. Hold to keep moving. The list stops at its ends. |
| D-pad Left / Right | The same as Up / Down (handy in the Cards look). |
| L1 / R1 | Jump five entries up / down. |
| L2 / R2 | Jump to the previous / next first letter (on Home: to the first system, then letter by letter through the systems). The letter shows large for a moment. Hold to keep jumping. Like Up and Down, this stops at the ends of the list: R2 in the last letter goes to the last entry. |
| Up on Continue | **Search**, the row above it: A opens the [search](playing.md#finding-a-game). |
| A | Play the selected game, or open the selected system, Favourites, Search, Curator, a smart shelf or a sticker. |
| B | From a system's game list, Favourites or a Curator list, back to where you opened it. |
| Y | Star the selected game, or unstar it. Starred games are listed under Favourites on Home. On a smart shelf or sticker: pin it to Home, or take it off. |
| X | Open the selected game's [card](library.md): facts, play status, your note, related games you own. On a system: its [console page](playing.md#console-pages); on a console page: its [Credits & links](playing.md#credits--links). On a smart shelf or sticker: its options (rename, delete …). |
| SELECT (press and release) | Open the [game switcher](playing.md#game-switcher). |
| START (press and release) | Open [Settings](controls-settings.md#in-settings), from Home and from a system's game list or Favourites alike. Settings is a button, not an entry of the list. It opens on its categories; B there comes back to the list you came from. START held together with SELECT or a volume key is a combo, not this. |
| SELECT + START, held for 1 second | Asks what to do: **Cancel**, **Restart pocket-ui** or **Open EmulationStation** (ROCKNIX's own menu, with a warning first; see [Safe mode and EmulationStation](safe-mode.md#opening-emulationstation)). Escape on a keyboard does the same. |

In the **Cabinet** look, Left / Right move along a shelf and Up / Down go to the shelf above or below
(to the case standing nearest). L1 / R1 jump to the previous / next first letter, and so do L2 / R2. Up on the
top shelf opens the [search](playing.md#finding-a-game). B does nothing there: all the shelves are on one screen.
The one exception is a Curator list opened from the Curator shelf or the Lens: B goes back there.

**Up next.** After you quit a game, the shelf may show [Up next](curator.md#up-next), a game of yours like the one
you played. While it shows, **A** plays that game and **B** puts it away; any other button puts it away and does
what it always does. It goes by itself after a few seconds, and then A is the shelf's again. A tap on it plays it.

## In the search

| Button | What it does |
|---|---|
| D-pad | Move on the keyboard. |
| A | Type the key. **Results** on the keyboard goes to the list, as START does. |
| B | Delete the last letter. With nothing typed, close the search. |
| START | Go to the list of games found (with nothing typed: the recent searches). |

In the list:

| Button | What it does |
|---|---|
| D-pad Up / Down, L1 / R1 | Move, one or five at a time. |
| A | Play the game. On a recent search: search it again. |
| Y | Star the game, or unstar it. |
| X | Open the game's [card](library.md); B there comes back to the search. |
| B or START | Back to the keyboard. What you typed stays. |

## In the Lens

[Curator](curator.md#curator) → Lens: filters on the left, their values on the right.

| Button | What it does |
|---|---|
| D-pad Up / Down | Move in the column the selection is in. L1 / R1 move six rows. |
| D-pad Right, or A on a filter | Into its values. D-pad Left: back to the filters. |
| A on a value | Pick it or drop it. |
| Y | Clear the picks of the selected filter. |
| START | Show the games. B in that list comes back to the Lens. |
| X | Save as a smart shelf (with the order). |
| SELECT | The order of the games: Name, Last played first, Most played first, Best rated first, Oldest first, Shortest first ([Lens](curator.md#lens)). |
| B | Back. The Lens keeps its picks and order until pocket-ui restarts. |

On a game's card, Down moves onto the Curator rows (Echoes, Versions, Stickers) and A opens them; on the touch
screen, tap a row to select it and tap it again to open it. On the Echoes page
A plays and X opens the card; on the Versions page A plays and Y makes a version the preferred one. The card's
other buttons are in [Your library](library.md#opening-a-card); Y there also sets your [rating](library.md#my-rating) (Left /
Right on the My rating row). On a card opened by [Surprise me](curator.md#surprise-me), D-pad Right picks another game.

## In History

[Curator](curator.md#play-history) → History, and the Year (or Month) in review.

| Button | History | Year in review |
|---|---|---|
| D-pad Up / Down | Move in the most played games | — |
| D-pad Left / Right | The week, month or year before or after | The card before or after |
| L1 / R1 | Week, month, year | The year (or month) before or after |
| A | The selected game's card; B there comes back | The card's game, on cards about one game |
| Y | The review of this month or year | — |
| B | Back | Back (to History, when it was opened from there) |

On the touch screen, tap a game to select it and again to open its card; tap the left or right
part of the chart (of a review card) for the one before or after. A tap on Week, Month or Year opens it.

## Button layout

Settings → Playing → [**Button layout**](settings.md#button-layout) chooses which face button plays A, B, X and Y:

| Layout | A (confirm) | B (back) | X | Y |
|---|---|---|---|---|
| **Nintendo** (the default) | right | bottom | top | left |
| **Xbox** | bottom | right | left | top |

It applies at once, everywhere in pocket-ui (the pause menu and quick settings included), on the Pocket's buttons and
on a connected controller alike. The hints at the bottom of every
screen, the pills you can tap and this manual use the letters of the layout you chose.

What it does not change:

- **Games.** A game keeps the console's own layout; Options → Controls changes it per game, and the shortcuts with HOME
  use the printed buttons. (GameCube games have their own **Button layout (GameCube)** in
  [Options](options-big.md).)
- **Safe mode.** It follows the device’s printed layout (Xbox on Odin 2 Mini/Portal, Nintendo elsewhere)
  ([Safe mode](safe-mode.md#safe-mode)). If you lose your way after a switch, Left/Right on the Button layout row
  switches back; in safe mode, **Reset pocket-ui settings…** does too.
- The D-pad, the shoulder buttons, START, SELECT and HOME, and taps on the touch screen.

## With a Bluetooth controller

A paired controller (Settings → Connections → [Bluetooth](bluetooth.md)) works in the menus as soon as it is connected,
without a restart. pocket-ui goes by where a button sits, not by what is printed on it: the **right** face button
confirms and the **bottom** one goes back, as on the Pocket. On an Xbox controller that is B to confirm and A to go
back; on a PlayStation controller Circle and Cross. With [Button layout](settings.md#button-layout) on **Xbox** it is
the other way round, on the Pocket and on every controller: the bottom button confirms (an Xbox controller's A, a
PlayStation controller's Cross) and the right one goes back. D-pad, sticks, shoulder buttons, START and SELECT work as on
the Pocket.

Several controllers can be connected at once, and any of them moves through the menus. They don't get in each other's
way: when one holds a direction to scroll, another one pressing and letting go of the same direction doesn't stop it,
and a controller that switches off or loses its connection lets go only of its own buttons. In games they are players,
see [Playing together](#playing-together).

## Playing together

Games on pocket-ui's own host take up to four players. The Pocket's own buttons are Player 1, the first controller you
connect is Player 2, the next one Player 3, and so on. When a controller connects or goes away during a game, a short
message says so, for example "DualSense connected — Player 2". **Players** in the pause menu changes who is who, including
making a controller Player 1 (see [Players](pause-menu.md#players)).

- HOME, the shortcuts and the pause menu work on every controller. To quit with SELECT + START, hold both on one
  controller.
- Every player's buttons do what Options → Controls sets for the game.
- Rumble goes to each player's own controller; the Pocket's buttons have the Pocket's motor.

RetroArch numbers the controllers itself: normally the Pocket is Player 1 and connected controllers follow in the order
RetroArch finds them. Change it in RetroArch's own menu, under Settings → Input. The other emulators (PlayStation 2,
GameCube, Wii, 3DS and the rest) have their own controller settings, which pocket-ui leaves as ROCKNIX set them up.

## In a game

**Button layout does not reach the game.** The buttons that go to the game, the shortcuts below and Options → Controls
use the Pocket's buttons as printed (A on the right) whatever [Button layout](settings.md#button-layout) says; only
pocket-ui's own screens, the pause menu and quick settings among them, follow it. To play a game with A and B swapped,
use Options → Controls for that game.

| Button | What it does |
|---|---|
| HOME, tapped | Pause the game and open the pause menu. |
| HOME + SELECT | Pause the game and open the [game switcher](playing.md#game-switcher). |
| HOME + A | Take a [screenshot](playing.md#screenshots) of the game. Not a tap: the pause menu does not open. |
| HOME + another button | Not a tap. On pocket-ui's own host these are the shortcuts below; in RetroArch games they stay RetroArch shortcuts. |
| SELECT + START, held for 1 second | Quit a game on pocket-ui's own host. It saves the moment first, whatever Settings → Playing → [Remember where I quit](settings.md#remember-where-i-quit) says. |
| All other buttons | Go to the game. On pocket-ui's own host you can change what each one presses (Options → Controls). |

Shortcuts on pocket-ui's own host (each can be switched off in Options → Shortcuts):

| Buttons | What it does |
|---|---|
| HOME + R1 | Save to the selected slot. |
| HOME + L1 | Load the selected slot. |
| HOME + Left / Right | Previous / next save slot. |
| HOME + R2 | Fast-forward on / off. |
| Hold HOME + L2 | Rewind while held (only when Rewind is on under Extras). |
| HOME + Y | [Over the game](pause-menu.md#over-the-game): the next of Off, FPS, FPS + sensors and Pixel Path, until you quit. |
| HOME + A | [Screenshot](playing.md#screenshots): a picture of the game, saved to the SD card. "Screenshot saved" shows for a moment. Always the button printed A, whatever [Button layout](settings.md#button-layout) says. |

HOME + A also works in the games of the standalone emulators (PlayStation 2, GameCube, Wii, 3DS) and in PortMaster
ports, where pocket-ui takes the picture of the screen; there it cannot be switched off and nothing shows over the game
(see [Screenshots](playing.md#screenshots)). RetroArch has screenshot keys of its own.

While HOME is held, the game gets no buttons. See [Options](options.md#shortcuts).

<!-- TODO: list RetroArch's HOME shortcuts on ROCKNIX, and say whether SELECT + START quits
     RetroArch games too. Until then, use the pause menu's Quit. -->

## In the pause menu

| Button | What it does |
|---|---|
| D-pad Up / Down | Move. The menu wraps: Up from Continue is Quit. |
| D-pad Left / Right | On Save or Load: choose the save slot (not with RetroArch, which has one save). |
| A | Choose. On **Quit**: end the game, keeping the moment or not as Settings → Playing → [Remember where I quit](settings.md#remember-where-i-quit) says. |
| B or HOME | Back to the game. In the quick settings opened from this menu: back to this menu. |
| START | Open the [quick settings](quick-settings.md) (brightness, volume, fan); B there comes back to this menu. |
| SELECT | Open the game switcher; B there comes back to this menu. |

On the **Achievements** page, Up / Down move and L1 / R1 skip five; B goes back to the menu. On **Pixel Path**,
Up / Down move between the stages, A explains the selected one (A or B closes the explanation), Y shows Pixel Path
over the game or hides it, B goes back. On **Players**, Up / Down choose a player, A on a controller makes that
controller the player, B goes back.

## In the game switcher

| Button | What it does |
|---|---|
| D-pad Left / Right | Previous / next game. Up / Down do the same. |
| L1 / R1 | First / last game. |
| A | Resume the selected game. In a game: back to it (the running game) or switch to another one. |
| X | Take the selected game off the list. Its saves stay. |
| Y | Star or unstar the selected game. |
| B, SELECT or HOME | Close the switcher. |

## In Options

In **Options** of the pause menu and its pages:

| Button | What it does |
|---|---|
| D-pad Up / Down | Move. The list wraps around at the ends. |
| D-pad Left / Right | Change the selected value. |
| L1 / R1 | On the Emulator page: skip five values in a long list. |
| A | Open a group, or step to the next value. |
| B | Back one level. |

The buttons in Settings and its pages, during setup, on the touch screen and for power and volume are on the next page,
[Controls in Settings, setup and on the touch screen](controls-settings.md).

On experimental **AYN Odin 2 Mini** and **Odin 2 Portal**, the default menu layout is **Xbox** (A bottom,
B right, X left, Y top), matching the printed letters. The original Odin 2 defaults to Nintendo. A saved choice
wins. Safe mode follows the device’s printed layout. Games keep their positional controls: the screenshot
shortcut is HOME + the right face button (printed B on Mini/Portal), regardless of the menu layout.
