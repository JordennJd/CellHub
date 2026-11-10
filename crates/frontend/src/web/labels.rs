//! One-time rasterization of the static map labels into a GPU atlas.

use super::error::error;
use wasm_bindgen::prelude::*;

pub(super) const ATLAS_WIDTH: u32 = 512;
pub(super) const ATLAS_HEIGHT: u32 = 128;
const LABELS: [&str; 4] = [
    "СОЛНЕЧНОЕ МЕЛКОВОДЬЕ",
    "ПИТАТЕЛЬНЫЙ ПОЯС",
    "ТЁМНЫЕ ГЛУБИНЫ",
    "РАДИАЦИОННЫЙ КАРМАН",
];

/// Rasterize the four static map labels once, in Rust, into the atlas that the
/// fragment shader samples. The browser only ever uploads the resulting bytes
/// to the GPU; no JavaScript canvas is involved in drawing.
pub(super) fn rasterize_labels() -> Result<Vec<u8>, JsValue> {
    use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
    let font = FontRef::try_from_slice(epaint_default_fonts::HACK_REGULAR).map_err(error)?;
    let scale = PxScale::from(20.);
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();
    let mut pixels = vec![0u8; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize];
    for (row, label) in LABELS.iter().enumerate() {
        let baseline = row as f32 * 32. + ascent;
        let mut pen = 0.;
        for ch in label.chars() {
            let id = font.glyph_id(ch);
            let glyph = id.with_scale_and_position(scale, point(pen, baseline));
            if let Some(outline) = font.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                outline.draw(|gx, gy, coverage| {
                    let x = bounds.min.x as i32 + gx as i32;
                    let y = bounds.min.y as i32 + gy as i32;
                    if x < 0 || y < 0 || x >= ATLAS_WIDTH as i32 || y >= ATLAS_HEIGHT as i32 {
                        return;
                    }
                    let index = ((y as u32 * ATLAS_WIDTH + x as u32) * 4) as usize;
                    let src = coverage.clamp(0., 1.);
                    let dst = pixels[index + 3] as f32 / 255.;
                    pixels[index] = 255;
                    pixels[index + 1] = 255;
                    pixels[index + 2] = 255;
                    pixels[index + 3] = ((src + dst * (1. - src)).clamp(0., 1.) * 255.) as u8;
                });
            }
            pen += scaled.h_advance(id) + 3.;
        }
    }
    Ok(pixels)
}
