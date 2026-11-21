# Safe mode

pocket-ui watches out for itself when something breaks. If it crashes, it restarts automatically. If it keeps crashing,
it rolls back to the last version that worked. And if that still can't start, it opens **safe mode**, a clean rescue
screen with everything you need to recover. ROCKNIX's own menu only opens if you ask for it or if pocket-ui cannot start
at all.

## What happens when pocket-ui keeps stopping

1. **Auto-restart:** pocket-ui tries starting again up to three times in a row with a brief pause between tries. Once a
   session stays up for two minutes or more, the counter resets.
2. **Automatic rollback:** any version that ran cleanly for two minutes with a picture on screen is marked as known-good.
   If a newer version keeps crashing, pocket-ui automatically boots back into that previous working version and tells you:
   "Version … didn't start properly, so the last version that worked is back." Your games and settings stay untouched.
3. **Safe mode:** if the older version fails too (or there isn't one), pocket-ui opens in safe mode.
4. **ROCKNIX's own menu:** used only as a last resort if even safe mode can't run (for example after an incompatible
   system update). The next time pocket-ui starts, a card explains what happened.

A frozen black screen counts as a crash too: if pocket-ui draws nothing for 45 seconds after launch, it restarts through
the same steps.

## Safe mode

If [Kids mode](kids-mode.md) is active, children see a friendly picture saying "Please ask a grown-up" first. Hold SELECT
and START for 5 seconds and enter your PIN to reach safe mode.

Safe mode boots a stripped-down pocket-ui with no covers, artwork or background extras (Curator, achievements, Sync and
cover downloads all stay off), using the default dark or light appearance you chose. Your games and saves are never
touched.

At the top, **What happened** shows why safe mode opened, the last line of the crash log and the last startup steps. Below
that:

| Choice | What it does |
|---|---|
| **Restart normally** | Launches pocket-ui normally. If it crashes again, safe mode returns. |
| **Use previous version** | Appears when a prior working version exists (version number in brackets). Switches back to it and boots normally, keeping your settings and games. |
| **Reset pocket-ui settings…** | Confirms first, then moves your settings, layout, Curator data, favourites, play history, covers and downloaded extras aside into `/storage/.config/pocket-ui.rescue-<date>`. Nothing is deleted. Games, saves, logs and File sharing / remote passwords stay intact (File sharing turns off). Pick Restart normally afterwards for a clean first-start setup. |
| **Open EmulationStation…** | Opens ROCKNIX's built-in menu after the warning below. |
| **Power off** | Shuts down your Pocket. |

Navigate with the D-pad and A, or tap a row; B cancels any confirmation. Safe mode always matches your handheld's printed
face buttons (A bottom / B right on Odin 2 Mini and Portal, A right / B bottom elsewhere), regardless of Settings → Controls
→ Buttons → [**Button layout**](settings-sound.md#button-layout).

If safe mode still appears even after resetting settings, grab the files listed under
[What to send when something crashes](troubleshooting.md#what-to-send-when-something-crashes).

## Opening ROCKNIX's own menu

ROCKNIX's original menu stays installed as an emergency exit whenever you need it:

- On the shelf, hold **SELECT + START** for one second (or press Escape on a keyboard). Pick from **Cancel**, **Restart
  pocket-ui**, **Report a problem** (saves a diagnostic bundle; see [Report a problem](troubleshooting.md#report-a-problem))
  or **Open EmulationStation**.
- Settings → Help & tools → **About** → **Open EmulationStation…**.
- Inside safe mode: **Open EmulationStation…**.

Each time, pocket-ui warns you what to avoid there with **Cancel** highlighted by default so an accidental A press won't
launch it.

**Skip these two options inside ROCKNIX's menu:**

- **Updates:** on the Retroid Pocket Mini V2, running a ROCKNIX update there also overwrites the bootloader Android needs.
  pocket-ui turns off its built-in update check beforehand. Always update through pocket-ui's [Updates](updates.md) page,
  which blocks unsafe updates.
- **Factory reset:** wipes pocket-ui and all of its settings.

**Getting back to pocket-ui:** simply restart your Pocket (or quit ROCKNIX's menu) and pocket-ui starts right back up.

Note: connecting to a saved Wi-Fi network inside ROCKNIX's menu can sometimes blank out the system's stored password. On
next boot, pocket-ui restores it from the saved network profile automatically so Wi-Fi keeps working
([Wi-Fi dropped and stayed off](troubleshooting.md#questions)).
