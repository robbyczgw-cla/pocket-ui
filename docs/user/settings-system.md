# Settings: Connections and System

The second half of [Settings](settings.md): the **Connections** and **System** categories (Wi-Fi, Bluetooth, Tailscale, Sync, remote access,
the Web companion; the clock, sleep and power, updates, storage, backups, the Tour and this guide). Look & feel,
Playing and Games, and how Settings opens, are on [the first page](settings.md#open-settings).

## Connections

The Wi-Fi page agrees with the Quick settings radio switch: while it is off, the page says so and its first row turns it on again with A. Old scan results are hidden while the radio is off.

Settings → **Connections**. Networks, controllers, and reaching the Pocket from your other devices.

| Entry | What it holds |
|---|---|
| **Wi-Fi** | Connect to a network, switch between saved ones, forget one, join a hidden network, set the Wi-Fi country. See [Getting started](getting-started.md#4-connect-to-wi-fi) |
| **Bluetooth** | Pair a controller, headphones or a speaker and see what is connected. See [Bluetooth](bluetooth.md) |
| **Tailscale** | Reach the Pocket from your other devices over a private network, even away from home. See [Tailscale](remote.md) |
| **Sync** | Keep saves and save states in step with your other devices with Syncthing, and, if you like, your favourites, notes and play history too. See [Sync](sync.md) |
| **Remote access (SSH)** | Log in from a computer. See [Remote access](#remote-access-ssh) |
| **Web companion** | Your library, play time and uploads in a browser on your phone or computer, over Tailscale only. Off by default. See [Web companion](web-companion.md) |

### Tailscale

Settings → Connections → **Tailscale** links the Pocket with your phone, computer and other devices over Tailscale, a private
network that works on any Wi-Fi, a phone hotspot or mobile data. See [Tailscale](remote.md) for the whole page.

1. Select **Tailscale** and press A to switch it on. It stays on after a restart until you switch it off.
2. Select **Sign in** and press A: a QR code appears. Scan it with your phone and approve the Pocket. The page shows
   **Connected as** your tailnet, the Pocket's Tailscale address and its name.
3. **Sign in with a key** is for a key made in the Tailscale admin console. **Name** changes what the Pocket is called on your
   tailnet. **Log out** asks twice.

Tailscale only sets up the network. To log in with SSH over it, switch on Remote access (SSH) as well.

### Remote access (SSH)

Settings → Connections → **Remote access (SSH)**. Remote access is **off** by default, and it is off again after every restart.

1. Press A ("Turn on remote access"). The screen shows `ssh root@<address>`, the user and a password made for this
   Pocket.
2. Log in from your computer with those. The password stays the same for this Pocket.
3. It turns itself off after 30 minutes without an open connection. Press A again to turn it off sooner.

Remote access is meant for people who want to copy files or look at logs. You do not need it to play. See
[SECURITY-HARDENING.md](../SECURITY-HARDENING.md) for what it changes.

## System

Settings → **System**. The clock, sleep and the power button, new versions, what takes up space on the SD card, backups,
the Tour, this guide and what the Pocket is running.

| Entry | What it holds |
|---|---|
| **Time** | Time zone, date and network time. See [Time](#time) |
| **Sleep & power** | When the screen goes off and the Pocket sleeps, when it switches off after sleeping, and what the power button does. The row shows the screen-off time ("2 min >"). See [Sleep & power](#sleep--power) |
| **Updates** | New versions of pocket-ui and the system. See [Updates](updates.md) |
| **Storage** | What takes up space on the SD card, what can go, and the games you hid. The row shows the free space ("41.2 GB free >"). See [Storage](storage.md) |
| **Backup & reset** | Back up pocket-ui's settings and library notes to the SD card, restore a backup, or reset the settings. See [Backup & reset](#backup--reset) |
| **Tour** | Every feature on a card, with a small picture of what it does: switch it on or off, or put it off. See [Features and the Tour](tour.md) |
| **Manual** | This guide, on the device. See [Manual](#manual) |
| **About** | Version, number of games, free space, network address, battery, where the logs are kept, and where the licence texts are (a Licences row when the page has the room: `/storage/pocket-ui/licences`: pocket-ui's licence, the credits, the licences of the programs it is built from and of its fonts; they come with the install package and the SD card image); on a Retroid Pocket 5, Flip 2 or first Mini also which one it is, marked experimental ([Getting started](getting-started.md#other-retroid-devices)). Two rows: **Report a problem** saves a file for a bug report in the `reports` folder of the games share ([Report a problem](troubleshooting.md#report-a-problem)); **Open EmulationStation…** opens ROCKNIX's own menu, after a warning ([Safe mode and EmulationStation](safe-mode.md#opening-emulationstation)) |

### Time

Settings → System → **Time**.

- Choose a time zone by region and city. The list shows each city's offset from UTC and its local time right now.
  A search row (the on-screen keyboard) finds a city by name.
- The clock changes at once on every screen, the time at the top right of Home too ([Home](playing.md#home)).
- **Network time** shows whether the clock is in sync (it needs Wi-Fi). Press A on it to ask again.
- **Date and time** sets the clock by hand, for a Pocket without Wi-Fi. A opens five boxes: year, month, day, hour
  and minute. Left and Right move between them, Up and Down change one (hold to keep going), A sets the clock, B leaves
  it as it was. You can also tap a box, and tap the top or bottom half of the chosen one. The time is the local time
  of your time zone, on the 24-hour clock; under the boxes is the date with its weekday. The day never goes past the
  end of its month (29 February only in a leap year). The clock keeps the time after a restart (if the Pocket can't
  keep it, the message after A says so). Network time stays
  on: once Wi-Fi reaches a time server, it sets the clock again. While network time has set the clock, the row says
  **From network time** and there is nothing to set.
- **A wrong clock is pointed out in orange under the list.** If the date is before 2026 or before the day this version of
  pocket-ui was made (a flat battery can leave the clock far in the past), the line says so. Play history, the night
  light and the dates on your saves follow the clock, so a game played with a wrong clock is filed under a wrong date.
  Set it under Date and time, or turn Wi-Fi on and let network time set it; then the line goes away. When the clock
  is very wrong, Date and time opens at the day this version of pocket-ui was made, so only a few presses are needed.
  A date before that day is not taken. The dates already written stay as they were. A second, milder line appears
  when the clock looks fine but Wi-Fi is off and no server has set it since it started ("it may be a little off");
  it does not appear after you set the clock by hand. The shelf also says it once, about 45 seconds after the start,
  when the clock is clearly wrong and nothing else is on screen.
- Setting the clock adds no play time: a game's time is counted on its own, whatever the clock does. A game that was
  already running when the clock changed keeps its start under the old date.

### Sleep & power

Settings → System → **Sleep & power**: Screen off after, Sleep after, Switch off after sleeping, and the power
button's short press and hold. The rows and their choices are under
[Sleep & power settings](quick-settings.md#sleep--power-settings), with what sleep does.

On experimental Pocket 6 and Nova, screen off replaces Sleep; ROCKNIX controls the fan.
See [Other Retroid devices](getting-started.md#other-retroid-devices).

### Backup & reset

Settings → System → **Backup & reset**.

- **Back up now** copies pocket-ui's settings, every Options page and your library notes (play status, notes, time
  played, favourites, recent games) to a folder named after the date and time, in `/storage/roms/backup/pocket-ui`
  on the SD card. Not your games, saves or states, not the file-sharing password and not your RetroAchievements sign-in.
  Over file sharing the folder is `backup/pocket-ui` in the games share.
- The backups are listed below, newest first, with why they were made (**Backup**, **Before reset**, **Before
  restore**) and their size. **A** on one, then **A** again, restores it: settings and library notes go back to how
  they were then. What you had is backed up first, so a restore can be undone.
- **Reset settings**, then **A** again: the look, appearance, motion, sounds, haptics, LEDs, sleep, Performance & fan,
  Curator and every console's and game's Options go back to their defaults. A backup is made first. Kept: your
  library notes, Sync and Tailscale, your accounts, what the Store installed, and the first start (you land on Home).
- After a restore or a reset pocket-ui starts again by itself, which takes a few seconds.

ROCKNIX's own settings are not part of it, nor are the options PlayStation 2, GameCube, Wii and 3DS keep in ROCKNIX's
settings file.

### Manual

Settings → System → **Manual** is this guide, on the device. It is part of the program, so it always matches the version
you run. The pages are the ones listed on the first page; the title line shows which one you are on.

| Button | What it does |
|---|---|
| D-pad Up / Down, or either stick | Scroll. Held, it goes on by itself; a stick scrolls faster the further you push it. |
| D-pad Left / Right | Scroll a screen up / down. |
| L1 / R1 | Previous / next page. After the last page comes the first. |
| B | Back to Settings → System. |

A swipe on the touch screen scrolls as well. Every page remembers where you were until pocket-ui restarts.
Button names in the text are drawn like the buttons in the hint bar at the bottom of the screen, pictures are
left out, and a link shows only its words. On the first-start screens "Welcome" and "You're all set", SELECT opens
the manual and B comes back to that screen.
