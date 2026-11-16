# Hardcore: the user guide's section, kept until it is offered

*Part of [HOST.md](../HOST.md#retroachievements-srcachievements). Hardcore is built and tested but hidden
(`achievements::hardcore::OFFERED` is false): the RetroAchievements server treats pocket-ui as an unknown emulator and
counts no hardcore unlock from it until RetroAchievements has approved pocket-ui. The day it does, set `OFFERED` to
true, put the section below back into docs/user/achievements.md (after "While you play"; its first bullet there points
to it again), add step 4 to Settings → RetroAchievements in docs/user/settings.md, the Hardcore row to the table in
docs/user/pause-menu.md (and its case to `doc_checks::the_pause_menu_table_matches_the_menu_of_each_emulator`), and
the Hardcore paragraph to docs/user/android-mvp.md.*

## Hardcore (for docs/user/achievements.md)

RetroAchievements' **Hardcore** mode counts an unlock only when you played the game straight, and the site shows
those unlocks apart from softcore ones. Switch it on in Settings → Playing → RetroAchievements → **Hardcore** (A or
Left/Right). It holds for every game until you switch it off, and it is kept with your sign-in.

While Hardcore is on, in games that have achievements on the site:

- **No save states:** Save, Load and Earlier leave the pause menu. One line there says why:
  "Hardcore: no save states, rewind or cheats. Save in the game before you quit."
  The Save and Load shortcuts say "Save states are off in Hardcore" and "Loading states is off in Hardcore".
- **No rewind, no cheats:** their rows (and the state shortcuts) leave Options, the rewind shortcut says it is off,
  and the game's switched-on cheats wait until Hardcore is off.
- **Fast-forward stays:** RetroAchievements allows speeding a game up in Hardcore (as RetroArch does), only slowing
  it down, stepping it frame by frame, rewinding, states and cheats are out.
- **Continue starts from the in-game save.** A game starts as after power-on and loads its own save (battery save,
  memory card), never a moment. Its case on the shelf stays closed, the game card says "Hardcore: starts from the
  in-game save", and when a moment was waiting the game says "Hardcore: started from the in-game save". The moment
  itself stays: with Hardcore off again, **Load** finds it under Where I quit.
- **Quit** keeps no moment (it reads **Quit**, "Doesn't save where you are. In-game saves are kept."). Saving inside
  the game works as always.
- **Pausing right after a pause** waits until the game has played a moment (a third of a second at first, longer if
  you keep pausing), as RetroAchievements asks of every emulator, so the menu can't step through a game frame by frame.

**During a game:** the pause menu's **Achievements** page starts with a **Hardcore** row. Off → A asks first ("Start
again?"), because RetroAchievements requires the game to begin again: a second A switches Hardcore on for every game
and starts this game from its power-on. What you did since your last in-game save is lost; B keeps Hardcore off. On → A switches it off at once, and Save, Load, Earlier and Cheats are back in the menu.

A game RetroAchievements doesn't know (a hack without a set, homebrew) has nothing to protect: its states, rewind and
cheats stay, as in RetroArch. A game pocket-ui doesn't run itself (RetroArch, standalone emulators) gets no
achievements from pocket-ui either way.

Offline, Hardcore works with what the Pocket remembers of the game; unlocks wait and are sent as hardcore unlocks the
next time a game starts online.

