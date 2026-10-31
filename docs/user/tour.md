# Features and the Tour

pocket-ui has more than it shows at first, and the extras that change what A does or what is on screen are off until you want them.
Settings → Help & tools → **Tour** shows them one card at a time, the same cards as the first start's "Choose for yourself"
([Getting started](getting-started.md#how-do-you-like-to-play)): the feature's name, one line about it, a small
picture of its screen, and where it is in Settings.

- A card with a switch has three rows: **On**, **Off** and **Later**. "Now" marks what is set. Up/Down choose, A takes
  the choice and goes to the next card. On and Off change the same switch as the feature's own Settings page, at once.
  **Later** changes nothing and keeps the card for later: the Tour row in Settings → Help & tools then says how many you put off
  ("2 put off").
- A card is only shown on a device that has what it is about, like the Settings rows: no Stick lights on a handheld without sticks;
  on the Steam Deck no Haptics, Sync, Tailscale, Tools or Performance & fan.
- A card without a switch (Search, Players and the others below) only says where the feature is. A goes on.
- L1/R1 (or Left/Right) go to the previous or next card without choosing anything. B or START closes the Tour.

## The cards and the profiles

The cards, in their order, and what the first start's profiles set. **Simple is pocket-ui as it ships**: it sets
exactly the defaults, so pressing START on that step instead gives the same result. Only Up next, Echoes beyond
this device, Pixel Path, frames and "Straight back into your game" are off by default.

| Card | Simple | Enthusiast | Changed in |
|---|---|---|---|
| **Lens & smart shelves** | On | On | Settings → Games → Curator |
| **Echoes** | On | On | Settings → Games → Curator |
| **Up next after a game** | Off | On | Settings → Games → Curator ([Up next](curator.md#up-next)) |
| **Echoes beyond this device** | Off | On | Settings → Games → Curator ("Games not on this device in Echoes") |
| **Versions** | On | On | Settings → Games → Curator ("Merge versions in mixed lists") |
| **Stickers** | On | On | Settings → Games → Curator |
| **Play history** | As it is | On | Settings → Games → Curator ([Play history](curator.md#play-history)); on by default |
| **Pixel Path** | Off | On | Settings → Games → Curator ("Pixel Path in the pause menu") |
| **Pixel Path over every game** | As it is | On | Settings → Playing → [Over the game](settings.md#over-the-game) on Pixel Path ([Pixel Path over the game](pause-menu.md#pixel-path-over-the-game)); Off takes only that level away |
| **Screen frames on every console** | Off | On | Settings → Playing → [Screen frames](settings.md#screen-frames) |
| **Straight back into your game** | Off | On | Settings → Playing → [On start](settings.md#on-start) (Last game) |
| **Remember where I quit** | As it is | As it is | Settings → Playing → [Remember where I quit](settings.md#remember-where-i-quit) |
| **Motion** | On | On | Settings → Sound & feel → [Motion](settings.md#motion) (Off on the card is Motion Off) |
| **Menu sounds** | As it is | As it is | Settings → Sound & feel → Sounds |
| **Haptics** | As it is | As it is | Settings → Sound & feel → Haptics |
| **Stick lights** | As it is | As it is | Settings → Sound & feel → [LEDs](settings.md#leds) (On is Colour, unless Battery was chosen) |

Cards without a switch, after those: **Search and the letter jump**, **Console credits**, **Players**, **Run-ahead**, **Runs on**,
**RetroAchievements**, **Sync**, **Tailscale**, **Tools**, **Cozy extras** (real cases, the memory card, stickers and the other
little touches, each switched in Settings → Look → [Cozy extras](settings.md#cozy-extras)) and **Performance & fan**.

## New in this version

After an update, pocket-ui shows the cards of the features that are new since the version you had, at most three,
once: when you are on Home with nothing open, even if the Pocket went straight into your last game first. Each has
its switch, as in the Tour. A takes the choice and goes on, B puts the cards away. They don't come back; the Tour has
them all. A version without new features shows nothing, and so does the first start of a new Pocket.
