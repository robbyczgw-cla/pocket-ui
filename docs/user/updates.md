# Updates

Settings → System → **Updates** ("New versions of pocket-ui and the system.") updates pocket-ui over Wi-Fi in one press,
or offline straight from your SD card ([No Wi-Fi? Update from the card](#no-wi-fi-update-from-the-card)).

Your games, saves and settings stay completely untouched. On the TrimUI Brick, unchanged engine files stay in place and
updated files are staged smoothly before the menu opens.

1. Open Settings → System → **Updates**; it checks automatically.
2. When a new release is ready, the pocket-ui row shows it ("0.2.0, 0.2.1 available") and adds **Install pocket-ui …**.
   Press A.
3. pocket-ui downloads, verifies the signature and checksum, installs and restarts automatically ("Installed.
   Restarting…").

Every release is cryptographically signed and checksum-verified ([What makes it safe](#what-makes-it-safe)), and if a
new version fails to boot, pocket-ui automatically rolls back to the previous working version. Offline? You'll see
"Couldn't reach the update server. Check Wi-Fi." Behind a hotel or train login page? It tells you "Connected, but this
network wants a sign-in first. Open Settings › Connections › Wi-Fi." ([Hotel and train Wi-Fi](hotel-wifi.md)). If no
update feed exists yet, it says "No updates published yet. A new version installs like the first one; your games and
settings stay." You can also re-run the installer anytime ([Getting started](getting-started.md)) to upgrade in place.

During a download, pocket-ui shows live status: connecting, switching mirrors, verifying or preparing. If a mirror stalls
for 30 seconds or errors out, it automatically fails over to the next signed mirror. Press **A / Cancel** to stop, or
**Check again** to retry once your connection recovers. Each mirror gets five minutes plus extra time scaled to large
files on steady slow connections; connections that stall out completely end cleanly with "The download is too slow on
this network. Nothing was installed. Try again on a faster Wi-Fi." Every file is verified regardless of which mirror
served it.

Before downloading, pocket-ui verifies you have enough SD card headroom plus space for saves ("The SD card is too full
for this update: it needs … MB more. Nothing was changed."). If your handheld's clock is way off after a dead battery,
HTTPS fails and it explains: "The clock looks wrong, so the update server can't be checked. Set it in Settings › System
› Time, or wait for network time over Wi-Fi."

## No Wi-Fi? Update from the card

No Wi-Fi on your handheld (RG28XX, R36S and clones), or Wi-Fi turned off? Update offline using a single zip file; your
games, saves and settings stay safe just like an online update:

1. Grab the matching offline update zip from [pocketui.xyz/install](https://pocketui.xyz/install):
   - `pocket-ui-update-<version>-rocknix.zip` for pocket-ui OS cards
   - `pocket-ui-update-<version>-trimui.zip` for TrimUI Brick / Smart Pro cards
   - `pocket-ui-update-<version>-darkos.zip` for pocket-ui beta for dArkOS
   Keep it zipped; don't unpack it.
2. Power off, plug the SD card into your computer and drop the zip straight onto the visible partition:
   - **RG28XX:** the **GAMES** drive ([The GAMES drive](games-drive.md))
   - **Other pocket-ui OS cards:** the small first partition, or your TF2 games card (like on R36S)
   - **TrimUI Brick:** the root of the card, right beside the `pocket-ui` folder
   - **dArkOS:** the **EASYROMS** partition
3. Slide the card back in and boot up. A banner on the shelf announces "Update to pocket-ui … found on the card. Your
   games and settings stay." Press **A** to install (plug in a charger first if battery is below 20%) or **B** to skip.
   It restarts automatically ("Installed. Restarting…") and cleans up the zip afterwards.

Want to check manually later? Open Settings → System → Updates → **Look on the card**. Non-Wi-Fi handhelds remind you:
"No Wi-Fi on this handheld: put the update file from pocketui.xyz/install on the card, then choose Look on the card."

Offline zips go through the exact same signature, checksum and rollback safety checks
([What makes it safe](#what-makes-it-safe)). If something isn't right, nothing installs and you get a clear message:

| Situation | Message |
|---|---|
| No file found | No update file on the card. Put the update file from pocketui.xyz/install on the card, then look again. |
| Half-copied or corrupt zip | The update file on the card is incomplete or damaged. Copy it onto the card again. Nothing was installed. |
| Unsigned or third-party file | The update file on the card isn't signed by us. Nothing was installed. |
| Wrong handheld package | The update file on the card is for another kind of handheld. Get the one for this handheld at pocketui.xyz/install. Nothing was installed. |
| Same or older version | The update on the card (0.4.2) isn't newer than this pocket-ui. Nothing was changed. |
| Skipped versions missing files (Brick/dArkOS) | This update needs files that aren't in the update file on the card. Get the newest update file, or update over Wi-Fi. Nothing was installed. |

Each update file prompts once on the shelf. If macOS auto-unzipped it into a folder with the same name, pocket-ui still
accepts it.

## The hint on the shelf

When a new release drops, your shelf notifies you once shortly after boot (Wi-Fi required): "pocket-ui 0.3.0 is ready.
Your games and settings stay." Press **A** to jump to Settings → System → Updates, or **B** to dismiss. Once dismissed,
that version won't nag again until the next release, and it never interrupts a game, first start, Kids Mode or dev
builds. A tiny amber dot stays on Settings → System → Updates until you install.

Background checks run quietly at most once per day (plus once after boot), only on Wi-Fi, never mid-game and only while
idle, backing off if offline. They simply fetch the signed manifest: no account, no telemetry, never auto-installing.
System (ROCKNIX) and port updates never pop up on the shelf.

Prefer to check manually? Toggle **Check for updates automatically** to Off under Settings → System → Updates; **Check
again** still works anytime.

## The Updates page

![Settings → System with the Updates row and the installed version](pictures/updates.webp)

Opening Updates checks automatically (unless auto-check is Off) and displays your installed **pocket-ui** version plus
any newer release ("0.2.0, 0.2.1 available"), followed by:

| Action | What it does |
|---|---|
| **Install pocket-ui …** | Prompts **Install now** (downloads, installs and restarts automatically) or **Later** (B). Requires at least 20% battery or a charger |
| **Check again** | Manually queries the update server |
| **Look on the card** | Scans your SD card for an offline update zip ([No Wi-Fi? Update from the card](#no-wi-fi-update-from-the-card)) |

A progress bar tracks downloads live, with **A** to cancel ("Download cancelled."). Stalled mirrors fail over after 30
seconds, while slow steady Wi-Fi keeps going. Release notes for the new version appear at the bottom, or "Everything is
up to date." if you're already current.

After downloading, the bar tracks each step cleanly: "Step 1 of 4: Downloading", "Step 2 of 4: Unpacking", "Step 3 of 4:
Installing" and "Step 4 of 4: Restarting…" (**A** cancels up until Installing begins; unpacking and installing can take
~30 seconds on slower SD cards).

Development builds display "Development build: install a release package to use app updates." and don't offer OTA
updates (About marks them with `-dev` and commit hash, keeping the TrimUI model in its Device row; bug reports log both).
Install a release package to return to release updates; games and settings stay intact.

Once an update starts writing its final commit to the card, it's too late to cancel; let it finish and reboot while
keeping the battery charged. Didn't see a shelf banner? You may have dismissed it earlier; hit **Check again** anytime.

## What makes it safe

- **Signed releases only:** the update manifest is cryptographically signed; a bad signature aborts immediately ("The
  update list isn't signed by us. Nothing was changed.").
- **Verified downloads:** every file is checked over HTTPS against its signed size and checksum; corrupt downloads are
  discarded ("The download is damaged or was changed. Nothing was installed.").
- **Automatic rollback:**
  1. The new version unpacks beside your working version without touching it.
  2. pocket-ui runs a pre-flight check to confirm the new binary actually launches on your hardware; otherwise it aborts
     ("… doesn't start on this device. Nothing was changed.").
  3. It swaps cleanly and restarts.
  4. If the new build crashes or fails to draw within 20 seconds, pocket-ui automatically restores the previous version
     and tells you ("Version … didn't start properly, so the last version that worked is back.").

The previous version stays on your card until the next update replaces it, alongside the last version that ran smoothly
for two minutes (if everything fails, pocket-ui drops into [safe mode](safe-mode.md)).

On ROCKNIX, the installer stages all new files before switching, so even a power cut mid-install leaves a bootable
interface. Re-running the installer finishes any interrupted install cleanly.

## Android

On Android, open **Updates** in Settings. pocket-ui for Android uses its own signed APK channel over Wi-Fi only (with
progress and **B** to cancel). Android asks once for permission to install unknown apps, then confirms each update. Once
installed, reopen the app; all data and saves carry over. Auto-checks never interrupt gameplay, and active downloads
finish or cancel before launching a game. See [Android updates](android-mvp.md#updates-over-wi-fi).
