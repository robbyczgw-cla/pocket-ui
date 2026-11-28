<!-- Android's handheld view: Settings (manual::ANDROID_PAGES "settings"). The rows are the ones Android shows
     (app_settings_groups ANDROID_ROWS). -->

# Settings

**START** on the shelf opens Settings. Up and Down choose a category, **A** opens it, **B** goes back. Each row has a line
under it that says what it does.

| Category | What is there |
|---|---|
| Look | Layout (Shelf, Easy, Split or Cabinet), which systems are on the shelf, cozy extras, appearance, colours, style, text size, motion and the game start animation. |
| Screen & sound | Brightness and volume (the same as the quick settings), menu sounds and case sounds. |
| Controls | Buttons and controller setup. |
| Playing | What happens at the start, Remind me where I was, screen frames and RetroAchievements. |
| Games | Add games, Game check, extras (box art, the starter pack), Curator and Android games. |
| System | Updates, Language, View (handheld or Mobile), Home app, Android extras, Run in background and Kids mode. |
| Help & tools | The first-start setup again, how you like to play, the Tour, this guide and About. |

## Android extras

> **Optional. pocket-ui works fully without this.**

Settings › System › **Android extras** is for players who want pocket-ui to feel even more like the whole handheld. Nothing
changes until you set a step up, and every step can be undone. The first row, **Level**, shows how far you came.

| Level | What you do | What it gives you | How to undo it |
|---|---|---|---|
| **Standard** | Nothing | Everything without special rights: the shelf, the player, Settings › System › **Home app** and **Run in background**, pocket-ui's own volume bar | Nothing to undo |
| **ADB grant** | Once, from a computer: `adb shell pm grant xyz.pocketui.app android.permission.WRITE_SECURE_SETTINGS`, then **A** on the row | The four switches (below). It stays after restarts | Switches **Off**, then `adb shell pm revoke xyz.pocketui.app android.permission.WRITE_SECURE_SETTINGS`, or uninstall pocket-ui |
| **Shizuku** | No computer, Android 11 or newer: get Shizuku, Wireless debugging on, pair and start it, allow pocket-ui | The ADB grant with one **A** (and back), plus the whole system log in **Report a problem** | **A** on **ADB grant** (**Take back**). After a restart start Shizuku again; the switches keep working |
| **Root** | Only if your handheld is already rooted: **A** on **Root (Magisk)**, allow pocket-ui in Magisk | The same as Shizuku, through Magisk | **A** on **ADB grant** (**Take back**) |

**Root is for tinkerers and not needed.** Shizuku covers nearly everything, and today root adds nothing more on this page.
There is no guide to rooting here.

**ADB grant, step by step:** turn on Android's Developer options (Settings › About handheld › tap **Build number** seven
times), switch on **USB debugging**, connect a computer, run the line above (no text back means it worked), then press **A**
on **ADB grant** (**Check again**). It says **Granted**.

**Shizuku, step by step:** on **Shizuku** press **A** (**Get Shizuku**, or scan the code) and install it. Switch on
**Wireless debugging** in Developer options. In the Shizuku app tap **Pairing** and enter the 6-digit code from its
notification, then **Start**. Back here press **A** on **Shizuku** (**Allow**) and choose **Allow all the time**, then **A** on
**ADB grant** (**Grant**). After a restart Android stops Shizuku: **A** on **Shizuku** (**Open Shizuku**), **Start**, no new pairing.

With the grant these switches work all over Android; **Off** puts Android's own value back: **No pop-up notifications**,
**No animations**, **Stay awake while charging**, and on Android 10 and older **Hide system bars**.

**Pause menu in other emulators** needs no level at all: with it on, HOME or SELECT + START opens pocket-ui's pause menu over
a game that pocket-ui started in another emulator app (ARMSX2, Dolphin, Azahar, PPSSPP, DuckStation, hakuX, RetroArch):
Continue, Back to the shelf and the emulator's own menu (save and quit there). With Shizuku or root on Android 13 and newer
it also has **Save state** and **Load state**: the menu presses F2 and F4 in the emulator (on Android 14 and newer only there; on Android 13
pocket-ui checks first that the emulator is in front, but in rare cases another app can get the key). RetroArch has those keys already; for the others open their controls or
hotkey page, choose save state, then pick **Teach the save key (F2)** in pocket-ui's menu (and the same for load). A line at
the bottom says **Saved** once pocket-ui sees a new or changed save state file, **Save key sent** when the key arrived but no new file
showed up, **Not saved**, or **Not sent safely** when pocket-ui can't confirm where the key went (another window just then,
or no answer in time). **A** on the row opens Android's accessibility
page, where you turn "pocket-ui pause menu" on or off (Android 13 and newer may first ask you to allow restricted settings
in pocket-ui's app info).

Some handhelds keep HOME for Android; SELECT + START still works there. The game sees a short SELECT press.

## Case sound credits

The NES door sound comes from "NES door slam.mp3" by florn (freesound.org/s/454468/),
under CC BY 3.0 (creativecommons.org/licenses/by/3.0/). It was trimmed, mixed to mono,
filtered, faded and levelled. The other new recordings are CC0 1.0, by XenosNS (NES),
lartti (tray), ottoojala (jewel case), and ryanharding95 and ewellis (DVD cases).

## Quick settings

**HOME** on the shelf opens quick settings: brightness and volume. Left and Right change them.

## Android games

Settings → Games → **Android games** lists the apps on your device. **A** puts a game on its own shelf, **Android**, beside
your consoles, in a case with its icon on it; **A** again takes it off. "Looks like a game" is only Android's guess: you
decide. On the shelf, **A** starts the game; when you leave it, you are back on the same case. It is the same list as in the
Mobile view, so a game you put there in one view is there in the other. Android games have no save states; they keep their
own progress.

## Updates

Settings → System → **Updates** shows your version and looks for a newer one (only on a network that costs nothing,
never over mobile data). When one is out, the page says
so and offers **Install pocket-ui …** with what is new in it. **A** asks first ("After updating, pocket-ui closes. Open it
again; your games and settings stay."), then downloads it, and Android asks before it installs. **B** while it downloads
stops it. **Check again** looks once more and says "Everything is up to date." or why it couldn't look (no network, for
example). **Check for updates automatically** turns the card below on or off; it is the same switch
as in the Mobile view.

When a newer pocket-ui is out, a card on the shelf says so a few seconds after pocket-ui opens, and again when you come
back to pocket-ui an hour or more later. **A** installs it, **B** closes it. A version you closed is not said again; the
next one is. A card you never closed (pocket-ui was swiped away from Recents first) comes back the next time.

## RetroAchievements

Settings → Playing → **RetroAchievements**: sign in with the on-screen keyboard (the D-pad and **A**). After that, games
pocket-ui's own player plays earn achievements, and unlocks show while you play. pocket-ui keeps only the sign-in key, not
your password.

## About

Settings → Help & tools → **About** shows the version of pocket-ui, how many games you have, the free space and the battery.
