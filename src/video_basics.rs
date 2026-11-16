//! The small things the screen frames (`bezel.rs`) and the picture's place (`dest_rect`) take from the picture renderer,
//! kept apart from it so Android's player, which has no `video.rs`, draws the same frames at the same place.

use glow::HasContext;

/// A destination rectangle in viewport pixels, origin top-left.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Our shaders are GLSL ES 3.00. On a desktop OpenGL context (a core that renders with it, see
/// `hw_render.rs`) the same source runs as GLSL 3.30 core, which accepts the precision qualifiers.
/// Every shader pocket-core compiles (picture, frame, overlay) goes through this.
pub fn glsl_for(gl: &glow::Context, src: &str) -> String {
    if gl.version().is_embedded {
        src.to_string()
    } else {
        desktop_glsl(src)
    }
}

pub(crate) fn desktop_glsl(src: &str) -> String {
    src.replacen("#version 300 es", "#version 330 core", 1)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Scaling {
    /// Whole-number scale of the picture height, in the core's aspect ratio.
    #[default]
    Native,
    /// One whole-number step below Native (GBA 4x instead of 5x), so a frame has room. Never
    /// below 1x.
    IntegerSmaller,
    /// As large as fits, in the core's aspect ratio.
    Aspect,
    /// As large as fits, in the screen's aspect ratio. The viewport is the screen, so this fills
    /// it, like Fullscreen; they differ only when the viewport is not the whole panel.
    /// The whole viewport; pixels stretch.
    Fullscreen,
}

impl Scaling {
    pub fn parse(s: &str) -> Self {
        match s {
            "integer-smaller" => Self::IntegerSmaller,
            "aspect" => Self::Aspect,
            // "Aspect screen" (the screen's own shape) always filled the screen exactly like Fullscreen and was
            // dropped on 2026-10-24; a saved choice of it reads as Fullscreen.
            "aspect-screen" | "fullscreen" => Self::Fullscreen,
            _ => Self::Native,
        }
    }
}

impl Rect {
    fn centered(view_w: u32, view_h: u32, w: f32, h: f32) -> Rect {
        let w = (w.round() as u32).clamp(1, view_w.max(1));
        let h = (h.round() as u32).clamp(1, view_h.max(1));
        Rect { x: (view_w - w) / 2, y: (view_h - h) / 2, w, h }
    }
}

/// Where the picture goes. `aspect` is the core's display aspect ratio (width / height); a value
/// of 0 or less means the frame's own shape.
pub fn dest_rect(scaling: Scaling, view_w: u32, view_h: u32, src_w: u32, src_h: u32, aspect: f32) -> Rect {
    let (vw, vh) = (view_w.max(1), view_h.max(1));
    let (sw, sh) = (src_w.max(1), src_h.max(1));
    let aspect = if aspect > 0.0 { aspect } else { sw as f32 / sh as f32 };
    let fit = |aspect: f32| {
        let (vwf, vhf) = (vw as f32, vh as f32);
        if vwf / vhf > aspect {
            Rect::centered(vw, vh, vhf * aspect, vhf)
        } else {
            Rect::centered(vw, vh, vwf, vwf / aspect)
        }
    };
    match scaling {
        Scaling::Fullscreen => Rect { x: 0, y: 0, w: vw, h: vh },
        Scaling::Aspect => fit(aspect),
        Scaling::Native | Scaling::IntegerSmaller => {
            // Largest whole multiple of the picture height whose width, in the core's aspect
            // ratio, still fits. Most consoles' pixels are not square, so the width is rarely a
            // whole multiple too; Crisp sharpness hides that unevenness.
            let width_at = |n: u32| (n * sh) as f32 * aspect;
            let mut n = vh / sh;
            while n > 1 && width_at(n).round() > vw as f32 {
                n -= 1;
            }
            if n == 0 || width_at(n).round() > vw as f32 {
                fit(aspect) // not even 1x fits: scale down instead
            } else {
                if scaling == Scaling::IntegerSmaller {
                    n = (n - 1).max(1);
                }
                Rect::centered(vw, vh, width_at(n), (n * sh) as f32)
            }
        }
    }
}
