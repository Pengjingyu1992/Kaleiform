//! Chinese composition (中文排版): 避头尾 line breaking, 标点挤压 and 两端对齐.
//!
//! The official composer owns punctuation compression and justification. Local coverage keeps
//! emergency wrapping safe and verifies that the official paragraph options still act on Chinese.

use super::*;
use kurbo::{Affine, Shape};
use vectorcraft_doc::{CharStyle, Justify, Mojikumi, TextKind};
use vectorcraft_geom::PathData;

/// Deterministic on native and web, without installed system fonts.
fn db() -> &'static FontDb {
    static DB: std::sync::OnceLock<FontDb> = std::sync::OnceLock::new();
    DB.get_or_init(|| FontDb::with_font_dirs(Vec::new()))
}

fn has_chinese_font() -> bool {
    let available = db().face("Noto Sans CJK SC", "Regular").is_some_and(|face| face.glyph_for('中') != 0);
    if !available {
        eprintln!("skipped: Chinese glyph metrics require the optional craft-fonts input");
    }
    available
}

fn style(size: f64) -> CharStyle {
    CharStyle { size, font_family: "Noto Sans CJK SC".into(), ..CharStyle::default() }
}

fn cjk_area(text: &str, frame: Rect, justify: Justify) -> TextObject {
    let mut t = TextObject::point(Point::ZERO, text, style(12.0));
    t.kind = TextKind::Area { frame: PathData::from_bezpath(&frame.to_path(0.1)) };
    t.xf = Affine::IDENTITY;
    t.para.justify = justify;
    t
}

/// The advance the face gives `c` before any 标点挤压.
fn raw_advance(family: &str, c: char, size: f64) -> f64 {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, Size};
    let f = db().face(family, "Regular").expect("face");
    let gid = f.glyph_for(c);
    assert_ne!(gid, 0, "{family} {c}: missing glyph");
    let sk = f.skrifa().expect("skrifa");
    let m = sk.glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(skrifa::GlyphId::new(gid));
    f64::from(m.unwrap_or(0.0)) / f.units_per_em() * size
}

/// The advance the layout gives a one-character text object of `c`.
fn laid_advance(family: &str, c: char, size: f64) -> f64 {
    let st = CharStyle { font_family: family.into(), size, ..CharStyle::default() };
    layout(db(), &TextObject::point(Point::ZERO, c.to_string().as_str(), st)).glyphs.first().expect("glyph").advance
}

#[test]
fn chinese_punctuation_respects_the_official_mojikumi_option() {
    if !has_chinese_font() {
        return;
    }
    for c in ['，', '。', '、', '）', '》'] {
        let mut text = TextObject::point(Point::ZERO, &format!("中文{c}"), style(20.0));
        text.para.mojikumi = Mojikumi::None;
        let full = layout(db(), &text).glyphs.last().unwrap().advance;
        text.para.mojikumi = Mojikumi::LineEndHalf;
        let half = layout(db(), &text).glyphs.last().unwrap().advance;
        assert!((full - 20.0).abs() < 0.01 && (half - 10.0).abs() < 0.01, "{c}: full {full}, half {half}");
    }
}

#[test]
fn no_chinese_punctuation_is_squeezed_out_of_existence() {
    if !has_chinese_font() {
        return;
    }
    // The trim must never take more than half a mark's cell, so the advance stays positive and the
    // glyph keeps its id even in faces that carry these marks narrower than a full em.
    let marks = "，。、；：！？（）【】《》「」“”‘’…·";
    for family in ["Noto Sans CJK SC", "Source Sans 3"] {
        for c in marks.chars() {
            let st = CharStyle { font_family: family.into(), size: 20.0, ..CharStyle::default() };
            let l = layout(db(), &TextObject::point(Point::ZERO, c.to_string().as_str(), st));
            let Some(g) = l.glyphs.first() else { panic!("{family} {c}: no glyph") };
            assert!(g.advance > 0.0, "{family} {c}: advance {} squeezed to nothing", g.advance);
            assert!(g.gid != 0, "{family} {c}: missing glyph (gid 0)");
        }
    }
}

#[test]
fn compressed_brackets_do_not_overlap_the_enclosed_text() {
    if !has_chinese_font() {
        return;
    }
    let l = layout(db(), &TextObject::point(Point::ZERO, "甲（乙）丙《丁》戊", style(20.0)));
    for pair in l.glyphs.windows(2) {
        let a = pair[0].outline.bounding_box();
        let b = pair[1].outline.bounding_box();
        assert!(a.x1 <= b.x0 + 0.01, "overlapping ink: {a:?} and {b:?}");
    }
}

#[test]
fn latin_curly_quotes_and_apostrophes_keep_the_fonts_spacing() {
    for c in ['‘', '’', '“', '”'] {
        let raw = raw_advance("Source Sans 3", c, 40.0);
        let laid = laid_advance("Source Sans 3", c, 40.0);
        assert!((raw - laid).abs() < 1e-6, "{c}: {raw} vs {laid}");
    }
}

#[test]
fn narrow_frames_keep_brackets_with_the_enclosed_character() {
    if !has_chinese_font() {
        return;
    }
    let text = "甲甲《乙》丙丙丙";
    for width in [12.0, 24.0, 30.0, 40.0, 50.0] {
        let l = layout(db(), &cjk_area(text, Rect::new(0.0, 0.0, width, 1000.0), Justify::Left));
        for line in &l.lines {
            let s = &text[line.start..line.end];
            assert!(!s.starts_with('》') && !s.ends_with('《'), "width {width}: {s:?}");
        }
    }
}

#[test]
fn justified_chinese_reaches_the_frame_edge() {
    if !has_chinese_font() {
        return;
    }
    let text: String = "中文排版测试两端对齐标点挤压".repeat(12);
    let frame = Rect::new(0.0, 0.0, 200.0, 4000.0);
    let l = layout(db(), &cjk_area(&text, frame, Justify::JustifyLeft));
    assert!(l.lines.len() > 3, "the text wraps: {} lines", l.lines.len());
    for line in &l.lines[..l.lines.len() - 1] {
        let avail = line.avail.1 - line.avail.0;
        assert!((line.x1 - line.x0 - avail).abs() < 1e-6, "justified line does not reach the edge: {line:?}");
    }
    // The last line is left ragged rather than stretched.
    assert!(l.lines.last().is_some_and(|l| l.x1 - l.x0 < 200.0));
}

#[test]
fn chinese_justification_does_not_track_out_latin_words() {
    let frame = Rect::new(0.0, 0.0, 70.0, 4000.0);
    let mut text = cjk_area("abcdefghijklmnopqrstuvwx".repeat(4).as_str(), frame, Justify::Left);
    text.runs[0].style.font_family = "Source Sans 3".into();
    let left = layout(db(), &text);
    text.para.justify = Justify::JustifyLeft;
    let justified = layout(db(), &text);
    assert!(left.lines.len() > 2);
    for (a, b) in left.glyphs.iter().zip(&justified.glyphs) {
        assert!((a.advance - b.advance).abs() < 1e-9);
    }
}

#[test]
fn lines_never_begin_with_closing_punctuation_or_end_with_an_opening_one() {
    if !has_chinese_font() {
        return;
    }
    let text: String = "他说：「中文排版，非常讲究。」然后他走了。".repeat(20);
    let l = layout(db(), &cjk_area(&text, Rect::new(0.0, 0.0, 120.0, 6000.0), Justify::Left));
    assert!(l.lines.len() > 5);
    for line in &l.lines {
        let s = text.get(line.start..line.end).unwrap_or("");
        let first = s.chars().next();
        assert!(first.is_none_or(|c| !"，。、；：！？）】》」”’…".contains(c)), "line begins with closing punctuation {first:?}: {s:?}");
        let last = s.chars().next_back();
        assert!(last.is_none_or(|c| !"（【《「“‘[{".contains(c)), "line ends with an opening bracket {last:?}: {s:?}");
    }
}
