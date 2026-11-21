# The web companion: your library in a browser

Settings → Connections → **Web companion** opens your handheld's library right in your phone or computer's browser:
browse and search games, edit play status, ratings, notes and smart shelves, upload ROMs, BIOS files and PortMaster
data, check play time and battery, and view your Pocket's screen. It matches your handheld's language (Settings → System
→ Language), including dates and file sizes. Works on home Wi-Fi or over [Tailscale](remote.md), and stays **off**
until you switch it on.

> **Status:** toggling, Tailscale listening, signing in over a real tailnet and the hidden screenshot placeholder were
> verified on a Mini V2. Home Wi-Fi pairing, file uploads and in-game access were verified in desktop tests with a mock
> library. Let us know how it runs on your handheld.

Just want to drop a few ROMs from your phone without enabling the web companion? Press Y inside Add games
([From your phone](phone-upload.md)) for a one-off upload page that closes as soon as you leave.

## Turn it on

1. Connect to Wi-Fi (Settings → Connections → **Wi-Fi**) on the same network as your phone or PC, or connect both to the
   same [Tailscale](remote.md) tailnet when away from home.
2. Open Settings → Connections → **Web companion**, highlight **Web companion**, and press **A** to turn it **On**.
3. Open one of the addresses shown below the list in your browser: your Wi-Fi IP (e.g. `http://192.168.1.42`), local
   hostname (e.g. `http://sm8250.local`), or Tailscale name (`http://pocket-mini`).
4. Browsers on Wi-Fi pair once with a code: open **Pair a browser** for a QR code and six-digit code (see
   [Who gets in](#who-gets-in)).

If the screen reads **Not listening**, it explains why (such as another service using the port) and retries every 30
seconds. **Waiting for Wi-Fi** means neither Wi-Fi nor Tailscale is connected yet; the companion starts automatically
once you reconnect, and follows network switches within 30 seconds. `.local` names work on most phones and PCs (some
Android phones don't support them), while numeric IP addresses always work (the Brick shows IP addresses only).
Tailscale hostnames require MagicDNS (on by default in Tailscale); `100.x.y.z` always works.

The toggle persists across reboots and starts a few seconds after boot so it never slows down startup.

| Entry | What it does |
|---|---|
| **Web companion** | On or Off (off by default). When off, nothing runs or listens |
| **Pair a browser** | Displays a one-time QR code and six digits for pairing (visible while On) |
| **Paired browsers** | Lists paired browsers, newest first. Press A twice to revoke one |
| **Your Tailscale devices** | **Open** (default): devices on your own Tailscale account enter without a code. **Code**: every browser requires pairing. Wi-Fi browsers always need a code |

## Who gets in

- **Home Wi-Fi browsers** pair once using the six-digit code (or QR code) and stay paired. Anyone on your Wi-Fi who can
  read the Pocket's screen can pair, just like with File sharing.
- **Your own Tailscale devices** open the companion directly via your Tailscale name or `100.x.y.z` address (the Wi-Fi
  IP still asks for a code). Your Pocket checks identity with Tailscale automatically. Note that this covers **every**
  untagged device signed into your Tailscale account, including shared family machines or servers. Switch **Your
  Tailscale devices** to **Code** if you want every browser to pair explicitly (otherwise a revoked device on your
  account can reconnect over Tailscale). A device removed from your tailnet may keep access for up to a minute unless
  you toggle the setting.
- **Other browsers** on your tailnet (shared devices, tagged devices, or all browsers under **Code**) pair like this:
  1. Open Settings → Connections → Web companion → **Pair a browser** to display a QR code and six digits.
  2. Scan the QR code with your phone (opens and pairs automatically), or open the URL and enter the six digits.
  3. Pairing lasts one year. Click **Forget this browser** at the bottom of the page, or revoke it under **Paired
     browsers** on your Pocket, to end access immediately.

Pairing codes last five minutes for one browser and expire after five wrong attempts, after use, or when you leave the
pairing screen. The Pocket rate-limits guesses to one per second and gives the same error (**That code is not right, or
no code is showing.**) for wrong, expired or inactive codes. Just generate a fresh code if needed.

## What you can do

- **Library**: your systems with their games, a search over all games, and a page per game with its box art (a letter
  tile when there is none), facts from the offline game list, time played, status, your rating and note.
  Status, rating (five stars; tap the stars you gave again to take the rating away), note and favourite
  change on the Pocket at once, as if you had pressed the buttons there. A game hidden on the Pocket
  ([Hide or delete](library.md#hide-or-delete-a-game)) is not listed; shown again, it is back at the next page.
- **Box art.** Open a game and tap **Choose cover…**. Pick a picture from your phone or computer. Your browser
  shrinks it to at most 1024 pixels on the longest side and sends a PNG of up to 8 MB. **Reset to automatic**
  brings back the Pocket’s usual cover, if it has one. The Pocket keeps a copy of a replaced or reset custom cover.
- **Photo album and screenshots**: open **More → Photo album & screenshots** to browse all album photos (Photos of big
  moments, `<data>/album/`) and screenshots (HOME + A in a game, plus shelf pictures) in one place, or view a game's
  own album and screenshots on its page. You can view any picture full size, download it individually, delete
  individual pictures (with confirmation), or download everything (or a single game's pictures) as a ZIP archive. Not
  available while a game is running.
- **Software update**: open **More → Software update** (or tap the version in the footer) to see your current version
  and any newer release (for example **0.6.0, new: 0.6.1**), check for updates, and start **Install** with live step
  progress. Updates come straight from pocket-ui, keep your games and settings, and never start while a game is
  running.
- **Shelves**: Favourites and Curator's smart shelves (rename, delete; make them on the
  Pocket with the Lens). A smart shelf lists its games in the order chosen for it on the Pocket. Deleting a shelf never deletes a game.
- **Upload**:
  - **Games** go into their system's folder, chosen by the file type (with a question when several systems take it).
    A file the system does not take is refused, with the types it does take.
  - **BIOS files** go into `roms/bios` (NAOMI and Atomiswave files into `roms/bios/dc`). The page lists what the
    Pocket's own emulator still misses, and ticks it off after the upload.
  - **PortMaster files**: for each installed port that waits for your own game files, its instructions and a place to
    drop files or a whole folder. They go straight into the port's folder, `roms/ports/<port>/<folder>`, as with file
    sharing ([Box art & downloads](extras.md)); scripts and programs are refused.
  - A file of the same name is only replaced after you say so. An upload that is cut off (the browser closed, the
    network dropped) leaves nothing behind. At most 9 GB per game or port file and 128 MB per BIOS file, and at
    least 256 MB stay free on the SD card, even with several uploads at once. Scripts and programs (`.sh`, `.py`, `.so`,
    `.elf` and any file that starts like a Linux program) are never taken, and nothing goes into the `ports` folder,
    where the Pocket runs programs from.
- **Saves**:
  - On a game's page: its saves (the game's own save, the cartridge clock, a memory card) and its save states
    (Continue, Before load, Slot 1 to 8), each with its time and size and a **Download** button, and **All of this
    game as a ZIP**.
  - **Put a save back** on the game's page: drop a save (`.srm`, `.sav` …) or a save state (`.pstate`, `.state` …).
    The Pocket names it after this game, so a save from another device or with another name fits (`Emerald.srm`
    becomes `Pokemon - Emerald Version (USA).srm`). Before it replaces a save or state, the page asks, and the one that
    was there goes into a saves backup first: Settings › System › Backup & reset lists it as **Before restore**, and
    can put it back. Saves move well between devices; save states only work with the same emulator. Only while no game
    runs: a game that starts during the upload ends it and nothing changes.
  - The **Saves** tab: how many saves and states the Pocket has, **Download all saves (ZIP)** and **Saves and states
    (ZIP)**, with the folders of the SD card inside (`roms/gba/…`), so they can be copied back as they are. A
    download stops when a game starts.
  - **Pocket Sync** on the same tab, read only: each paired Pocket with how it stands (**Synced · 3 min ago**, **Not
    reachable since 14:05**), the last saves that came and went, and the saves played on two Pockets that wait for
    your choice. Pairing and choosing stay on the Pockets (Settings › Connections › Pocket Sync).
- **Stats**:
  - **Play history** (the same numbers as Curator → History on the Pocket, see [Your library](curator.md#play-history)):
    **Week**, **Month** or **Year** with a bar per day (per week for a year), the time, sessions and days played, the
    most played games, the systems, your longest run of days in a row, games you came back to after a long break,
    games you marked completed, and the last twenty sessions. A session without time is one that did not end normally
    (the game crashed or the battery ran out). A period from before the history began says **Not recorded**; if part
    of the history file could not be read, the page says so. With Play history switched off (Settings → Games → Curator), or
    before anything was recorded, the page says that instead.
  - **All time, by game**: time played in all, sessions, games played, time per system and the most played games,
    as the library keeps them for every game (including from before the play history began).
  - The battery over the last 48 hours.
- **Diagnosis.** At the bottom of the companion, open **Diagnosis** and tap **Download diagnosis ZIP**.
  It contains the app version, the device model and recent app events. The package leaves out Wi-Fi passwords,
  tokens, pairing details and file paths. If there are no shareable events yet, the page and the package say so.
- **Now playing**: while a game runs, its name and since when, and **Screenshot**: a picture of the Pocket's screen,
  at most one new one every 10 seconds. While the Pocket shows Settings or the first-start steps (which can show the
  pairing code, passwords and keys), the screenshot is a grey picture that says it is hidden.

## While a game runs

The game has the Pocket to itself. The companion answers only what costs next to nothing: Now playing and the
screenshot. The lists, edits, saves and new uploads wait until the game ends (the page says so and carries on by
itself). An upload that was already running pauses and continues when the game ends.

## How it is kept safe

- **Your Wi-Fi and Tailscale, nothing else.** The companion listens on the Pocket's Wi-Fi address and its Tailscale
  addresses. It takes browsers only from your home network or your tailnet, never from the internet, and on the Wi-Fi
  every browser needs the code. Over Tailscale the connection is encrypted end to end. **On the Wi-Fi it is not
  encrypted:** someone else on the same Wi-Fi could read the code or what the page sends. Use it on your own Wi-Fi at
  home; on a Wi-Fi you don't trust (a hotel, a café), switch it off or use Tailscale.
- **Off by default.** While it is off, nothing listens at all. Switching it off closes it at once: an upload under way
  stops and leaves nothing, and what was still waiting is not done.
- **Only you, or a browser you paired.** The Pocket keeps only a fingerprint of each paired browser, readable by the
  system alone; revoking one in Settings ends it at once.
- **Only this page can change things.** Every change needs a token that only the companion's own page has, so another
  website open in the same browser can't change anything on the Pocket. The page loads nothing from the internet.
- **Only the right folders.** The Upload tab sends files to game folders, `roms/bios` and the folders of PortMaster
  games that wait for your files. Names that try to reach other folders are cleaned or refused. Own cover images
  go into the Pocket’s covers folder and belong to the game you opened.
- **It stays out of the way.** It runs at the lowest priority, apart from the menus and games, and does next to
  nothing while no browser is connected.

Who on your tailnet may reach the Pocket at all is set in the Tailscale admin console (access controls).

## When something is not right

| What you see | What to do |
|---|---|
| The browser can't connect | Check that Web companion says On and that the phone is on the same Wi-Fi as the Pocket (not on mobile data or a guest network). Try the numbers (`http://192.168…`) instead of the `.local` name. Over Tailscale: Tailscale on and connected on both devices; try the `100.x.y.z` address |
| **Not listening** on the Pocket | The line below it says why; the companion tries again every 30 seconds |
| **Play history is off, or nothing has been recorded yet** on Stats | Switch on Settings → Games → Curator → Play history, and play a game; the library's totals show in the meantime |
| **Open the companion with the Pocket's own name or address** | Open it with the address the Pocket shows, not another name |
| **Pair this browser first** | Pair it (above), or set Your Tailscale devices to Open for your own devices |
| **That code is not right, or no code is showing** | Check the digits on the Pocket, and that its code page is still open; after five wrong tries make a new code |
| **The page is out of date. Reload it.** | Reload the page |
| **A game is running** | Quit the game; the page carries on by itself |
| **Not enough space on the SD card** | Free some space; 256 MB always stay free |
| A game you uploaded is not on Home yet | It appears when you are on Home or in Settings, not during a game |

The companion's settings are in `web.json`, the paired browsers in `web/` ([Troubleshooting](troubleshooting.md)).
Backup & reset keeps `web.json` with the other services and never copies the paired browsers.
