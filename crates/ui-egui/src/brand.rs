//! The square Kaleiform logo (`assets/app-icon/`, see its README), decoded once per context into a
//! mipmapped texture so it stays crisp in the About box.

use egui::{Color32, Context, Id, Rect, TextureHandle, TextureOptions, Ui, pos2};

/// 512 px wide: sharp at 66 pt on a 2x display.
const LOGO_PNG: &[u8] = include_bytes!("../../../assets/app-icon/kaleiform-runtime.png");

/// The icon's pixels (one transparent pixel if it couldn't be decoded, which a test rules out).
fn decode() -> egui::ColorImage {
    match image::load_from_memory_with_format(LOGO_PNG, image::ImageFormat::Png) {
        Ok(img) => {
            let img = img.to_rgba8();
            egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
        }
        Err(_) => egui::ColorImage::filled([1, 1], Color32::TRANSPARENT),
    }
}

/// The icon texture, uploaded on first use and kept in the context.
fn texture(ctx: &Context) -> TextureHandle {
    let id = Id::new("kaleiform-brand-logo");
    if let Some(tex) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return tex;
    }
    let options = TextureOptions { mipmap_mode: Some(egui::TextureFilter::Linear), ..TextureOptions::LINEAR };
    let tex = ctx.load_texture("kaleiform-brand-logo", decode(), options);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}

/// Paint the square logo into `r` (1:1).
pub fn paint_logo(ui: &Ui, r: Rect) {
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    ui.painter().image(texture(ui.ctx()).id(), r, uv, Color32::WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VectorcraftApp;

    #[test]
    fn the_logo_decodes_to_an_opaque_square_image() {
        let img = decode();
        assert_eq!(img.size, [512, 512]);
        assert_eq!(img.pixels[0].a(), 255);
        assert_eq!(img.pixels[170 * 512 + 256].a(), 255);
    }

    #[test]
    fn the_logo_painter_reuses_one_cached_texture() {
        let ctx = Context::default();
        crate::theme::install_fonts(&ctx);
        let frame = || {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(66.0, 66.0), egui::Sense::hover());
                paint_logo(ui, r);
            });
            let uploads = out.textures_delta.set.values().flat_map(|d| d.iter()).filter(|d| d.image.size() == [512, 512]).count();
            out.textures_delta.clear();
            let logos: Vec<Rect> = out
                .shapes
                .iter()
                .filter_map(|c| match &c.shape {
                    egui::Shape::Mesh(m) if m.texture_id == texture(&ctx).id() => Some(m.calc_bounds()),
                    _ => None,
                })
                .collect();
            (uploads, logos)
        };
        let (uploads, logos) = frame();
        assert_eq!(uploads, 1);
        assert_eq!(logos.len(), 1);
        assert_eq!(logos[0].size(), egui::vec2(66.0, 66.0));
        // Later frames reuse the texture.
        let (uploads, logos) = frame();
        assert_eq!((uploads, logos.len()), (0, 1));
    }

    #[test]
    fn the_app_bar_does_not_upload_the_brand_logo() {
        let ctx = Context::default();
        crate::theme::install_fonts(&ctx);
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        for integrated in [false, true] {
            app.integrated_titlebar = integrated;
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| crate::chrome::app_bar(&mut app, ui));
            assert!(ctx.data(|d| d.get_temp::<TextureHandle>(Id::new("kaleiform-brand-logo"))).is_none());
            out.textures_delta.clear();
        }
    }
}
