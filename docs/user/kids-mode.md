# Kids mode

A safe, simple handheld built for kids: one big game case at a time, only the games you choose, and zero settings to
accidentally break. Left and Right pick a game, A plays, and HOME pauses with just **Continue** and **Stop**. Your own
layout, settings and saves stay untouched, and Kids mode stays off until you turn it on.

## Setting it up

During first start, press **Y** ("For a child…") on the welcome screen. Once you've added games, setup walks straight into
Kids mode instead of picking a layout. Already set up? Open Settings → System → **Kids mode**. Setup takes about two
minutes:

1. **Set a PIN:** four digits, entered twice. Keep this one for grown-ups.
2. **Pick the games:** choose 5 to 20 games (or fewer if that's all on the card). Press A to pick and START to continue.
   No games yet? Opening from Settings takes you to Add games first, or press **X: Starter pack** to grab a handful of
   free games over Wi-Fi (B brings you back). During first start, Kids mode setup only runs once there are compatible
   games on the card; otherwise setup continues normally and reminds you to enable it later.
3. **Pick a colour:** **Blue** or **Pink**.
4. **Daily play limit:** anywhere from 30 minutes to 2 hours, or no limit.
5. **Max volume:** caps the hardware volume buttons, great for little ears and headphones.

A note underneath tells you how many games were left out of the list. RetroArch and PortMaster games stay hidden because
they have their own menus a child could get stuck in. PS2, GameCube, Wii, 3DS and Xbox appear on handhelds that support
them. Games tagged **Saves only in the game** don't auto-save a moment on exit; only in-game saves are kept.

## What your child sees

- **The shelf:** one giant game case, big title text, A to play.
- **HOME mid-game:** just **Continue** and **Stop**. Stop saves automatically, so pressing A next time jumps right back
  in.
- **5-minute warning:** five minutes before the daily limit, the game pauses with a clock and "5 minutes left"; A resumes.
- **Time's up:** the game saves automatically and shows "That's all for today!" until tomorrow.
- **Game won't start:** shows a friendly picture asking for a grown-up and returns to the shelf on any button press. It
  never falls back to another emulator here; turn off Kids mode to troubleshoot it.
- **Buttons:** volume works up to your cap; Power shows Sleep, Restart and Power off.

Emulator hotkeys are disabled in Kids mode; HOME is the only way out of a game.

If pocket-ui itself crashes repeatedly, your child sees a friendly picture saying "Please ask a grown-up" instead of the
[safe mode](safe-mode.md) menu. Hold SELECT and START for 5 seconds and enter your PIN to reach safe mode.

## Getting back in

Hold **SELECT and START for 5 seconds** on the shelf or in the pause menu (a progress bar counts down), then enter your
PIN. The grown-ups' screen gives you:

- **30 more minutes today** and **Start today's time again**
- **Games**, **Colours**, **Time per day**, **Loudest volume**, **New PIN**
- **Turn off Kids mode:** puts your full shelf and settings right back as you left them.

If any chosen games were removed from the SD card, the page tells you ("2 of the picked games aren't on the card any
more.") and trims the child's shelf accordingly.

On lock screens ("That's all for today!" or an empty shelf), a small footer reminds you: "Grown-ups: hold SELECT and
START."

### If you forgot the PIN

The PIN screen shows this rescue trick right away: drop an empty file named `kids-mode-off.txt` into the root folder of
the card's ROCKNIX partition and reboot. Any computer or phone with a card reader can create it on the first partition, or,
if File sharing is already enabled over Wi-Fi, drop it into the shared games folder (you can't turn File sharing on while
locked out). Kids mode turns off and clears the PIN; your games, saves and settings stay completely safe.

## Good to know

- **Clock resets:** losing the handheld's clock won't trick the daily timer into starting a fresh day; it keeps counting
  from the last date it knew. Use **Start today's time again** if you want to reset it manually.
- **Shared saves:** your child uses the same save files as you.
- **File sharing:** stays however you left it. Your child can't open its settings page, though anyone on your Wi-Fi with
  the password can still access the share.
- **Kid-friendly, not tamper-proof:** the PIN prevents accidental changes, not a determined kid who knows how to pull the
  SD card.
- Kids mode is not available in pocket-ui for Android.
