//! The macOS icon is a padded, rounded tile, not the full-bleed interface logo.

use image::{RgbaImage, imageops::FilterType};

fn macos_icon(source: &RgbaImage) -> Result<RgbaImage, String> {
    let side = source.width();
    if side != source.height() || !(16..=4096).contains(&side) {
        return Err("app icon master must be square, between 16 and 4096 px".into());
    }
    let margin = (f64::from(side) * 0.09).round() as u32;
    let tile_side = side - margin * 2;
    let tile = image::imageops::resize(source, tile_side, tile_side, FilterType::Lanczos3);
    let half = f64::from(tile_side) / 2.0;
    let radius = f64::from(tile_side) * 0.225;
    let mut out = RgbaImage::new(side, side);
    for (x, y, pixel) in tile.enumerate_pixels() {
        // Signed distance to the rounded tile gives a one-pixel antialiased alpha edge.
        let dx = (f64::from(x) + 0.5 - half).abs() - (half - radius);
        let dy = (f64::from(y) + 0.5 - half).abs() - (half - radius);
        let distance = dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius;
        let coverage = (0.5 - distance).clamp(0.0, 1.0);
        let mut color = *pixel;
        color.0[3] = (f64::from(color.0[3]) * coverage).round() as u8;
        if color.0[3] != 0 {
            out.put_pixel(x + margin, y + margin, color);
        }
    }
    Ok(out)
}

pub fn run(args: &[&str]) -> Result<(), String> {
    let [input, output] = args else {
        return Err("usage: cargo xtask macos-icon <master.png> <out.png>".into());
    };
    let source = image::open(input).map_err(|e| format!("{input}: {e}"))?.to_rgba8();
    macos_icon(&source)?.save(output).map_err(|e| format!("{output}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn has_transparent_margins_and_rounded_corners_without_changing_the_center() {
        let color = Rgba([32, 90, 180, 255]);
        let source = RgbaImage::from_pixel(100, 100, color);
        let icon = macos_icon(&source).unwrap();
        assert_eq!(icon.dimensions(), (100, 100));
        for (x, y, pixel) in icon.enumerate_pixels() {
            if !(9..91).contains(&x) || !(9..91).contains(&y) {
                assert_eq!(pixel.0[3], 0);
            }
        }
        assert_eq!(icon.get_pixel(9, 9).0[3], 0);
        assert_eq!(*icon.get_pixel(50, 50), color);
        assert_eq!(*icon.get_pixel(50, 9), color);
        assert!(icon.pixels().any(|pixel| (1..255).contains(&pixel.0[3])));
    }

    #[test]
    fn rejects_non_square_or_invalid_sizes() {
        for (width, height) in [(100, 80), (0, 0), (8, 8)] {
            assert!(macos_icon(&RgbaImage::new(width, height)).is_err());
        }
    }
}
