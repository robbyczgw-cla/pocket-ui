# Safe mode and EmulationStation

pocket-ui looks after itself when something goes wrong. If it stops, it starts again. If it keeps stopping, it tries
the last version that worked. If that does not help either, it opens in **safe mode**: a calm page with only what you
need to get going again. ROCKNIX's own menu, EmulationStation, only opens when you ask for it, or when pocket-ui
cannot start at all.

## What happens when pocket-ui keeps stopping

1. **It starts again**, up to three times in a row, with a short pause each time. After a long run (two minutes or
   more) the count starts afresh.
2. **The last version that worked comes back.** Every version that ran for two minutes and showed its screen counts
   as one that works. If the current version keeps stopping and an older one worked, the older one starts. The next
   screen says so: "Version … didn't start properly, so the last version that worked is back." Your games and
   settings are not affected.
3. **Safe mode.** If that version stops too (or there is no other), pocket-ui starts in safe mode.
4. **EmulationStation**, only if even safe mode cannot start: for example after a system update that pocket-ui does
   not work with. At the next start of pocket-ui a card says why it happened.

A screen that stays black also counts: if pocket-ui shows nothing for 45 seconds after it starts, it is ended and
started again in the same way.

## Safe mode

Safe mode starts pocket-ui with only what this page needs. It shows no covers or other pictures and runs no extras
(Curator, achievements, Sync, the Store, cover downloads), and it uses the standard look in dark or light, as you set
it. Your games and saves are not touched.

At the top, **What happened** says why safe mode started, the last line of the crash report, and the last steps of
the start. Under it:

| Choice | What it does |
|---|---|
| **Restart normally** | Starts pocket-ui as usual. If it fails again, this page comes back. |
| **Use previous version** | Shown only when there is one: goes back to the version that ran before (its number is in brackets), then starts it as usual. Your settings and games stay. |
| **Reset pocket-ui settings…** | Asks first. Moves pocket-ui's settings aside: your look, Curator, favourites, play history, covers and what the Store installed go to a folder next to them, `/storage/.config/pocket-ui.rescue-<date>`. Nothing is deleted. Games, saves, the logs and the passwords for file sharing and remote access stay. Then choose Restart normally: pocket-ui starts like new, with the first-start setup. |
| **Open EmulationStation…** | ROCKNIX's own menu, after the warning below. |
| **Power off** | Switches the Pocket off. |

Use the D-pad and A, or tap a row. B closes a question without doing anything. Safe mode follows the device’s printed buttons: A bottom and B right on Odin 2 Mini/Portal, A right and B bottom elsewhere, whatever Settings → Playing → [Button layout](settings.md#button-layout) says.

If safe mode comes back after a reset as well, send the files described in
[What to send when something crashes](troubleshooting.md#what-to-send-when-something-crashes).

## Opening EmulationStation

EmulationStation stays installed as an emergency exit. You open it on purpose:

- On the shelf, hold **SELECT + START** for one second (or press Escape on a keyboard). A question opens:
  **Cancel**, **Restart pocket-ui**, **Report a problem** (saves the file for a bug report, see
  [Report a problem](troubleshooting.md#report-a-problem)) or **Open EmulationStation**.
- Settings → System → **About** → **Open EmulationStation…**.
- In safe mode, **Open EmulationStation…**.

Each time pocket-ui first says what to avoid there, and Cancel is selected, so A pressed by mistake does nothing.

**In EmulationStation, do not use these two things:**

- **Updates.** On the Mini V2 a ROCKNIX update started there also rewrites the bootloader that Android needs.
  pocket-ui switches EmulationStation's own update check off before it opens it. Update through pocket-ui's
  [Updates](updates.md) page instead, which refuses such updates.
- **Factory reset.** It deletes pocket-ui and its settings.

**To come back to pocket-ui**, restart the Pocket (or quit EmulationStation). pocket-ui is the start screen again.

A connection to a saved Wi-Fi network made in EmulationStation can leave the system's copy of the password empty.
pocket-ui puts it back from the network's saved profile at its next start, so the network keeps working
([Wi-Fi dropped and stayed off](troubleshooting.md#questions)).
