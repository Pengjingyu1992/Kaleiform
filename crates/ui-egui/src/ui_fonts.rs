//! Fallback fonts for the app's own text: layer and file names (and anything else the UI shows) in
//! Chinese, Japanese, Korean or other scripts the UI fonts lack are drawn in an installed font that
//! covers them. Builds made with craft-fonts (`CRAFT_FONTS_DIR`, all releases) carry Japanese fonts,
//! which `theme::install_fonts` adds after the UI fonts, so Japanese never gets here; nothing else
//! is bundled for them: the first time a frame paints a character the UI fonts
//! don't have, an installed font covering it is looked for (in the background on native) and added
//! to egui's families as their last fallback, from the next frame on. Text the UI fonts cover costs
//! no font loading or memory. On the web, which has no system fonts, such characters stay
//! missing-glyph boxes (Japanese too, unless built with craft-fonts).

use std::collections::HashSet;
use std::sync::Arc;

use egui::epaint::Shape;
use egui::epaint::text::FontTweak;
use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use egui::{FontData, FontFamily, FontId, LayerId};
use vectorcraft_text::{FontDb, FontFace};

type FallbackFace = (FontFamily, Arc<FontFace>);

#[derive(Default)]
pub(crate) struct UiFonts {
    /// Characters looked at already: the UI fonts have them, a fallback was added (or is being
    /// looked for), or no font has them.
    seen: HashSet<(FontFamily, char)>,
    /// The fonts added to egui, by name.
    added: HashSet<(FontFamily, String)>,
    /// The faces covering the characters being looked for (native: looked for on a thread).
    #[cfg(not(target_arch = "wasm32"))]
    pending: Option<std::sync::mpsc::Receiver<Vec<FallbackFace>>>,
}

impl UiFonts {
    /// After a frame is laid out: add the fallback fonts found since the last frame, and look for
    /// fonts covering the characters this frame painted that the UI fonts lack.
    pub(crate) fn frame(&mut self, ctx: &egui::Context, root: LayerId) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(faces) = self.pending.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.pending = None;
            self.install(ctx, &faces);
        }
        let missing = self.missing(ctx, root);
        if missing.is_empty() {
            return;
        }
        let language = crate::i18n::current().code();
        let find = move || {
            let mut faces: Vec<FallbackFace> = vec![];
            for (family, c) in missing {
                // The face is looked up at the weight the character was painted in, so a semibold
                // heading gets a semibold CJK face rather than the family's regular one.
                if let Some(f) = fallback_face(FontDb::global(), c, family_weight(&family), language)
                    && !faces.iter().any(|(fam, x)| *fam == family && x.id() == f.id())
                {
                    faces.push((family, f));
                }
            }
            faces
        };
        // Native: the installed fonts are read off the UI thread. Characters met meanwhile wait for
        // the next search (they aren't marked seen until then).
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let repaint = ctx.clone();
            let spawned = std::thread::Builder::new().name("ui-font-fallback".into()).spawn(move || {
                // The receiver is gone only when the app is.
                let _ = tx.send(find());
                repaint.request_repaint();
            });
            if spawned.is_ok() {
                self.pending = Some(rx);
            }
        }
        // The web has only the loaded fonts to look through: quick, on this thread.
        #[cfg(target_arch = "wasm32")]
        self.install(ctx, &find());
    }

    /// Characters painted this frame that the UI fonts lack and haven't been looked for, now
    /// marked seen. None while a search is running.
    fn missing(&mut self, ctx: &egui::Context, root: LayerId) -> Vec<(FontFamily, char)> {
        #[cfg(not(target_arch = "wasm32"))]
        if self.pending.is_some() {
            return vec![];
        }
        let mut layers: Vec<LayerId> = ctx.memory(|m| m.layer_ids().collect());
        if !layers.contains(&LayerId::background()) {
            layers.push(LayerId::background());
        }
        // Native root panels need not appear in Memory's list of floating areas.
        if !layers.contains(&root) {
            layers.push(root);
        }
        let mut chars = HashSet::new();
        let seen = &self.seen;
        let mut note = |text: &str, family: &FontFamily, _size: f32| {
            // Most UI text is ASCII, which the UI fonts cover.
            if !text.is_ascii() {
                chars.extend(
                    text.chars()
                        .filter(|c| !c.is_ascii() && !c.is_whitespace() && !c.is_control())
                        .map(|c| (family.clone(), c))
                        .filter(|key| !seen.contains(key)),
                );
            }
        };
        ctx.graphics(|g| {
            for l in &layers {
                for s in g.get(*l).into_iter().flat_map(|list| list.all_entries()) {
                    painted_text(&s.shape, &mut note);
                }
            }
        });
        let missing = ctx.fonts_mut(|f| chars.iter().filter(|(family, c)| !f.has_glyph(&FontId::new(13.0, family.clone()), *c)).cloned().collect());
        self.seen.extend(chars);
        missing
    }

    /// Add each face only to the family that requested its weight.
    fn install(&mut self, ctx: &egui::Context, faces: &[FallbackFace]) {
        for (family, face) in faces {
            // egui ignores add_font when its name already exists, including new family links.
            // A fallback discovered later for another UI weight therefore needs its own name.
            let name = format!("{} {} #{} ({family})", face.family, face.style, face.face_index());
            if !self.added.insert((family.clone(), name.clone())) {
                continue;
            }
            // The face's file data, shared with the process-wide font database (which keeps every
            // face it loads for the session) instead of copied: one handle is leaked per font added.
            let face: &'static FontFace = Box::leak(Box::new(face.clone()));
            // Align fallback metrics with the Latin UI baseline without changing the point size.
            let mut tweak = latin_face().map_or_else(FontTweak::default, |latin| baseline_tweak(face, &latin));
            // Named instances share bytes: retain their axes so a requested semibold does not
            // silently render at the variable font's default weight.
            tweak.coords = egui::epaint::text::VariationCoords::new(face.variations().iter().map(|(tag, value)| (tag, *value)));
            let data = FontData { index: face.face_index(), ..FontData::from_static(face.file_data()) }.tweak(tweak);
            let families = vec![InsertFontFamily { family: family.clone(), priority: FontPriority::Lowest }];
            ctx.add_font(FontInsert { name, data, families });
        }
        if !faces.is_empty() {
            ctx.request_repaint();
        }
    }

    /// Wait for the font search running, if any, and add what it found (tests).
    #[cfg(test)]
    pub(crate) fn finish(&mut self, ctx: &egui::Context) {
        if let Some(faces) = self.pending.take().and_then(|rx| rx.recv().ok()) {
            self.install(ctx, &faces);
        }
    }
}

fn fallback_face(db: &FontDb, c: char, weight: f32, language: &str) -> Option<Arc<FontFace>> {
    if language == "zh-hans" && cfg!(any(target_os = "windows", target_os = "linux")) {
        // A generic CJK fallback can choose the Japanese face of a collection on Linux.
        // Use local simplified-Chinese UI forms first; never bundle proprietary OS fonts.
        let family = if cfg!(target_os = "windows") { "Microsoft YaHei UI" } else { "Noto Sans CJK SC" };
        let style = if weight >= 600.0 {
            "Bold"
        } else if weight >= 500.0 {
            "Medium"
        } else {
            "Regular"
        };
        if let Some(face) = db.face(family, style)
            && face.family.eq_ignore_ascii_case(family)
            && face.covers(c)
        {
            return Some(face);
        }
    }
    db.face_covering_weighted(c, weight)
}

/// Call `f` with the text of `shape` and the shapes it holds.
fn painted_text(shape: &Shape, f: &mut impl FnMut(&str, &FontFamily, f32)) {
    match shape {
        Shape::Text(t) => {
            let text = t.galley.text();
            for section in &t.galley.job.sections {
                let Some(slice) = text.get(section.byte_range.start.0..section.byte_range.end.0) else { continue };
                f(slice, &section.format.font_id.family, section.format.font_id.size);
            }
        }
        Shape::Vec(v) => v.iter().for_each(|s| painted_text(s, f)),
        _ => {}
    }
}

fn baseline_tweak(face: &FontFace, family: &FontFace) -> FontTweak {
    FontTweak { scale: 1.0, y_offset_factor: baseline_tweak_factor(face, family), ..Default::default() }
}

fn baseline_tweak_factor(face: &FontFace, family: &FontFace) -> f32 {
    let em = |f: &FontFace| {
        let (ascent, descent) = f.vertical_metrics();
        let upem = f.units_per_em().max(1.0);
        (ascent / upem, (ascent + descent) / upem)
    };
    let (face_ascent, face_height) = em(face);
    let (family_ascent, family_height) = em(family);
    baseline_factor(face_ascent, face_height, family_ascent, family_height)
}

fn baseline_factor(face_ascent: f64, face_height: f64, family_ascent: f64, family_height: f64) -> f32 {
    // Positive `y_offset_factor` moves a glyph down, and epaint counts the tweak in the pen
    // position as well as the raster offset, so the correction is twice the fallback's own
    // displacement, with the sign that brings it back to the family's ink.
    (2.0 * (face_ascent - family_ascent + 0.5 * (family_height - face_height))) as f32
}

fn latin_face() -> Option<Arc<FontFace>> {
    static METRICS: std::sync::OnceLock<Option<Arc<FontFace>>> = std::sync::OnceLock::new();
    METRICS
        .get_or_init(|| {
            let db = vectorcraft_text::FontDb::with_font_dirs(Vec::new());
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            {
                db.add_font(include_bytes!("../../../assets/fonts/Inter-Regular.ttf").to_vec());
                db.face("Inter", "Regular")
            }
            #[cfg(not(any(target_os = "windows", target_os = "linux")))]
            {
                db.add_font(include_bytes!("../../../assets/fonts/SourceSans3-Regular.ttf").to_vec());
                db.face("Source Sans 3", "Regular")
            }
        })
        .clone()
}

fn family_weight(family: &FontFamily) -> f32 {
    if *family == FontFamily::Name(crate::theme::FONT_UI_SEMIBOLD.into()) {
        600.0
    } else if *family == FontFamily::Name(crate::theme::FONT_UI_MEDIUM.into()) {
        500.0
    } else {
        400.0
    }
}

pub(crate) fn craft_font_data(font: &vectorcraft_text::CraftFont) -> FontData {
    static DB: std::sync::OnceLock<FontDb> = std::sync::OnceLock::new();
    let db = DB.get_or_init(|| FontDb::with_font_dirs(Vec::new()));
    let tweak = db.face(font.family, font.style).zip(latin_face()).map_or_else(FontTweak::default, |(face, latin)| baseline_tweak(&face, &latin));
    FontData::from_static(font.bytes).tweak(tweak)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_characters_are_tracked_per_family_not_font_size() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut fonts = UiFonts::default();
        ctx.run_ui(Default::default(), |ui| {
            ui.label(egui::RichText::new("\u{10FFFD}").font(FontId::proportional(26.0)));
            ui.label(egui::RichText::new("\u{10FFFD}").font(crate::theme::semibold(12.0)));
            ui.label(egui::RichText::new("\u{10FFFD}").font(FontId::new(12.0, FontFamily::Name(crate::theme::FONT_UI_MEDIUM.into()))));
            let missing = fonts.missing(ui.ctx(), ui.layer_id());
            assert_eq!(missing.len(), 3);
            let mut weights: Vec<_> = missing.iter().map(|(family, _)| family_weight(family) as u32).collect();
            weights.sort_unstable();
            assert_eq!(weights, [400, 500, 600]);
        })
        .textures_delta
        .clear();
    }

    /// Run a frame painting `text`, then the fallback search after it.
    fn frame(ctx: &egui::Context, fonts: &mut UiFonts, text: &str) {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.label(text);
            fonts.frame(ui.ctx(), ui.layer_id());
        });
        out.textures_delta.clear();
    }

    fn has_glyph(ctx: &egui::Context, c: char) -> bool {
        ctx.fonts_mut(|f| f.has_glyph(&FontId::new(13.0, FontFamily::Proportional), c))
    }

    #[test]
    fn latin_text_loads_no_fonts() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut fonts = UiFonts::default();
        frame(&ctx, &mut fonts, "Layer 1 — Café");
        assert!(fonts.pending.is_none() && fonts.added.is_empty());
    }

    #[test]
    fn root_panel_titles_request_their_own_cjk_weight() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut fonts = UiFonts::default();
        let root = LayerId::new(egui::Order::Background, egui::Id::new("native-root-panel"));
        ctx.run_ui(Default::default(), |ui| {
            ui.scope_builder(egui::UiBuilder::new().layer_id(root), |ui| {
                ui.label(egui::RichText::new("品牌画板").font(FontId::new(12.0, FontFamily::Name(crate::theme::FONT_UI_MEDIUM.into()))));
            });
            fonts.frame(ui.ctx(), root);
        })
        .textures_delta
        .clear();
        for c in "品牌画板".chars() {
            assert!(fonts.seen.contains(&(FontFamily::Name(crate::theme::FONT_UI_MEDIUM.into()), c)), "root title was not scanned: {c}");
        }
        fonts.finish(&ctx);
    }

    #[test]
    fn a_shared_fallback_can_be_added_to_another_family_in_a_later_frame() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
        let mut fonts = UiFonts::default();
        let face = FontDb::global().face("Source Sans 3", "Regular").unwrap();
        let families = [FontFamily::Name(crate::theme::FONT_UI_SEMIBOLD.into()), FontFamily::Name(crate::theme::FONT_UI_MEDIUM.into())];
        for family in &families {
            fonts.install(&ctx, &[(family.clone(), face.clone())]);
            ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
        }
        for family in &families {
            assert!(
                ctx.fonts(|f| f.definitions().families.get(family).unwrap().iter().any(|name| name.starts_with("Source Sans 3 Regular #"))),
                "later family lost its link to the shared face: {family}"
            );
        }
    }

    #[test]
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    fn simplified_chinese_ui_prefers_the_installed_locale_face() {
        let db = FontDb::global();
        let family = if cfg!(target_os = "windows") { "Microsoft YaHei UI" } else { "Noto Sans CJK SC" };
        if !db.has_family(family) {
            return; // Minimal CI hosts need not have a CJK system font.
        }
        for weight in [400.0, 500.0, 600.0] {
            let face = fallback_face(db, '构', weight, "zh-hans").unwrap();
            assert_eq!(face.family, family);
            assert!(face.covers('构'));
        }
    }

    #[test]
    fn japanese_renders_with_the_craft_fonts_without_installed_fonts() {
        if !vectorcraft_text::CRAFT_FONTS.iter().any(|f| f.is_japanese()) {
            eprintln!("skipped: built without craft-fonts (set CRAFT_FONTS_DIR to a craft-fonts checkout to run it)");
            return;
        }
        let text = "日本語の文字";
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut fonts = UiFonts::default();
        frame(&ctx, &mut fonts, text);
        // Real glyphs (no tofu) in every family from the first frame, with no installed font looked for.
        assert!(fonts.pending.is_none() && fonts.added.is_empty(), "no system-font search");
        let families: Vec<FontFamily> = ctx.fonts(|f| f.definitions().families.keys().cloned().collect());
        for family in families {
            let stack = ctx.fonts(|f| f.definitions().families.get(&family).cloned().unwrap_or_default());
            assert!(stack.last().is_some_and(|n| n.starts_with("craft-fonts ")), "{family:?}: craft-fonts last: {stack:?}");
            for c in text.chars() {
                assert!(ctx.fonts_mut(|f| f.has_glyph(&FontId::new(13.0, family.clone()), c)), "{family:?} draws {c}");
            }
        }
        // The UI prefers BIZ UDPGothic.
        let stack = ctx.fonts(|f| f.definitions().families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
        let first = stack.iter().find(|n| n.starts_with("craft-fonts ")).cloned().unwrap_or_default();
        assert_eq!(first, "craft-fonts BIZ UDPGothic Regular", "{stack:?}");
    }

    #[test]
    fn the_ui_works_without_the_craft_fonts() {
        // Built either way, the fonts install and Japanese text lays out (as installed-font
        // fallbacks or missing-glyph boxes when no font has it).
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut fonts = UiFonts::default();
        frame(&ctx, &mut fonts, "日本語の文字 Layer 1");
        fonts.finish(&ctx);
        frame(&ctx, &mut fonts, "日本語の文字 Layer 1");
        if vectorcraft_text::CRAFT_FONTS.is_empty() {
            let names: Vec<String> = ctx.fonts(|f| f.definitions().font_data.keys().cloned().collect());
            assert!(names.iter().all(|n| !n.starts_with("craft-fonts")), "{names:?}");
        }
        assert!(has_glyph(&ctx, 'L'));
    }

    #[test]
    fn cjk_names_get_an_installed_fallback_font() {
        let text = "Logo 标志 ロゴ 로고";
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut fonts = UiFonts::default();
        frame(&ctx, &mut fonts, text);
        if vectorcraft_text::CRAFT_FONTS.iter().any(|f| f.scripts.contains(&"Hans")) {
            assert!(has_glyph(&ctx, '标'), "Chinese is available without waiting for system fonts");
        }
        fonts.finish(&ctx);
        // The fonts arrive with the next frame.
        frame(&ctx, &mut fonts, text);
        let installed: Vec<char> = text.chars().filter(|c| !c.is_ascii() && FontDb::global().face_covering(*c).is_some()).collect();
        if installed.is_empty() {
            eprintln!("no installed font covers {text}: nothing to check");
            return;
        }
        for c in installed {
            assert!(has_glyph(&ctx, c), "{c} has a glyph");
        }
        // Looked for once: a later frame starts no search.
        frame(&ctx, &mut fonts, text);
        assert!(fonts.pending.is_none());
    }
}
