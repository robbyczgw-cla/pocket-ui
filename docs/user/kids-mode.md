# Kids mode

A handheld for a child: one big case at a time, only the games you picked, and nothing to break. Left and Right choose, A
plays, HOME pauses with **Continue** and **Stop**. Your settings, look and saves wait for you, and the mode is off unless you choose it.

## Setting it up

At the first start, choose **For a child** on "How do you like to play?". Later: Settings → System → **Kids mode**. It takes two minutes:

1. **A PIN**, four digits, twice. It is for you, not for your child.
2. **The games**, 5 to 20 (fewer if the card has fewer). A picks, START goes on. No games on the card yet? Add games comes
   first, and you pick right after; if none arrive, the first start ends on the page that adds them. That page also has **X: Starter pack**, a handful of free games
   (Wi-Fi needed); B comes back to the games to pick.
3. **Blue or Pink.**
4. **How long per day**: 30 minutes to 2 hours, or no limit.
5. **How loud at most**: the volume keys stop there. Good for small ears and headphones.

A line under the list says how many games aren't offered. RetroArch and PortMaster have menus of their own a child could reach, so their games
stay out. PlayStation 2, GameCube, Wii, 3DS and Xbox are there on the handhelds that play them. A game marked **Saves only in the game**
keeps no moment when your child stops: only what they saved inside the game.

## What your child sees

- **The shelf:** one big case, the name in big letters, A to play.
- **HOME in a game:** Continue and Stop. Stop saves by itself, and A next time goes on from there.
- **Five minutes before the time is up:** the game pauses with a clock and "5 minutes left"; A goes on.
- **When the time is up:** the game saves itself and "That's all for today!" stays until tomorrow.
- **A game that won't start** shows a picture asking for a grown-up, and a press goes back to the shelf. It never starts in another emulator here: check it with Kids mode off.
- **Volume keys** up to your limit. **The power button:** Sleep, Restart and Power off.

The emulators' own shortcuts are off in this mode. HOME is the only way out of a game.

If pocket-ui itself keeps crashing, your child sees a picture and "Please ask a grown-up" instead of the [safe mode](safe-mode.md) page.
Hold SELECT and START for 5 seconds and enter your PIN to get to that page.

## Getting back in

Hold **SELECT and START for 5 seconds** on the shelf or in the pause menu; a bar shows the time. Then your PIN. The grown-ups' page has:

- **30 more minutes today**, and **Start today's time again**
- **Games**, **Colours**, **Time per day**, **Loudest volume**, **New PIN**

If games you picked were deleted or moved off the card meanwhile, the page says so ("2 of the picked games aren't on the card any more."): the child's shelf is shorter by that many.

On the screens that stop your child ("That's all for today!" and an empty shelf), a small line reminds you: "Grown-ups: hold SELECT and START."
- **Turn off Kids mode**: everything is as it was before.

### If you forgot the PIN

The PIN page says this from the first look. Put a file called `kids-mode-off.txt` in the top folder of the card's ROCKNIX drive and
restart. A computer or a phone with a card reader can write it to the card's first partition; on a handheld with Wi-Fi, the file
share's games folder works too, if file sharing is already on (a locked-out grown-up can't turn it on). Kids mode goes off and forgets the PIN. Games, saves and settings stay.

## Good to know

- **The day's time and the clock:** a handheld that loses its clock doesn't start a new day by that: the time goes on counting from the last
  date it knew. If that isn't what you want, use **Start today's time again**.
- **Saves are shared:** your child plays with the same saves as you.
- **File sharing** stays as you left them. Your child can't reach their pages, but anyone on your Wi-Fi with the password can still copy files.
- **The PIN stops accidents, not a determined kid.** Pulling the card gets past it.
- **Not on the Steam Deck** (Steam's own button can't be closed) and not in the Android app.
