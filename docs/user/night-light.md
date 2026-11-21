# Night light

Settings → Screen & sound → **Night light** warms up your screen and cuts harsh blue light in the evening—both in menus
and in games, just like night mode on a phone. It starts **Off** until you switch it on. *(Built and tested off-device;
exact colour tint on the Mini V2's panel hasn't been eyeballed yet.)*

| Row | What it does |
|---|---|
| **Night light** | **Off** (default): normal screen colours. **On**: warm colours all the time. **Scheduled**: turns warm automatically every evening between your chosen hours |
| **From** | When the warm shift begins (Scheduled only; default 21:00) |
| **Until** | When colours return to normal (Scheduled only; default 07:00). An Until time earlier than From spans midnight |
| **Strength** | How warm the screen gets, in five steps: **Slight**, **Gentle**, **Medium** (default), **Warm** and **Very warm** |

Move between rows with Up/Down, adjust with Left/Right or press **A** to step forward (Strength wraps from Very warm back
to Slight). Times step in half-hour increments. You can also tap rows and the arrows beside each value on touchscreens. A
status line underneath always tells you what it's doing ("On now, until 07:00."). Changes save immediately and persist
across reboots and updates.

## How it behaves

- **Smooth fades.** Colours shift gently over a second or two when the schedule kicks in or when you tweak Strength—even
  mid-game, never with a jarring snap. If [Motion](settings.md#motion) is Off, it switches instantly.
- **Uses your clock.** Scheduled mode follows your chosen time zone ([Time](settings-system.md#time)). Because your
  handheld has no GPS, it uses fixed times instead of sunrise/sunset. It doesn't drain battery polling the clock all
  night—it simply wakes at the next switch time, on wake from sleep or when network time syncs.
- **Stacks with colour temperature.** [SELECT + Vol](controls-settings.md#volume-and-brightness-keys) still adjusts your
  baseline colour temperature; Night light layers on top. Turn Night light off and your baseline temperature comes right
  back.
- **In games.** On displays whose compositor supports gamma tables, Night light covers everything—pocket-ui, pocket-core,
  RetroArch and standalone emulators. On other hardware, pocket-ui tints its menus and pocket-core games directly, but
  **not** RetroArch, PS2, GameCube, Wii, 3DS or PortMaster games; the Night light page tells you if that applies to your
  handheld.
- **Screenshots.** With a hardware gamma table, warmth is applied at the display, so screenshots (and the web companion's
  screen view) keep true colours. With software tinting, the warm tint is baked into the image, just like colour
  temperature.
- **Zero performance hit.** Night light sits idle during gameplay except at scheduled transition times—no frame-time or
  launch-time cost.

## Ideas that are not in it

No sunrise/sunset tracking, no hourly strength curve and no extra row in quick settings (which stays compact at seven
rows).
