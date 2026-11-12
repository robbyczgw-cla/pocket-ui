//! Software pictures retained by the Android player, including the bootstrap run before playback.
use pocket_ui::{image::Image, libretro::Frame};
use std::sync::Arc;

pub(super) fn image(frame: Frame) -> Arc<Image> {
    let rgb = frame.to_rgb();
    Arc::new(Image {
        w: frame.width,
        h: frame.height,
        px: rgb.chunks_exact(3).map(|p| (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2])).collect(),
    })
}

pub(super) fn startup(initial: Option<Frame>, resumed: Option<Frame>) -> Option<Arc<Image>> {
    // The timing/geometry bootstrap is already a real video delivery. A core may
    // duplicate that picture forever; prefer a newer post-resume picture when supplied.
    resumed.or(initial).map(image)
}
