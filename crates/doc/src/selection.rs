//! Selection state (objects and, for direct selection, individual anchors; ruler guides).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{Document, Node, NodeId, NodeKind};

/// (subpath index, anchor index) inside a path.
pub type AnchorRef = (usize, usize);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Selection {
    /// Selected objects in selection order.
    pub objects: Vec<NodeId>,
    /// Direct-selected anchors per path. A path in `objects` with no entry here is fully selected.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub anchors: BTreeMap<NodeId, BTreeSet<AnchorRef>>,
    /// Key object for Align.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<NodeId>,
    /// The object, group or layer targeted through its target circle in the Layers panel, which
    /// appearance, transparency and opacity-mask commands then act on (a targeted layer has its
    /// art selected). Any other selection change clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<NodeId>,
    /// Slices selected with the Slice Selection tool: user slice ids and the ids of objects whose
    /// object slice is selected (see [`crate::slices`]). Any change to the object selection
    /// clears them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slices: Vec<NodeId>,
    /// Ruler guides selected with a selection tool, as indexes into [`Document::guides`]. They are
    /// selected on their own: any change to the object selection clears them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guides: Vec<usize>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
    /// Is anything selected that Delete and the arrow keys act on: objects or ruler guides?
    pub fn has_objects_or_guides(&self) -> bool {
        !self.objects.is_empty() || !self.guides.is_empty()
    }
    pub fn len(&self) -> usize {
        self.objects.len()
    }
    pub fn contains(&self, id: NodeId) -> bool {
        self.objects.contains(&id)
    }
    pub fn clear(&mut self) {
        self.objects.clear();
        self.anchors.clear();
        self.key = None;
        self.target = None;
        self.slices.clear();
        self.guides.clear();
    }
    pub fn set(&mut self, ids: impl IntoIterator<Item = NodeId>) {
        self.clear();
        let mut seen = HashSet::new();
        self.objects.extend(ids.into_iter().filter(|id| seen.insert(*id)));
    }
    pub fn add(&mut self, id: NodeId) {
        self.target = None;
        self.slices.clear();
        self.guides.clear();
        if !self.objects.contains(&id) {
            self.objects.push(id);
        }
    }
    pub fn remove(&mut self, id: NodeId) {
        self.target = None;
        self.slices.clear();
        self.guides.clear();
        self.objects.retain(|x| *x != id);
        self.anchors.remove(&id);
        if self.key == Some(id) {
            self.key = None;
        }
    }
    /// Select slices `ids` (see [`Selection::slices`]) and nothing else.
    pub fn set_slices(&mut self, ids: impl IntoIterator<Item = NodeId>) {
        self.clear();
        for id in ids {
            if !self.slices.contains(&id) {
                self.slices.push(id);
            }
        }
    }
    /// Select ruler guides `indexes` (see [`Selection::guides`]) and nothing else.
    pub fn set_guides(&mut self, indexes: impl IntoIterator<Item = usize>) {
        self.clear();
        for i in indexes {
            if !self.guides.contains(&i) {
                self.guides.push(i);
            }
        }
    }
    pub fn toggle(&mut self, id: NodeId) {
        if self.contains(id) { self.remove(id) } else { self.add(id) }
    }
    /// Are some (not all) anchors of `id` direct-selected?
    pub fn partial(&self, id: NodeId) -> Option<&BTreeSet<AnchorRef>> {
        self.anchors.get(&id)
    }
    /// Drop ids that no longer exist or are no longer editable.
    pub fn prune(&mut self, doc: &Document) {
        // Ungrouping a dense trace selects all its paths; don't search the tree once per path.
        let requested: Vec<_> = self.objects.iter().chain(self.anchors.keys()).chain(self.key.iter()).chain(self.target.iter()).copied().collect();
        let existing: HashSet<_> = doc.nodes(&requested).iter().map(|n| n.id).collect();
        self.objects.retain(|id| existing.contains(id));
        self.anchors.retain(|id, _| existing.contains(id));
        if self.key.is_some_and(|k| !existing.contains(&k)) {
            self.key = None;
        }
        if self.target.is_some_and(|t| !existing.contains(&t)) {
            self.target = None;
        }
        self.slices.retain(|id| doc.is_slice(*id));
        self.guides.retain(|i| *i < doc.guides.len());
    }
    /// Target `id` (see [`Selection::target`]): a layer gets its visible, unlocked art selected
    /// (the art of its sublayers too), anything else is selected itself.
    pub fn set_target(&mut self, doc: &Document, id: NodeId) {
        match doc.node(id) {
            Some(n) if n.is_layer() => self.set(n.layer_art(true)),
            _ => self.set([id]),
        }
        self.target = Some(id);
    }
    /// What appearance and transparency edits act on: the targeted object, group or layer, else
    /// the selected objects.
    pub fn subjects(&self) -> &[NodeId] {
        match &self.target {
            Some(t) => std::slice::from_ref(t),
            None => &self.objects,
        }
    }
    /// Top-level ordering: selected ids sorted by paint order (bottom first).
    pub fn in_paint_order(&self, doc: &Document) -> Vec<NodeId> {
        doc.paint_order(self.objects.iter().copied())
    }

    /// Existing selected nodes in selection order, found by one tree traversal.
    pub fn nodes<'a>(&self, doc: &'a Document) -> Vec<&'a Node> {
        doc.nodes(&self.objects)
    }

    /// Closest matching nodes at or above the selected objects, in selection order, without
    /// duplicates. Walk once instead of looking up every object's ancestry from the tree root.
    pub fn matching_ancestors(&self, doc: &Document, mut matches: impl FnMut(&Node) -> bool) -> Vec<NodeId> {
        if self.is_empty() {
            return vec![];
        }
        let mut selected: HashSet<_> = self.objects.iter().copied().collect();
        let mut found = HashMap::new();
        let mut stack: Vec<_> = doc.layers.iter().rev().map(|n| (n.as_ref(), None)).collect();
        while let Some((node, ancestor)) = stack.pop() {
            let nearest = if matches(node) { Some(node.id) } else { ancestor };
            if selected.remove(&node.id) {
                if let Some(id) = nearest {
                    found.insert(node.id, id);
                }
                if selected.is_empty() {
                    break;
                }
            }
            if let Some(children) = node.children() {
                stack.extend(children.iter().rev().map(|n| (n.as_ref(), nearest)));
            }
        }
        let mut seen = HashSet::new();
        self.objects.iter().filter_map(|id| found.get(id).copied()).filter(|id| seen.insert(*id)).collect()
    }

    /// Selected roots in paint order: compound members stand for their compound, selected
    /// ancestors suppress descendants, and layers themselves are not editing roots.
    pub fn root_nodes<'a>(&self, doc: &'a Document) -> Vec<&'a Node> {
        if self.is_empty() {
            return vec![];
        }
        let selected: HashSet<_> = self.objects.iter().copied().collect();
        let mut roots = vec![];
        let mut stack: Vec<_> = doc.layers.iter().rev().map(|n| n.as_ref()).collect();
        while let Some(node) = stack.pop() {
            let compound_member = matches!(&node.kind, NodeKind::Compound { children, .. } if children.iter().any(|n| selected.contains(&n.id)));
            if selected.contains(&node.id) || compound_member {
                if !node.is_layer() {
                    roots.push(node);
                }
            } else if let Some(children) = node.children() {
                stack.extend(children.iter().rev().map(|n| n.as_ref()));
            }
        }
        roots
    }
}

impl Document {
    /// Existing nodes in the requested order (including duplicates), found by one tree traversal.
    pub fn nodes(&self, ids: &[NodeId]) -> Vec<&Node> {
        if ids.is_empty() {
            return vec![];
        }
        let mut remaining: HashSet<_> = ids.iter().copied().collect();
        let mut found = HashMap::new();
        let mut stack: Vec<_> = self.layers.iter().rev().map(|n| n.as_ref()).collect();
        while let Some(node) = stack.pop() {
            if remaining.remove(&node.id) {
                found.insert(node.id, node);
                if remaining.is_empty() {
                    break;
                }
            }
            if let Some(children) = node.children() {
                stack.extend(children.iter().rev().map(|n| n.as_ref()));
            }
        }
        ids.iter().filter_map(|id| found.get(id).copied()).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn query_document() -> Document {
        let leaf = |id| Arc::new(Node::group(NodeId(id), vec![]));
        let compound = Node::new(NodeId(3), NodeKind::Compound { children: vec![leaf(4), leaf(5)], rule: Default::default() });
        let group = Node::group(NodeId(2), vec![Arc::new(compound), leaf(6)]);
        let mut doc = Document::new(100.0, 100.0);
        Arc::make_mut(doc.layers.first_mut().unwrap()).children_mut().unwrap().extend([Arc::new(group), leaf(7)]);
        doc.fix_next_id();
        doc
    }

    #[test]
    fn bulk_queries_keep_selection_order_and_compound_root_semantics() {
        let doc = query_document();
        // Compare every subset, in reverse selection order, with the original per-id queries.
        for bits in 0..256 {
            let ids = [1, 2, 3, 4, 5, 6, 7, 999].into_iter().enumerate().filter_map(|(i, id)| (bits & (1 << i) != 0).then_some(NodeId(id)));
            let selection = Selection { objects: ids.rev().collect(), ..Default::default() };
            assert_eq!(selection.nodes(&doc), selection.objects.iter().filter_map(|id| doc.node(*id)).collect::<Vec<_>>());
            let pred = |n: &Node| matches!(n.id.0, 2 | 3 | 7);
            let mut expected = vec![];
            for id in &selection.objects {
                if let Some(a) = doc.ancestry(*id).unwrap_or_default().into_iter().rev().find(|a| doc.node(*a).is_some_and(pred))
                    && !expected.contains(&a)
                {
                    expected.push(a);
                }
            }
            assert_eq!(selection.matching_ancestors(&doc, pred), expected);
            let mut roots: Vec<_> = selection
                .in_paint_order(&doc)
                .into_iter()
                .map(|id| match doc.parent_of(id).and_then(|p| doc.node(p).map(|n| (p, n))) {
                    Some((p, n)) if matches!(n.kind, NodeKind::Compound { .. }) => p,
                    _ => id,
                })
                .collect();
            roots.dedup();
            let expected: Vec<_> = roots
                .iter()
                .copied()
                .filter(|id| {
                    let ancestors = doc.ancestry(*id).unwrap_or_default();
                    !ancestors[..ancestors.len().saturating_sub(1)].iter().any(|a| roots.contains(a)) && doc.node(*id).is_some_and(|n| !n.is_layer())
                })
                .collect();
            assert_eq!(selection.root_nodes(&doc).iter().map(|n| n.id).collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn bulk_ancestor_query_visits_each_node_once() {
        let mut doc = Document::new(100.0, 100.0);
        let children: Vec<_> = (2..10_002).map(|id| Arc::new(Node::group(NodeId(id), vec![]))).collect();
        Arc::make_mut(doc.layers.first_mut().unwrap()).children_mut().unwrap().extend(children);
        let selection = Selection { objects: (2..10_002).map(NodeId).collect(), ..Default::default() };
        let mut visits = 0;
        assert!(
            selection
                .matching_ancestors(&doc, |_| {
                    visits += 1;
                    false
                })
                .is_empty()
        );
        assert_eq!(visits, 10_001);
        assert_eq!(selection.root_nodes(&doc).len(), 10_000);
        assert_eq!(selection.nodes(&doc).len(), 10_000);
    }

    #[test]
    fn basic_ops() {
        let mut s = Selection::default();
        s.add(NodeId(1));
        s.add(NodeId(1));
        s.toggle(NodeId(2));
        assert_eq!(s.objects, vec![NodeId(1), NodeId(2)]);
        s.toggle(NodeId(1));
        assert_eq!(s.objects, vec![NodeId(2)]);
        s.clear();
        assert!(s.is_empty());
    }

    #[test]
    fn bulk_set_and_prune_keep_order_and_independent_anchor_targets() {
        let doc = query_document();
        let mut s = Selection::default();
        s.set([NodeId(7), NodeId(4), NodeId(7), NodeId(999), NodeId(2)]);
        assert_eq!(s.objects, vec![NodeId(7), NodeId(4), NodeId(999), NodeId(2)]);
        s.anchors.insert(NodeId(6), BTreeSet::from([(0, 0)]));
        s.anchors.insert(NodeId(999), BTreeSet::from([(0, 0)]));
        s.key = Some(NodeId(5));
        s.target = Some(NodeId(3));
        s.prune(&doc);
        assert_eq!(s.objects, vec![NodeId(7), NodeId(4), NodeId(2)]);
        assert_eq!(s.anchors.keys().copied().collect::<Vec<_>>(), vec![NodeId(6)]);
        assert_eq!(s.key, Some(NodeId(5)));
        assert_eq!(s.target, Some(NodeId(3)));
        s.key = Some(NodeId(998));
        s.target = Some(NodeId(999));
        s.prune(&doc);
        assert_eq!((s.key, s.target), (None, None));
    }

    #[test]
    fn bulk_nodes_preserve_duplicates_and_ignore_missing_ids() {
        let doc = query_document();
        let ids = [NodeId(7), NodeId(4), NodeId(999), NodeId(7), NodeId(2)];
        assert_eq!(doc.nodes(&ids), ids.iter().filter_map(|id| doc.node(*id)).collect::<Vec<_>>());
        assert!(doc.nodes(&[]).is_empty());
    }

    #[test]
    fn guides_are_selected_on_their_own() {
        let mut s = Selection::default();
        s.add(NodeId(1));
        s.set_guides([2, 0, 2]);
        assert_eq!((s.objects.clone(), s.guides.clone()), (vec![], vec![2, 0]));
        assert!(s.has_objects_or_guides() && s.is_empty());
        s.add(NodeId(1));
        assert!(s.guides.is_empty());
        // Pruning drops guides the document no longer has.
        let mut d = Document::new(100.0, 100.0);
        d.guides.push(crate::Guide::new(true, 10.0));
        s.set_guides([0, 1]);
        s.prune(&d);
        assert_eq!(s.guides, vec![0]);
    }

    #[test]
    fn selection_changes_clear_the_target() {
        let mut s = Selection::default();
        s.set([NodeId(2), NodeId(3)]);
        s.target = Some(NodeId(1));
        assert_eq!(s.subjects(), &[NodeId(1)]);
        s.add(NodeId(4));
        assert_eq!(s.target, None);
        assert_eq!(s.subjects(), &[NodeId(2), NodeId(3), NodeId(4)]);
        s.target = Some(NodeId(1));
        s.remove(NodeId(2));
        assert_eq!(s.target, None);
        s.target = Some(NodeId(1));
        s.set([NodeId(3)]);
        assert_eq!(s.target, None);
    }
}
