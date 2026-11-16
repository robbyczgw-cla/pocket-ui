//! Software pictures retained by the Android player, including the bootstrap run before playback.
use pocket_ui::{image::Image, libretro::Frame};
use std::sync::Arc;
use std::path::Path;

/// A held moment and its matching picture, for both the session start and periodic Earlier captures.
pub(super) fn moment(path: &Path, bytes: &[u8], image: Option<&Image>) -> Result<(), String> {
    let preview = pocket_ui::slots::preview_path(path);
    match std::fs::remove_file(&preview) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.to_string()),
    }
    pocket_ui::state_file::write(path, bytes).map_err(|e| e.to_string())?;
    if let Some(image) = image { png(&preview, image)?; }
    Ok(())
}

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

pub(super) fn png(path: &Path, image: &Image) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.w, image.h);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        let rgb: Vec<u8> = image.px.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8]).collect();
        writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
    }
    pocket_ui::fsutil::write_atomic(path, &bytes).map_err(|e| e.to_string())
}
