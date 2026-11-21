# Achievements (RetroAchievements)

[RetroAchievements](https://retroachievements.org) is a free, community-run site that adds trophies and challenges to
classic games—"beat the first boss", "collect 100 coins" and thousands more. pocket-ui tracks them live while you play
and syncs unlocks straight to your account.

Achievements work across all systems running in pocket-ui's own player (pocket-core)—see
[On pocket-ui's own host](systems.md#on-pocket-uis-own-host). Games running in RetroArch or standalone emulators don't
use pocket-ui's achievement tracker. On Android, pocket-ui's built-in player supports them too: see
[RetroAchievements on Android](android-mvp.md#retroachievements).

## Sign in

1. Create a free account at retroachievements.org on your phone or computer.
2. On your Pocket, open Settings → Playing → **RetroAchievements** → **Sign in**.
3. Type your username with the on-screen keyboard and press **START** (Next).
4. Type your password and press **START** (Sign in). Letters mask as dots with the last character briefly visible.

You need Wi-Fi to sign in—offline, Sign in says "Connect to Wi-Fi first to sign in" before opening the keyboard. Your
password is sent once over HTTPS and never stored; pocket-ui saves only the login token RetroAchievements sends back
(readable only by the system). Wrong username or password? An error appears in red, and pressing A lets you retry with
your username already filled in.

**Log out:** once signed in, the same row turns into **Log out**. Press A twice to confirm and clear the saved token from
your Pocket—everything you've unlocked stays safely on your RetroAchievements account.

## While you play

- When you start a game, pocket-ui hashes the ROM and loads its achievement list in the background so startup stays just
  as snappy. A brief banner pops up once ready ("Achievements: 12 of 40"); games with no RetroAchievements set stay quiet.
- Unlocking an achievement pops up a clean card at the top of the screen for four seconds showing the badge art,
  "Achievement unlocked", its point value and title. Multiple unlocks queue smoothly one after another.
- With **Menu sounds** enabled (Settings → Screen & sound → Menu sounds), a gentle chime plays alongside the pop-up; turn
  Menu sounds off for silent cards.
- Save states, rewind, fast-forward and cheats all stay available—what RetroAchievements calls *softcore* mode. Hardcore
  mode isn't offered because RetroAchievements only accepts hardcore unlocks from officially approved emulators. Loading
  a state or rewinding resets achievement triggers to that state, so loading can't accidentally pop an achievement.

## The pause menu

When signed in and playing in pocket-ui's own player, the pause menu (HOME) adds an **Achievements** row right under
Options ([The pause menu](pause-menu.md)). Locked achievements come first—with live progress counters where supported
("612/1000") or point values—followed by unlocked ones. A dedicated line beneath the title keeps full achievement and
point totals readable even in Retro style or Large text, and the selected achievement's description sits at the bottom.
Press **B** to return.

If a game isn't on RetroAchievements—or you launched it offline for the very first time—the page tells you plainly.

## Without Wi-Fi

- Any game you've launched online at least once caches its achievements locally, so you can keep earning unlocks on a
  flight or commute.
- Offline unlocks save to your Pocket with their timestamp and upload automatically the next time you launch a game on
  Wi-Fi. The pause menu header shows how many are queued ("2 to send").
- A game never launched online before can't fetch achievements offline, and plays completely normally.
- On handhelds without Wi-Fi (RG28XX, RG35XX 2024, R36S), the page explains: "RetroAchievements needs the internet, and
  this handheld has no Wi-Fi."

## On the game card and the console page

- Open any game card (X) while signed in to see **Unlocked: 12 of 40 achievements** under "Your game". For games you
  haven't started yet, pocket-ui fetches the total in the background on Wi-Fi (at most once per hour per game).
- For games whose save file pocket-ui can't read for [where you are](library.md#where-you-are-in-the-game), your unlock
  progress—like **34 of 60 achievements · 56%**—steps in right under the title on the back of the box, beside
  **Continue** and on Where was I? (shown once you're signed in and have at least one unlock; the "Unlocked" row isn't
  duplicated underneath). Numbers are cached for offline viewing and refreshed in the background on Wi-Fi after you play
  (at most once an hour). Softcore unlocks count (including any earned in Hardcore).
- Console pages (X on a system) tally your progress across that whole console under "On this Pocket"—for example
  **Unlocked: 35 of 120 achievements in 3 games**, covering every game you've played or inspected on its card.

## Good to know

- Patched games (fan translations or ROM hacks beside the ROM) are identified by their patched hash, just like RetroArch.
  Some translations and hacks have their own achievement sets; if not, the base game's achievements won't trigger while
  the patch is active ([Patches](patches-msu1.md#saves-and-achievements)).
- Everything stays lightweight: cached lists, unlocked badge icons and your login token live in
  `/storage/.config/pocket-ui/achievements/` and `retroachievements.json`.
- pocket-ui plays nice with RetroAchievements servers: it identifies itself properly, sends only needed requests (game
  data, unlocks and standard two-minute Rich Presence updates), runs one request at a time and backs off automatically on
  errors.
- Changed your RetroAchievements password? You'll see "RetroAchievements: sign in again in Settings"—just sign back in;
  none of your unlocks are lost.
