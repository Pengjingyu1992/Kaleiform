use std::sync::Arc;

use vectorcraft_geom::{Affine, Rect, shapes};

use crate::{Appearance, Document, Node, NodeId};

#[test]
fn root_mutation_matches_individual_updates_and_keeps_untouched_arcs() {
    let mut doc = Document::new(400.0, 300.0);
    let mut nodes = vec![];
    let mut roots = vec![];
    for i in 0..200 {
        let id = doc.alloc_id();
        let node = Arc::new(Node::path(id, shapes::rectangle(Rect::new(i as f64, 10.0, i as f64 + 2.0, 15.0)), Appearance::default_art()));
        if i % 3 == 0 {
            roots.push(id);
        }
        nodes.push(node);
    }
    let untouched = nodes[1].clone();
    Arc::make_mut(doc.layers.first_mut().unwrap()).children_mut().unwrap().extend(nodes);
    let mut expected = doc.clone();
    let transform = Affine::translate((12.5, -7.0));
    for id in &roots {
        expected.node_mut(*id).unwrap().transform(transform, false);
    }
    roots.push(NodeId(u64::MAX));
    doc.for_each_root_mut(&roots, |n| n.transform(transform, false));
    assert_eq!(serde_json::to_value(&doc).unwrap(), serde_json::to_value(&expected).unwrap());
    let kept = &doc.layers[0].children().unwrap()[1];
    assert!(Arc::ptr_eq(kept, &untouched));
}

#[test]
fn a_selected_ancestor_suppresses_its_children() {
    let mut doc = Document::new(100.0, 100.0);
    let child_id = doc.alloc_id();
    let group_id = doc.alloc_id();
    let child = Node::path(child_id, shapes::rectangle(Rect::new(10.0, 10.0, 20.0, 20.0)), Appearance::default_art());
    let group = Node::group(group_id, vec![Arc::new(child)]);
    Arc::make_mut(doc.layers.first_mut().unwrap()).children_mut().unwrap().push(Arc::new(group));
    let mut visits = 0;
    doc.for_each_root_mut(&[child_id, group_id, group_id], |n| {
        visits += 1;
        n.transform(Affine::translate((5.0, 7.0)), false);
    });
    assert_eq!(visits, 1);
    assert_eq!(doc.node(child_id).unwrap().geometric_bounds().unwrap().origin(), (15.0, 17.0).into());
}
