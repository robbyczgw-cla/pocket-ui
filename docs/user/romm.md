# Games from your RomM server

[RomM](https://romm.app) is a game server you run yourself, on a home server, a NAS or a computer. Pair the Pocket with
it once, and the games on your RomM stand on your shelf beside the ones on the card, with RomM's names, covers and descriptions. The
first time you start one, the Pocket downloads it; from then on it is on the card like every other game and plays
without the server.

## Pair the Pocket

1. Open Settings → Games → **RomM**.
2. **Server**: type your RomM's address with the on-screen keyboard, the way you open it in a browser:
   `192.0.2.10:8080`, `romm.example.com` or `https://games.home/romm`. Without `http://` or `https://` in front, the
   Pocket uses `http://`. Press **START** when it's typed.
3. **Pair with QR code**: the Pocket shows a QR code and an 8-letter code. Scan the QR code with your phone, sign in to
   RomM if it asks, and approve the Pocket on the page that opens. The Pocket notices by itself and says
   **Paired as** and your name. (No camera at hand: open the page shown under the QR code, `…/pair/device`, and type the code.)
4. The Pocket reads the list of games and puts them on the shelf.

The QR code needs RomM 5.0 or newer. With RomM 4.8 or 4.9, use **Pair with a code** instead: in RomM, open your profile
→ *Client API Tokens*, make a token for the Pocket (reading is enough), press *Pair* beside it, and type the 8-digit code
it shows. A code works once, for 5 minutes.

The Pocket never asks for your RomM password. It keeps the token RomM gave it, readable only by pocket-ui, and it only
asks to read: your games, their systems and files, the covers and your name.

## Playing a RomM game

A RomM game looks like any other on the shelf. Start it, and while the start runs a bar says **Getting the game from
RomM, once**, with how far it is ("3 MB / 8 MB"). When the download is done the game starts. Press **B** to stop the
download; the next start goes on from where it stopped.

- The file goes into its system's folder on the card, with the name it has on the server (`roms/gba/Game.gba`). A game
  that is several files (a disc's `.cue` and its tracks) lands as its folder and starts from its `.cue` or `.m3u`.
- A file that is already on the card is never replaced. A game you already have under the same file name shows once.
- Favourites, play time and saves work as for every game, before and after the download.
- Downloaded games stay when you unpair or the server is gone.

## Keeping the list up to date

The Pocket reads RomM's list again a little while after each start (when it's online and no game runs), and when you
choose **Refresh the games**. The page says how many games and systems it found and when it last read the list.
Games of systems the Pocket doesn't play (a PS5's, a PC's) stay off the shelf; the page counts them.

## When something goes wrong

Every problem is said in plain words where it happens: on the RomM page, or in the note after a start that didn't
begin.

| What you see | What to do |
|---|---|
| Can't reach your RomM server | Check Wi-Fi, and that the server is on and the address is right. |
| That address answers, but not as a RomM server | The address is another web page (a router, another app). Check the port. |
| RomM doesn't accept this Pocket any more | The Pocket's token was removed in RomM. Pair it again. |
| This RomM is older than 5.0 | Use **Pair with a code**. |
| Not enough free space: this game needs … more | Make room on the card (Settings → System → Storage), then start it again. |
| The download stopped | The network dropped. Start the game again: it goes on from where it stopped. |
| RomM doesn't have this game any more | Choose **Refresh the games**. |

## Unpair

Settings → Games → RomM → **Unpair**, and A once more to confirm. The Pocket forgets the server and its token, and the
games that were only on the server leave the shelf. Games you downloaded stay on the card. In RomM you can also remove
the Pocket's token (*Client API Tokens*); the Pocket then says it has to be paired again.
