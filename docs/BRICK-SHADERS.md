# Picture options on the Brick (shaders, frames, scaling)

Built 2026-11-03 for the Brick's own card. How it works is in `docs/TRIMUI-BRICK.md`
("Shaders, screen frames and picture effects").

## Rule

Every picture choice the Brick's menus show changes the game picture. With `pocket-fb --gl` (the card's default) pocket-core's own renderer
draws the game; without the GPU (`gl-off`, a probe that failed, a GPU lost mid-game) the Picture group, the This game page's Picture rows and
Settings > Screen frames are left out instead of shown dead (`brick_profile::picture_rows_out`). Choices that cost the GE8300 much carry
"(heavy)" (`brick_profile::HEAVY`).

## Proof on the host (Mesa, no device)

`cargo test --features brick --test brick_fb every_picture_choice_changes_the_game_frame`: a 240 by 160 GBA picture (checkers and a ramp, no
black in it) on a 1024 by 768 screen, the same size as the Brick's. For each choice the screen is read back from the framebuffer file before
and after, and the two must differ; the box of what is not black must be what the choice says.

| Choice | Read-back |
|---|---|
| Integer (base) | 960 by 640 (4x), centred |
| Integer, smaller | 720 by 480 (3x), centred |
| Aspect | 1024 by 683 |
| Fullscreen | 1024 by 768 |
| Sharpness Soft | differs from Sharp |
| Sharpness Crisp | differs from Sharp at Aspect (at a whole-number scale Crisp is Sharp: the choice has nothing to add there) |
| Effect LCD grid, Scanlines, CRT | each differs from None |
| LCD colours (GBA, under the grid) | differs from the grid without it |
| Frame: Auto, Ambient, Shell, Brick, Pocket slim, Clamshell, Wide, Gloss, Swan, Watch, Goggles, CRT, Studio monitor, Portable TV, Woodgrain, Arcade, Glow | each differs from no frame |
| Frame colours: shell (6), brick (5), pocket (1), clamshell (3), swan (1), watch (1), TV cabinet (1) | each differs from the frame's default |
| Orientation left / right | picture stands up, 480 by 720 |

The frame is prepared on a thread and drawn with the game's next picture (a running game sends one every frame); the test sends it again.
The `Twin` frame is offered for the Nintendo DS only and is not in the GBA's menu.

## Only the device can confirm

- Speed with the heavy choices (scanlines, CRT, Ambient) on the GE8300: `brick-egl --shaders` on the kit stage, `POCKET_CORE_GPU_TIMER=1`.
- Colours and the look of the LCD grid / CRT on the real panel; the PVR driver's own rounding.
- That the card starts `pocket-fb --gl` by default on this unit (the `pocket_shaders` line in the log says "on the GPU").
- A real GBA game from the pause menu: change a choice, resume, see the picture change.
