# Achievements (RetroAchievements)

[RetroAchievements](https://retroachievements.org) is a free site with achievements for classic games: "beat the
first boss", "collect 100 coins" and the like, made by its community. pocket-ui checks them while you play and
counts what you unlock on your account there.

Achievements work in the games that run on pocket-ui's own emulator host (pocket-core): the systems listed under
[On pocket-ui's own host](systems.md#on-pocket-uis-own-host). Games on RetroArch or a standalone emulator don't get
them from pocket-ui.

## Sign in

1. Make a free account on retroachievements.org (on a phone or a computer).
2. On the Pocket: Settings → Playing → **RetroAchievements** → **Sign in**.
3. Type your user name on the gamepad keyboard and press **START** (Next).
4. Type your password and press **START** (Sign in). It shows as dots, with the last letter you typed readable.

The Pocket needs Wi-Fi for this step: without it, Sign in says "Connect to Wi-Fi first to sign in" before anything is
typed. The password is sent once, over HTTPS, and is not kept anywhere: the site
answers with a sign-in key, and only that key is saved on the Pocket (readable by the system only). If the user name
or password is wrong, the page says so in red; A starts again with your user name filled in.

**Log out**: the same row says **Log out** once you are signed in. Press A twice (the first press asks). This
removes the key from the Pocket. What you unlocked stays on the site.

## While you play

- When a game starts, pocket-ui identifies it from its file and loads its achievements in the background. The game
  starts as fast as without an account; achievements join a moment later. A short note says how many you have
  ("Achievements: 12 of 40"). A game that has no achievements on the site gets no note.
- When you unlock one, a small card appears at the top of the screen for four seconds: the achievement's picture,
  "Achievement unlocked" with its points, and its title. Several unlocks come one after the other.
- With **Sounds** on (Settings → Sound & feel → Sounds), a short chime plays with the card, at the menu sounds' volume. With
  Sounds off, the card comes silently.
- Saving and loading states, rewind, fast-forward and cheats all stay available. RetroAchievements calls this
  *softcore*. Hardcore mode, which forbids loading states, is not offered: pocket-ui's instant resume is a state
  load. After you load a state or rewind, achievements start watching again from that moment, so loading a state
  can't unlock anything by itself.

## The pause menu

In a game on pocket-ui's own host, while you are signed in, the pause menu (HOME) has an **Achievements** entry
right after Options ([The pause menu](pause-menu.md)). It lists
the game's achievements: the ones still locked first, with your progress where the game counts it ("612/1000"),
otherwise their points; then the unlocked ones. The top line says how many you have and how many points. The
selected achievement's description is at the bottom. **B** goes back.

If the game isn't known to RetroAchievements, or the Pocket was offline when the game started and has never seen
this game online, the page says so instead.

## Without Wi-Fi

- A game you have played online before keeps its achievements: the Pocket remembers them from the last time. You
  can unlock achievements offline too.
- Unlocks that the site hasn't confirmed yet are kept on the Pocket and sent the next time a game starts with Wi-Fi,
  with the time you earned them. The pause menu's top line shows how many are waiting ("2 to send").
- A game you have never started online can't load its achievements offline; it plays normally.

## On the game card and the console page

- The game card (X on a game) shows **Unlocked: 12 of 40 achievements** under "Your game" when the game has
  achievements and you are signed in. For a game you haven't played yet, the card asks the site in the background
  (at most once an hour per game) and fills the line in when the answer comes.
- A console's page (X on a system) shows the total over its games under "On this Pocket", for example
  **Unlocked: 35 of 120 achievements in 3 games**. It counts the games pocket-ui has seen: played, or looked up
  on their card.

## Good to know

- A game with a patch next to it (a translation or hack) is looked up as the patched game, as RetroArch does. Some
  translations and hacks have achievements of their own; any other patched game has none, and your original's
  achievements are not counted while the patch is there ([Patches](patches-msu1.md#saves-and-achievements)).
- The data stays small: the achievements of the games you played, their pictures once unlocked, and the key.
  All of it is under `/storage/.config/pocket-ui/achievements/` and `retroachievements.json`.
- pocket-ui follows RetroAchievements' rules for programs: it names itself to the site, asks only what a game
  needs (its achievements, your unlocks, and every two minutes what you are doing in the game, as every
  RetroAchievements emulator does), one request at a time, and waits longer after each failed try.
- If the site doesn't accept the saved key any more (you changed your password), a game shows
  "RetroAchievements: sign in again in Settings". Sign in again; nothing you unlocked is lost.
