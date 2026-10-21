//! Lossless original PNG interchange, or a full-resolution transparent picture of selected art.
use std::collections::BTreeSet;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vectorcraft_color::BlendMode;
use vectorcraft_doc::{Node, NodeId, NodeKind};
use vectorcraft_geom::{Affine, Point};

use super::*;

pub const FOLDER_PREFIX: &str = "Kaleiform-Compositor-Transfer-";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub version: u32,
    pub width: f64,
    pub height: f64,
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub flip_x: bool,
    #[serde(default)]
    pub flip_y: bool,
    #[serde(default)]
    pub name: Option<String>,
}

impl Placement {
    pub fn valid(&self) -> bool {
        self.version == 1
            && [self.width, self.height, self.rotation].iter().all(|n| n.is_finite())
            && (1.0..=300_000.0).contains(&self.width)
            && (1.0..=300_000.0).contains(&self.height)
            && self.rotation.abs() <= 360_000.0
    }

    /// Unit square into document coordinates, centred at the drop, independent of DPI.
    pub fn transform(&self, center: Point) -> Affine {
        Affine::translate((center.x, center.y))
            * Affine::rotate(self.rotation.to_radians())
            * Affine::scale_non_uniform(self.width * if self.flip_x { -1.0 } else { 1.0 }, self.height * if self.flip_y { -1.0 } else { 1.0 })
            * Affine::translate((-0.5, -0.5))
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![cmd!(
        query "image.transfer", "Drag Image to Another App", [], None,
        "{} → {dataBase64 (PNG), placement, original}: selected images pass through their original PNG when their appearance permits; otherwise render selected art on transparency without the clipboard's 4096-pixel limit; leaves document, selection and clipboard unchanged",
        has_selection, export
    )]
}

fn plain(n: &Node) -> bool {
    n.opacity == 1.0
        && n.blend == BlendMode::Normal
        && n.mask.is_none()
        && n.appearance.items.is_empty()
        && n.appearance.effects.is_empty()
        && !n.clips()
}

fn retain_selected(n: &mut Node, roots: &BTreeSet<NodeId>) -> bool {
    if roots.contains(&n.id) {
        return true;
    }
    let clipping = n.clips();
    let Some(children) = n.children_mut() else { return false };
    let clip_id = clipping.then(|| children.first().map(|n| n.id)).flatten();
    children.retain_mut(|n| Some(n.id) == clip_id || retain_selected(Arc::make_mut(n), roots));
    children.len() > usize::from(clipping)
}

fn export(s: &mut Session, _: &Value) -> Result<Value> {
    const C: &str = "image.transfer";
    let roots = super::edit::selected_roots(s)?;
    let st = s.doc()?;
    if let [id] = roots.as_slice()
        && let Some(n) = st.doc.node(*id)
        && let NodeKind::Image(i) = &n.kind
        && plain(n)
        && st.doc.ancestry(*id).unwrap_or_default().iter().all(|id| st.doc.node(*id).is_some_and(plain))
        && let Some(blob) = st.doc.images.get(&i.key)
        && blob.mime == "image/png"
    {
        let [a, b, c, d, _, _] = i.xf.as_coeffs();
        let (sx, sy) = (a.hypot(b), c.hypot(d));
        if sx > 0.0 && sy > 0.0 && (a * c + b * d).abs() <= 1e-9 * sx * sy {
            let source = super::fileio::raster_image(&blob.bytes)?;
            if (source.width, source.height) != (i.width, i.height) {
                return Err(bad(C, "the linked image holds only a preview; relink or embed its full original first"));
            }
            let p = Placement {
                version: 1,
                width: sx * f64::from(i.width),
                height: sy * f64::from(i.height),
                rotation: b.atan2(a).to_degrees(),
                flip_x: false,
                flip_y: a * d - b * c < 0.0,
                name: n.name.as_ref().map(|s| s.chars().take(255).collect()),
            };
            if !p.valid() {
                return Err(bad(C, "image placement is outside supported limits"));
            }
            return Ok(json!({"dataBase64": vectorcraft_format::base64_encode(&blob.bytes), "placement": p, "original": true}));
        }
    }
    let roots: BTreeSet<_> = roots.into_iter().collect();
    let mut doc = (*st.doc).clone();
    doc.layers.retain_mut(|n| retain_selected(Arc::make_mut(n), &roots));
    let bounds = doc
        .layers
        .iter()
        .filter_map(|n| n.visual_bounds())
        .reduce(|a, b| a.union(b))
        .filter(|b| b.width() >= 1.0 && b.height() >= 1.0)
        .ok_or_else(|| bad(C, "selection has no image area"))?;
    let mut scale: f64 = 1.0;
    for n in &doc.layers {
        n.walk(&mut |n| {
            if let NodeKind::Image(i) = &n.kind {
                let [a, b, c, d, _, _] = i.xf.as_coeffs();
                let edge = a.hypot(b).min(c.hypot(d));
                if edge > 0.0 {
                    scale = scale.max(1.0 / edge);
                }
            }
        });
    }
    let _ = vectorcraft_render::raster_size(bounds, scale).map_err(|e| bad(C, e))?;
    let doc = super::fileio::with_single_artboard(doc, bounds, "Image transfer");
    let png = super::fileio::encode(&doc, "png", &json!({"scale": scale, "background": "transparent"}))?;
    let p = Placement {
        version: 1,
        width: bounds.width(),
        height: bounds.height(),
        rotation: 0.0,
        flip_x: false,
        flip_y: false,
        name: Some("Image".into()),
    };
    if !p.valid() {
        return Err(bad(C, "image placement is outside supported limits"));
    }
    Ok(json!({"dataBase64": vectorcraft_format::base64_encode(&png), "placement": p, "original": false}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageBuffer, Rgba};

    fn png(depth: u8) -> Vec<u8> {
        let image = if depth == 16 {
            DynamicImage::ImageRgba16(ImageBuffer::from_fn(6000, 8, |x, _| Rgba([x as u16, 12345, 54321, (x % 4) as u16 * 21845])))
        } else {
            DynamicImage::ImageRgba8(ImageBuffer::from_fn(6000, 8, |x, _| Rgba([(x % 256) as u8, 48, 192, (x % 4) as u8 * 85])))
        };
        let mut data = vec![];
        image.write_to(&mut std::io::Cursor::new(&mut data), image::ImageFormat::Png).unwrap();
        data
    }

    fn session(bytes: &[u8]) -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width":800,"height":600})).unwrap();
        s.execute("file.place", &json!({"name":"Source.png","dataBase64":vectorcraft_format::base64_encode(bytes),"link":false})).unwrap();
        s
    }

    #[test]
    fn original_png_survives_transfer_and_native_save_at_both_depths() {
        for depth in [8, 16] {
            let bytes = png(depth);
            let mut s = session(&bytes);
            let doc = s.doc().unwrap().doc.clone();
            let clipboard = s.clipboard.clone();
            let undo = s.doc().unwrap().history.undo.len();
            let data = s.execute("image.transfer", &json!({})).unwrap();
            assert_eq!(data["original"], true, "{depth}");
            assert_eq!(vectorcraft_format::base64_decode(data["dataBase64"].as_str().unwrap()).unwrap(), bytes);
            assert!(Arc::ptr_eq(&doc, &s.doc().unwrap().doc));
            assert_eq!(s.clipboard, clipboard);
            assert_eq!(s.doc().unwrap().history.undo.len(), undo);
            let saved = super::super::fileio::encode(&doc, "vectorcraft", &json!({})).unwrap();
            let reloaded = super::super::fileio::load("roundtrip.vectorcraft", &saved).unwrap();
            assert!(reloaded.doc.images.values().any(|b| b.bytes.as_ref() == bytes.as_slice()));
        }
    }

    #[test]
    fn shared_placement_embeds_at_the_drop_without_resampling_and_undoes_once() {
        let bytes = png(16);
        let mut s = Session::new();
        s.execute("file.new", &json!({"width":800,"height":600})).unwrap();
        let p =
            Placement { version: 1, width: 1500.0, height: 2.0, rotation: 30.0, flip_x: true, flip_y: false, name: Some("From Compositor".into()) };
        let v = s
            .execute(
                "file.place",
                &json!({"name":"image.png","dataBase64":vectorcraft_format::base64_encode(&bytes),"link":true,"at":[350,250],"imageTransfer":p}),
            )
            .unwrap();
        assert_eq!(v["linked"], false);
        let st = s.doc().unwrap();
        let n = st.doc.node(st.selection.objects[0]).unwrap();
        let NodeKind::Image(i) = &n.kind else { panic!() };
        assert!(i.link.is_none());
        assert_eq!((i.width, i.height), (6000, 8));
        let center = i.xf * Point::new(3000.0, 4.0);
        assert!(center.distance(Point::new(350.0, 250.0)) < 1e-7);
        assert_eq!(n.name.as_deref(), Some("From Compositor"));
        assert_eq!(st.doc.images.get(&i.key).unwrap().bytes.as_ref(), bytes.as_slice());
        let transfer = s.execute("image.transfer", &json!({})).unwrap();
        assert_eq!(vectorcraft_format::base64_decode(transfer["dataBase64"].as_str().unwrap()).unwrap(), bytes);
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(s.doc().unwrap().doc.images.is_empty());
        s.execute("edit.redo", &json!({})).unwrap();
        assert!(!s.doc().unwrap().doc.images.is_empty());
    }

    #[test]
    fn edited_appearance_is_transparent_and_never_clamped_to_clipboard_size() {
        let bytes = png(8);
        let mut s = session(&bytes);
        s.execute("transparency.set", &json!({"item":null,"opacity":50})).unwrap();
        let data = s.execute("image.transfer", &json!({})).unwrap();
        assert_eq!(data["original"], false);
        let png = vectorcraft_format::base64_decode(data["dataBase64"].as_str().unwrap()).unwrap();
        let image = image::load_from_memory(&png).unwrap().to_rgba8();
        assert!(image.width() >= 6000);
        assert!(image.pixels().any(|p| p.0[3] == 0));
        assert!(image.pixels().any(|p| (125..=129).contains(&p.0[3])));
        assert!(s.doc().unwrap().doc.images.values().any(|b| b.bytes.as_ref() == bytes.as_slice()));
    }

    #[test]
    fn malformed_placement_rejects_without_document_change() {
        let bytes = png(8);
        let mut s = session(&bytes);
        for p in [
            json!({"version":2,"width":10,"height":20}),
            json!({"version":1,"width":0,"height":20}),
            json!({"version":1,"width":10,"height":400_000}),
        ] {
            let before = s.doc().unwrap().doc.clone();
            assert!(
                s.execute("file.place", &json!({"name":"image.png","dataBase64":vectorcraft_format::base64_encode(&bytes),"imageTransfer":p}))
                    .is_err()
            );
            assert!(Arc::ptr_eq(&before, &s.doc().unwrap().doc));
        }
    }

    #[test]
    fn selecting_art_inside_a_clip_keeps_its_ancestor_clipping_path() {
        use vectorcraft_doc::Appearance;
        use vectorcraft_geom::{Rect, shapes};
        let shape = |id| Arc::new(Node::path(NodeId(id), shapes::rectangle(Rect::new(0.0, 0.0, 30.0, 20.0)), Appearance::default_art()));
        let mut group = Node::group(NodeId(4), vec![shape(1), shape(2), shape(3)]);
        if let NodeKind::Group { clip, .. } = &mut group.kind {
            *clip = true;
        }
        assert!(!plain(&group));
        assert!(retain_selected(&mut group, &BTreeSet::from([NodeId(2)])));
        let children = group.children().unwrap();
        assert_eq!(children.iter().map(|n| n.id).collect::<Vec<_>>(), vec![NodeId(1), NodeId(2)]);
        assert!(!retain_selected(&mut group, &BTreeSet::from([NodeId(99)])));
    }

    #[test]
    fn external_fixture_interoperates_when_supplied() {
        let Ok(path) = std::env::var("KALEIFORM_IMAGE_TRANSFER_FIXTURE") else { return };
        let bytes = std::fs::read(path).unwrap();
        let mut s = session(&bytes);
        let data = s.execute("image.transfer", &json!({})).unwrap();
        assert_eq!(vectorcraft_format::base64_decode(data["dataBase64"].as_str().unwrap()).unwrap(), bytes);
    }
}
