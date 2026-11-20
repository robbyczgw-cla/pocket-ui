# Settings: Playing and Games

The **Playing** and **Games** categories of [Settings](settings.md) (what happens when you start and quit, how a game goes in and what is drawn over it, how fast games run, achievements; your games, their pictures, the shelf's systems and the Curator). Look and how Settings opens are on [the first page](settings.md#open-settings).

## Playing

Settings → **Playing**. What happens when you start and quit, how a game goes in and what is drawn over it, how fast games run, and achievements.

| Entry | What it holds |
|---|---|
| **On start** | What you see when the Pocket starts: **Home** (the default) or **Last game**, straight back into the game you played last. A or Left/Right switches it. See [On start](#on-start) |
| **Continue where I quit** | Whether the pause menu's Quit keeps the moment you are at for the next start: **On** (the default) or **Off**, only the game's own saves. A or Left/Right switches it. See [Continue where I quit](#continue-where-i-quit) |
| **Console boot screen** | Whether a fresh start of a Game Boy, Game Boy Color, Game Boy Advance, PlayStation, Dreamcast, Saturn, Mega-CD or PC Engine CD game runs through the console's own boot screen (needs your own BIOS file): **Off** (the default) or **On**. A or Left/Right switches it. See [Console boot screen](#console-boot-screen) |
| **Remind me where I was** | Back to a game after more than two weeks: when you played it last and your note, while its moment loads. **Off** (the default) or **On**. A or Left/Right switches it. See [Remind me where I was](#remind-me-where-i-was) |
| **Screen frames** | Where a game gets a frame when you haven't chosen one: **Off** (the default), **Handhelds** or **Every console**. A or Left/Right switches it. See [Screen frames](#screen-frames) |
| **Over the game** | The frame rate, the sensors or (with Pixel Path in the pause menu) Pixel Path's line over every game, or nothing (the default). See [Over the game](#over-the-game) |
| **Zoom in the pause menu** | Whether the pause menu's first page lists Zoom: **Off** (the default) or **On**. Zoom is always under Options, and listed here too while Big text is on. A or Left/Right switches it. See [Zoom in the pause menu](#zoom-in-the-pause-menu) |
| **Performance & fan** | How fast the processor runs and how early the fan starts in games, the fan in the menus, and the temperatures right now. See [Performance and fan](performance.md) |
| **RetroAchievements** | Sign in to retroachievements.org to earn achievements in pocket-core games. See [RetroAchievements](#retroachievements) |

### On start

Settings → Playing → **On start** chooses what the Pocket shows when it starts:

- **Home**, the default: the shelf.
- **Last game**: the game you played last opens right away and continues where you left off, like a console woken
  from standby. The shelf shows for a moment and fades into the game's last picture.

To keep a game that went wrong from starting over and over, Last game only opens it when the Pocket was really
started (switched on or restarted), and only when that game ended normally last time. The Pocket shows Home
instead when the game stopped with an error, when pocket-ui stopped unexpectedly and started again, when the
device shut down unexpectedly, or after an update of pocket-ui.

A game you left by **Power off** in the power menu opens again at the next start whatever this setting says,
as described under [Sleep and power](quick-settings.md#sleep-and-power).

### Continue where I quit

Settings → Playing → **Continue where I quit** decides what the pause menu's **Quit** does with the moment you are at. A
switches it.

- **On**, the default: Quit saves the moment ("Saving and quitting…"). The next start of the game, and
  **Last game** under [On start](#on-start), continue from there.
- **Off**: Quit writes only the game's own saves (its save feature, memory cards) and leaves the moment out
  ("Quitting…"). On pocket-ui's own host the next start is as if no moment had been kept; the older moment stays
  under **Load** as **Where I quit**, until a Quit saves a new one. On PlayStation 2, GameCube, Wii and 3DS the next start still
  continues from the moment of your last Quit with the switch on. A moment saved by sleep counts as that last Quit. The
  details per emulator are in [The pause menu](pause-menu.md#the-pause-menu-home).

This is how NextUI and MinUI quit: their in-game menu has one Quit that writes the game's own saves, and they keep a
moment to resume only when the device sleeps or switches off. With the switch off, the Pocket does the same.

### Console boot screen

Settings → Playing → **Console boot screen**: with **On**, a game that starts from the beginning runs through its console's own boot
screen first: the Nintendo logo of the Game Boy, Game Boy Color and Game Boy Advance, the start-up of the PlayStation, the
Dreamcast's swirl, the Saturn's and the Mega-CD's logo, the PC Engine CD's System Card. **Off** is the default and starts the
game at once. A switches it. The game's display or TV switches on as always, and the boot screen comes up on it.

It needs your own BIOS file in the BIOS folder (`roms/bios`; pocket-ui ships none). The file must be the original, of exactly this size:

| Console | File | Size |
|---|---|---|
| Game Boy (and games without Color) | `gb_bios.bin` | 256 bytes |
| Game Boy Color (a game with Color support runs through this one) | `gbc_bios.bin` | 2304 bytes |
| Game Boy Advance | `gba_bios.bin` | 16384 bytes |
| PlayStation | `scph5501.bin` or another usual PlayStation BIOS name (`scph1001.bin`, `scph5500.bin`, …) | 512 KB |
| Dreamcast (Flycast) | `dc/dc_boot.bin` (in a folder `dc` inside the BIOS folder) | 2 MB |
| Saturn | `saturn_bios.bin`, `sega_101.bin` or `mpr-17933.bin` (Beetle Saturn reads only the last two) | 512 KB |
| Mega-CD | `bios_CD_E.bin`, `bios_CD_U.bin` or `bios_CD_J.bin`: the one of the disc's region, as its name says it (`(Europe)`, `(USA)`, `(Japan)`) | 128 KB |
| PC Engine CD | `syscard3.pce` | 256 KB |

Without the file the game starts as before and a short note on the game's first picture names the file it needs. The other
consoles (SNES, Mega Drive, N64 and the rest) have no boot screen to run and start as before. The Dreamcast, Saturn, Mega-CD and
PC Engine CD use your BIOS file for every start once it is there (the last two can't start a disc without it); with **On** no
built-in replacement skips its boot screen. The DS is not among them: its emulators either start the game directly or stop in
the DS's own menu, which waits for a touch.

On Android the handheld view's switch is the same, and the BIOS files come from the folders you chose (Game check copies them).

Only a fresh start runs it, never one that continues a moment: with [Continue where I quit](#continue-where-i-quit) **On** (the
default) the next start picks up the moment Quit kept, so there is no boot screen. For a game that always starts fresh, with the
logo, switch **Continue where I quit** **Off** and set [On start](#on-start) to **Last game**: the Pocket then starts into
the last game through its boot screen. A game's own saves (battery, memory card) carry on as usual.

### Remind me where I was

Settings → Playing → **Remind me where I was**: when a game continues its moment and you last played it more than two
weeks ago, a quiet card shows while the moment loads: when you played it last, how long you have played it, and your note
from the game card if there is one. On pocket-ui’s own host the game stands still at that moment until you press any button. That button just starts play. **Off** is the default; A switches it. See [Where was I?](continue.md#where-was-i).

The switch applies to pocket-ui's own host and to PlayStation 2, GameCube, Wii and 3DS. **RetroArch games still save
the moment on Quit** with the switch off: ROCKNIX sets RetroArch to save on every exit, and RetroArch has no way to
leave it out. Sleep, Power off and [Recent games](playing.md#recent-games) always keep the moment, whatever
the switch says.

### Over the game

Settings → Playing → **Over the game**: what is drawn over every game while you play, **Off** (the default),
**FPS** or **FPS + sensors**, and **Pixel Path** with Settings → Games → Curator → **Pixel Path in the pause menu** on (the
Enthusiast profile turns it on); what each shows is in [Over the game](pause-menu.md#over-the-game). Right
or A goes on, Left goes back. A console or game with its own choice under Options → Speed, rumble, fan keeps it, and **HOME + Y**
switches to the next level while you play. The [Easy](easy.md) look never draws any of it, whatever this says, and leaves the setting alone.

### Zoom in the pause menu

Settings → Playing → **Zoom in the pause menu**: the [pause menu](pause-menu.md) on pocket-ui's own host lists **Zoom** before
Quit. **Off** is the default, because the magnifier is a help for small text and most players never need it: then Zoom is a
row of Options, next to Picture and Controls, and can be a [shortcut](options.md#shortcuts) (HOME + X). While
[Big text](big-text.md#big-text) is on, the row is on the first page whatever this switch says. A or Left/Right switches it.
See [Zoom](big-text.md#zoom).

### Screen frames

Settings → Playing → **Screen frames** says which games get a [screen frame](options-screen.md#screen-frame) when you haven't chosen
one yourself. A or Left/Right switches it.

- **Off**, the default: no console starts with a frame, so games show a clean picture on black.
- **Handhelds**: the handhelds (Game Boy, Game Boy Advance, Game Gear and the others) start with frame Auto, the home
  consoles with None.
- **Every console**: every console starts with frame Auto. The home consoles get the monitor surround, the Nintendo DS
  its DS frame.

A frame saved for a console or a game in the pause menu (Options → Picture, kept for the game or the console) stays as it is either
way. The choice applies from the next game you start.

### Performance and fan

Settings → Playing → **Performance & fan**: how fast the processor runs and how early the fan starts in games, the fan in the
menus, **Smooth fan**, and the temperatures right now. It has its own page: [Performance and fan](performance.md).

### RetroAchievements

Settings → Playing → **RetroAchievements** signs the Pocket in to your free account on retroachievements.org, so the games on
pocket-ui's own host count achievements. See [Achievements](achievements.md) for everything that follows.

1. Select **Sign in** and press A. Type your user name on the gamepad keyboard, START; then your password, START.
   The Pocket needs Wi-Fi for this.
2. The page shows **Signed in as** your name. The password is not kept: only the sign-in key the site answers with.
3. **Log out** asks twice and removes the key. Your unlocks stay on the site.

In the Playing list the row shows your name when you are signed in, "Off" otherwise.

## Games

Settings → **Games**. Getting games and what runs them onto the Pocket, which systems are on the shelf, and the tools around them.

| Entry | What it holds |
|---|---|
| **Add games** | A tile per system with its folder, file types and BIOS needs, file sharing, sending games from your phone (Y), and folders or disc games it can't read. See [Getting started](getting-started.md#the-systems-grid) and [From your phone](phone-upload.md) |
| **Box art & downloads** | Box art, the Pictures choice and Starter pack; PortMaster games on ROCKNIX and TrimUI, and optional Cheats on TrimUI. The row was called Extras. See [Box art & downloads](extras.md) |
| **Game check** | "Will my games start?": goes through your card and says, in plain words, what is missing and what to do. Only looks; it can hide a duplicate from the shelf when you press A. See [Getting started](game-check.md) |
| **Systems on the shelf** | Which systems are on Home, a tick each. The row says "1 hidden" (or 2, 3...) while a system with games is off the shelf: this is the way back. See [Systems on the shelf](#systems-on-the-shelf) |
| **Shelf space** | **Off** (the default), **10**, **20** or **50**: only so many games stand on the shelf, the rest wait in a crate under it. A or Left/Right changes it. See [Shelf space](#shelf-space) |
| **Curator** | The Lens and smart shelves, Echoes, versions and play history, each on or off. See [Curator](#curator) |
| **Screenshots** | The pictures you took with HOME + A, newest first, in groups by day, with a small picture each. A opens one full screen, X deletes it. The first row, **My shelf as a picture**, makes one square picture (1440 × 1440, in the colours of your page, whichever look the shelf is in) of your shelf to post: up to 20 of your games, the favourites, stickered and box-art ones first, with the trophy, the plant and a small pocket-ui mark. A shelf with no game says so. The value at the right is how many there are; with none the page says how to take one. See [Looking at your screenshots](playing.md#looking-at-your-screenshots) |
| **From EmulationStation** | The favourites and ratings in your EmulationStation `gamelist.xml` files: A adds the favourites to Favourites or uses the ratings as My rating, without changing a thing in EmulationStation. Says "No gamelist" when there is none. See [Your EmulationStation gamelists](library-files.md#your-emulationstation-gamelists) |
| **Android games** | Only in pocket-ui for Android: the apps on your device, A puts a game on its own Android shelf beside your consoles or takes it off. The same list as the Mobile view's. |
| **RomM** | The games on your own RomM server, on the shelf: type its address, pair with a QR code (or a code on RomM 4.8 and 4.9), and each game downloads the first time you start it. The value says how many games it found, or "Not paired". Refresh the games, Unpair. See [Games from your RomM server](romm.md) |

### Systems on the shelf

Settings → Games → **Systems on the shelf** . The list has every system that has games, each with a tick
box, its console and the number of games. All are ticked by default. When Moonlight is installed, the last row is
**PC streaming**, unticked by default: tick it to have the [PC streaming](pc-streaming.md) entry on Home.

- **A** unticks the selected system: its card leaves Home, and its games leave Home's recent games and the game
  switcher (SELECT). A again puts it back.
- The games themselves stay where they are. **Starred games stay in Favourites**, even those of a hidden system:
  a star is a choice for that one game. A recent game of a hidden system comes back the moment its system is
  ticked again.
- The last ticked system cannot be unticked; the page says why.
- The choice is saved at once (in `settings.json`) and kept across restarts and updates.
- **B** goes back to the Settings list.

The first start does not have this list; every system is on the shelf until you change it here.

When no system has games yet, the page says so and shows how they come: once games are on the SD card, their systems
show up here.

### Shelf space

Settings → Games → **Shelf space** is for a smaller pile of games to choose from: only 10, 20 or 50 games stand on the
shelf, and the rest wait in the **crate**, cardboard boxes on the floor under it, one per console. **Off** (the default):
every game stands on the shelf, as before.

- **Switching it on** asks which games go on the shelf: **The games I played last** or **My favourites** (then the others
  by name, until the shelf is full). Cancel leaves it off.
- **Home** gets a **Crate** row before your systems ("12 games in the crate, the shelf is full"). The systems' rows show
  only the games on the shelf; a system with none of its games on the shelf is not on Home. Search still finds every game.
- **In the crate** (A on Crate): Left/Right goes through the boxes; the selected case comes up out of its box with its
  cover above it. **A** takes it out. With a free place it goes straight onto the shelf. With the shelf full the shelf
  shows over the boxes and the game you played longest ago is picked to make room; Left/Right picks another, **A**
  swaps them: the old one goes into its box, then the new one takes its place on the shelf. **B** cancels.
- **On a game's card**, above Hide or delete at its end: **Into the crate** puts the game into the crate and leaves its place free.
- Switching between 10, 20 and 50 keeps what is on the shelf (as far as it fits; more places fill up with the games you
  played last). **Off** puts every game back on the shelf; switched on again, the shelf is as you left it.
- Games you copy onto the card later go into the crate. A game deleted from the card frees its place.
- The crate with nothing in it shows its empty box and says that every game is on the shelf.

It works in the Shelf, Split and Cabinet layouts (the crate looks the same in each); the Easy layout keeps every game in
its one row. Where the shelf is kept: `shelf_space.json` in the data folder.

### Curator

Settings → Games → **Curator** turns the parts of [Curator](curator.md#curator) on or off. A (or Left/Right) switches the selected
row; the line under the list says what it does. Changes apply at once.

| Row | Default | What it does |
|---|---|---|
| **Lens & smart shelves** | Off | The Curator row on Home, with the Lens, and your pinned smart shelves. On with Enthusiast, and on a Pocket that was set up before this became a choice |
| **Just Play on Home** | On | The row on Home: A starts a game that suits you now, X shows it first with the reason ([Just Play](curator.md#just-play)). Off after an update only if the Lens was off before |
| **Echoes on the game card** | Off | "More like this" on every card |
| **Merge versions in mixed lists** | Off | A game you own on several systems shows once in Favourites, smart shelves and Echoes, and starts its preferred version |
| **Games not on this device in Echoes** | Off | Echoes also names a few games you don't have. Names only |
| **Pixel Path in the pause menu** | Off | Adds Pixel Path to the pause menu: how a game's picture and sound reach you, stage by stage, graded in colour. See [Pixel Path](pause-menu.md#pixel-path). It also adds the Pixel Path level to [Over the game](#over-the-game), the line beside the picture while you play; switched off, that level draws the FPS line |
| **Play history** | On | Curator → History and the Year in review: what you played this week, month and year. Kept on this Pocket only. Off: nothing new is recorded, and what was recorded stays. See [Play history](curator.md#play-history) |

Switching a part off hides it; your smart shelves and preferred versions stay and come back when you
switch it on again.
