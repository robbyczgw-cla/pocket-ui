# The web companion: your library in a browser

Settings → Connections → **Web companion** lets you open the Pocket's library on your phone or computer: browse and search your
games, set a game's status, rating, note and stickers, manage stickers and smart shelves, upload games, BIOS files and
PortMaster files, see your play time and the battery, and look at the Pocket's screen. It works only over
[Tailscale](remote.md), and it is **off** until you switch it on.

> **Status:** switching it on and off, where it listens (only on Tailscale), signing in over a real tailnet and the
> screenshot placeholder were checked on a Mini V2. Pairing a browser with a code, uploading files and using it while
> a game runs were only tried on a computer with a stand-in library. Please tell us what happens.

## Turn it on

1. Set up Tailscale first: Settings → Connections → **Tailscale**, switch it on and sign in ([Tailscale](remote.md)). Your phone or
   computer needs Tailscale too, signed in to the same tailnet.
2. Settings → Connections → **Web companion**. Select **Web companion** and press **A**. It says **On**.
3. Below the list the page shows the address to open, for example `http://pocket-mini`, and the Pocket's Tailscale
   addresses (`http://100.x.y.z`). Open one of them in a browser on your phone or computer.

If the page says **Not listening**, it gives the reason (for example something else is using the address) and tries again
every 30 seconds. If it says **Waiting for Tailscale**, Tailscale is off or not signed in; the companion starts listening by
itself as soon as Tailscale is connected. The short name (`http://pocket-mini`) needs MagicDNS, which Tailscale
switches on by default; the `100.x.y.z` address always works.

The switch is kept across restarts. When it is on, the companion starts a few seconds after the Pocket, so it never
slows down the start.

| Entry | What it does |
|---|---|
| **Web companion** | On or Off. Off by default. Off, nothing listens and nothing runs |
| **Pair a browser** | Shows a one-time code for a browser that needs one (see below). Only while the companion is on |
| **Paired browsers** | The browsers paired with a code, newest first. A, then A again, revokes one |
| **Your Tailscale devices** | **Open** (the default): your own devices on your tailnet get in without a code. **Code**: every browser needs a code, yours too |

## Who gets in

- **Your own devices**: a phone or computer signed in to Tailscale with **your** account opens the companion straight
  away. The Pocket asks Tailscale who is connecting; nothing to type. This means **every** device signed in with your
  Tailscale account, unless it is tagged: including a computer someone else in your family uses with your account, or a
  server you added with your own key. Switch **Your Tailscale devices** to **Code** if that is not what you want; then
  every browser needs a code, and revoking one locks that device out (with Open, a device of your account gets in
  again through Tailscale even after its browser was revoked). A device you take out of your account in Tailscale may
  get in for up to a minute more, or until you switch Your Tailscale devices.
- **Any other browser** on your tailnet (a device shared with you by someone else, a tagged device, or every browser
  when Your Tailscale devices is set to Code) needs a code:
  1. On the Pocket: Settings → Connections → Web companion → **Pair a browser**. A QR code and six digits appear.
  2. Scan the QR code with the phone's camera (it opens the address with the code and pairs by itself), or open the
     address and type the six digits.
  3. The browser stays paired for a year; then it needs a new code. **Forget this browser** at the bottom of the
     page, or revoking it under **Paired browsers** on the Pocket, ends that at once.

A code is valid for five minutes and for one browser. It is gone after five wrong tries, when you leave the code's
page, or when it was used. The Pocket answers at most one try a second. A wrong code, an old one and a try while no
code is showing all get the same answer: **That code is not right, or no code is showing.** Make a new code on the
Pocket then.

## What you can do

- **Library**: your systems with their games, a search over all games, and a page per game with its box art (a letter
  tile when there is none), facts from the offline game list, time played, status, your rating, note and stickers.
  Status, rating (five stars; tap the stars you gave again to take the rating away), note, stickers and favourite
  change on the Pocket at once, as if you had pressed the buttons there. A game hidden on the Pocket
  ([Hide or delete](library.md#hide-or-delete-a-game)) is not listed; shown again, it is back at the next page.
- **Screenshots**: a game's page lists the screenshots you took in that game (HOME + A, see
  [Screenshots](playing.md#screenshots)), newest first. Tap one to open it full size. The page only reads them: it
  cannot delete or add any (use the file share for that). They are not available while a game is running.
- **Shelves**: Favourites, your stickers (rename, delete) and Curator's smart shelves (rename, delete; make them on the
  Pocket with the Lens). A smart shelf lists its games in the order chosen for it on the Pocket. Deleting a sticker or a shelf never deletes a game.
- **Upload**:
  - **Games** go into their system's folder, chosen by the file type (with a question when several systems take it).
    A file the system does not take is refused, with the types it does take.
  - **BIOS files** go into `roms/bios` (NAOMI and Atomiswave files into `roms/bios/dc`). The page lists what the
    Pocket's own emulator still misses, and ticks it off after the upload.
  - **PortMaster files**: for each installed port that waits for your own game files, its instructions and a place to
    drop files or a whole folder. They wait in `roms/port-files/<port>` and move into the port when you start it, as
    with file sharing ([Extras](extras.md)).
  - A file of the same name is only replaced after you say so. An upload that is cut off (the browser closed, the
    network dropped) leaves nothing behind. At most 9 GB per game or port file and 128 MB per BIOS file, and at
    least 256 MB stay free on the SD card, even with several uploads at once. Scripts and programs (`.sh`, `.py`, `.so`,
    `.elf` and any file that starts like a Linux program) are never taken, and nothing goes into the `ports` folder,
    where the Pocket runs programs from.
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
- **Now playing**: while a game runs, its name and since when, and **Screenshot**: a picture of the Pocket's screen,
  at most one new one every 10 seconds. While the Pocket shows Settings or the first-start steps (which can show the
  pairing code, passwords and keys), the screenshot is a grey picture that says it is hidden.

## While a game runs

The game has the Pocket to itself. The companion answers only what costs next to nothing: Now playing and the
screenshot. The lists, edits and new uploads wait until the game ends (the page says so and carries on by
itself). An upload that was already running pauses and continues when the game ends.

## How it is kept safe

- **Only over Tailscale.** The companion listens only on the Pocket's Tailscale addresses, never on the Wi-Fi, so
  devices on the same Wi-Fi network cannot reach it. Tailscale encrypts the connection end to end. There is no
  switch to open it to the local network.
- **Off by default.** While it is off, nothing listens at all. Switching it off closes it at once: an upload under way
  stops and leaves nothing, and what was still waiting is not done.
- **Only you, or a browser you paired.** The Pocket keeps only a fingerprint of each paired browser, readable by the
  system alone; revoking one in Settings ends it at once.
- **Only this page can change things.** Every change needs a token that only the companion's own page has, so another
  website open in the same browser can't change anything on the Pocket. The page loads nothing from the internet.
- **Only the right folders.** Uploads go only into game folders, `roms/bios` and the PortMaster drop folders. Names
  that try to reach other folders (`..`, `/`, links, invisible characters) are cleaned or refused.
- **It stays out of the way.** It runs at the lowest priority, apart from the menus and games, and does next to
  nothing while no browser is connected.

Who on your tailnet may reach the Pocket at all is set in the Tailscale admin console (access controls).

## When something is not right

| What you see | What to do |
|---|---|
| The browser can't connect | Check that Tailscale is on and connected on both devices, and that Web companion says On. Try the `100.x.y.z` address |
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
