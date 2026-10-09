//! The text of an Illustrator editing copy.
//!
//! The editing copy of a file keeps the characters of its text objects, with their fonts, sizes,
//! colours and where they sit, in one text document of its own: between `%AI11_BeginTextDocument`
//! and `%AI11_EndTextDocument`, ASCII85 of nested dictionaries with numbers for keys. A text object
//! in a layer is only a stub naming its story in it (`/StoryIndex`).
//!
//! The format isn't documented. What is read here was worked out from the files of the project's
//! users, used locally and never committed, by comparing it with the type their pages draw: the
//! meaning of each key below was checked against the page's own type in more than a thousand text
//! objects, and it is only used for the keys that matched. A story the reader doesn't recognise (area
//! type, type on a path, anything with a frame that has more than a point and a matrix) is left
//! alone: it has no text, and the importer says so.
//!
//! Where type sits: a story lays its lines out round a point `F` (the anchor: the start of a line of
//! left-aligned type, the middle of a centred one, the end of a right-aligned one), each line with an
//! offset from it. The frame of the story gives a matrix that carries that layout onto the canvas of
//! the app, which is 16383 points a side; the canvas's centre, 8191.5, is the centre of the file's
//! `%AI3_TemplateBox`. See [`Story::place`].

use std::collections::BTreeMap;

use vectorcraft_color::{Color, Paint};
use vectorcraft_doc::{CharStyle, Justify, Node, NodeId, NodeKind, ParaStyle, TextObject, TextRun};
use vectorcraft_geom::{Affine, Point};

/// Longest story read (bytes).
const MAX_STORY: usize = 1 << 20;
/// Biggest text document read (bytes, decoded).
const MAX_BYTES: usize = 64 << 20;
/// Deepest nesting read.
const MAX_DEPTH: usize = 64;
/// Most values read.
const MAX_VALUES: usize = 4_000_000;
/// The centre of the app's canvas, in the units of its text document.
const CANVAS_CENTRE: f64 = 8191.5;

/// A value of the text document.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Val {
    Num(f64),
    Str(String),
    Name(String),
    Bool(bool),
    List(Vec<Val>),
    Dict(BTreeMap<String, Val>),
}

impl Val {
    fn get(&self, key: &str) -> Option<&Val> {
        match self {
            Val::Dict(d) => d.get(key),
            _ => None,
        }
    }

    fn at(&self, i: usize) -> Option<&Val> {
        match self {
            Val::List(l) => l.get(i),
            _ => None,
        }
    }

    fn num(&self) -> Option<f64> {
        match self {
            Val::Num(n) if n.is_finite() => Some(*n),
            _ => None,
        }
    }

    fn str(&self) -> Option<&str> {
        match self {
            Val::Str(s) => Some(s),
            _ => None,
        }
    }

    fn list(&self) -> &[Val] {
        match self {
            Val::List(l) => l,
            _ => &[],
        }
    }

    fn nums(&self) -> Option<Vec<f64>> {
        self.list().iter().map(Val::num).collect()
    }

    /// The `name` entry of a dictionary that is a typed node (`/99 /name`).
    fn is_node(&self, name: &str) -> bool {
        matches!(self.get("99"), Some(Val::Name(n)) if n == name)
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
    budget: usize,
}

impl Reader<'_> {
    fn skip_space(&mut self) {
        while self.b.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Option<Val> {
        self.skip_space();
        self.budget = self.budget.checked_sub(1)?;
        let c = *self.b.get(self.at)?;
        match c {
            b'<' if self.b.get(self.at + 1) == Some(&b'<') => {
                if depth > MAX_DEPTH {
                    return None;
                }
                self.at += 2;
                let mut d = BTreeMap::new();
                loop {
                    self.skip_space();
                    if self.b.get(self.at..self.at + 2) == Some(b">>") {
                        self.at += 2;
                        return Some(Val::Dict(d));
                    }
                    let key = self.key()?;
                    let v = self.value(depth + 1)?;
                    d.insert(key, v);
                }
            }
            b'[' => {
                if depth > MAX_DEPTH {
                    return None;
                }
                self.at += 1;
                let mut l = vec![];
                loop {
                    self.skip_space();
                    if self.b.get(self.at) == Some(&b']') {
                        self.at += 1;
                        return Some(Val::List(l));
                    }
                    l.push(self.value(depth + 1)?);
                }
            }
            b'(' => self.string(),
            b'/' => Some(Val::Name(self.key()?)),
            _ => {
                let start = self.at;
                while self.b.get(self.at).is_some_and(|c| !c.is_ascii_whitespace() && !matches!(c, b'<' | b'>' | b'[' | b']' | b'(' | b')' | b'/')) {
                    self.at += 1;
                }
                let word = std::str::from_utf8(self.b.get(start..self.at)?).ok()?;
                match word {
                    "true" => Some(Val::Bool(true)),
                    "false" => Some(Val::Bool(false)),
                    "" => None,
                    w => Some(Val::Num(w.parse().ok()?)),
                }
            }
        }
    }

    /// A `/name`, without its slash.
    fn key(&mut self) -> Option<String> {
        self.skip_space();
        if self.b.get(self.at) != Some(&b'/') {
            return None;
        }
        self.at += 1;
        let start = self.at;
        while self.b.get(self.at).is_some_and(|c| !c.is_ascii_whitespace() && !matches!(c, b'<' | b'>' | b'[' | b']' | b'(' | b')' | b'/')) {
            self.at += 1;
        }
        Some(String::from_utf8_lossy(self.b.get(start..self.at)?).into_owned())
    }

    /// A string: up to the first `)` that isn't escaped; UTF-16 when it starts with a byte order mark.
    fn string(&mut self) -> Option<Val> {
        self.at += 1;
        let mut bytes = vec![];
        loop {
            let c = *self.b.get(self.at)?;
            self.at += 1;
            match c {
                b')' => break,
                b'\\' => {
                    let e = *self.b.get(self.at)?;
                    self.at += 1;
                    bytes.push(match e {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'b' => 8,
                        b'f' => 12,
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.b.get(self.at) {
                                    Some(d @ b'0'..=b'7') => {
                                        v = v * 8 + u32::from(d - b'0');
                                        self.at += 1;
                                    }
                                    _ => break,
                                }
                            }
                            (v & 0xff) as u8
                        }
                        other => other,
                    });
                }
                c => bytes.push(c),
            }
            if bytes.len() > MAX_BYTES {
                return None;
            }
        }
        Some(Val::Str(match bytes.strip_prefix(&[0xfe, 0xff]) {
            Some(rest) => {
                let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|p| u16::from_be_bytes(*p)).collect();
                String::from_utf16_lossy(&units)
            }
            None => bytes.iter().map(|b| char::from(*b)).collect(),
        }))
    }
}

/// The values of a text document: its top-level `/key value` pairs.
fn read(bytes: &[u8]) -> Option<Val> {
    let mut r = Reader { b: bytes, at: 0, budget: MAX_VALUES };
    let mut d = BTreeMap::new();
    loop {
        r.skip_space();
        if r.at >= bytes.len() {
            return Some(Val::Dict(d));
        }
        let key = r.key()?;
        let v = r.value(0)?;
        d.insert(key, v);
    }
}

/// How a paragraph's lines sit against its anchor.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Align {
    Left,
    Center,
    Right,
}

/// The text of one story, in the units of the text document.
#[derive(Debug)]
pub(super) struct Story {
    /// The characters, with `\r` ending paragraphs and `\x03` ending lines.
    text: String,
    /// The anchor of the layout.
    anchor: (f64, f64),
    /// Where each line starts, from the anchor.
    lines: Vec<(f64, f64)>,
    /// Carries the layout onto the canvas.
    matrix: [f64; 6],
    align: Align,
    /// `(characters, style)` in order.
    runs: Vec<(usize, Style)>,
}

#[derive(Clone, Debug)]
struct Style {
    font: Option<String>,
    size: f64,
    fill: Option<[f64; 4]>,
    stroke: Option<[f64; 4]>,
    stroke_width: f64,
}

/// The text document of an editing copy.
pub(super) struct Texts {
    stories: Vec<Val>,
    fonts: Vec<String>,
    /// The character style a story starts from.
    defaults: Val,
    frames: Vec<Val>,
}

impl Texts {
    /// The text document in the editing copy `data`, if it has one that can be read.
    pub(super) fn read(data: &[u8]) -> Option<Texts> {
        fn find(h: &[u8], n: &[u8], from: usize) -> Option<usize> {
            h.get(from..)?.windows(n.len()).position(|w| w == n).map(|p| p + from)
        }
        let start = find(data, b"%AI11_BeginTextDocument", 0)?;
        let from = find(data, b"/ASCII85Decode", start)? + b"/ASCII85Decode".len();
        let end = find(data, b"~>", from)? + 2;
        // Each line after the first starts with a `%`.
        // ASCII85 makes at most four bytes of a character (`z`): more than this would be over `MAX_BYTES`.
        if end - from > MAX_BYTES / 4 {
            return None;
        }
        let packed: String = String::from_utf8_lossy(data.get(from..end)?)
            .split(['\r', '\n'])
            .enumerate()
            .map(|(i, l)| if i > 0 { l.strip_prefix('%').unwrap_or(l) } else { l })
            .collect::<String>()
            .trim_start_matches([' ', ','])
            .to_string();
        let bytes = crate::ps::ascii85_decode(&packed)?;
        if bytes.len() > MAX_BYTES {
            return None;
        }
        let doc = read(&bytes)?;
        let fonts = doc
            .get("0")?
            .get("1")?
            .get("0")?
            .list()
            .iter()
            .map(|f| f.get("0").and_then(|f| f.get("0")).and_then(|f| f.get("0")).and_then(Val::str).unwrap_or_default().to_string())
            .collect();
        let main = doc.get("1")?;
        Some(Texts {
            stories: main.get("1")?.list().to_vec(),
            fonts,
            defaults: main.get("2")?.clone(),
            frames: doc.get("0")?.get("8")?.get("0")?.list().to_vec(),
        })
    }

    /// Story `i`, if it is one of point type this reads.
    pub(super) fn story(&self, i: usize) -> Option<Story> {
        let s = self.stories.get(i)?;
        let text = s.get("0")?.get("0")?.str().filter(|t| t.len() <= MAX_STORY)?.to_string();
        // The frame the story is in: a matrix and nothing else but its place.
        let frame = self.frames.get(s.get("1")?.get("0")?.at(0)?.get("0")?.num()? as usize)?.get("0")?;
        let carries = frame.get("2")?;
        if carries.get("2").is_none() || carries.as_dict().is_some_and(|d| d.keys().any(|k| k != "2")) {
            return None;
        }
        let matrix: [f64; 6] = carries.get("2")?.nums()?.try_into().ok()?;
        let f = find_node(s.get("1")?.get("2")?, "F")?;
        let anchor = f.get("0")?.get("0")?.nums()?;
        let anchor = (*anchor.first()?, *anchor.get(1)?);
        let mut lines = vec![];
        let mut found = vec![];
        find_nodes(f, "L", &mut found);
        for l in found {
            let off = l.get("0").and_then(|o| o.get("0")).and_then(Val::nums).unwrap_or_else(|| vec![0.0, 0.0]);
            let seg = l.get("6")?.list().iter().find(|c| c.is_node("S"))?;
            let so = seg.get("0").and_then(|o| o.get("0")).and_then(Val::nums).unwrap_or_else(|| vec![0.0, 0.0]);
            lines.push((off.first()? + so.first()?, *off.get(1)?));
        }
        if lines.is_empty() {
            return None;
        }
        let align = match s
            .get("0")
            .and_then(|t| t.get("5"))
            .and_then(|p| p.get("0"))
            .and_then(|p| p.at(0))
            .and_then(|p| p.get("0"))
            .and_then(|p| p.get("0"))
            .and_then(|p| p.get("5"))
            .and_then(|p| p.get("0"))
            .and_then(Val::num)
        {
            Some(1.0) => Align::Right,
            Some(2.0) => Align::Center,
            _ => Align::Left,
        };
        let mut runs = vec![];
        for run in s.get("0")?.get("6")?.get("0")?.list() {
            let n = run.get("1")?.num()? as usize;
            let style = run.get("0")?.get("0")?.get("6")?;
            runs.push((n, self.style(style)));
        }
        Some(Story { text, anchor, lines, matrix, align, runs })
    }

    /// A character style: its own keys, else the document's.
    fn style(&self, s: &Val) -> Style {
        let pick = |k: &str| s.get(k).or_else(|| self.defaults.get(k));
        let paint = |k: &str| -> Option<[f64; 4]> {
            let p = pick(k)?.get("0")?;
            // Kind 2 is CMYK: an opacity, then the four inks.
            (p.get("0")?.num()? == 2.0).then(|| p.get("1")?.nums()).flatten().and_then(|v| Some([*v.get(1)?, *v.get(2)?, *v.get(3)?, *v.get(4)?]))
        };
        let on = |k: &str| !matches!(pick(k), Some(Val::Bool(false)));
        let font = pick("0").and_then(Val::num).and_then(|i| self.fonts.get(i as usize)).filter(|f| !f.is_empty()).cloned();
        Style {
            font,
            size: pick("1").and_then(Val::num).filter(|v| *v > 0.0).unwrap_or(12.0),
            fill: if on("56") { Some(paint("53").unwrap_or([0.0, 0.0, 0.0, 1.0])) } else { None },
            stroke: if matches!(pick("57"), Some(Val::Bool(true))) { paint("54") } else { None },
            stroke_width: pick("63").and_then(Val::num).filter(|v| *v >= 0.0).unwrap_or(1.0),
        }
    }
}

impl Val {
    fn as_dict(&self) -> Option<&BTreeMap<String, Val>> {
        match self {
            Val::Dict(d) => Some(d),
            _ => None,
        }
    }
}

fn find_node<'a>(v: &'a Val, name: &str) -> Option<&'a Val> {
    let mut out = vec![];
    find_nodes(v, name, &mut out);
    out.into_iter().next()
}

/// The typed nodes called `name` in `v`, in order, outermost first.
fn find_nodes<'a>(v: &'a Val, name: &str, out: &mut Vec<&'a Val>) {
    match v {
        Val::Dict(d) => {
            if v.is_node(name) {
                out.push(v);
            }
            for x in d.values() {
                find_nodes(x, name, out);
            }
        }
        Val::List(l) => {
            for x in l {
                find_nodes(x, name, out);
            }
        }
        _ => {}
    }
}

/// Where a story's first line starts: on the canvas of the app (what [`Story::place`] gives).
impl Story {
    /// How many bytes of text the story has.
    pub(super) fn len(&self) -> usize {
        self.text.len()
    }

    /// The anchor of the story on the file's canvas: `(x, y)` in the units of the art, y up, given
    /// the centre `(tx, ty)` of the file's template box.
    pub(super) fn place(&self, template: (f64, f64)) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.matrix;
        let (x, y) = (a * self.anchor.0 + c * self.anchor.1 + e, b * self.anchor.0 + d * self.anchor.1 + f);
        (x - CANVAS_CENTRE + template.0, CANVAS_CENTRE + template.1 - y)
    }

    /// Each line's characters and where it starts (in the units of the art, y up): where the page
    /// draws the line when the type is shown.
    pub(super) fn line_starts(&self, template: (f64, f64)) -> Vec<(String, (f64, f64))> {
        let [a, b, c, d, e, f] = self.matrix;
        let text: Vec<&str> = self.text.split(['\r', '\x03']).collect();
        self.lines
            .iter()
            .enumerate()
            .map(|(i, (lx, ly))| {
                let (x, y) = (self.anchor.0 + lx, self.anchor.1 + ly);
                let (x, y) = (a * x + c * y + e, b * x + d * y + f);
                (text.get(i).copied().unwrap_or_default().to_string(), (x - CANVAS_CENTRE + template.0, CANVAS_CENTRE + template.1 - y))
            })
            .collect()
    }

    /// The story as a text object whose anchor is at `at` on the document (`to_doc` takes the art's
    /// units onto it), or `None` for type that is mirrored or has no size.
    pub(super) fn node(&self, id: NodeId, template: (f64, f64), to_doc: Affine) -> Option<Node> {
        let [a, b, c, d, ..] = self.matrix;
        let scale = (a * d - b * c).abs().sqrt();
        if !(scale.is_finite() && scale > 1e-6 && a * d - b * c > 0.0) {
            return None;
        }
        let (x, y) = self.place(template);
        let at = to_doc * Point::new(x, y);
        let text: String = self.text.trim_end_matches('\r').chars().map(|c| if matches!(c, '\r' | '\x03') { '\n' } else { c }).collect();
        if text.trim().is_empty() {
            return None;
        }
        let leading = self.lines.windows(2).map(|w| (w[1].1 - w[0].1) * scale).find(|l| l.is_finite() && *l > 0.0);
        let chars: Vec<char> = text.chars().collect();
        let mut runs = vec![];
        let mut from = 0usize;
        let styles: Vec<&(usize, Style)> = self.runs.iter().collect();
        for (n, st) in styles {
            let to = from.saturating_add(*n).min(chars.len());
            let piece: String = chars.get(from..to).unwrap_or_default().iter().collect();
            from = to;
            if piece.is_empty() {
                continue;
            }
            runs.push(TextRun::new(piece, char_style(st, scale, leading)));
        }
        if from < chars.len() {
            let last = self.runs.last().map(|r| &r.1)?;
            runs.push(TextRun::new(chars.get(from..).unwrap_or_default().iter().collect::<String>(), char_style(last, scale, leading)));
        }
        if runs.is_empty() {
            return None;
        }
        let mut t = TextObject::point(Point::ZERO, "", runs.first()?.style.clone());
        t.runs = runs;
        t.para = ParaStyle {
            justify: match self.align {
                Align::Left => Justify::Left,
                Align::Center => Justify::Center,
                Align::Right => Justify::Right,
            },
            ..ParaStyle::default()
        };
        t.xf = Affine::new([a / scale, b / scale, c / scale, d / scale, at.x, at.y]);
        Some(Node::new(id, NodeKind::Text(Box::new(t))))
    }
}

fn char_style(s: &Style, scale: f64, leading: Option<f64>) -> CharStyle {
    let (family, style) = s.font.as_deref().map_or_else(|| (CharStyle::default().font_family, "Regular".to_string()), crate::family_style);
    let family = vectorcraft_text::FontDb::global().find_family(&family).unwrap_or(family);
    let ink = |c: [f64; 4]| Paint::solid(Color::cmyk(c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32));
    CharStyle {
        font_family: family,
        font_style: style,
        size: s.size * scale,
        leading,
        fill: s.fill.map_or(Paint::None, ink),
        stroke: s.stroke.map_or(Paint::None, ink),
        stroke_width: if s.stroke.is_some() { s.stroke_width * scale } else { 0.0 },
        ..CharStyle::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_read_nested_dictionaries_lists_and_utf16_strings() {
        let v = read(b" /1 << /0 [ 1 2.5 -3 ] /2 (\xfe\xff\x00h\x00i) /3 true /99 /Name >> /0 (a\\)b\\n)").unwrap();
        let one = v.get("1").unwrap();
        assert_eq!(one.get("0").unwrap().nums(), Some(vec![1.0, 2.5, -3.0]));
        assert_eq!(one.get("2").unwrap().str(), Some("hi"));
        assert_eq!(one.get("3"), Some(&Val::Bool(true)));
        assert!(one.is_node("Name"));
        assert_eq!(v.get("0").unwrap().str(), Some("a)b\n"));
    }

    #[test]
    fn damaged_documents_are_not_read() {
        for bad in [&b"/0 << /1"[..], b"/0 [ 1 2", b"/0 (abc", b"/0 nope", b"0 1"] {
            assert!(read(bad).is_none(), "{}", String::from_utf8_lossy(bad));
        }
        let deep = format!("/0 {}", "[".repeat(200));
        assert!(read(deep.as_bytes()).is_none());
    }
}
