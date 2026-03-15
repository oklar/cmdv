use image::ImageReader;
use std::io::Cursor;

pub fn decode_to_rgba(image_data: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let img = ImageReader::new(Cursor::new(image_data))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    Ok((rgba.into_raw(), width, height))
}

pub fn convert_to_webp(image_data: &[u8], quality: f32) -> Result<Vec<u8>, String> {
    let img = ImageReader::new(Cursor::new(image_data))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;

    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    let encoder = webp::Encoder::from_rgba(&rgba, width, height);
    let webp_data = encoder.encode(quality);

    Ok(webp_data.to_vec())
}

pub fn rgba_to_webp(rgba: &[u8], width: u32, height: u32, quality: f32) -> Result<Vec<u8>, String> {
    let expected = (width * height * 4) as usize;
    if rgba.len() != expected {
        return Err(format!(
            "RGBA buffer size mismatch: expected {} bytes ({}x{}x4), got {}",
            expected, width, height, rgba.len()
        ));
    }

    let encoder = webp::Encoder::from_rgba(rgba, width, height);
    let webp_data = encoder.encode(quality);

    Ok(webp_data.to_vec())
}

pub fn is_image_data(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }
    // PNG
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return true;
    }
    // JPEG
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return true;
    }
    // GIF
    if data.starts_with(b"GIF8") {
        return true;
    }
    // WebP
    if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        return true;
    }
    // BMP
    if data.starts_with(&[0x42, 0x4D]) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_png_header() {
        assert!(is_image_data(&[0x89, 0x50, 0x4E, 0x47, 0x00]));
    }

    #[test]
    fn detect_jpeg_header() {
        assert!(is_image_data(&[0xFF, 0xD8, 0xFF, 0xE0]));
    }

    #[test]
    fn detect_gif_header() {
        assert!(is_image_data(b"GIF89a"));
    }

    #[test]
    fn reject_non_image() {
        assert!(!is_image_data(b"Hello world"));
        assert!(!is_image_data(&[0x00, 0x01]));
    }

    #[test]
    fn convert_dynamic_image_to_webp() {
        use image::{RgbaImage, DynamicImage};
        use std::io::Cursor;

        let img = RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
        let dyn_img = DynamicImage::ImageRgba8(img);
        let mut png_buf = Vec::new();
        dyn_img
            .write_to(&mut Cursor::new(&mut png_buf), image::ImageFormat::Png)
            .unwrap();

        let result = convert_to_webp(&png_buf, 80.0);
        assert!(result.is_ok());
        let webp = result.unwrap();
        assert!(!webp.is_empty());
    }

    #[test]
    fn convert_raw_rgba_to_webp() {
        let width: u32 = 4;
        let height: u32 = 4;
        let pixel = [0u8, 128, 255, 255]; // RGBA
        let rgba: Vec<u8> = pixel.iter().copied().cycle().take((width * height * 4) as usize).collect();

        let result = rgba_to_webp(&rgba, width, height, 80.0);
        assert!(result.is_ok());
        let webp = result.unwrap();
        assert!(!webp.is_empty());
        assert!(webp.len() >= 12);
        assert_eq!(&webp[0..4], b"RIFF");
        assert_eq!(&webp[8..12], b"WEBP");
    }
}
