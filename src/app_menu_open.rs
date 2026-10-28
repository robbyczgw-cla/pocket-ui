//! Part of the pocket-ui binary: how the pause menu goes up at once (docs/ARCHITECTURE.md, "The pause menu").
//!
//! * **Paused before covered.** For a pocket-core game `pause` goes out first, and pocket-ui puts its window in front
//!   only when pocket-core says it draws no more (`fast_switch::PAUSED`, within a frame; `pause`): a pocket-core
//!   covered in the middle of a frame waits in `eglSwapBuffers` for a frame callback a hidden window never gets and
//!   reads no command after that, the Quit included.
//! * **No screenshot first.** The menu goes up over black; pocket-core renders its last frame into
//!   `pocket_ui::preview::SHOT_FILE` (`shot <seq>`, right after `pause`), a helper thread reads it, and it comes in
//!   behind the menu (and behind the switcher's "Now playing" card, if that was opened meanwhile) a frame or two
//!   later. Before, the UI thread took a screenshot with grim and waited for it before the menu could go up. Other
//!   emulators can't draw for us: they are still captured with grim before pocket-ui covers them (`open_menu`).
//! * **No frame at the old size.** Behind a game sway keeps pocket-ui's window tiled next to it, half the screen
//!   wide (after the first Continue). Putting it in front resizes it, and sway shows nothing new until a frame of
//!   the new size comes; a frame drawn before the resize has arrived held the buffer sway needed back, which cost
//!   200 ms in the virtual device (sway's transaction timeout) at the pause menu after a Continue. So right after pocket-ui put itself
//!   in front it draws only at its full size (`Raise`, at most `RAISE_WAIT` for it).
//! * **The darkened picture made once.** The picture behind the menu is darkened the same way in every frame
//!   (`paint_game_behind`): a menu frame starts with a copy instead of a scaled blit and a fade of the whole screen.
//!   Two darkenings are kept (the menu's, and Pixel Path's a little darker), so going in and out of that page costs
//!   one blit the first time and copies after.

use std::path::Path;
use std::time::Duration;

use pocket_ui::control;
use pocket_ui::fast_switch;
use pocket_ui::gfx::Canvas;
use pocket_ui::image::Image;
use pocket_ui::preview;

use winit::event_loop::EventLoopProxy;

use super::{App, UserEvent};

/// How long a frame waits for the window's full size after pocket-ui put itself in front (it comes in a millisecond).
const RAISE_WAIT: Duration = Duration::from_millis(100);

/// pocket-ui's window size in front of everything (the largest it has had), and whether it was just put there.
#[derive(Default)]
pub(crate) struct Raise {
    full: (u32, u32),
    /// When it was put in front (`App::raised`; `open_menu` sets it itself, its borrows allow no call).
    pub(crate) since: Option<std::time::Instant>,
}

/// How long the helper thread waits for the picture (it takes a few milliseconds; a game still starting has none).
const SHOT_WAIT: Duration = Duration::from_secs(1);

/// What a darkened background was made from: the picture's pixels' address and length, the canvas size, the
/// darkening. Kept only until the next menu or picture (`MenuShot::asked`), so the address is never another's.
type BehindKey = (usize, usize, i32, i32, u8);

/// The picture asked of pocket-core, and the darkened backgrounds made from the menu's picture (at most
/// `BEHIND_KEPT`, the newest last).
#[derive(Default)]
pub(crate) struct MenuShot {
    seq: Option<u32>,
    behind: Vec<(BehindKey, Vec<u32>)>,
}

/// Darkened backgrounds kept: the menu's and the Pixel Path page's.
const BEHIND_KEPT: usize = 2;

impl MenuShot {
    /// A pause menu has opened with picture `seq` on its way (None: not asked): nothing of an earlier menu is kept.
    pub(crate) fn asked(seq: Option<u32>) -> MenuShot {
        MenuShot { seq, behind: Vec::new() }
    }
}

/// The pause menu of a pocket-core game (process group `pgid`): `pause`, and the wait until it says it draws no more
/// (`fast_switch::PAUSED`, within a frame), so pocket-ui doesn't cover it in the middle of one; then asks for the
/// picture behind the menu. Returns its number, for `MenuShot::asked`.
pub(crate) fn pause(pgid: Option<i32>, proxy: EventLoopProxy<UserEvent>) -> Option<u32> {
    let before = pgid.map(fast_switch::pauses);
    let paused = control::pocket_core("pause");
    if let (true, Some(g), Some(n)) = (paused, pgid, before) {
        if !fast_switch::wait_paused(g, n, control::MENU_PAUSE) {
            println!("pause menu: pocket-core didn't say it paused within {:?}", control::MENU_PAUSE);
        }
    }
    pocket_ui::trace::step("menu: game paused");
    ask(proxy)
}

/// Asks pocket-core for the picture behind the menu; its number.
fn ask(proxy: EventLoopProxy<UserEvent>) -> Option<u32> {
    let seq = preview::next_shot_seq();
    if !control::pocket_core(&format!("shot {seq}")) {
        return None;
    }
    std::thread::spawn(move || {
        let image = preview::wait_for_shot(Path::new(preview::SHOT_FILE), seq, SHOT_WAIT);
        let _ = proxy.send_event(UserEvent::MenuShot { seq, image });
    });
    Some(seq)
}

impl App {

    /// The picture came (or didn't, `None`): behind the menu it is, if the menu it was asked for is still open.
    pub(crate) fn menu_shot_arrived(&mut self, seq: u32, image: Option<preview::Pixels>) {
        if self.menu_shot.seq != Some(seq) {
            return;
        }
        self.menu_shot = MenuShot::default();
        let Some(image) = image else {
            println!("pause menu: no picture from pocket-core");
            return;
        };
        pocket_ui::trace::step("menu: picture in");
        let Some(menu) = self.menu.as_mut() else { return };
        menu.frame = Some(Image { w: image.w, h: image.h, px: image.px });
        self.switcher_now_playing_picture();
        self.request_redraw();
    }

    /// Clears `c` to `void` and paints the pause menu's picture of the game over it, darkened by `fade` (0..255
    /// towards black), from the copy made the first time.
    pub(crate) fn paint_game_behind(&mut self, c: &mut Canvas, void: u32, fade: u8) {
        let Some(frame) = self.menu.as_ref().and_then(|m| m.frame.as_ref()) else {
            c.clear(void);
            return;
        };
        let key = (frame.px.as_ptr() as usize, frame.px.len(), c.w, c.h, fade);
        let behind = &mut self.menu_shot.behind;
        // Made from another picture or for another size: gone.
        behind.retain(|((ptr, len, w, h, _), px)| (*ptr, *len, *w, *h) == (key.0, key.1, key.2, key.3) && px.len() == c.buf.len());
        if let Some((_, px)) = behind.iter().find(|(k, _)| *k == key) {
            c.buf.copy_from_slice(px);
            return;
        }
        c.clear(void);
        c.blit_scaled(frame, 0, 0, c.w, c.h);
        c.fade(fade);
        if behind.len() >= BEHIND_KEPT {
            behind.remove(0);
        }
        behind.push((key, c.buf.to_vec()));
    }

    /// pocket-ui has just put itself in front of a game (a `swaymsg` that makes it fullscreen).
    pub(crate) fn raised(&mut self) {
        self.raise.since = Some(std::time::Instant::now());
    }

    /// Whether a frame of `w` x `h` should wait: just put in front, and sway's resize to the full size not in yet
    /// (its `Resized` asks for the frame). Notes the full size as it goes.
    pub(crate) fn wait_for_full_size(&mut self, w: u32, h: u32) -> bool {
        let r = &mut self.raise;
        if w as u64 * h as u64 > r.full.0 as u64 * r.full.1 as u64 {
            r.full = (w, h);
        }
        match r.since {
            Some(t) if (w, h) != r.full && t.elapsed() < RAISE_WAIT => {
                pocket_ui::trace::step("frame waits for the full size");
                true
            }
            _ => {
                r.since = None;
                false
            }
        }
    }
}
