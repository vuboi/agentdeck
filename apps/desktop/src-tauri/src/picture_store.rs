//! Pictures as the app keeps them on disk: a PNG or JPEG (a screenshot, a pasted or dropped picture) becomes a WebP no
//! wider or taller than `MAX_SIDE`, so the chat history and the Audit log load a fraction of the bytes. GIF and WebP stay
//! as they are (a GIF may be animated). A picture that does not decode is kept as it came, never dropped.

use std::io::Cursor;

/// Longest side of a stored picture, in pixels. Screenshots of a 4K display shrink to this.
const MAX_SIDE: u32 = 2048;
/// Lossy WebP quality (0–100). Screenshots keep their text readable at this setting.
const QUALITY: f32 = 82.0;

/// The extension and the bytes to store for a picture of this MIME type.
pub fn stored_picture(mime: &str, bytes: Vec<u8>) -> (&'static str, Vec<u8>) {
    match mime {
        "image/png" | "image/jpeg" => match to_webp(&bytes) {
            Ok(webp) => ("webp", webp),
            Err(_) => (if mime == "image/png" { "png" } else { "jpg" }, bytes),
        },
        "image/gif" => ("gif", bytes),
        _ => ("webp", bytes),
    }
}

/// Decodes a PNG or JPEG and encodes it as WebP, shrunk to fit `MAX_SIDE`.
fn to_webp(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let picture = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| error.to_string())?
        .decode()
        .map_err(|error| error.to_string())?;
    let picture = if picture.width() > MAX_SIDE || picture.height() > MAX_SIDE {
        picture.thumbnail(MAX_SIDE, MAX_SIDE)
    } else {
        picture
    };
    let rgba = picture.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok(webp::Encoder::from_rgba(&rgba, width, height).encode(QUALITY).to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let picture = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
        picture.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).expect("a png");
        out
    }

    #[test]
    fn png_becomes_webp_and_is_smaller() {
        let source = png(800, 600);
        let (extension, stored) = stored_picture("image/png", source.clone());
        assert_eq!(extension, "webp");
        assert!(stored.starts_with(b"RIFF") && &stored[8..12] == b"WEBP");
        assert!(stored.len() < source.len());
    }

    #[test]
    fn large_picture_is_shrunk() {
        let (_, stored) = stored_picture("image/png", png(4000, 1000));
        let decoded = webp::Decoder::new(&stored).decode().expect("a webp");
        assert_eq!(decoded.width(), MAX_SIDE);
    }

    #[test]
    fn undecodable_picture_is_kept_as_it_came() {
        let (extension, stored) = stored_picture("image/png", b"not a picture".to_vec());
        assert_eq!(extension, "png");
        assert_eq!(stored, b"not a picture");
    }

    #[test]
    fn gif_is_kept() {
        let (extension, _) = stored_picture("image/gif", b"GIF89a".to_vec());
        assert_eq!(extension, "gif");
    }
}
