//! Design tokens, native macOS semantic colours, fonts and egui style.

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostColors {
    pub window: [u8; 4],
    pub control: [u8; 4],
    pub label: [u8; 4],
    pub secondary: [u8; 4],
    pub disabled: [u8; 4],
    pub separator: [u8; 4],
    pub under_page: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostAppearance {
    pub accent: [u8; 3],
    pub colors: Option<HostColors>,
    pub reduce_transparency: bool,
    pub reduce_motion: bool,
}

/// Use the same neutral desktop surfaces as the Mac workspace when a host has no semantic
/// colour provider. Keep the backing opaque; this does not claim compositor blur support.
pub fn ensure_desktop_appearance(ctx: &egui::Context, brightness: Brightness, glass: bool) {
    if ctx.data(|d| d.get_temp::<HostAppearance>(egui::Id::new("host-appearance"))).is_none() {
        set_host_appearance(
            ctx,
            HostAppearance { accent: [0, 122, 255], colors: None, reduce_transparency: false, reduce_motion: false },
            brightness,
            glass,
        );
    }
}

pub fn set_host_appearance(ctx: &egui::Context, appearance: HostAppearance, brightness: Brightness, glass: bool) {
    let id = egui::Id::new("host-appearance");
    if ctx.data(|d| d.get_temp::<HostAppearance>(id)) == Some(appearance) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(id, appearance));
    apply_material(ctx, brightness, glass);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Brightness {
    Dark,
    #[default]
    MediumDark,
    MediumLight,
    Light,
}

impl Brightness {
    pub const ALL: [Brightness; 4] = [Brightness::Dark, Brightness::MediumDark, Brightness::MediumLight, Brightness::Light];
    pub fn label(self) -> &'static str {
        match self {
            Brightness::Dark => "Dark",
            Brightness::MediumDark => "Medium Dark",
            Brightness::MediumLight => "Medium Light",
            Brightness::Light => "Light",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Brightness::Dark => "dark",
            Brightness::MediumDark => "mediumDark",
            Brightness::MediumLight => "mediumLight",
            Brightness::Light => "light",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.id().eq_ignore_ascii_case(s) || b.label().eq_ignore_ascii_case(s))
    }
}

/// All colours and metrics the UI uses. Never hard-code a colour in a widget.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub dark: bool,
    pub glass: bool,
    /// Chrome over the native window material; controls keep an opaque backing.
    pub surface: Color32,
    pub glass_tint: Color32,
    pub glass_edge: Color32,
    pub app_bar: Color32,
    pub panel: Color32,
    pub panel_darker: Color32,
    pub input: Color32,
    pub input_border: Color32,
    pub border: Color32,
    pub divider: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_disabled: Color32,
    pub icon: Color32,
    pub hover: Color32,
    pub tool_active: Color32,
    pub accent: Color32,
    pub accent_strong: Color32,
    pub accent_text: Color32,
    pub row_selected: Color32,
    pub pasteboard: Color32,
    pub ruler: Color32,
    pub ruler_tick: Color32,
    pub smart_guide: Color32,
    pub guide: Color32,
    pub measure_bg: Color32,
    pub button: Color32,
    pub radius: u8,
    /// Field values, button labels, link labels.
    pub text_strong: Color32,
    /// Panel tab strips, doc-tab bar, dock header.
    pub tab_strip: Color32,
    /// Outline of secondary (outlined) buttons.
    pub button_border: Color32,
    /// Default Layer 1 selection colour and selected-anchor fill.
    pub selection: Color32,
    pub anchor_selected: Color32,
    /// Active item text in tool flyouts.
    pub flyout_active: Color32,
    /// Hint-bar / tab-bar heights etc. scale with this base font size.
    pub font_size: f32,
    /// Window Close button (custom title bar) when hovered, and its glyph there.
    pub caption_close: Color32,
    pub caption_close_text: Color32,
    /// What preview images mark (Flattener Preview highlights).
    pub highlight: Color32,
    /// The bleed outline around artboards (Document Setup → Bleed).
    pub bleed: Color32,
    /// Error marks (a missing linked file).
    pub error: Color32,
    /// Warning marks (a modified linked file).
    pub warning: Color32,
}

impl Tokens {
    pub fn for_brightness(b: Brightness) -> Self {
        let hex = |s: u32| Color32::from_rgb((s >> 16) as u8, (s >> 8) as u8, s as u8);
        let base = Tokens {
            dark: true,
            glass: false,
            surface: hex(0x323232),
            glass_tint: hex(0x323232),
            glass_edge: hex(0x464646),
            app_bar: hex(0x262626),
            panel: hex(0x323232),
            panel_darker: hex(0x282828),
            input: hex(0x1f1f1f),
            input_border: hex(0x464646),
            border: hex(0x1b1b1b),
            divider: hex(0x3f3f3f),
            text: hex(0xd8d8d8),
            text_dim: hex(0xa8a8a8),
            text_disabled: hex(0x6e6e6e),
            icon: hex(0xcfcfcf),
            hover: hex(0x444444),
            tool_active: hex(0x1d1d1d),
            accent: hex(0x378ef0),
            accent_strong: hex(0x1473e6),
            accent_text: Color32::WHITE,
            row_selected: hex(0x484b50),
            pasteboard: hex(0x1e1e1e),
            ruler: hex(0x2f2f2f),
            ruler_tick: hex(0x8a8a8a),
            smart_guide: hex(0xff3dfc),
            guide: hex(0x4affff),
            measure_bg: Color32::from_rgba_unmultiplied(92, 92, 92, 235),
            button: hex(0x444444),
            radius: 4,
            text_strong: hex(0xf5f5f5),
            tab_strip: hex(0x282828),
            button_border: hex(0x6a6a6a),
            selection: hex(0x4f80ff),
            anchor_selected: hex(0x3d82ff),
            flyout_active: hex(0x54b2f7),
            font_size: 13.0,
            caption_close: hex(0xc42b1c),
            caption_close_text: Color32::WHITE,
            highlight: hex(0xff3030),
            bleed: hex(0xf03030),
            error: hex(0xe34850),
            warning: hex(0xf0a330),
        };
        match b {
            Brightness::Dark => base,
            // Measured from Illustrator 2026 (plan/illustrator/10-observed-ui.md §1).
            Brightness::MediumDark => Tokens {
                app_bar: hex(0x535353),
                panel: hex(0x535353),
                panel_darker: hex(0x424242),
                tab_strip: hex(0x424242),
                input: hex(0x454545),
                input_border: hex(0x5f5f5f),
                border: hex(0x383838),
                divider: hex(0x4b4b4b),
                text: hex(0xd1d1d1),
                text_strong: hex(0xffffff),
                text_dim: hex(0xb0b0b0),
                text_disabled: hex(0x7a7a7a),
                icon: hex(0xc2c2c2),
                hover: hex(0x606060),
                tool_active: hex(0x303030),
                row_selected: hex(0x52677c),
                pasteboard: hex(0x606060),
                ruler: hex(0x333333),
                ruler_tick: hex(0x8a8a8a),
                button: hex(0x535353),
                button_border: hex(0x747474),
                ..base
            },
            Brightness::MediumLight => Tokens {
                tab_strip: hex(0xa6a6a6),
                text_strong: hex(0x000000),
                button_border: hex(0x7a7a7a),
                dark: false,
                app_bar: hex(0xa9a9a9),
                panel: hex(0xb8b8b8),
                panel_darker: hex(0xaaaaaa),
                input: hex(0xd6d6d6),
                input_border: hex(0x8c8c8c),
                border: hex(0x969696),
                divider: hex(0x9e9e9e),
                text: hex(0x1f1f1f),
                text_dim: hex(0x3c3c3c),
                text_disabled: hex(0x7c7c7c),
                icon: hex(0x2a2a2a),
                hover: hex(0xc8c8c8),
                tool_active: hex(0x969696),
                row_selected: hex(0x9fb4d6),
                pasteboard: hex(0xa0a0a0),
                ruler: hex(0xc2c2c2),
                ruler_tick: hex(0x4a4a4a),
                button: hex(0xcacaca),
                ..base
            },
            Brightness::Light => Tokens {
                tab_strip: hex(0xdddddd),
                text_strong: hex(0x000000),
                button_border: hex(0x9a9a9a),
                dark: false,
                app_bar: hex(0xe4e4e4),
                panel: hex(0xf0f0f0),
                panel_darker: hex(0xe2e2e2),
                input: hex(0xffffff),
                input_border: hex(0xb4b4b4),
                border: hex(0xcfcfcf),
                divider: hex(0xd6d6d6),
                text: hex(0x1f1f1f),
                text_dim: hex(0x4b4b4b),
                text_disabled: hex(0x9a9a9a),
                icon: hex(0x303030),
                hover: hex(0xdddddd),
                tool_active: hex(0xcdcdcd),
                row_selected: hex(0xc8daf7),
                pasteboard: hex(0xdcdcdc),
                ruler: hex(0xeaeaea),
                ruler_tick: hex(0x5a5a5a),
                button: hex(0xe0e0e0),
                ..base
            },
        }
    }

    pub fn get(ctx: &egui::Context) -> Tokens {
        ctx.data(|d| d.get_temp::<Tokens>(egui::Id::NULL)).unwrap_or_else(|| Tokens::for_brightness(Brightness::MediumDark))
    }

    fn with_glass(mut self, brightness: Brightness) -> Self {
        self.glass = true;
        let rgb = |r, g, b| Color32::from_rgb(r, g, b);
        let rgba = |r, g, b, a| Color32::from_rgba_unmultiplied(r, g, b, a);
        if self.dark {
            self.app_bar = rgba(30, 32, 37, 135);
            self.surface = rgba(40, 42, 47, 185);
            self.panel = rgb(47, 49, 54);
            self.panel_darker = rgb(35, 37, 42);
            self.tab_strip = rgba(24, 26, 31, 110);
            self.glass_tint = rgba(35, 37, 43, 205);
            self.glass_edge = rgba(255, 255, 255, 45);
            self.input = rgb(30, 32, 37);
            self.input_border = rgb(83, 85, 93);
            self.border = rgba(0, 0, 0, 90);
            self.divider = rgba(235, 239, 246, 22);
            self.text = rgb(231, 233, 237);
            self.text_strong = rgb(250, 250, 252);
            self.text_dim = rgb(188, 191, 199);
            self.text_disabled = rgb(128, 132, 141);
            self.icon = rgb(218, 222, 229);
            self.hover = rgb(66, 69, 76);
            self.tool_active = rgb(26, 28, 33);
            self.row_selected = rgb(64, 88, 113);
            self.button = rgb(58, 61, 68);
            self.button_border = rgb(100, 104, 114);
        } else {
            self.app_bar = rgba(244, 246, 250, 130);
            self.surface = rgba(236, 239, 244, 185);
            self.panel = rgb(238, 240, 244);
            self.panel_darker = rgb(221, 225, 232);
            self.tab_strip = rgba(210, 215, 224, 110);
            self.glass_tint = rgba(242, 244, 248, 210);
            self.glass_edge = rgba(255, 255, 255, 170);
            self.input = rgb(252, 252, 254);
            self.input_border = rgb(170, 176, 186);
            self.border = rgba(65, 71, 82, 70);
            self.divider = rgba(30, 35, 45, 28);
            self.text = rgb(35, 38, 44);
            self.text_strong = rgb(20, 23, 29);
            self.text_dim = rgb(78, 83, 93);
            self.text_disabled = rgb(132, 137, 146);
            self.icon = rgb(54, 59, 69);
            self.hover = rgb(218, 224, 233);
            self.tool_active = rgb(200, 210, 224);
            self.button = rgb(228, 232, 239);
            self.button_border = rgb(159, 166, 178);
        }
        let adjustment: i16 = match brightness {
            Brightness::MediumDark => 14,
            Brightness::MediumLight => -18,
            _ => 0,
        };
        for color in [
            &mut self.app_bar,
            &mut self.surface,
            &mut self.panel,
            &mut self.panel_darker,
            &mut self.tab_strip,
            &mut self.glass_tint,
            &mut self.input,
            &mut self.hover,
            &mut self.tool_active,
            &mut self.button,
        ] {
            let [r, g, b, a] = color.to_srgba_unmultiplied();
            let shift = |v: u8| (v as i16 + adjustment).clamp(0, 255) as u8;
            *color = rgba(shift(r), shift(g), shift(b), a);
        }
        self.radius = 5;
        self
    }
    pub fn cr(&self) -> CornerRadius {
        CornerRadius::same(self.radius)
    }
}

pub const FONT_UI: &str = "ui";
pub const FONT_UI_SEMIBOLD: &str = "ui-semibold";
pub const FONT_MONO: &str = "mono";

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FONT_UI_SEMIBOLD.into()))
}

pub fn emphasis(ctx: &egui::Context, size: f32) -> FontId {
    if ctx.data(|d| d.get_temp::<bool>(egui::Id::new("ui-fonts-installed"))).unwrap_or(false) { semibold(size) } else { FontId::proportional(size) }
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FONT_MONO.into()))
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |f: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        f.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "SourceSans3", include_bytes!("../../../assets/fonts/SourceSans3-Regular.ttf"));
    add(&mut fonts, "SourceSans3-Semibold", include_bytes!("../../../assets/fonts/SourceSans3-Semibold.ttf"));
    add(&mut fonts, "JetBrainsMono", include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"));
    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut prop = vec!["SourceSans3".to_string()];
    prop.extend(fallback.clone());
    fonts.families.insert(FontFamily::Proportional, prop);
    let mut semi = vec!["SourceSans3-Semibold".to_string()];
    semi.extend(fallback.clone());
    fonts.families.insert(FontFamily::Name(FONT_UI_SEMIBOLD.into()), semi);
    let mut ui = vec!["SourceSans3".to_string()];
    ui.extend(fallback.clone());
    fonts.families.insert(FontFamily::Name(FONT_UI.into()), ui);
    let mut mono = vec!["JetBrainsMono".to_string()];
    mono.extend(fallback);
    fonts.families.insert(FontFamily::Name(FONT_MONO.into()), mono);
    add_craft_fonts(&mut fonts);
    #[cfg(target_os = "macos")]
    if ctx.data(|d| d.get_temp::<HostAppearance>(egui::Id::new("host-appearance"))).is_some() {
        add_system_ui_fonts(&mut fonts);
    }
    ctx.set_fonts(fonts);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("ui-fonts-installed"), true));
}

#[cfg(target_os = "macos")]
fn add_system_ui_fonts(fonts: &mut FontDefinitions) {
    static SYSTEM: std::sync::OnceLock<FontDefinitions> = std::sync::OnceLock::new();
    let native = SYSTEM.get_or_init(|| {
        let mut native = FontDefinitions::empty();
        // One bounded process-lifetime cache, shared by weights and language changes.
        if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNS.ttf") {
            let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
            for (name, weight, families) in [
                ("macOS UI", 400.0, vec![FontFamily::Proportional, FontFamily::Name(FONT_UI.into())]),
                ("macOS UI Semibold", 600.0, vec![FontFamily::Name(FONT_UI_SEMIBOLD.into())]),
            ] {
                let mut data = FontData::from_static(bytes);
                data.tweak.coords = egui::epaint::text::VariationCoords::new([(b"wght", weight), (b"opsz", 13.0)]);
                native.font_data.insert(name.into(), Arc::new(data));
                for family in families {
                    native.families.entry(family).or_default().push(name.into());
                }
            }
        }
        for (style, families) in [
            ("Regular", vec![FontFamily::Proportional, FontFamily::Name(FONT_UI.into())]),
            ("Semibold", vec![FontFamily::Name(FONT_UI_SEMIBOLD.into())]),
        ] {
            if let Some(face) = vectorcraft_text::FontDb::global().face("PingFang SC", style) {
                // The font database already shares this installed collection's bytes.
                let face: &'static vectorcraft_text::FontFace = Box::leak(Box::new(face));
                let name = format!("macOS CJK {style}");
                native.font_data.insert(name.clone(), Arc::new(FontData { index: face.face_index(), ..FontData::from_static(face.file_data()) }));
                for family in families {
                    native.families.entry(family).or_default().push(name.clone());
                }
            }
        }
        native
    });
    fonts.font_data.extend(native.font_data.iter().map(|(name, data)| (name.clone(), data.clone())));
    for (family, preferred) in &native.families {
        let stack = fonts.families.entry(family.clone()).or_default();
        for name in preferred.iter().rev() {
            if name.starts_with("macOS CJK") && crate::i18n::current().code() != "zh-hans" {
                continue;
            }
            stack.insert(0, name.clone());
        }
    }
}

/// The egui name of a craft-fonts face.
pub(crate) fn craft_font_name(f: &vectorcraft_text::CraftFont) -> String {
    format!("craft-fonts {} {}", f.family, f.style)
}

/// The Japanese craft-fonts faces (when built with `CRAFT_FONTS_DIR`) as fallbacks at the end of
/// every family, after the app's own fonts and before the installed fonts `ui_fonts` adds: BIZ
/// UDPGothic first (bold first in the semibold family), then the Mincho faces.
fn add_craft_fonts(fonts: &mut FontDefinitions) {
    for f in vectorcraft_text::CRAFT_FONTS.iter().filter(|f| f.is_japanese() || f.scripts.contains(&"Hans")) {
        let data = if f.scripts.contains(&"Hans") { crate::ui_fonts::craft_font_data(f) } else { FontData::from_static(f.bytes) };
        fonts.font_data.insert(craft_font_name(f), Arc::new(data));
    }
    for (family, stack) in fonts.families.iter_mut() {
        let bold = *family == FontFamily::Name(FONT_UI_SEMIBOLD.into());
        let chinese: Vec<String> = vectorcraft_text::CRAFT_FONTS.iter().filter(|f| f.scripts.contains(&"Hans")).map(craft_font_name).collect();
        if crate::i18n::current().code() == "zh-hans" {
            stack.extend(chinese.iter().cloned());
        }
        stack.extend(vectorcraft_text::craft_fonts::japanese_ui_fonts(bold).into_iter().map(craft_font_name));
        if crate::i18n::current().code() != "zh-hans" {
            stack.extend(chinese);
        }
    }
}

/// Apply tokens to egui's global style.
pub fn apply(ctx: &egui::Context, b: Brightness) {
    apply_material(ctx, b, false);
}

pub fn apply_material(ctx: &egui::Context, b: Brightness, glass: bool) {
    let host = ctx.data(|d| d.get_temp::<HostAppearance>(egui::Id::new("host-appearance")));
    let mut t = Tokens::for_brightness(b);
    t.surface = t.panel;
    if glass && !host.is_some_and(|h| h.reduce_transparency) {
        t = t.with_glass(b);
        if !crate::glass::native_material(ctx) {
            t.app_bar = t.panel_darker;
            t.surface = t.panel;
        }
    }
    if let Some(host) = host {
        let rgb = |value| Color32::from_gray(value);
        // Keep all four app brightness choices distinct in the shared desktop fallback.
        // Native semantic colours, when supplied, still override this palette below.
        let (surface, panel, input, ink, secondary) = match b {
            Brightness::Dark => (30, 32, 24, 242, 190),
            Brightness::MediumDark => (42, 46, 36, 242, 190),
            Brightness::MediumLight => (228, 232, 244, 26, 85),
            Brightness::Light => (244, 246, 255, 26, 85),
        };
        t.panel = rgb(panel);
        t.panel_darker = rgb(if t.dark { 30 } else { 235 });
        t.input = rgb(input);
        t.app_bar = rgb(if t.dark { 30 } else { 246 });
        t.surface = rgb(surface);
        if t.glass && crate::glass::native_material(ctx) {
            t.app_bar = Color32::from_rgba_unmultiplied(surface, surface, surface, 220);
            t.surface = Color32::from_rgba_unmultiplied(panel, panel, panel, 230);
        }
        t.glass_tint = if t.glass { Color32::from_rgba_unmultiplied(panel, panel, panel, if t.dark { 205 } else { 210 }) } else { rgb(panel) };
        t.tab_strip = rgb(panel);
        t.pasteboard = rgb(if t.dark { 28 } else { 210 });
        t.ruler = rgb(if t.dark { 35 } else { 239 });
        t.ruler_tick = rgb(if t.dark { 155 } else { 100 });
        t.text = rgb(ink);
        t.text_strong = rgb(ink);
        t.text_dim = rgb(secondary);
        t.text_disabled = rgb(if t.dark { 125 } else { 145 });
        t.icon = rgb(if t.dark { 210 } else { 75 });
        t.hover = rgb(if t.dark { 63 } else { 225 });
        t.input_border = rgb(if t.dark { 75 } else { 180 });
        t.divider = rgb(if t.dark { 54 } else { 215 });
        t.border = t.divider;
        t.accent = Color32::from_rgb(host.accent[0], host.accent[1], host.accent[2]);
        t.accent_strong = t.accent;
        // Native accent colours can be yellow or graphite as well as blue.
        let linear = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        let luminance = 0.2126 * linear(host.accent[0]) + 0.7152 * linear(host.accent[1]) + 0.0722 * linear(host.accent[2]);
        // Match AppKit's white glyphs on system blue; reserve dark glyphs for pale accents.
        t.accent_text = if luminance > 0.45 { Color32::BLACK } else { Color32::WHITE };
        t.row_selected = t.panel.lerp_to_gamma(t.accent, 0.24);
        t.tool_active = t.row_selected;
        t.flyout_active = t.text_strong;
        t.button = t.hover;
        t.button_border = t.input_border;
        if let Some(colors) = host.colors {
            let rgba = |[r, g, b, a]: [u8; 4]| Color32::from_rgba_unmultiplied(r, g, b, a);
            t.panel = rgba(colors.window);
            t.panel_darker = rgba(colors.control);
            t.app_bar = t.panel;
            t.surface = t.panel;
            if t.glass && crate::glass::native_material(ctx) {
                t.app_bar = t.panel.gamma_multiply(0.9);
                t.surface = t.panel.gamma_multiply(0.94);
            }
            t.input = rgba(colors.control);
            t.text = rgba(colors.label);
            t.text_strong = t.text;
            t.text_dim = rgba(colors.secondary);
            t.text_disabled = rgba(colors.disabled);
            t.icon = t.text;
            t.divider = rgba(colors.separator);
            t.border = t.divider;
            t.input_border = t.text.gamma_multiply(0.16);
            t.button_border = t.input_border;
            t.hover = t.panel.lerp_to_gamma(t.text, 0.10);
            t.button = t.panel.lerp_to_gamma(t.text, 0.07);
            t.tab_strip = t.panel;
            t.glass_tint = t.panel.gamma_multiply(0.78);
            t.pasteboard = rgba(colors.under_page);
            t.ruler = t.panel;
            t.ruler_tick = t.text_dim;
            t.row_selected = t.panel.lerp_to_gamma(t.accent, 0.24);
            t.tool_active = t.row_selected;
        }
        t.radius = 6;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, t));
    let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = t.panel;
    v.window_fill = t.panel;
    v.extreme_bg_color = t.input;
    v.faint_bg_color = t.panel_darker;
    v.window_stroke = Stroke::new(1.0, t.border);
    v.window_corner_radius = CornerRadius::same(6);
    v.menu_corner_radius = CornerRadius::same(6);
    v.selection.bg_fill = t.accent.gamma_multiply(0.55);
    v.selection.stroke = Stroke::new(1.0, t.accent);
    v.hyperlink_color = t.accent;
    v.override_text_color = Some(t.text);
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(if t.dark { 120 } else { 60 }) };
    v.window_shadow = egui::epaint::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if t.dark { 140 } else { 70 }) };
    let w = &mut v.widgets;
    for (wv, fill) in [
        (&mut w.noninteractive, t.panel),
        (&mut w.inactive, t.button),
        (&mut w.hovered, t.hover),
        (&mut w.active, t.tool_active),
        (&mut w.open, t.hover),
    ] {
        wv.bg_fill = fill;
        wv.weak_bg_fill = fill;
        wv.corner_radius = t.cr();
        wv.fg_stroke = Stroke::new(1.0, t.text);
    }
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.divider);
    w.inactive.bg_stroke = Stroke::NONE;
    w.hovered.bg_stroke = Stroke::new(1.0, t.input_border);
    w.active.bg_stroke = Stroke::new(1.0, t.accent);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(6.0, 2.0);
        s.spacing.interact_size = egui::vec2(24.0, 24.0);
        s.spacing.menu_margin = egui::Margin::same(6);
        s.spacing.slider_width = 120.0;
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Body, FontId::proportional(t.font_size)),
            (TextStyle::Button, FontId::proportional(t.font_size)),
            (TextStyle::Heading, FontId::new(if host.is_some() { 17.0 } else { 15.0 }, FontFamily::Name(FONT_UI_SEMIBOLD.into()))),
            (TextStyle::Monospace, FontId::new(11.0, FontFamily::Name(FONT_MONO.into()))),
        ]
        .into();
        s.animation_time = match host {
            Some(h) if h.reduce_motion => 0.0,
            Some(_) => 0.16,
            None => 0.08,
        };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_fallback_keeps_chrome_opaque_and_preserves_host_accessibility() {
        let ctx = egui::Context::default();
        ensure_desktop_appearance(&ctx, Brightness::Dark, true);
        let mut panels = std::collections::HashSet::new();
        for brightness in Brightness::ALL {
            apply_material(&ctx, brightness, true);
            let t = Tokens::get(&ctx);
            assert_eq!(t.radius, 6);
            assert_eq!((t.surface.a(), t.app_bar.a(), t.input.a(), t.pasteboard.a()), (255, 255, 255, 255));
            assert_eq!(t.accent, Color32::from_rgb(0, 122, 255));
            assert!(t.glass_tint.a() < 255, "floating panels retain the in-app canvas blur");
            panels.insert(t.panel.to_array());
        }
        assert_eq!(panels.len(), 4, "brightness must affect all four desktop themes");
        let host = HostAppearance { accent: [255, 210, 30], colors: None, reduce_transparency: true, reduce_motion: true };
        set_host_appearance(&ctx, host, Brightness::Light, true);
        ensure_desktop_appearance(&ctx, Brightness::Light, true);
        let t = Tokens::get(&ctx);
        assert!(!t.glass);
        assert_eq!(t.accent, Color32::from_rgb(255, 210, 30));
        assert_eq!(ctx.global_style().animation_time, 0.0);
    }

    #[test]
    fn host_accessibility_and_accent_keep_controls_legible_and_canvas_opaque() {
        let ctx = egui::Context::default();
        crate::glass::set_native_material(&ctx, true);
        let host = HostAppearance { accent: [255, 210, 30], colors: None, reduce_transparency: true, reduce_motion: true };
        set_host_appearance(&ctx, host, Brightness::Dark, true);
        let t = Tokens::get(&ctx);
        assert!(!t.glass);
        assert_eq!(t.surface.a(), 255);
        assert_eq!(t.pasteboard.a(), 255);
        assert_eq!(t.accent_text, Color32::BLACK);
        assert_eq!(ctx.global_style().animation_time, 0.0);
        let host = HostAppearance { accent: [0, 60, 180], colors: None, reduce_transparency: false, reduce_motion: false };
        set_host_appearance(&ctx, host, Brightness::Light, true);
        let t = Tokens::get(&ctx);
        assert!(t.glass);
        assert!(t.surface.a() < 255);
        assert_eq!(t.input.a(), 255);
        assert_eq!(t.pasteboard.a(), 255);
        assert_eq!(t.accent_text, Color32::WHITE);
        assert!(ctx.global_style().animation_time > 0.0);
    }

    #[test]
    fn brightness_roundtrip() {
        for b in Brightness::ALL {
            assert_eq!(Brightness::parse(b.id()), Some(b));
            assert_eq!(Brightness::parse(b.label()), Some(b));
        }
        assert!(!Tokens::for_brightness(Brightness::Light).dark);
    }
}
