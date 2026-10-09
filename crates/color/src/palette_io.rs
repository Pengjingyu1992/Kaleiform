//! Swatch library files: the native `.vcswatches` JSON (colour models, global, spot, gradients
//! and colour groups exactly), `.gpl` palettes (8-bit RGB; colour groups as `# Group:` comment
//! headers) and CSS custom properties (written only). Swatch exchange files (`.ase`) are read
//! only ([`read_bytes`]).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{Color, GradientKind, Paint, Swatch, SwatchGroup, SwatchLibrary};

/// A swatch library file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteFormat {
    /// `.vcswatches`: JSON, lossless.
    Native,
    /// `.gpl`: a plain-text RGB palette many paint and design tools read.
    Gpl,
    /// `.css`: custom properties on `:root` (colours and gradients; written only).
    Css,
}

impl PaletteFormat {
    pub const ALL: [PaletteFormat; 3] = [PaletteFormat::Native, PaletteFormat::Gpl, PaletteFormat::Css];

    /// The format's id, which is also its extension.
    pub fn id(self) -> &'static str {
        match self {
            PaletteFormat::Native => "vcswatches",
            PaletteFormat::Gpl => "gpl",
            PaletteFormat::Css => "css",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            PaletteFormat::Native => "Kaleiform Swatches (.vcswatches)",
            PaletteFormat::Gpl => "GPL Palette (.gpl)",
            PaletteFormat::Css => "CSS Custom Properties (.css)",
        }
    }
    /// The format with id or extension `s` (any case, leading dot allowed).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim_start_matches('.').to_ascii_lowercase();
        Self::ALL.into_iter().find(|f| f.id() == s)
    }
    /// Can libraries be read back from this format?
    pub fn readable(self) -> bool {
        self != PaletteFormat::Css
    }
}

/// The native file: a header around the library.
#[derive(Serialize, Deserialize)]
struct NativeFile {
    format: String,
    version: u32,
    #[serde(flatten)]
    library: SwatchLibrary,
}

const NATIVE_FORMAT: &str = "vcswatches";
const GPL_HEADER: &str = "GIMP Palette";
const GPL_GROUP: &str = "# Group:";
const ASE_SIGNATURE: &[u8] = b"ASEF";
const ASE_GROUP_START: u16 = 0xC001;
const ASE_GROUP_END: u16 = 0xC002;
const ASE_COLOR: u16 = 0x0001;
/// The most colors and groups read from one `.ase` file.
const ASE_MAX_ENTRIES: usize = 100_000;
/// The most UTF-16 code units of a name that are kept.
const ASE_MAX_NAME: usize = 1024;
const ASE_CUT_SHORT: &str = "the swatch exchange file is cut short";

/// Write `lib` in `format`. Pattern swatches (their tiles live in a document) and None are left
/// out; `.gpl` keeps solid colours only, as 8-bit RGB.
pub fn write(lib: &SwatchLibrary, format: PaletteFormat) -> String {
    let keep = |w: &&Swatch| matches!(w.paint, Paint::Solid { .. } | Paint::Gradient(_));
    match format {
        PaletteFormat::Native => {
            let strip = |list: &[Swatch]| list.iter().filter(keep).cloned().collect::<Vec<_>>();
            let library = SwatchLibrary {
                name: lib.name.clone(),
                swatches: strip(&lib.swatches),
                groups: lib.groups.iter().map(|g| SwatchGroup { name: g.name.clone(), swatches: strip(&g.swatches) }).collect(),
            };
            let file = NativeFile { format: NATIVE_FORMAT.into(), version: 1, library };
            serde_json::to_string_pretty(&file).unwrap_or_default()
        }
        PaletteFormat::Gpl => write_gpl(lib),
        PaletteFormat::Css => write_css(lib, keep),
    }
}

/// One line of text: no line breaks or tabs.
fn one_line(s: &str) -> String {
    s.split(['\n', '\r', '\t']).collect::<Vec<_>>().join(" ").trim().to_string()
}

fn write_gpl(lib: &SwatchLibrary) -> String {
    let mut out = format!("{GPL_HEADER}\nName: {}\nColumns: 0\n#\n", one_line(&lib.name));
    let colors = |out: &mut String, list: &[Swatch]| {
        for w in list {
            if let Some(c) = w.paint.color() {
                let [r, g, b, _] = c.to_rgba8(1.0);
                out.push_str(&format!("{r:3} {g:3} {b:3}\t{}\n", one_line(&w.name)));
            }
        }
    };
    colors(&mut out, &lib.swatches);
    for g in &lib.groups {
        out.push_str(&format!("{GPL_GROUP} {}\n", one_line(&g.name)));
        colors(&mut out, &g.swatches);
    }
    out
}

/// `name` as a CSS custom property (`--` and an escaped identifier): lower case, spaces as
/// hyphens, other characters an identifier can't hold escaped with a backslash.
pub fn css_property(name: &str) -> String {
    let mut out = String::from("--");
    for word in name.split_whitespace() {
        if out.len() > 2 {
            out.push('-');
        }
        for ch in word.chars().flat_map(char::to_lowercase) {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || !ch.is_ascii() {
                out.push(ch);
            } else if ch.is_ascii_graphic() {
                out.push('\\');
                out.push(ch);
            } else {
                out.push_str(&format!("\\{:x} ", ch as u32));
            }
        }
    }
    if out.len() == 2 {
        out.push_str("swatch");
    }
    out
}

/// A colour as CSS (`#rrggbb`, or `#rrggbbaa` below full opacity).
fn css_color(c: &Color, opacity: f32) -> String {
    let [r, g, b, a] = c.to_rgba8(opacity);
    if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") }
}

/// A comment's text: `*/` can't end it early.
fn css_comment(s: &str) -> String {
    one_line(s).replace("*/", "* /")
}

fn write_css(lib: &SwatchLibrary, keep: impl Fn(&&Swatch) -> bool) -> String {
    let mut out = format!("/* {} */\n:root {{\n", css_comment(&lib.name));
    let mut taken: Vec<String> = vec![];
    let mut props = |out: &mut String, list: &[Swatch]| {
        for w in list.iter().filter(&keep) {
            let value = match &w.paint {
                Paint::Solid { color, .. } => css_color(color, 1.0),
                Paint::Gradient(g) if g.gradient.kind != GradientKind::Freeform => {
                    let stops: Vec<String> =
                        g.gradient.stops.iter().map(|s| format!("{} {}%", css_color(&s.color, s.opacity), (s.offset * 100.0).round())).collect();
                    match g.gradient.kind {
                        GradientKind::Radial => format!("radial-gradient(circle, {})", stops.join(", ")),
                        _ => format!("linear-gradient({}deg, {})", (90.0 - g.angle).rem_euclid(360.0).round(), stops.join(", ")),
                    }
                }
                _ => continue,
            };
            // Names differing only in case or punctuation map to one property: number the others.
            let base = css_property(&w.name);
            let name = (1..).map(|i| if i == 1 { base.clone() } else { format!("{base}-{i}") }).find(|n| !taken.contains(n)).unwrap_or(base);
            taken.push(name.clone());
            let note = if w.spot { " /* spot */" } else { "" };
            out.push_str(&format!("  {name}: {value};{note}\n"));
        }
    };
    props(&mut out, &lib.swatches);
    for g in &lib.groups {
        out.push_str(&format!("  /* {} */\n", css_comment(&g.name)));
        props(&mut out, &g.swatches);
    }
    out.push_str("}\n");
    out
}

/// Read a `.vcswatches` or `.gpl` library (detected from its content). `name` names it when the
/// file doesn't.
pub fn read(text: &str, name: &str) -> Result<SwatchLibrary, String> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.starts_with(GPL_HEADER) {
        return Ok(read_gpl(text, name));
    }
    if text.starts_with('{') {
        let f: NativeFile = serde_json::from_str(text).map_err(|e| format!("not a swatch library: {e}"))?;
        if f.format != NATIVE_FORMAT {
            return Err(format!("not a swatch library (format `{}`)", f.format));
        }
        let mut lib = f.library;
        if lib.name.trim().is_empty() {
            lib.name = name.into();
        }
        return Ok(lib);
    }
    Err("not a swatch library (.vcswatches or .gpl)".into())
}

/// Is `text` a library [`read`] understands?
pub fn sniff(text: &str) -> bool {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    t.starts_with(GPL_HEADER) || (t.starts_with('{') && t.contains(NATIVE_FORMAT))
}

/// Read the bytes of a library file: a swatch exchange `.ase` file, else a `.vcswatches` or
/// `.gpl` library ([`read`]). `name` names it when the file doesn't.
pub fn read_bytes(bytes: &[u8], name: &str) -> Result<SwatchLibrary, String> {
    if bytes.starts_with(ASE_SIGNATURE) {
        return read_ase(bytes, name);
    }
    read(&String::from_utf8_lossy(bytes), name)
}

/// Are `bytes` a library [`read_bytes`] understands? Text that isn't valid UTF-8 is checked as
/// [`read_bytes`] reads it, with the invalid bytes replaced (its first KiB is enough).
pub fn sniff_bytes(bytes: &[u8]) -> bool {
    bytes.starts_with(ASE_SIGNATURE)
        || std::str::from_utf8(bytes).is_ok_and(sniff)
        || sniff(&String::from_utf8_lossy(bytes.get(..1024).unwrap_or(bytes)))
}

/// `.gpl`: `Name:` names the library, `# Group: name` starts a colour group, other `#` lines and
/// headers (`Columns:`) are skipped, as are lines that aren't `r g b [name]`. Unnamed colors are
/// named by their values and unnamed groups "Color Group"; a name a color or group already has gets
/// a number ([`UniqueNames`]).
fn read_gpl(text: &str, fallback: &str) -> SwatchLibrary {
    let mut lib = SwatchLibrary { name: fallback.into(), ..Default::default() };
    let mut names = UniqueNames::default();
    for line in text.lines().skip(1) {
        let line = line.trim();
        if let Some(g) = line.strip_prefix(GPL_GROUP) {
            lib.groups.push(SwatchGroup { name: names.unique(group_name(g)), swatches: vec![] });
            continue;
        }
        if let Some(n) = line.strip_prefix("Name:") {
            lib.name = n.trim().to_string();
            continue;
        }
        // Comments, headers (`Columns: 4`) and anything else that isn't a colour are skipped.
        let words: Vec<&str> = line.split_whitespace().collect();
        let rgb: Vec<u8> = words.iter().take(3).map_while(|v| v.parse().ok()).collect();
        let [r, g, b] = rgb[..] else { continue };
        let rest = words[3..].join(" ");
        let base = if rest.is_empty() { format!("R={r} G={g} B={b}") } else { rest };
        let w = Swatch { name: names.unique(base), paint: Paint::solid(Color::rgb8(r, g, b)), global: false, spot: false };
        match lib.groups.last_mut() {
            Some(grp) => grp.swatches.push(w),
            None => lib.swatches.push(w),
        }
    }
    lib
}

/// `.ase`, from public descriptions of the format: the layout from
/// <https://www.selapa.net/swatches/colors/fileformats.php>, the component ranges from
/// <https://github.com/nsfmc/swatch>. After the signature come, big-endian, a `u16` major (1) and
/// minor version, a `u32` block count and the blocks, each a `u16` type, a `u32` body length and
/// the body. A group start's body is a name. A color's body is a name, a four-character model
/// (`RGB `, `CMYK`, `LAB `, `Gray`), its components as `f32` (3, 4, 3 and 1 of them) and a `u16`
/// color type (0 global, 1 spot, 2 process). RGB, CMYK and gray components run from 0 to 1 (a
/// gray level of 1 is white), Lab lightness from 0 to 1 and a and b from −128 to 127. A lightness
/// above 1 is read as L* itself. A name is a `u16` count of UTF-16 code units, the terminating zero
/// included, then the units.
///
/// Color groups become groups: a group start inside an open group ends that group, and a group
/// left open ends with the file. Blocks of other types and colors in other models are skipped.
/// Unnamed colors are named by their values and unnamed groups "Color Group"; a name a color or
/// group already has gets a number ([`UniqueNames`]). A truncated file, a major version other than
/// 1 or more than [`ASE_MAX_ENTRIES`] colors and groups is an error.
fn read_ase(bytes: &[u8], fallback: &str) -> Result<SwatchLibrary, String> {
    let mut data = AseData(bytes);
    data.bytes(ASE_SIGNATURE.len())?;
    let (major, minor) = (data.u16()?, data.u16()?);
    if major != 1 {
        return Err(format!("swatch exchange version {major}.{minor} isn't supported (only 1.x)"));
    }
    let blocks = data.u32()?;
    let mut lib = SwatchLibrary { name: fallback.into(), ..Default::default() };
    let mut names = UniqueNames::default();
    let (mut in_group, mut entries) = (false, 0usize);
    for _ in 0..blocks {
        let kind = data.u16()?;
        let len = usize::try_from(data.u32()?).unwrap_or(usize::MAX);
        let mut body = AseData(data.bytes(len)?);
        if matches!(kind, ASE_GROUP_START | ASE_COLOR) {
            entries += 1;
            if entries > ASE_MAX_ENTRIES {
                return Err(format!("the swatch exchange file holds more than {ASE_MAX_ENTRIES} colors and groups"));
            }
        }
        match kind {
            ASE_GROUP_START => {
                lib.groups.push(SwatchGroup { name: names.unique(group_name(&body.name()?)), swatches: vec![] });
                in_group = true;
            }
            ASE_GROUP_END => in_group = false,
            ASE_COLOR => {
                let Some(mut w) = ase_swatch(&mut body)? else { continue };
                w.name = names.unique(std::mem::take(&mut w.name));
                match lib.groups.last_mut().filter(|_| in_group) {
                    Some(g) => g.swatches.push(w),
                    None => lib.swatches.push(w),
                }
            }
            // Other blocks are skipped.
            _ => {}
        }
    }
    Ok(lib)
}

/// The swatch of an `.ase` color block, `None` for a model other than RGB, CMYK, Lab and Gray.
/// Components are clamped to their ranges.
fn ase_swatch(body: &mut AseData) -> Result<Option<Swatch>, String> {
    let name = body.name()?;
    let unit = |v: f32| v.clamp(0.0, 1.0);
    let color = match &body.array::<4>()? {
        b"RGB " => {
            let [r, g, b] = [body.f32()?, body.f32()?, body.f32()?];
            Color::rgb(unit(r), unit(g), unit(b))
        }
        b"CMYK" => {
            let [c, m, y, k] = [body.f32()?, body.f32()?, body.f32()?, body.f32()?];
            Color::cmyk(unit(c), unit(m), unit(y), unit(k))
        }
        // Lightness is stored as a fraction of 100. Some writers store L* itself, which shows as a
        // value above 1.
        b"LAB " => {
            let [l, a, b] = [body.f32()?, body.f32()?, body.f32()?];
            let l = if l > 1.0 { l.min(100.0) } else { unit(l) * 100.0 };
            Color::lab(l, a.clamp(-128.0, 127.0), b.clamp(-128.0, 127.0))
        }
        // A gray level (1 is white); a gray color holds its ink.
        b"Gray" => Color::gray(1.0 - unit(body.f32()?)),
        _ => return Ok(None),
    };
    // Some writers leave the color type out, which makes a process color.
    let kind = if body.0.len() >= 2 { body.u16()? } else { 2 };
    // Spot colors are global.
    let (global, spot) = match kind {
        0 => (true, false),
        1 => (true, true),
        _ => (false, false),
    };
    let name = if name.is_empty() { value_name(color) } else { name };
    Ok(Some(Swatch { name, paint: Paint::solid(color), global, spot }))
}

/// A color's values in its own model, as new swatches are named ("C=10 M=20 Y=30 K=0",
/// "R=255 G=128 B=0", "Gray K=40", "L=52 a=70 b=-30").
fn value_name(c: Color) -> String {
    let pct = |v: f32| (v * 100.0).round();
    let byte = |v: f32| (v * 255.0).round();
    match c {
        Color::Cmyk { c, m, y, k } => format!("C={} M={} Y={} K={}", pct(c), pct(m), pct(y), pct(k)),
        Color::Rgb { r, g, b } => format!("R={} G={} B={}", byte(r), byte(g), byte(b)),
        Color::Gray { k } => format!("Gray K={}", pct(k)),
        // `+ 0.0` turns a rounded -0 into 0.
        Color::Lab { l, a, b } => format!("L={} a={} b={}", l.round() + 0.0, a.round() + 0.0, b.round() + 0.0),
    }
}

/// `.ase` data, read field by field; reading past its end is an error.
struct AseData<'a>(&'a [u8]);

impl<'a> AseData<'a> {
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        let (head, rest) = self.0.split_at_checked(n).ok_or(ASE_CUT_SHORT)?;
        self.0 = rest;
        Ok(head)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let (head, rest) = self.0.split_first_chunk::<N>().ok_or(ASE_CUT_SHORT)?;
        self.0 = rest;
        Ok(*head)
    }
    fn u16(&mut self) -> Result<u16, String> {
        self.array().map(u16::from_be_bytes)
    }
    fn u32(&mut self) -> Result<u32, String> {
        self.array().map(u32::from_be_bytes)
    }
    /// A component; one that isn't a finite number reads as 0.
    fn f32(&mut self) -> Result<f32, String> {
        let v = f32::from_be_bytes(self.array()?);
        Ok(if v.is_finite() { v } else { 0.0 })
    }
    /// A name up to its terminating zero, trimmed. A block that ends before it has no name.
    fn name(&mut self) -> Result<String, String> {
        if self.0.is_empty() {
            return Ok(String::new());
        }
        let n = usize::from(self.u16()?);
        let (units, _) = self.bytes(n * 2)?.as_chunks::<2>();
        let units: Vec<u16> = units.iter().take(ASE_MAX_NAME).map(|u| u16::from_be_bytes(*u)).take_while(|&u| u != 0).collect();
        Ok(String::from_utf16_lossy(&units).trim().to_string())
    }
}

/// A group's name as a file gives it, trimmed; "Color Group" when it has none.
fn group_name(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() { "Color Group".into() } else { name.to_string() }
}

/// The swatch and group names a reader has handed out. Swatches and color groups share one set of
/// names, as in a document, and a name already taken gets the first free number ("Black 2").
#[derive(Default)]
struct UniqueNames {
    taken: HashSet<String>,
    /// The last number tried for each repeated name.
    last: HashMap<String, usize>,
}

impl UniqueNames {
    fn unique(&mut self, base: String) -> String {
        if self.taken.insert(base.clone()) {
            return base;
        }
        // Each pass tries the next number, and only finitely many names are taken.
        let n = self.last.entry(base.clone()).or_insert(1);
        loop {
            *n += 1;
            let name = format!("{base} {n}");
            if self.taken.insert(name.clone()) {
                return name;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Gradient, GradientPaint, GradientStop};

    fn sample() -> SwatchLibrary {
        let grad = Paint::Gradient(Box::new(GradientPaint::new(Gradient {
            kind: GradientKind::Linear,
            stops: vec![
                GradientStop { midpoint: 0.4, ..GradientStop::new(0.0, Color::cmyk(0.1, 0.2, 0.3, 0.0)) },
                GradientStop { opacity: 0.5, ..GradientStop::new(1.0, Color::rgb8(255, 0, 0)) },
            ],
        })));
        SwatchLibrary {
            name: "Brand */ Colours".into(),
            swatches: vec![
                Swatch { name: "Ink".into(), paint: Paint::solid(Color::cmyk(1.0, 0.5, 0.0, 0.2)), global: true, spot: true },
                Swatch { name: "Fade".into(), paint: grad, global: false, spot: false },
                Swatch { name: "Tiles".into(), paint: Paint::Pattern { pattern: "Dots".into(), xf: Default::default() }, global: false, spot: false },
            ],
            groups: vec![SwatchGroup {
                name: "Neutrals".into(),
                swatches: vec![
                    Swatch { name: "R=255 G=0 B=0".into(), paint: Paint::solid(Color::rgb8(255, 0, 0)), global: false, spot: false },
                    Swatch { name: "Mist".into(), paint: Paint::solid(Color::gray(0.25)), global: true, spot: false },
                ],
            }],
        }
    }

    #[test]
    fn native_round_trip_keeps_cmyk_spot_and_groups() {
        let lib = sample();
        let text = write(&lib, PaletteFormat::Native);
        assert!(sniff(&text));
        let back = read(&text, "fallback").unwrap();
        let mut expected = lib.clone();
        expected.swatches.retain(|w| !matches!(w.paint, Paint::Pattern { .. }));
        assert_eq!(back, expected, "everything but the pattern comes back exactly");
        assert!(back.swatches[0].spot && back.swatches[0].global);
        assert_eq!(back.swatches[0].paint.color(), Some(Color::cmyk(1.0, 0.5, 0.0, 0.2)));
        assert!(read("{\"format\": \"other\", \"version\": 1, \"name\": \"x\"}", "x").is_err());
    }

    #[test]
    fn gpl_writes_rgb_with_group_headers_and_reads_comments_and_headers() {
        let text = write(&sample(), PaletteFormat::Gpl);
        assert!(text.starts_with("GIMP Palette\nName: Brand */ Colours\n"));
        assert!(text.contains("# Group: Neutrals\n255   0   0\tR=255 G=0 B=0\n"), "{text}");
        assert!(!text.contains("Fade") && !text.contains("Tiles"), "only solid colours");
        let back = read(&text, "x").unwrap();
        assert_eq!(back.name, "Brand */ Colours");
        assert_eq!(back.swatches.len(), 1);
        assert_eq!(back.groups[0].swatches[1].paint.color(), Some(Color::rgb8(191, 191, 191)), "grey as 8-bit RGB");
        // Another tool's palette: comments, a Columns header, blank lines, unnamed and repeated names.
        let other = "GIMP Palette\r\nName: Sunset\r\nColumns: 4\r\n# a comment\r\n\r\n  0 128 255\tSky Blue\r\n10 20 30\r\n0 0 0 Black\r\n1 1 1 Black\r\nbad line\r\n";
        let lib = read(other, "file").unwrap();
        assert_eq!(lib.name, "Sunset");
        let names: Vec<&str> = lib.swatches.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["Sky Blue", "R=10 G=20 B=30", "Black", "Black 2"]);
        assert_eq!(lib.swatches[0].paint.color(), Some(Color::rgb8(0, 128, 255)));
        assert_eq!(read("GIMP Palette\n1 2 3 x\n", "Fallback").unwrap().name, "Fallback");
        // Colors and groups share names; an unnamed group is "Color Group".
        let lib = read("GIMP Palette\n0 0 0 Black\n# Group: Black\n1 1 1 Black\n# Group:\n", "x").unwrap();
        let groups: Vec<(&str, Vec<&str>)> =
            lib.groups.iter().map(|g| (g.name.as_str(), g.swatches.iter().map(|w| w.name.as_str()).collect())).collect();
        assert_eq!(groups, [("Black 2", vec!["Black 3"]), ("Color Group", vec![])]);
    }

    #[test]
    fn css_escapes_names_and_writes_gradients() {
        assert_eq!(css_property("Sky Blue"), "--sky-blue");
        assert_eq!(css_property("R=255 G=0 B=0"), "--r\\=255-g\\=0-b\\=0");
        assert_eq!(css_property("#FF00CC"), "--\\#ff00cc");
        assert_eq!(css_property("Café 50%"), "--café-50\\%");
        assert_eq!(css_property("  "), "--swatch");
        let text = write(&sample(), PaletteFormat::Css);
        assert!(text.starts_with("/* Brand * / Colours */\n:root {\n"), "{text}");
        assert!(text.contains("  --ink: #"), "{text}");
        assert!(text.contains("; /* spot */"));
        assert!(text.contains("  --fade: linear-gradient(90deg, #"), "{text}");
        assert!(text.contains("#ff000080 100%)"), "half-transparent stop: {text}");
        assert!(text.contains("  /* Neutrals */\n  --r\\=255-g\\=0-b\\=0: #ff0000;\n"));
        assert!(!text.contains("tiles"));
        // Names that collide once escaped are numbered.
        let lib = SwatchLibrary {
            name: "x".into(),
            swatches: ["Red", "red"].map(|n| Swatch { name: n.into(), paint: Paint::solid(Color::BLACK), global: false, spot: false }).to_vec(),
            groups: vec![],
        };
        let text = write(&lib, PaletteFormat::Css);
        assert!(text.contains("--red: #000000;") && text.contains("--red-2: #000000;"));
        assert!(read(&text, "x").is_err(), "CSS is written only");
    }

    /// A swatch exchange file of `blocks` (type, body).
    fn ase(blocks: &[(u16, Vec<u8>)]) -> Vec<u8> {
        let mut out = b"ASEF\0\x01\0\0".to_vec();
        out.extend((blocks.len() as u32).to_be_bytes());
        for (kind, body) in blocks {
            out.extend(kind.to_be_bytes());
            out.extend((body.len() as u32).to_be_bytes());
            out.extend(body);
        }
        out
    }

    /// A name as `.ase` stores it.
    fn ase_name(name: &str) -> Vec<u8> {
        let units: Vec<u16> = name.encode_utf16().chain([0]).collect();
        let mut out = (units.len() as u16).to_be_bytes().to_vec();
        out.extend(units.iter().flat_map(|u| u.to_be_bytes()));
        out
    }

    /// A color block: name, model, components and color type.
    fn ase_color(name: &str, model: &[u8; 4], values: &[f32], kind: u16) -> (u16, Vec<u8>) {
        let mut body = ase_name(name);
        body.extend(model);
        body.extend(values.iter().flat_map(|v| v.to_be_bytes()));
        body.extend(kind.to_be_bytes());
        (ASE_COLOR, body)
    }

    #[test]
    fn ase_reads_color_models_kinds_and_groups() {
        let bytes = ase(&[
            ase_color("Sky", b"RGB ", &[0.0, 0.5, 1.0], 0),
            ase_color("Ink", b"CMYK", &[1.0, 0.5, 0.0, 0.2], 1),
            (ASE_GROUP_START, ase_name("Neutrals")),
            ase_color("Mist", b"Gray", &[0.75], 2),
            ase_color("Clay", b"LAB ", &[0.5, 20.0, -30.0], 2),
            (ASE_GROUP_END, vec![]),
            ase_color("Paper", b"RGB ", &[1.0, 1.0, 1.0], 2),
        ]);
        assert!(sniff_bytes(&bytes));
        let sw = |name: &str, color, global, spot| Swatch { name: name.into(), paint: Paint::solid(color), global, spot };
        let expected = SwatchLibrary {
            name: "Brand".into(),
            swatches: vec![
                sw("Sky", Color::rgb(0.0, 0.5, 1.0), true, false),
                sw("Ink", Color::cmyk(1.0, 0.5, 0.0, 0.2), true, true),
                sw("Paper", Color::WHITE, false, false),
            ],
            groups: vec![SwatchGroup {
                name: "Neutrals".into(),
                swatches: vec![sw("Mist", Color::gray(0.25), false, false), sw("Clay", Color::lab(50.0, 20.0, -30.0), false, false)],
            }],
        };
        assert_eq!(read_bytes(&bytes, "Brand").unwrap(), expected, "named after the file; 0 global, 1 spot, 2 process");
        // A lightness above 1 is L* itself.
        let deep = read_bytes(&ase(&[ase_color("Deep", b"LAB ", &[53.0, 20.0, -30.0], 2)]), "x").unwrap();
        assert_eq!(deep.swatches[0].paint.color(), Some(Color::lab(53.0, 20.0, -30.0)));
        // Text libraries read through the same call, also when they aren't valid UTF-8.
        assert!(sniff_bytes(b"GIMP Palette\n1 2 3 x\n") && !sniff_bytes(b"ASE") && !sniff_bytes(&[0xff, 0xfe, 0]));
        assert_eq!(read_bytes(b"GIMP Palette\n1 2 3 x\n", "F").unwrap().swatches.len(), 1);
        let latin1 = b"GIMP Palette\n1 2 3 Caf\xe9\n";
        assert!(sniff_bytes(latin1));
        assert_eq!(read_bytes(latin1, "F").unwrap().swatches.len(), 1);
    }

    #[test]
    fn ase_names_unnamed_and_repeated_entries_and_skips_other_blocks() {
        let mut untyped = ase_color("Plain", b"RGB ", &[0.2, 0.4, 0.6], 2);
        untyped.1.truncate(untyped.1.len() - 2);
        let bytes = ase(&[
            ase_color("Café 🎨", b"RGB ", &[1.0, 0.0, 0.0], 2),
            ase_color("", b"CMYK", &[0.1, 0.2, 0.3, 0.0], 2),
            ase_color(" Red ", b"RGB ", &[2.0, -1.0, f32::NAN], 0),
            ase_color("Red", b"Gray", &[0.0], 2),
            untyped,
            ase_color("Hue", b"HSB ", &[0.1, 0.2, 0.3], 2),
            (0x0042, vec![1, 2, 3]),
            (ASE_GROUP_END, vec![]),
            (ASE_GROUP_START, ase_name("")),
            (ASE_GROUP_START, ase_name("Red")),
            ase_color("Red", b"Gray", &[1.0], 2),
        ]);
        let lib = read_bytes(&bytes, "x").unwrap();
        let names: Vec<&str> = lib.swatches.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["Café 🎨", "C=10 M=20 Y=30 K=0", "Red", "Red 2", "Plain"], "an HSB color and an unknown block are skipped");
        assert_eq!(lib.swatches[2].paint.color(), Some(Color::rgb(1.0, 0.0, 0.0)), "clamped, NaN read as 0");
        assert!(lib.swatches[2].global && !lib.swatches[4].global, "without a color type: process");
        // A group start ends the open group, and groups and colors share names.
        let groups: Vec<(&str, Vec<&str>)> =
            lib.groups.iter().map(|g| (g.name.as_str(), g.swatches.iter().map(|w| w.name.as_str()).collect())).collect();
        assert_eq!(groups, [("Color Group", vec![]), ("Red 3", vec!["Red 4"])]);
        assert_eq!(lib.groups[1].swatches[0].paint.color(), Some(Color::gray(0.0)), "gray level 1 is white");
    }

    #[test]
    fn damaged_ase_files_are_errors() {
        let good = ase(&[(ASE_GROUP_START, ase_name("G")), ase_color("Sky", b"RGB ", &[0.0, 0.5, 1.0], 0)]);
        assert_eq!(read_bytes(&good, "x").unwrap().len(), 1);
        for n in ASE_SIGNATURE.len()..good.len() {
            assert!(read_bytes(&good[..n], "x").is_err(), "cut at {n}");
        }
        let mut v2 = good.clone();
        v2[5] = 2;
        assert!(read_bytes(&v2, "x").unwrap_err().contains("2.0"), "version 2.0");
        // Too many colors, or too many groups (empty group starts take 6 bytes each).
        let colors: Vec<(u16, Vec<u8>)> = (0..=ASE_MAX_ENTRIES).map(|_| ase_color("Gray", b"Gray", &[0.5], 2)).collect();
        assert!(read_bytes(&ase(&colors), "x").unwrap_err().contains("more than"));
        let groups: Vec<(u16, Vec<u8>)> = (0..=ASE_MAX_ENTRIES).map(|_| (ASE_GROUP_START, vec![])).collect();
        assert!(read_bytes(&ase(&groups), "x").unwrap_err().contains("more than"));
        assert_eq!(read_bytes(&ase(&groups[1..]), "x").unwrap().groups.len(), ASE_MAX_ENTRIES, "up to the limit");
    }
}
