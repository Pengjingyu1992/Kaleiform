//! Home previews are ephemeral, bounded and read off the UI thread. No preview opens a document,
//! follows an SVG's external links, installs profiles, or writes a cache to disk.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{ColorImage, TextureHandle};

use crate::PreviewReadFn;

#[cfg(any(not(target_arch = "wasm32"), test))]
const SIZE: u32 = 320;
#[cfg(any(not(target_arch = "wasm32"), test))]
const MAX_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Default)]
struct Preview {
    image: Option<Arc<ColorImage>>,
    texture: Option<TextureHandle>,
}

#[derive(Default)]
pub(crate) struct Previews {
    files: Vec<String>,
    entries: HashMap<String, Preview>,
    active: bool,
    dirty: bool,
    #[cfg(not(target_arch = "wasm32"))]
    rx: Option<std::sync::mpsc::Receiver<(String, Option<ColorImage>)>>,
}

impl Previews {
    pub fn leave(&mut self) {
        self.active = false;
    }

    pub fn forget_textures(&mut self) {
        for preview in self.entries.values_mut() {
            preview.texture = None;
        }
    }

    pub fn sync(&mut self, files: &[String], ctx: &egui::Context, read: Option<PreviewReadFn>) {
        self.dirty |= !self.active || self.files != files;
        self.active = true;
        self.files = files.to_vec();
        self.entries.retain(|path, _| files.contains(path));
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(rx) = &self.rx {
                loop {
                    match rx.try_recv() {
                        Ok((path, image)) => {
                            if files.contains(&path) {
                                self.entries.insert(path, Preview { image: image.map(Arc::new), texture: None });
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            self.rx = None;
                            break;
                        }
                    }
                }
            }
            // At most one worker and six small textures, even while the recent list changes.
            if self.dirty && self.rx.is_none() {
                self.dirty = false;
                let Some(read) = read.filter(|_| !files.is_empty()) else { return };
                let paths = files.to_vec();
                let ctx = ctx.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                if std::thread::Builder::new()
                    .name("home-previews".into())
                    .spawn(move || {
                        for path in paths {
                            let image = vectorcraft_engine::guard::catch_panic(|| {
                                let bytes = read(&path, MAX_BYTES).ok()?;
                                decode(&path, &bytes)
                            })
                            .ok()
                            .flatten();
                            if tx.send((path, image)).is_err() {
                                break; // The app closed; no more reads are needed.
                            }
                            ctx.request_repaint();
                        }
                        drop(tx);
                        ctx.request_repaint();
                    })
                    .is_ok()
                {
                    self.rx = Some(rx);
                }
                // A failed spawn leaves file-type placeholders, never blocks normal Open.
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (ctx, read); // Browsers cannot read a recent path without a fresh file pick.
    }

    pub fn texture(&mut self, path: &str, ctx: &egui::Context) -> Option<&TextureHandle> {
        let preview = self.entries.get_mut(path)?;
        let image = preview.image.as_ref()?;
        Some(preview.texture.get_or_insert_with(|| ctx.load_texture("recent-preview", image.clone(), egui::TextureOptions::LINEAR)))
    }
}

#[cfg(any(not(target_arch = "wasm32"), test))]
fn raster(bytes: &[u8]) -> Option<ColorImage> {
    use std::io::Cursor;
    let mut reader = image::ImageReader::new(Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let (w, h) = image::ImageDecoder::dimensions(&decoder);
    if u64::from(w) * u64::from(h) > 24_000_000 {
        return None;
    }
    let orientation = image::ImageDecoder::orientation(&mut decoder).unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = image::DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    let image = image.thumbnail(SIZE.min(image.width()), SIZE.min(image.height())).into_rgba8();
    Some(ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw()))
}

#[cfg(any(not(target_arch = "wasm32"), test))]
fn decode(name: &str, bytes: &[u8]) -> Option<ColorImage> {
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    if image::guess_format(bytes).is_ok() {
        return raster(bytes);
    }
    let native = vectorcraft_format::sniff(bytes);
    if native && let Some(preview) = vectorcraft_format::preview(bytes) {
        return raster(&preview);
    }
    if vectorcraft_pdf::is_postscript(bytes) {
        return vectorcraft_eps::thumbnail(bytes).and_then(|p| raster(&p));
    }
    // Unusually large vector files remain openable, but are not parsed just to draw Home.
    if bytes.len() > 4 * 1024 * 1024 {
        return None;
    }
    let doc = if native {
        vectorcraft_format::load(bytes).ok()?
    } else if bytes.starts_with(b"%PDF") {
        let options = vectorcraft_pdf::ImportOptions { max_pages: Some(1), pages: Some(vec![0]), ..Default::default() };
        vectorcraft_pdf::import_with_report(bytes, &options).ok()?.document
    } else if matches!(vectorcraft_engine::cmd::fileio::extension(name).as_str(), "svg" | "svgz") {
        let text = vectorcraft_svg::text_of(bytes).ok()?;
        if text.len() > 4 * 1024 * 1024 {
            return None;
        }
        vectorcraft_svg::import(&text).ok()?
    } else {
        return None;
    };
    if doc.node_count() > 20_000 {
        return None;
    }
    let region = doc.artboards.first().map(|a| a.rect).or_else(|| doc.art_bounds())?;
    let scale = f64::from(SIZE) / region.width().max(region.height());
    vectorcraft_render::raster_size(region, scale).ok()?;
    let image = vectorcraft_render::Renderer::new().render_region(&doc, region, scale, false);
    Some(ColorImage::from_rgba_premultiplied([image.width as usize, image.height as usize], &image.pixels))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(w, h, image::Rgba([220, 30, 50, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    #[test]
    fn raster_and_vector_previews_fit_without_cropping() {
        let image = decode("art.png", &png(640, 160)).unwrap();
        assert_eq!(image.size, [320, 80]);
        assert_eq!(image.pixels[0], egui::Color32::from_rgb(220, 30, 50));
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="200" height="100" fill="#dc1e32"/></svg>"##;
        let image = decode("art.svg", svg).unwrap();
        assert_eq!(image.size, [320, 160]);
        assert_eq!(image.pixels[160 * 80 + 80], egui::Color32::from_rgb(220, 30, 50));
        let doc = vectorcraft_svg::import(std::str::from_utf8(svg).unwrap()).unwrap();
        let native = vectorcraft_format::save(&doc, false);
        assert_eq!(decode("art.vectorcraft", &native).unwrap().size, [320, 160]);
        let pdf = vectorcraft_pdf::export(&doc, &Default::default()).unwrap();
        assert_eq!(decode("art.pdf", &pdf).unwrap().size, [320, 160]);
    }

    #[test]
    fn bad_or_unusually_large_previews_are_optional() {
        assert!(decode("broken.png", b"not a picture").is_none());
        assert!(decode("broken.svg", b"<svg").is_none());
        assert!(decode("unsupported.xyz", b"unknown").is_none());
        assert!(decode("huge.png", &png(6000, 5000)).is_none());
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn background_reads_do_not_block_and_refresh_on_home_entry() {
        let (ready, wait) = std::sync::mpsc::sync_channel(0);
        let (release, blocked) = std::sync::mpsc::sync_channel(0);
        let blocked = std::sync::Mutex::new(blocked);
        let read: PreviewReadFn = Arc::new(move |_, limit| {
            assert_eq!(limit, MAX_BYTES);
            ready.send(()).unwrap();
            blocked.lock().unwrap().recv().unwrap();
            Ok(png(80, 40))
        });
        let files = vec!["art.png".into()];
        let mut previews = Previews::default();
        let ctx = egui::Context::default();
        previews.sync(&files, &ctx, Some(read.clone()));
        wait.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert!(previews.texture("art.png", &ctx).is_none());
        // Another frame cannot spawn another reader, or wait on the blocked worker.
        previews.sync(&files, &ctx, Some(read.clone()));
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while previews.rx.is_some() {
            assert!(std::time::Instant::now() < deadline);
            previews.sync(&files, &ctx, Some(read.clone()));
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(previews.texture("art.png", &ctx).unwrap().size(), [80, 40]);
        previews.forget_textures();
        assert!(previews.entries["art.png"].texture.is_none());
        assert!(previews.texture("art.png", &egui::Context::default()).is_some());
        previews.leave();
        previews.sync(&files, &ctx, Some(read));
        wait.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        release.send(()).unwrap();
        previews.sync(&[], &ctx, None);
        assert!(previews.entries.is_empty(), "clearing recents also clears their preview cache");
    }
}
