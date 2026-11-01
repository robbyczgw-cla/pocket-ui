# Kids Mode

A grown-up sets it up, a child gets one big case at a time. The setting is off unless a grown-up chooses it. Player's
page: [user/kids-mode.md](user/kids-mode.md). Research on what other systems do: none of the handheld systems we read
has a child's own screen, a time limit or a loudest volume; the way out is everyone's weak spot.

## What a grown-up does

- First start, "How do you like to play?" → **For a child**, or later Settings → System → **Kids mode**.
- A 4-digit PIN with the D-pad, twice. 5 to 20 games (fewer if the card has fewer). Blue or Pink. A time per day and a
  loudest volume, both optional. The first start then goes on with Wi-Fi and the rest.
- **No games yet:** Add games comes first. From Settings, A on the step opens Add games and the list is read again when it
  closes; START goes on without (the child's shelf then says "No games here yet"). In the first start Add games is one
  of the steps that follow anyway, so the games are picked right after them, before the shelf.

## What a child sees

- One big case with its neighbours, Left and Right, A plays. Clock and battery. The name in big type. The case goes
  into its console and the old-TV switch-on plays as always, whatever the Animations setting says (Motion below Full
  still skips them; the setting is the grown-up's).
- In a game HOME shows **Continue** and **Stop**. Stop is the pause menu's own Quit: it keeps the moment, and A brings
  it back. No slots, Options, quick settings, switcher or card.
- Volume keys work up to the ceiling. Power button as always, with Sleep, Restart and Power off only.
- **Five minutes before the time is up** (or at once, when a game starts with less left): the game pauses behind a clock with
  those minutes in colour and "5 minutes left"; A goes on. More time from a grown-up gets its own warning.
- Time up: the game's normal Quit runs first, then "That's all for today!". The clock counts only while a game runs
  unpaused. It stays closed until the next day or until a grown-up adds 30 minutes.
- A power-off in a game: the same save and resume note as ever; the next start resumes it if it is still on the list and
  time is left.

The design case is a young child on a small handheld (640×480, no Wi-Fi, no touch), with a grown-up nearby: so
the words stay and symbols stand beside them (▶ and ■ in the pause menu, a picture on the warning, time-up, empty shelf and
low battery). 640×480 is the size to check first. Nothing in the mode needs a network; the grown-up picks from what is on
the card. A word-free child's side is for later.

## Which games can be picked

The picker asks the device's own routing, per game (`App::kids_library`), so a device only offers what it plays:

| Plays in | Offered | What closes it |
|---|---|---|
| pocket-core with states (N64, PSP, DS on strong devices; on H700 only what pocket-core plays there) | yes | `POCKET_CORE_NO_SHORTCUTS`: no HOME shortcut |
| ARMSX2 (PS2) | yes | `[Hotkeys]` lines of `PCSX2.ini` kept aside for the game: HOME + X was ARMSX2's whole menu, HOME + a shoulder a state |
| Dolphin nogui (GameCube, Wii) | yes | `Buttons/Hotkey` of the pad profiles (below) |
| Azahar (3DS) | yes | `HOTKEY` in `gptokeyb`'s environment (below) |
| hakuX in RetroArch (Xbox) | yes, "Saves only in the game" | The appendconfig takes RetroArch's menu and hotkeys away, as for every Xbox game |
| Wii channels (`.wad`) | yes, "Saves only in the game" | As Dolphin; their states have no name we know |
| RetroArch, PortMaster, Dolphin's Qt builds, ScummVM, PICO-8, TIC-80 | no: a grey line under the list says how many and why | Menus of their own a child can reach |

**Dolphin.** ROCKNIX's patch `000-hotkeys` reads `GCPadStatus` for one binding, `Buttons/Hotkey`. With SELECT it quits with no save (+ START), saves
and loads (+ R, L), changes slot, takes screenshots, shows FPS, changes the picture. pocket-ui's own Save and Quit go through the same hotkeys,
so the binding can't go. For a Kids Mode's game it also wants a stick click, which every chord of ours holds (`defaults::with_hotkey_for`):
``Buttons/Hotkey= (`Back` & (`Thumb L` | `Thumb R`))``, no blank before `=`, so the start scripts' `sed '/^Buttons\/Hotkey =/c…'` leaves it.
What is left: holding a stick click and SELECT and a third button together still reaches them. Three buttons, one a stick pressed in; not by
mashing, but not impossible. Any other start and the mode going off put the plain line back.

**Azahar.** `start_azahar.sh` starts `gptokeyb -sudokill azahar`: SELECT + START kills the emulator, HOME + a shoulder is Azahar's quick save and
load, HOME + a trigger its screen layout. All of it hangs on gptokeyb's hotkey, and gptokeyb reads that from the environment: `HOTKEY=misc1`, a
button these handhelds haven't got (`standalone::KIDS_GPTOKEYB_ENV`). pocket-ui's own Save presses Azahar's keys on InputPlumber's keyboard, not through gptokeyb.

**ROCKNIX itself.** `input_sense` answers L1 + A (screenshot), L1 + X (MangoHud), L1 + Y (the game guide over the game), L1 + SELECT + START (kills the
emulator, no save), and the volume keys, with no ceiling. While the mode is on `key.hotkey.a`, `key.volume.up` and `key.volume.down` in `system.cfg` name
a key that doesn't exist and `input.service` is restarted (`fn_keys::kids_keys`; what they were waits in `kids-rocknix-keys`). pocket-ui takes the
volume keys itself then, under the ceiling (`Sys::kids_volume_enter`). A start with the mode off opens what a crash left closed.

**Seen on real devices (2026-11-01, real pad input, with a control run without the closure):**

| Device, emulator | In Kids Mode | Without the closure |
|---|---|---|
| Odin 2 Mini, Dolphin (Double Dash) | SELECT + START: game goes on; SELECT + R2, L2, A, Y: no state written. Stop saved through the closed binding (0.6 s), A brought it back | SELECT + START ended Dolphin at once |
| Odin 2 Mini, ARMSX2 (Ico) | Guide + the menu's button: PINE stays Running. Guide + shoulders: no state written. Stop saved slot 99, A loaded it | PINE went to Paused: ARMSX2's menu opened |
| Mini V2, Azahar (a 3DSX) | SELECT + START, HOME + shoulders, HOME + L2: game goes on. Stop saved its Auto state | SELECT + START ended Azahar at once |
| Mini V2, pocket-core (Game Boy) | First start with "For a child", L1 + Y, X, A, L1 + SELECT + START, HOME + R, L, A: nothing happens, no state, no screenshot. Volume keys stop at 40 %. Warning, time-up, Stop, resume, PIN, Turn off: normal mode and ROCKNIX's keys back as they were | |
| Mini V2, the crash loop | Four crashes in a row: the door with the plaster; hold, PIN, the rescue page | |

Not seen on a device: Xbox and Wii in the mode (same paths as above, not run). An H700 handheld (only pocket-core's systems there).

## Closed doors

- **Power menu:** no Restart pocket-ui and no Restart to boot menu (`power_menu::for_a_child`).
- **Crash loop:** safe mode shows a handheld with a plaster and "Please ask a grown-up" instead of the rescue page. SELECT + START for 5 seconds and the
  PIN open the rescue page; the file below does too, and turns the mode off at the next normal start (`app_rescue::KidsDoor`).
- **A game that can't start:** no fallback. Every start checks the route against what the picker offers (`kids::route_keeps_moment`), a pocket-core game that fails
  stops there, and `fallback.json` isn't touched: the child gets the plaster picture with "Please ask a grown-up", a press is the shelf again.
- **The dialog before EmulationStation:** closed, also to a keyboard's Escape; on the crash-loop door a keyboard's keys go to the door, as a pad's do.
- **File sharing and USB** stay as they were, on or off; their pages are in Settings, which a child can't reach. Add games opens only from a grown-up's flow.
- **Quick settings:** not there. Only the volume keys, up to the ceiling. Brightness stays where a grown-up left it.
- **Deck:** off. Steam's button belongs to SteamOS and opens its overlay over anything we show (`device::hides("kids", …)`: no row, no profile).
- **Android:** not in scope (docs/PLATFORMS.md).

What is still open past that: a launcher that fails even in safe mode ends in EmulationStation (`run-pocket-ui.sh` stage 5), with no door there.

## The clock

`kids-time.json` keeps the date last known and the seconds played. A later date starts at nought. An **earlier** date is a clock that lost its time (no RTC
battery, no network): the minutes go on counting under the last date known, and the grown-ups' page says so. So a reset clock is no new day.

What that leaves:

- With a clock stuck behind, no new day comes by itself until the clock passes that date: the time stays up. A grown-up gets 30 more minutes or **Start today's
  time again**, which also takes the clock's date as the new one (the way out of a clock that ran ahead and left a date in the future).
- A clock set **forward** is a new day. A child has no way to do that: the Time page is in Settings. A network clock correcting itself forward over midnight would do the same, once.

## Getting out

Hold **SELECT + START for 5 seconds** on the shelf or the pause menu; a bar shows the time. It doesn't work during
play, because a game may use both buttons. Then the PIN. In a paused game the right PIN stops the
game first, saved. After 3 wrong tries the page says what to do; from 5 on each try waits a minute.

The grown-ups' page: 30 more minutes today, Start today's time again, Games, Colours, Time per day, Loudest volume, New PIN, Turn off Kids mode.

### PIN forgotten

Put a file called `kids-mode-off.txt` (any content) on the card and restart. Where it counts: the card's first partition
(`/flash`, FAT, which a phone with a card reader or any computer can write), `/storage/roms` (the Wi-Fi share, the phone
upload) or `/storage/games-external`. The mode goes off and the PIN is forgotten; games, saves and settings stay. The file
is deleted where that works; one that can't be is remembered by its time, so it counts once. A child who can do this
has earned it: the mode is against accidents, not against a determined kid.

## Where things are

`kids.json` (choices, PIN) and `kids-time.json` (the day's count) in pocket-ui's data folder, `kids-normal-volume`, `kids-rocknix-keys` and
`kids-armsx2-hotkeys` while the mode is on. `settings.json` keeps the normal look and Quit setting: the mode changes them only in memory
(`App::kids_saved_settings`). Code: `src/kids.rs` (model, tests), `src/app_kids.rs` (input, painting), `KidsDoor` in `src/app_rescue.rs`,
`standalone::{kids_keeps_moment, defaults::with_hotkey_for, defaults::armsx2_hotkeys}`, `fn_keys::kids_keys`, small hooks in `main.rs`, `setup.rs`, `features.rs`,
`quick.rs`, `extras.rs`, `app_touch.rs`.

## Limits

- Saves are the device's: child and grown-up share them.
- The PIN is stored in plain text in `kids.json`, on purpose for a family device: it stops children, not attackers.
- The grown-ups' pages have no touch yet.
- Pulling the card or copying files over the Wi-Fi share gets past the mode. The share stays on if it was on.
