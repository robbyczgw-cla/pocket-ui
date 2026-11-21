# Games from your RomM server

[RomM](https://romm.app) is a self-hosted game library you run on a home server, NAS or PC. Pair your Pocket once and
your RomM games appear right on the shelf beside your local games—complete with RomM's titles, box art and descriptions.
The first time you launch one, pocket-ui downloads it; after that it lives on your SD card like any local game and plays
completely offline.

## Pair the Pocket

1. Open Settings → Games → **RomM**.
2. **Server:** enter your RomM address with the on-screen keyboard, just as you type it in a browser—like
   `192.0.2.10:8080`, `romm.example.com` or `https://games.home/romm` (if you omit `http://` or `https://`, pocket-ui
   assumes `http://`). Press **START** to save.
3. **Pair with QR code:** your Pocket displays a QR code and an 8-letter code. Scan it on your phone, log in to RomM if
   prompted, and approve the Pocket. Your Pocket detects approval automatically and shows **Paired as** with your
   username. *(No camera handy? Open the `…/pair/device` address shown under the QR code and type the 8-letter code.)*
4. pocket-ui pulls your RomM catalogue and puts your games right on the shelf.

QR pairing requires RomM 5.0 or newer. Running RomM 4.8 or 4.9? Use **Pair with a code** instead: in RomM open your
profile → *Client API Tokens*, create a read-only token for your Pocket, click *Pair*, and enter the 8-digit code shown
(valid once for 5 minutes).

pocket-ui never asks for your RomM password. It stores only the read-only token RomM issues (readable only by pocket-ui)
and only reads your games, consoles, files, covers and username.

## Playing a RomM game

RomM games look and feel just like games already on your card. Press A to launch, and a progress bar shows **Getting the
game from RomM, once** ("3 MB / 8 MB"). As soon as the download finishes, the game starts. Press **B** to cancel anytime;
the next launch picks up right where it left off.

- Downloaded games land in their console folder with their server filename (`roms/gba/Game.gba`). Multi-file disc games
  (`.cue` plus tracks) download into their own folder and launch from their `.cue` or `.m3u`.
- Existing files on your SD card are never overwritten, and duplicates with the same filename appear only once.
- Favourites, play time and saves work identically before and after downloading.
- Downloaded games stay on your card even if you unpair or your server goes offline.

## Keeping the list up to date

pocket-ui refreshes your RomM library automatically shortly after booting (when online and idle), or anytime you press
**Refresh the games**. The RomM page shows how many games and consoles were found and when it last synced. Systems your
Pocket doesn't play (like PS5 or PC) stay off the shelf and are counted separately on the page.

## When something goes wrong

Errors show up in plain English right on the RomM page or when a download can't start:

| What you see | What to do |
|---|---|
| Can't reach your RomM server | Check Wi-Fi, make sure the server is running and double-check the address. |
| That address answers, but not as a RomM server | Something else answered at that address (a router or another app). Check the port. |
| RomM doesn't accept this Pocket any more | The Pocket's token was revoked in RomM. Pair it again. |
| This RomM is older than 5.0 | Use **Pair with a code** instead. |
| Not enough free space: this game needs … more | Free up space on your SD card (Settings → System → Storage), then try again. |
| The download stopped | Your connection dropped. Launch the game again to resume where it left off. |
| RomM doesn't have this game any more | Run **Refresh the games**. |

## Unpair

Go to Settings → Games → RomM → **Unpair** and press A again to confirm. Your Pocket forgets the server and token, and
undownloaded server-only games leave the shelf while anything already downloaded stays on your card. You can also revoke
the token inside RomM (*Client API Tokens*), which prompts your Pocket to pair again.
