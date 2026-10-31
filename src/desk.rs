//! Window handling without sway: the Steam Deck (Gamescope's XWayland) or any X11 session (docs/STEAMDECK.md).
//! On the handhelds `control.rs` asks sway to put pocket-ui or the game in front; here the same questions are answered
//! with plain X11 requests (x11rb, no libxcb): raise a window and give it the input focus, find the game's window, ask
//! it to close (`WM_DELETE_WINDOW`). Gamescope shows the window that has the focus. Without a window manager (Xvfb in the
//! headless runs) the windows are also sized to the screen, which a manager would do for fullscreen windows.
//!
//! Windows are identified by `WM_CLASS`: pocket-ui's own is `control::OWN_APP_ID`; every other mapped top-level window
//! (a child of a frame window too) is a game. The id of a window is its X11 id, as sway's container id is on the handhelds.
//! Built with the `x11` feature; without it every call says "no".

/// Whether the desktop backend answers instead of sway: the Deck profile, or `POCKETUI_WINDOWING=x11`.
pub fn active() -> bool {
    crate::device::current().host_owned() || std::env::var("POCKETUI_WINDOWING").is_ok_and(|v| v == "x11")
}

#[cfg(feature = "x11")]
mod imp {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::*;
    use x11rb::rust_connection::RustConnection;
    use x11rb::CURRENT_TIME;

    struct Desk {
        conn: RustConnection,
        root: Window,
        size: (u16, u16),
        has_wm: bool,
    }

    fn connect() -> Option<Desk> {
        let (conn, n) = RustConnection::connect(None).ok()?;
        let screen = &conn.setup().roots[n];
        let root = screen.root;
        let size = (screen.width_in_pixels, screen.height_in_pixels);
        let check = conn.intern_atom(false, b"_NET_SUPPORTING_WM_CHECK").ok()?.reply().ok()?.atom;
        let has_wm = conn.get_property(false, root, check, AtomEnum::WINDOW, 0, 1).ok()?.reply().ok().is_some_and(|p| p.value_len > 0);
        Some(Desk { conn, root, size, has_wm })
    }

    impl Desk {
        /// `WM_CLASS`'s class (the second string) of `w`.
        fn class(&self, w: Window) -> Option<String> {
            let p = self.conn.get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256).ok()?.reply().ok()?;
            let mut parts = p.value.split(|b| *b == 0).filter(|s| !s.is_empty());
            let (instance, class) = (parts.next()?, parts.next().unwrap_or(&[]));
            Some(String::from_utf8_lossy(if class.is_empty() { instance } else { class }).into_owned())
        }

        /// Top-level windows with a class, bottom to top, as (id, class): a frame window's child counts.
        fn windows(&self) -> Vec<(Window, String)> {
            let mut out = Vec::new();
            let Some(tree) = self.conn.query_tree(self.root).ok().and_then(|c| c.reply().ok()) else { return out };
            for top in tree.children {
                let viewable = |w| {
                    self.conn.get_window_attributes(w).ok().and_then(|c| c.reply().ok()).is_some_and(|a| a.map_state == MapState::VIEWABLE && !a.override_redirect)
                };
                if !viewable(top) {
                    continue;
                }
                if let Some(c) = self.class(top) {
                    out.push((top, c));
                } else if let Some(kids) = self.conn.query_tree(top).ok().and_then(|c| c.reply().ok()) {
                    out.extend(kids.children.into_iter().filter(|k| viewable(*k)).filter_map(|k| self.class(k).map(|c| (k, c))));
                }
            }
            out
        }

        fn own(&self) -> Option<Window> {
            self.windows().into_iter().rev().find(|(_, c)| c == crate::control::OWN_APP_ID).map(|(w, _)| w)
        }

        /// The game's window: the top-most that is not pocket-ui's.
        fn game(&self) -> Option<Window> {
            self.windows().into_iter().rev().find(|(_, c)| c != crate::control::OWN_APP_ID).map(|(w, _)| w)
        }

        fn front(&self, w: Window) -> bool {
            let mut aux = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
            if !self.has_wm {
                aux = aux.x(0).y(0).width(u32::from(self.size.0)).height(u32::from(self.size.1));
            }
            // `check` waits for the server's verdict: a refused request says why in the log.
            let moved = self.conn.configure_window(w, &aux).map_err(|e| e.to_string()).and_then(|c| c.check().map_err(|e| e.to_string()));
            let focused = self.conn.set_input_focus(InputFocus::POINTER_ROOT, w, CURRENT_TIME).map_err(|e| e.to_string()).and_then(|c| c.check().map_err(|e| e.to_string()));
            if let Err(e) = moved.as_ref().and(focused.as_ref()) {
                println!("desk: window {w:#x}: {e}");
            }
            moved.is_ok() && focused.is_ok()
        }

        fn close(&self, w: Window) -> bool {
            let atom = |n: &[u8]| self.conn.intern_atom(false, n).ok().and_then(|c| c.reply().ok()).map(|r| r.atom);
            let (Some(protocols), Some(delete)) = (atom(b"WM_PROTOCOLS"), atom(b"WM_DELETE_WINDOW")) else { return false };
            let ev = ClientMessageEvent::new(32, w, protocols, [delete, CURRENT_TIME, 0, 0, 0]);
            self.conn.send_event(false, w, EventMask::NO_EVENT, ev).is_ok() && self.conn.flush().is_ok()
        }
    }

    /// One line in the log when a request found nothing to act on (the Steam Deck's windows are only ever debugged from the log).
    fn said(what: &str, ok: Option<bool>) -> bool {
        println!("desk: {what}: {}", match ok { Some(true) => "done", Some(false) => "refused", None => "no such window or no X11 connection" });
        ok == Some(true)
    }

    pub fn show_ui() -> bool {
        said("pocket-ui in front", connect().and_then(|d| d.own().map(|w| d.front(w))))
    }

    pub fn show_game() -> bool {
        said("game in front", connect().and_then(|d| d.game().map(|w| d.front(w))))
    }

    pub fn show_window(w: i64) -> bool {
        connect().is_some_and(|d| d.front(w as Window))
    }

    pub fn game_window() -> Option<i64> {
        connect()?.game().map(i64::from)
    }

    pub fn close_window(w: i64) -> bool {
        connect().is_some_and(|d| d.front(w as Window) && d.close(w as Window))
    }
}

#[cfg(not(feature = "x11"))]
mod imp {
    pub fn show_ui() -> bool {
        false
    }
    pub fn show_game() -> bool {
        false
    }
    pub fn show_window(_: i64) -> bool {
        false
    }
    pub fn game_window() -> Option<i64> {
        None
    }
    pub fn close_window(_: i64) -> bool {
        false
    }
}

/// pocket-ui in front, with the focus.
pub fn show_ui() -> bool {
    imp::show_ui()
}
/// The game's window in front, with the focus.
pub fn show_game() -> bool {
    imp::show_game()
}
/// The window `w` (an id from `game_window`) in front, with the focus.
pub fn show_window(w: i64) -> bool {
    imp::show_window(w)
}
/// The game's window id, when there is one.
pub fn game_window() -> Option<i64> {
    imp::game_window()
}
/// Asks the window `w` to close, as its close button would.
pub fn close_window(w: i64) -> bool {
    imp::close_window(w)
}

/// Whether the Flatpak `id` is installed (`flatpak info`); inside our own Flatpak asked of the host. Unknown (no `flatpak`) counts as installed.
pub fn flatpak_installed(id: &str) -> bool {
    let mut c = if std::env::var_os("FLATPAK_ID").is_some() {
        let mut c = std::process::Command::new("flatpak-spawn");
        c.args(["--host", "flatpak"]);
        c
    } else {
        std::process::Command::new("flatpak")
    };
    match crate::proc::run_with_timeout(c.args(["info", "--show-ref", id]), std::time::Duration::from_secs(3)) {
        Ok(o) => o.status.success(),
        Err(e) => e.kind() != std::io::ErrorKind::NotFound,
    }
}
