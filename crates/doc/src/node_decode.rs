//! Stream layers and groups without buffering their entire descendant tree. The native writer
//! puts `type` first; other field orders and all other variants keep Serde's existing reader.

use super::*;
use serde::de::{self, MapAccess, Visitor, value::BorrowedStrDeserializer, value::MapAccessDeserializer, value::StringDeserializer};

// Remote derive retains the original tagged representation and its defaults. Keep the variant
// fields in sync with NodeKind; format/model round trips cover every supported object kind.
#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(remote = "NodeKind", tag = "type", rename_all = "lowercase")]
enum Buffered {
    Layer {
        color: LayerColor,
        #[serde(default)]
        template: bool,
        #[serde(default = "yes")]
        printable: bool,
        children: Vec<Arc<Node>>,
        #[serde(default, skip_serializing_if = "crate::skip::is_default")]
        clip: bool,
        #[serde(default = "yes", skip_serializing_if = "crate::skip::is_true")]
        preview: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dim_images: Option<u8>,
    },
    Group {
        children: Vec<Arc<Node>>,
        #[serde(default)]
        clip: bool,
    },
    Path {
        path: PathData,
        #[serde(default, skip_serializing_if = "crate::skip::is_default")]
        rule: FillRule,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        live: Option<LiveShape>,
        #[serde(default, skip_serializing_if = "crate::skip::is_default")]
        clipping: bool,
        #[serde(default, skip_serializing_if = "crate::skip::is_default")]
        guide: bool,
    },
    Compound {
        children: Vec<Arc<Node>>,
        #[serde(default)]
        rule: FillRule,
    },
    Text(Box<TextObject>),
    Image(ImageObject),
    SymbolInstance {
        symbol: String,
        xf: Affine,
    },
    Blend {
        children: Vec<Arc<Node>>,
        #[serde(default)]
        spec: BlendSpec,
    },
    Envelope {
        content: Vec<Arc<Node>>,
        kind: EnvelopeKind,
        #[serde(default = "crate::live::default_fidelity")]
        fidelity: f64,
        #[serde(default)]
        editing: bool,
        #[serde(default, skip_serializing_if = "crate::skip::is_default")]
        options: crate::live::EnvelopeOptions,
        #[serde(default, skip_serializing_if = "crate::skip::is_default")]
        frame: Affine,
    },
    Mesh(GradientMesh),
    Repeat(RepeatSpec),
    PlacedDocument(Box<crate::placed_document::PlacedDocument>),
}

// A second tag must still be rejected, including null: Option<…> would accept a null duplicate.
fn duplicate_tag<'de, D: serde::Deserializer<'de>>(_: D) -> Result<(), D::Error> {
    Err(de::Error::duplicate_field("type"))
}

#[derive(Deserialize)]
struct Layer {
    color: LayerColor,
    #[serde(default)]
    template: bool,
    #[serde(default = "yes")]
    printable: bool,
    children: Vec<Arc<Node>>,
    #[serde(default)]
    clip: bool,
    #[serde(default = "yes")]
    preview: bool,
    #[serde(default)]
    dim_images: Option<u8>,
    #[serde(rename = "type", default, deserialize_with = "duplicate_tag")]
    _duplicate: (),
}

#[derive(Deserialize)]
struct Group {
    children: Vec<Arc<Node>>,
    #[serde(default)]
    clip: bool,
    #[serde(rename = "type", default, deserialize_with = "duplicate_tag")]
    _duplicate: (),
}

#[derive(Deserialize)]
struct Path {
    path: PathData,
    #[serde(default)]
    rule: FillRule,
    #[serde(default)]
    live: Option<LiveShape>,
    #[serde(default)]
    clipping: bool,
    #[serde(default)]
    guide: bool,
    #[serde(rename = "type", default, deserialize_with = "duplicate_tag")]
    _duplicate: (),
}

// Use Serde's identifier protocol, as the original enum does (including numeric identifiers
// from non-JSON deserializers). Known string tags need no allocation.
#[derive(Deserialize)]
#[serde(variant_identifier, rename_all = "lowercase")]
enum Tag {
    Layer,
    Group,
    Path,
    Compound,
    Text,
    Image,
    SymbolInstance,
    Blend,
    Envelope,
    Mesh,
    Repeat,
    PlacedDocument,
}

impl Tag {
    fn name(self) -> &'static str {
        match self {
            Self::Layer => "layer",
            Self::Group => "group",
            Self::Path => "path",
            Self::Compound => "compound",
            Self::Text => "text",
            Self::Image => "image",
            Self::SymbolInstance => "symbolinstance",
            Self::Blend => "blend",
            Self::Envelope => "envelope",
            Self::Mesh => "mesh",
            Self::Repeat => "repeat",
            Self::PlacedDocument => "placeddocument",
        }
    }
}

/// Replay the first key, and a tag value only when it was already read. Other values stay in
/// the original deserializer: converting them to JSON Value would lose duplicate fields.
struct Replay<M> {
    key: Option<String>,
    tag: Option<&'static str>,
    rest: M,
}

impl<'de, M: MapAccess<'de>> MapAccess<'de> for Replay<M> {
    type Error = M::Error;

    fn next_key_seed<K: de::DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error> {
        match self.key.take() {
            Some(key) => seed.deserialize(StringDeserializer::<M::Error>::new(key)).map(Some),
            None => self.rest.next_key_seed(seed),
        }
    }

    fn next_value_seed<V: de::DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Self::Error> {
        match self.tag.take() {
            Some(tag) => seed.deserialize(BorrowedStrDeserializer::<M::Error>::new(tag)),
            None => self.rest.next_value_seed(seed),
        }
    }
}

impl<'de> Deserialize<'de> for NodeKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Kind;
        impl<'de> Visitor<'de> for Kind {
            type Value = NodeKind;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a document object with a type tag")
            }

            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<NodeKind, M::Error> {
                let key = map.next_key::<String>()?;
                if key.as_deref() != Some("type") {
                    return Buffered::deserialize(MapAccessDeserializer::new(Replay { key, tag: None, rest: map }));
                }
                let tag = map.next_value::<Tag>()?;
                match tag {
                    Tag::Layer => {
                        let Layer { color, template, printable, children, clip, preview, dim_images, .. } =
                            Layer::deserialize(MapAccessDeserializer::new(map))?;
                        Ok(NodeKind::Layer { color, template, printable, children, clip, preview, dim_images })
                    }
                    Tag::Group => {
                        let Group { children, clip, .. } = Group::deserialize(MapAccessDeserializer::new(map))?;
                        Ok(NodeKind::Group { children, clip })
                    }
                    Tag::Path => {
                        let Path { path, rule, live, clipping, guide, .. } = Path::deserialize(MapAccessDeserializer::new(map))?;
                        Ok(NodeKind::Path { path, rule, live, clipping, guide })
                    }
                    other => Buffered::deserialize(MapAccessDeserializer::new(Replay { key, tag: Some(other.name()), rest: map })),
                }
            }
        }
        deserializer.deserialize_map(Kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_and_buffered_tag_orders_keep_container_defaults_and_children() {
        let d = crate::Document::new(120.0, 80.0);
        let layer = d.layers.first().unwrap().kind.clone();
        let group = NodeKind::Group { children: d.layers.clone(), clip: true };
        for kind in [layer, group] {
            let text = serde_json::to_string(&kind).unwrap();
            assert!(text.starts_with("{\"type\":"));
            assert_eq!(serde_json::from_str::<NodeKind>(&text).unwrap(), kind);
            let value = serde_json::to_value(&kind).unwrap();
            // JSON Value orders map keys: the tag is no longer first.
            assert_eq!(serde_json::from_value::<NodeKind>(value).unwrap(), kind);
        }
        let group: NodeKind = serde_json::from_str(r#"{"type":"group","children":[],"unrecognized":{"type":"path"}}"#).unwrap();
        assert_eq!(group, NodeKind::Group { children: vec![], clip: false });
    }

    #[test]
    fn malformed_or_duplicate_tags_are_rejected_in_every_order() {
        for text in [
            r#"{"type":"group","children":[],"type":null}"#,
            r#"{"type":"group","children":[],"type":"group"}"#,
            r#"{"children":[],"type":"group","type":"group"}"#,
            r#"{"type":"group","children":[],"children":[]}"#,
            r#"{"type":"layer","children":[]}"#,
            r#"{"type":"unknown","children":[]}"#,
            r#"{"type":null,"children":[]}"#,
            r#"{"children":[]}"#,
            r#"{"children":[{"id":2,"kind":{"type":"group","children":[],"type":"group"}}],"type":"group"}"#,
        ] {
            assert!(serde_json::from_str::<NodeKind>(text).is_err(), "{text}");
        }
    }

    #[test]
    fn non_string_tags_match_the_existing_tagged_reader() {
        for text in [
            r#"{"type":1,"children":[]}"#,
            r#"{"type":-1,"children":[]}"#,
            r#"{"type":1.0,"children":[]}"#,
            r#"{"type":false,"children":[]}"#,
            r#"{"type":null,"children":[]}"#,
        ] {
            let expected = Buffered::deserialize(&mut serde_json::Deserializer::from_str(text));
            let actual = serde_json::from_str::<NodeKind>(text);
            match (expected, actual) {
                (Ok(a), Ok(b)) => assert_eq!(a, b, "{text}"),
                (Err(_), Err(_)) => {}
                (a, b) => panic!("{text}: old {a:?}, new {b:?}"),
            }
        }
        let entries = vec![("type", serde_json::json!(1)), ("children", serde_json::json!([]))];
        let expected = Buffered::deserialize(de::value::MapDeserializer::<_, serde_json::Error>::new(entries.clone().into_iter()));
        let actual = NodeKind::deserialize(de::value::MapDeserializer::<_, serde_json::Error>::new(entries.into_iter()));
        match (expected, actual) {
            (Ok(a), Ok(b)) => assert_eq!(a, b),
            (Err(_), Err(_)) => {}
            (a, b) => panic!("ordered map: old {a:?}, new {b:?}"),
        }
    }

    #[test]
    fn paths_keep_geometry_flags_and_arbitrary_tag_order() {
        let path = NodeKind::Path {
            path: shapes::rectangle(Rect::new(1.25, 3.5, 60.75, 42.125)),
            rule: FillRule::EvenOdd,
            live: None,
            clipping: true,
            guide: true,
        };
        let text = serde_json::to_string(&path).unwrap();
        assert_eq!(serde_json::from_str::<NodeKind>(&text).unwrap(), path);
        assert_eq!(serde_json::from_value::<NodeKind>(serde_json::to_value(&path).unwrap()).unwrap(), path);
        let late: NodeKind = serde_json::from_str(r#"{"children":[],"clip":true,"type":"group"}"#).unwrap();
        assert_eq!(late, NodeKind::Group { children: vec![], clip: true });
        assert!(serde_json::from_str::<NodeKind>(r#"{"type":"path","path":{"subpaths":[]},"type":null}"#).is_err());
    }
}
