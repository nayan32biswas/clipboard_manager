//! Image clips: hashing, PNG + thumbnail files, and data URLs for the UI.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::{imageops, ImageFormat, RgbaImage};

use crate::store::hash_bytes;

pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
pub const THUMB_SIZE: u32 = 160;

#[derive(Debug, Clone, PartialEq)]
pub struct RawImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl RawImage {
    /// Covers the size too, so the same bytes in a different shape differ.
    pub fn hash(&self) -> String {
        let mut data = Vec::with_capacity(8 + self.rgba.len());
        data.extend_from_slice(&self.width.to_le_bytes());
        data.extend_from_slice(&self.height.to_le_bytes());
        data.extend_from_slice(&self.rgba);
        hash_bytes(&data)
    }

    fn to_buffer(&self) -> crate::Result<RgbaImage> {
        RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
            .ok_or_else(|| "image buffer size does not match its width and height".into())
    }
}

/// Writes `<name>.png` and `<name>.thumb.png` into `dir`. Returns None, and
/// writes nothing, if the PNG would be larger than `max_bytes`.
pub fn save(
    dir: &Path,
    name: &str,
    img: &RawImage,
    max_bytes: usize,
) -> crate::Result<Option<(PathBuf, PathBuf)>> {
    let buffer = img.to_buffer()?;
    let png = encode_png(&buffer)?;
    if png.len() > max_bytes {
        return Ok(None);
    }
    let (w, h) = thumb_size(img.width, img.height);
    let thumb = encode_png(&imageops::thumbnail(&buffer, w, h))?;

    std::fs::create_dir_all(dir)?;
    let png_path = dir.join(format!("{name}.png"));
    let thumb_path = dir.join(format!("{name}.thumb.png"));
    std::fs::write(&png_path, png)?;
    std::fs::write(&thumb_path, thumb)?;
    Ok(Some((png_path, thumb_path)))
}

pub fn data_url(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
}

fn encode_png(buffer: &RgbaImage) -> crate::Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    buffer.write_to(&mut out, ImageFormat::Png)?;
    Ok(out.into_inner())
}

/// Fits inside THUMB_SIZE x THUMB_SIZE, keeps the aspect ratio, never upscales.
fn thumb_size(width: u32, height: u32) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= THUMB_SIZE {
        return (width, height);
    }
    let scale = |v: u32| ((u64::from(v) * u64::from(THUMB_SIZE) / u64::from(longest)) as u32).max(1);
    (scale(width), scale(height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32) -> RawImage {
        RawImage { width, height, rgba: [10, 20, 30, 255].repeat((width * height) as usize) }
    }

    #[test]
    fn hash_depends_on_shape_as_well_as_bytes() {
        let a = solid(2, 8);
        let b = RawImage { width: 8, height: 2, rgba: a.rgba.clone() };
        assert_ne!(a.hash(), b.hash());
        assert_eq!(a.hash(), solid(2, 8).hash());
    }

    #[test]
    fn save_writes_full_png_and_small_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let (png, thumb) = save(dir.path(), "x", &solid(640, 320), MAX_IMAGE_BYTES).unwrap().unwrap();

        let full = image::open(&png).unwrap();
        assert_eq!((full.width(), full.height()), (640, 320));
        let small = image::open(&thumb).unwrap();
        assert_eq!((small.width(), small.height()), (THUMB_SIZE, THUMB_SIZE / 2));
    }

    #[test]
    fn small_images_are_not_upscaled_for_the_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let (_, thumb) = save(dir.path(), "x", &solid(20, 10), MAX_IMAGE_BYTES).unwrap().unwrap();
        let small = image::open(&thumb).unwrap();
        assert_eq!((small.width(), small.height()), (20, 10));
    }

    #[test]
    fn too_large_image_is_skipped_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(save(dir.path(), "x", &solid(64, 64), 10).unwrap(), None);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn wrong_buffer_length_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let bad = RawImage { width: 4, height: 4, rgba: vec![0; 3] };
        assert!(save(dir.path(), "x", &bad, MAX_IMAGE_BYTES).is_err());
    }

    #[test]
    fn data_url_is_base64_png() {
        let dir = tempfile::tempdir().unwrap();
        let (_, thumb) = save(dir.path(), "x", &solid(4, 4), MAX_IMAGE_BYTES).unwrap().unwrap();
        let url = data_url(&thumb).unwrap();
        assert!(url.starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert_eq!(data_url(&dir.path().join("missing.png")), None);
    }
}
