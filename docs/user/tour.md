# Features and the Tour

Settings → Help & tools → **Tour** covers **the basics in 7 quick cards**: playing, saving, quick settings, adding games
and a one-switch readability boost. Press A to step forward; on the final card, pick On, Off or Later with Up/Down and
press A to apply and exit. Press B anytime to close the Tour.

1. **Play, and HOME is the menu**: highlight a game and press A; tap HOME in-game for the [pause menu](pause-menu.md).
2. **Continue where you stopped**: Home opens on your last-played game, and quitting saves your exact spot ([Home](playing.md#home)).
3. **Saving**: Save, Load and Earlier all live in the pause menu ([Earlier](pause-menu-saves.md#earlier)).
4. **Quick settings**: press HOME on the shelf or START in the pause menu ([Quick settings](quick-settings.md#quick-settings)).
5. **Add games**: copy into each console folder on your SD card, or use File sharing over Wi-Fi ([Add games](getting-started.md#5-add-games)).
6. **Settings**: everything here (and every extra you encounter) can be changed under START › Settings.
7. **Big text**: turns on Large text, Reduced motion and pause-menu Zoom in one go ([Big text](big-text.md#big-text)).

Use L1/R1 (or Left/Right) to flip back and forth between cards. Extras like Curator, screen frames and Pixel Path stay
out of the Tour (apart from Big text); each introduces itself the first time you see it and lives in Settings. After an
update, [New in this version](#new-in-this-version) highlights what's new.

## The feature cards

Feature cards (one per extra) appear in two places: first start's "Choose for yourself"
([Getting started](settings-system.md#how-you-like-to-play)) and "New in this version" below. Each card shows the feature
name, a one-line summary, a preview image and where to find it in Settings.

- Cards with a toggle offer **On**, **Off** and **Later** ("Now" marks your current setting). Pick with Up/Down and press
  A to apply immediately and move on; **Later** leaves your current setting untouched.
- Cards only appear when your handheld actually has that hardware (no Stick lights card on a handheld without sticks).
- Info-only cards (like Search or Players) simply show where to find the feature; press A to continue.
- L1/R1 (or Left/Right) browse cards without changing anything. B steps back during first start or dismisses "New in this
  version"; START skips the rest.

## The cards and the profiles

Here is every feature card in order, alongside what each first-start profile sets. **The shelf matches pocket-ui's
out-of-the-box defaults** (the same result as pressing START to skip during first start). Running it again later resets
those switches, including On start and Screen frames. By default, only Echoes beyond this device, Pixel Path, Screen
frames and "Continue your last game" start Off.

| Card | The shelf | Enthusiast | Changed in |
|---|---|---|---|
| **Continue where I quit** | As it is | As it is | Settings → Playing → [Continue where I quit](settings-playing.md#continue-where-i-quit) |
| **Continue your last game** | Off | On | Settings → Playing → [On start](settings-playing.md#on-start) (Last game) |
| **Motion** | On | On | Settings → Look → [Motion](settings.md#motion) (Off sets Motion to Off) |
| **Big text** | As it is | As it is | Settings → Look → [Text size](settings.md#text-size) and Settings → Look → [Motion](settings.md#motion) ([Big text](big-text.md#big-text)): On sets Large text + Reduced motion (and adds [Zoom](big-text.md#zoom) to the pause menu); Off restores Normal text + Full motion |
| **Retro** | As it is | As it is | Settings → Look → [Style](theme.md#style) → Retro (Off returns to Barlow). After updating to 0.3.0 this card defaults to On: A tries it, B keeps your current style |
| **Menu sounds** | As it is | As it is | Settings → Screen & sound → Menu sounds |
| **Haptics** | As it is | As it is | Settings → Controls → Haptics |
| **Stick lights** | As it is | As it is | Settings → Controls → [LEDs](settings-sound.md#leds) (On sets Colour unless Battery was chosen) |
| **Screen frames on every console** | Off | On | Settings → Playing → [Screen frames](settings-playing.md#screen-frames) |
| **Echoes** | Off | On | Settings → Games → Curator |
| **Versions** | Off | On | Settings → Games → Curator ("Merge versions in mixed lists") |
| **Play history** | As it is | On | Settings → Games → Curator ([Play history](curator.md#play-history)); on by default |
| **Echoes beyond this device** | Off | On | Settings → Games → Curator ("Games not on this device in Echoes") |
| **Pixel Path** | Off | On | Settings → Games → Curator ("Pixel Path in the pause menu") |
| **Pixel Path over every game** | As it is | As it is | Settings → Playing → [Over the game](settings-playing.md#over-the-game) set to Pixel Path ([Pixel Path over the game](pause-menu.md#pixel-path-over-the-game)), enabling Pixel Path in the pause menu too; Off clears only the overlay |
| **Lens & smart shelves** | Off | On | Settings → Games → Curator |

Info-only cards that follow: **Search and the letter jump**, **Console credits**, **Players**, **Run-ahead**, **Runs on**,
**RetroAchievements**, **Sync**, **Tailscale**, **Tools**, **Cozy extras** (real cases, memory cards, stickers and other
cozy touches, each toggled individually in Settings → Look → [Cozy extras](settings.md#cozy-extras)), **Performance & fan**
and **Settings at once** (START jumps straight into settings rows with L1/R1 switching categories; see
[Open Settings](settings.md#open-settings)).

Choosing **For a child** skips all of these: [Kids mode](kids-mode.md) manages its own setup and leaves your switches
untouched.

## New in this version

After an update, pocket-ui shows up to three cards highlighting newly added features, shown once when you're idle on Home,
never interrupting a game (if you boot straight into your last game, they wait until you return to Home). Press A to apply
your pick and continue, or B to dismiss them. Try-it-out looks default to On (in 0.3.0, **Retro** offers "Try Retro" on A
and switches Settings → Look → Style right away, while B leaves it untouched). **Settings at once** points out the new
layout: START opens entries immediately, L1/R1 switch categories, and Tailscale, SSH and Sync live under Connections.
Once dismissed, the cards never reappear; everything stays in Settings. Updates without new feature cards (and brand-new
first starts) skip this screen.

While a card is open, long scrolling titles and Shelf/Cabinet artwork behind it pause automatically so your handheld
isn't burning extra redraws while you read.
