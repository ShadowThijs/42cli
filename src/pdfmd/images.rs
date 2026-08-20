//! Image XObjects → PNG bytes for the extracted figures.
//!
//! `Stream::data` already inflates Flate (applying PNG predictors) and
//! decodes DCT JPEGs to samples, so what arrives here is raw 8-bit pixel
//! data. A soft mask, when present, becomes the alpha channel.

use pdf::object::Resolve;
use pdf::object::XObject;

/// Encode one image XObject as PNG; `None` for anything exotic (masks,
/// non-8-bit, truncated data) so callers can skip the figure.
pub fn to_png(xobject: &XObject, resolve: &impl Resolve) -> Option<Vec<u8>> {
    let XObject::Image(image) = xobject else {
        return None;
    };
    if image.image_mask || image.bits_per_component.unwrap_or(8) != 8 {
        return None;
    }
    let width = image.width as usize;
    let height = image.height as usize;
    if width == 0 || height == 0 || width * height > 40_000_000 {
        return None;
    }
    let data = image.inner.data(resolve).ok()?;
    let channels = match data.len().div_ceil(width * height) {
        1..=4 => data.len() / (width * height),
        _ => return None,
    };

    // Normalize to RGB8.
    let rgb: Vec<u8> = match channels {
        1 => gray_to_rgb(&data),
        3 => data.to_vec(),
        4 => cmyk_to_rgb(&data),
        _ => return None,
    };

    // Soft mask → alpha channel.
    let alpha = image.smask.and_then(|mask| {
        let mask = resolve.get(mask).ok()?;
        let data = mask.data(resolve).ok()?;
        (data.len() >= width * height).then(|| data[..width * height].to_vec())
    });
    let _ = &alpha;

    encode_png(width, height, &rgb, alpha.as_deref())
}

fn gray_to_rgb(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() * 3);
    for &v in data {
        out.extend_from_slice(&[v, v, v]);
    }
    out
}

fn cmyk_to_rgb(data: &[u8]) -> Vec<u8> {
    // JPEG-CMYK arrives inverted for Adobe markers; the naive conversion is
    // close enough for subject figures.
    let mut out = Vec::with_capacity(data.len() / 4 * 3);
    for chunk in data.chunks_exact(4) {
        let [c, m, y, k] = [chunk[0] as f32, chunk[1] as f32, chunk[2] as f32, chunk[3] as f32];
        let (r, g, b) = (
            (255.0 - (c + k).min(255.0)) as u8,
            (255.0 - (m + k).min(255.0)) as u8,
            (255.0 - (y + k).min(255.0)) as u8,
        );
        out.extend_from_slice(&[r, g, b]);
    }
    out
}

fn encode_png(width: usize, height: usize, rgb: &[u8], alpha: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut buffer = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buffer, width as u32, height as u32);
        encoder.set_color(match alpha {
            Some(_) => png::ColorType::Rgba,
            None => png::ColorType::Rgb,
        });
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        let pixels: Vec<u8> = match alpha {
            Some(alpha) => rgb
                .chunks_exact(3)
                .zip(alpha)
                .flat_map(|(rgb, &a)| [rgb[0], rgb[1], rgb[2], a])
                .collect(),
            None => rgb.to_vec(),
        };
        writer.write_image_data(&pixels).ok()?;
    }
    Some(buffer)
}
