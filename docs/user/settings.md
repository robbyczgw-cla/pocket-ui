# Settings

## Open Settings

Press **START** on Home or in a system's game list. Settings is not an entry of the list. Settings opens on five
categories, each with a small icon and a line naming what is in it. Use Up/Down to move and A to open a category: its own short list
of entries, with their values on the right. In that list A opens an entry (or switches it), B goes back to
the categories with the one you came from selected, and B there comes back to where you were.
The values ("Home >" on Wi-Fi, "Shelf · Dark >" on Look, "Auto · Quiet >" on Performance & fan: what games run with)
are short; one that is looked up comes a moment later.

| Category | What is in it |
|---|---|
| **Look & feel** | Look, Language, Text size, Night light, Screen frames, Sounds, Haptics, LEDs, Animations. See [below](#look--feel) |
| **Playing** | On start, Remember where I quit, Button layout, Over the game, Performance & fan, Battery, RetroAchievements, Curator. See [Playing](#playing) |
| **Games** | Add games, From EmulationStation, Starter pack, Extras, Screenshots, Store (only on a device with a store set up), Tools, PC streaming. See [Games](#games) |
| **Connections** | Wi-Fi, Bluetooth, Tailscale, Sync, Remote access (SSH), Web companion. See [Connections](settings-system.md#connections) |
| **System** | Time, Sleep & power, Updates, Storage, Backup & reset, Tour, Manual, Report a problem, About. See [System](settings-system.md#system) |

A page that opens from somewhere else (a hint, a feature card, the pause menu) goes back with B to its category's
list, and from there to the categories, as if you had opened it from Settings. In this guide a place in Settings is
written as its path: Settings → Playing → **Battery** is the Battery entry in the Playing category.

The first-start steps (Look, Wi-Fi, Extras, Add games) are the same pages. Opened from Settings, B returns to
the category's list instead of moving on to the next step. The first start's time zone step has no page of its own here:
Settings → System → **Time** does the same job.

## Look & feel

Settings → **Look & feel**. How pocket-ui looks, sounds and feels in your hands.

| Entry | What it holds |
|---|---|
| **Look** | Choose how the shelf looks, dark or light, how much it moves, with a live preview, and which systems it shows. See [Look](#look) |
| **Language** | **English** or **Deutsch**: [Language](language.md) |
| **Text size** | **Normal** (the default), **Medium** or **Large**: larger text on every screen, fewer rows on a page. A or Left/Right switches it. See [Text size](#text-size) |
| **Night light** | Warmer, less blue colours in the evening, in the menus and in games: **Off** (the default), **On** or **Scheduled**, with a strength. See [Night light](night-light.md) |
| **Screen frames** | Where a game gets a frame when you haven't chosen one: **Off** (the default), **Handhelds** or **Every console**. A or Left/Right switches it. See [Screen frames](#screen-frames) |
| **Sounds** | Soft clicks in the menus. A turns them on or off, Left/Right set how loud. See [Playing](playing.md#menu-sounds) |
| **Haptics** | A very short tick in the menus as the selection moves. A turns it on or off. Off by default |
| **LEDs** | The lights around the sticks: off, one colour, or the battery colours. See [LEDs](#leds) |
| **Animations** | **Cozy**: watch your game slide into the console, and the pages, the box and the menus move a little slower and softer. **Quick**: straight into the game, the animations as short as they get. A or Left/Right switches it. Cozy in the Shelf and Cabinet looks until you choose, Quick in the others. See [Animations](#animations) |

### Look

Settings → Look & feel → **Look** is the first start's look step: a scaled-down picture of your shelf in each look, drawn by
the shelf itself with a few of your systems and one game (with its box art when it has one; on an SD card without
games, a built-in set of "Sample game"s). Left/Right or Up/Down switch between the five looks, and the shelf
behind Settings takes the look at once; A keeps the one shown. See [Playing](playing.md#looks) for what each look
does. On this page **Y** switches the [appearance](#appearance), **L1** the [animations](#animations), **R1** the
[motion](#motion) and **X** opens [Systems on the shelf](#systems-on-the-shelf).

#### Appearance

Under the look's name is the appearance, which **Y** switches. It applies to every look.

- **Dark**, the default: black pages. On the Mini V2's OLED screen black pixels are off, so Dark uses the
  least battery.
- **Light**: warm paper pages with dark text, easier to read in daylight. Lit pixels use power, so Light uses more
  battery than Dark.
- **Automatic**: light from 07:00, dark from 20:00, by the clock of your time zone ([Time](settings-system.md#time)). It changes
  only after a few seconds in which nothing was pressed, with a short fade; during a game it changes unseen.

The choice is saved at once. The pause menu, the power menu and the small messages that appear over the screen
(volume, brightness, notices) stay dark in both, since they sit over the game's picture.

#### Motion

Under the appearance is **Motion**, which **R1** switches (on Settings → Look & feel → Look only; the first start keeps the
default and has the [text size](#text-size) in its place).

- **Full**, the default: the selection glides to the row you move to, lists scroll smoothly, pages slide in,
  the game card rises out of its label, the hints and small messages fade.
- **Reduced**: only fades. Pages and messages still fade in, but nothing slides, grows or glides.
- **Off**: no animation at all; every press shows its result in the next frame.

None of them makes anything slower: a press changes what is selected or opened at once, whichever you choose,
and pressing again in the middle of a movement takes it straight to the new place. The choice is saved at once.

#### Animations

Under the appearance is **Animations** (**L1**; also Settings → Look & feel → Animations), the first start asks it on the same page.

- **Cozy**: when you start a game in the Shelf or the Cabinet look, you watch its case open and the cartridge or disc slide into the console
  (about a second, with a click). Pages, the game's box and the menus take about a third longer and ease in more softly.
- **Quick**: straight into the game; the animations as short as they get.

Until you choose, the look decides: Cozy for Shelf and Cabinet, Quick for Minimal, Card and Split. Once you choose, it stays
yours, whatever look you pick later. With Cozy, a game of pocket-core's is shown once the animation is over, so it starts
about a second later than with Quick (a game that needs longer to load is not held at all); any button skips the rest.
The picture then comes on like an old TV, in a third of a second: dark, a bright line in the middle, and the picture opens out of it
(with Motion on Full; Quick and Reduced skip it, and a button press cuts it short).
**Motion** (Reduced, Off) still overrides it: nothing slides where Motion says so.

#### Systems on the shelf

On the Look page, press **X** ("Systems on the shelf"). The list has every system that has games, each with a tick
box, its console and the number of games. All are ticked by default. When Moonlight is installed, the last row is
**PC streaming**, unticked by default: tick it to have the [PC streaming](pc-streaming.md) entry on Home.

- **A** unticks the selected system: its card leaves Home, and its games leave Home's recent games and the game
  switcher (SELECT). A again puts it back.
- The games themselves stay where they are. **Starred games stay in Favourites**, even those of a hidden system:
  a star is a choice for that one game. A recent game of a hidden system comes back the moment its system is
  ticked again.
- The last ticked system cannot be unticked; the page says why.
- The choice is saved at once (in `settings.json`) and kept across restarts and updates.
- **B** goes back to the Look page.

The first start does not have this list; every system is on the shelf until you change it here.

### Text size

On small screens such as the Anbernic H700 handhelds, the shelf, game cards and menus keep their full layout,
scaled down to fit the screen. If the writing feels small, choose Medium or Large here.

Settings → Look & feel → **Text size** makes the text larger on every screen. A or Right goes on to the next size,
Left back to the one before, and the screen shows the new size at once; the choice is saved at once and kept across restarts and updates.

- **Normal**, the default: the sizes pocket-ui is designed with.
- **Medium**: the text is about an eighth larger everywhere you read it, for when Normal is a little small and Large
  is more than you need. It changes the same things as Large, a step less.
- **Large**: the text is about a quarter larger everywhere you read it: the shelf and the game lists, Settings and
  its pages, the pause menu and its pages, the quick settings, the game card, Curator (the Lens, Echoes, Versions, play
  history), the search and its keyboard, the console pages, notices and this manual. The large titles (a pulled-out
  label's name, the big letter of a jump, the battery figure) are big enough already and stay as they are.

With Medium and Large the rows are taller, so a page shows fewer of them and scrolls a little sooner. A name or value that is
too long for its row ends with "…" and a long text is wrapped, never cut in the middle of a word; where a hint bar has no
room for every label, the last labels give way and all their buttons, including START, stay on screen. A few things keep their size because they
belong to a picture of a fixed size: the titles on the cases' spines and the shelf cards in the Cabinet look, the
pictures on the Tour's cards and the look previews' shrunken screens, and what is drawn over a running game by
pocket-core (the in-game messages and Pixel Path's line over the game).

The first start's look step has the same choice on **R1** (Normal, Medium, Large, and round again), so larger text is
there from the start.
Safe mode uses the size chosen here too.

### Screen frames

Settings → Look & feel → **Screen frames** says which games get a [screen frame](options.md#screen-frame) when you haven't chosen
one yourself. A or Left/Right switches it.

- **Off**, the default: no console starts with a frame, so games show a clean picture on black.
- **Handhelds**: the handhelds (Game Boy, Game Boy Advance, Game Gear and the others) start with frame Auto, the home
  consoles with None.
- **Every console**: every console starts with frame Auto. The home consoles get the monitor surround, the Nintendo DS
  its Twin frame.

A frame saved for a console or a game in the pause menu (Options → Picture, then Save changes) stays as it is either
way. The choice applies from the next game you start.

### LEDs

Settings → Look & feel → **LEDs** is for the lights around the two sticks. They are **off** unless you turn them on here:
ROCKNIX lights them at full brightness out of the box, and pocket-ui switches them off the first time it starts.

- **Lights** has three choices. **Off** keeps them dark. **Colour** makes both sticks glow in one colour. **Battery**
  uses ROCKNIX's own battery colours: green when charged, orange and then red as it runs low, always at full brightness.
- **Colour** (only with Lights on Colour) picks a calm colour: blue, warm, teal, green, violet or rose. **The game's**
  is warm in the menus and takes the colour of the system you are playing while a game runs.
- **Brightness** (also only with Colour) has four steps: 25, 50, 75 and 100%.

Left/Right change the selected row, A moves on to the next choice. The choice takes effect at once, is kept across restarts,
and is the one ROCKNIX applies at the next boot. The lights also go dark while the device sleeps, as ROCKNIX does it.

## Playing

Settings → **Playing**. What happens when you start and quit, how fast games run, the battery, achievements and Curator.

| Entry | What it holds |
|---|---|
| **On start** | What you see when the Pocket starts: **Home** (the default) or **Last game**, straight back into the game you played last. A or Left/Right switches it. See [On start](#on-start) |
| **Remember where I quit** | Whether the pause menu's Quit keeps the moment you are at for the next start: **On** (the default) or **Off**, only the game's own saves. A switches it. See [Remember where I quit](#remember-where-i-quit) |
| **Button layout** | Which face button confirms in pocket-ui's menus: **Nintendo** (the default, A on the right, as printed) or **Xbox** (A at the bottom). A or Left/Right switches it. See [Button layout](#button-layout) |
| **Over the game** | The frame rate, the sensors or Pixel Path's line over every game, or nothing (the default). See [Over the game](#over-the-game) |
| **Performance & fan** | How fast the processor runs and how early the fan starts in games, the fan in the menus, and the temperatures right now. See [Performance and fan](performance.md) |
| **Battery** | Battery history, time left, what a game costs. See [Battery](#battery) |
| **RetroAchievements** | Sign in to retroachievements.org to earn achievements in pocket-core games. See [RetroAchievements](#retroachievements) |
| **Curator** | The Lens and smart shelves, stickers, Echoes, Up next, versions and play history, each on or off. See [Curator](#curator) |

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

### Button layout

Settings → Playing → **Button layout** decides which face button plays A, B, X and Y in pocket-ui's own screens. A or
Left/Right switches it, and the next button you press already works the new way.

- **Nintendo**, the default: A on the right, B at the bottom, X on top, Y on the left, as printed on the Mini V2.
- **Xbox**: A at the bottom, B on the right, X on the left, Y on top, as on an Xbox controller.

Menus only: games and safe mode keep the printed buttons. What it changes and what it leaves alone is in
[Controls](controls.md#button-layout).

### Remember where I quit

Settings → Playing → **Remember where I quit** decides what the pause menu's **Quit** does with the moment you are at. A
switches it.

- **On**, the default: Quit saves the moment ("Saving and quitting…"). The next start of the game, and
  **Last game** under [On start](#on-start), continue from there.
- **Off**: Quit writes only the game's own saves (its save feature, memory cards) and leaves the moment out
  ("Quitting…"). The next start continues from the moment of your last Quit with the switch on, or of the last
  sleep, as if this session had not happened. The details per emulator are in
  [The pause menu](pause-menu.md#the-pause-menu-home).

This is how NextUI and MinUI quit: their in-game menu has one Quit that writes the game's own saves, and they keep a
moment to resume only when the device sleeps or switches off. With the switch off, the Pocket does the same.

The switch applies to pocket-ui's own host and to PlayStation 2, GameCube, Wii and 3DS. **RetroArch games still save
the moment on Quit** with the switch off: ROCKNIX sets RetroArch to save on every exit, and RetroArch has no way to
leave it out. Sleep, Power off and the [game switcher](playing.md#game-switcher) always keep the moment, whatever
the switch says.

### Over the game

Settings → Playing → **Over the game**: what is drawn over every game while you play, **Off** (the default),
**FPS**, **FPS + sensors** or **Pixel Path**; what each shows is in [Over the game](pause-menu.md#over-the-game). Right
or A goes on, Left goes back. A console or game with its own choice under Options → Extras keeps it, and **HOME + Y**
switches to the next level while you play.

### Performance and fan

Settings → Playing → **Performance & fan**: how fast the processor runs and how early the fan starts in games, the fan in the
menus, **Smooth fan**, and the temperatures right now. It has its own page: [Performance and fan](performance.md).

### Battery

Settings → Playing → **Battery** shows the charge now, a time estimate, a graph, and what the games you played cost.

- **Time left.** "About 3 h 20 min left" on battery, "Full in about 45 min" while charging, "Plugged in" on the
  charger when it isn't charging below 95% (the system calls that "full", but the battery isn't), and the rate it is
  based on ("At the current rate: 15% an hour, measured over the last 40 min"). The rate is the drop of the
  charge over the last 20 to 90 minutes, with the newest minutes counting most, so a game you started a while
  ago shows in it. It appears only after 20 minutes on battery without sleep ("Not enough data yet" until then),
  and the first minutes after waking start from nothing: what the battery lost while asleep is not a rate.
  Charging needs about 6 minutes. A battery that changes its reading by a jump (a recalibration) starts the
  measurement again.
- **Graph.** The last 24 hours when it opens (less while the history is younger than that); L1/R1 switch between 3, 6, 12, 24 and 48 hours. The line is green
  while charging. Where the device slept, was off, or pocket-ui was not running, the graph shows a hatched
  stretch and no line across it.
- **Under the graph.** Voltage, current, power and temperature as the battery reports them, and for the games
  played last, how much they cost: "PS2 uses about 9% an hour, 3.6 W", and how long the charge lasts at that
  rate. A system needs 15 minutes on battery for that; time on the charger or asleep does not count.
- **When the screen goes off and the Pocket sleeps** is no longer set here: a line under the graph points to
  Settings → System → [Sleep & power](settings-system.md#sleep--power), where those times are now, with the power button's.

pocket-ui reads the battery once a minute for as long as it runs, even while a game is open, and keeps the
last 48 hours in `battery-history.bin` in its data folder (about 46 kB). What each game used is kept apart, in
`battery-games.json`, for the pause menu's [Battery in this game](pause-menu.md#battery-in-this-game) and the card. A restart keeps it. A damaged file is
replaced by its backup copy, or the history starts again.

### RetroAchievements

Settings → Playing → **RetroAchievements** signs the Pocket in to your free account on retroachievements.org, so the games on
pocket-ui's own host count achievements. See [Achievements](achievements.md) for everything that follows.

1. Select **Sign in** and press A. Type your user name on the gamepad keyboard, START; then your password, START.
   The Pocket needs Wi-Fi for this.
2. The page shows **Signed in as** your name. The password is not kept: only the sign-in key the site answers with.
3. **Log out** asks twice and removes the key. Your unlocks stay on the site.

In the Playing list the row shows your name when you are signed in, "Off" otherwise.

### Curator

Settings → Playing → **Curator** turns the parts of [Curator](curator.md#curator) on or off. A (or Left/Right) switches the selected
row; the line under the list says what it does. Changes apply at once.

| Row | Default | What it does |
|---|---|---|
| **Lens & smart shelves** | On | The Curator row on Home, with the Lens, and your pinned smart shelves |
| **Echoes on the game card** | On | "More like this" on every card |
| **Up next after a game** | Off | When you quit a game you played for a minute or more, one line on the shelf suggests a game of yours like it. A plays it, any other button puts it away; it goes by itself after a few seconds. See [Up next](curator.md#up-next) |
| **Merge versions in mixed lists** | On | A game you own on several systems shows once in Favourites, smart shelves and Echoes, and starts its preferred version |
| **Stickers** | On | The card's Stickers row, stickers in the Curator list and pinned stickers on Home |
| **Games not on this device in Echoes** | Off | Echoes also names a few games you don't have. Names only |
| **Pixel Path in the pause menu** | Off | Adds Pixel Path to the pause menu: how a game's picture and sound reach you, stage by stage, graded in colour. See [Pixel Path](pause-menu.md#pixel-path). The line beside the picture while you play is [Over the game](#over-the-game) → Pixel Path, which doesn't need this switch |
| **Play history** | On | Curator → History and the Year in review: what you played this week, month and year. Kept on this Pocket only. Off: nothing new is recorded, and what was recorded stays. See [Play history](curator.md#play-history) |

Switching a part off hides it; your smart shelves, stickers and preferred versions stay and come back when you
switch it on again.

## Games

Settings → **Games**. Getting games and what runs them onto the Pocket, and the tools around them.

| Entry | What it holds |
|---|---|
| **Add games** | A tile per system with its folder, file types and BIOS needs, file sharing, and folders or disc games it can't read. See [Getting started](getting-started.md#the-systems-grid) |
| **From EmulationStation** | The favourites and ratings in your EmulationStation `gamelist.xml` files: A adds the favourites to Favourites or uses the ratings as My rating, without changing a thing in EmulationStation. Says "No gamelist" when there is none. See [Your EmulationStation gamelists](library.md#your-emulationstation-gamelists) |
| **Starter pack** | A handful of free games to download when the SD card has none: pick one or take them all. Also offered on Home while the SD card is empty. See [Getting started](getting-started.md#nothing-on-the-sd-card-yet-the-starter-pack) |
| **Extras** | Cover art, the Pictures choice (Box art, Box art and screenshots or Off) and PortMaster games. See [Extras](extras.md) |
| **Screenshots** | The pictures you took with HOME + A, newest first, in groups by day, with a small picture each. A opens one full screen, X deletes it. The value at the right is how many there are; with none the page says how to take one. See [Looking at your screenshots](playing.md#looking-at-your-screenshots) |
| **Store** | Emulators, frames, themes and tools. Shown only on a device that has a store set up; there is no public store yet, so you will not see this row. See [Extras](extras.md#store) |
| **Tools** | ROCKNIX's tools without its old menu: the emulators' own settings, RetroArch's menu, gamepad test and calibration, file manager, terminal, and tools installed from a store. See [Tools](#tools) |
| **PC streaming** | Pair a gaming PC for Moonlight on the Pocket itself, see the paired PCs, stream one of their games. See [PC streaming](pc-streaming.md) |

### Tools

Settings → Games → **Tools** starts ROCKNIX's own tools (the emulators' menus, RetroArch's menu, gamepad test and
calibration, file manager, terminal) and tools installed from a [store](extras.md#store). See [Tools](extras.md#tools).

The second half of Settings, [Connections and System](settings-system.md), has its own page: Wi-Fi, Bluetooth, Tailscale, Sync,
remote access, the Web companion, the clock, sleep and power, updates, storage, backups, the Tour and this guide.

On experimental **AYN Odin 2 Mini** and **Odin 2 Portal**, the default menu layout is **Xbox** (A bottom,
B right, X left, Y top), matching the printed letters. The original Odin 2 defaults to Nintendo. A saved choice
wins. Safe mode follows the device’s printed layout. Games keep their positional controls: the screenshot
shortcut is HOME + the right face button (printed B on Mini/Portal), regardless of the menu layout.
