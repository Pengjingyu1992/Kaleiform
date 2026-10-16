//! Opt-in translation previews for hosts that can display a cached, ordered artwork scene.
use std::sync::Arc;

use serde_json::{Value, json};
use vectorcraft_doc::{Document, NodeId};
use vectorcraft_geom::{Rect, Vec2};

use crate::{EngineError, Result, Session};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeferredMove {
    pub offset: Vec2,
    pub copy: bool,
}

pub(crate) struct PreparedMove {
    uid: u64,
    doc: Arc<Document>,
    selection: Vec<NodeId>,
    roots: Vec<NodeId>,
    bounds: Rect,
}

impl Session {
    /// Enable a cached move for this exact selection/snapshot. Other hosts retain live geometry.
    /// The host must render the roots independently in their original paint order.
    pub fn prepare_move_preview(&mut self, roots: Vec<NodeId>, bounds: Rect) {
        self.move_preview =
            self.active().map(|st| PreparedMove { uid: st.uid, doc: st.doc.clone(), selection: st.selection.objects.clone(), roots, bounds });
    }

    pub fn clear_move_preview(&mut self) {
        self.move_preview = None;
    }

    pub fn deferred_move(&self) -> Option<DeferredMove> {
        self.active()?.interaction.as_ref()?.deferred_move
    }

    pub(crate) fn preview_tool_action(&mut self, command: &str, params: &Value) -> Result<Value> {
        if let Some(result) = self.defer_move(command, params)? {
            return Ok(result);
        }
        self.preview(command, params)
    }

    fn defer_move(&mut self, command: &str, params: &Value) -> Result<Option<Value>> {
        let Some(prepared) = &self.move_preview else { return Ok(None) };
        let Some(st) = self.active() else { return Ok(None) };
        let Some(interaction) = &st.interaction else { return Ok(None) };
        if command != "object.transform"
            || !matches!(interaction.label.as_str(), "Move" | "Copy")
            || prepared.uid != st.uid
            || !Arc::ptr_eq(&prepared.doc, &interaction.doc)
            || prepared.selection != interaction.selection.objects
            || params.as_object().is_none_or(|p| p.keys().any(|k| k != "matrix" && k != "copy"))
        {
            return Ok(None);
        }
        let Some(matrix) = crate::cmd::matrix_param(params, "matrix") else { return Ok(None) };
        let [a, b, c, d, x, y] = matrix.as_coeffs();
        let copy = params.get("copy").and_then(Value::as_bool).unwrap_or(false);
        if [a, b, c, d] != [1.0, 0.0, 0.0, 1.0] || (copy && prepared.roots.len() != 1) {
            return Ok(None);
        }
        let offset = Vec2::new(x, y);
        let bounds = prepared.bounds + offset;
        if [bounds.x0, bounds.y0, bounds.x1, bounds.y1].iter().any(|v| !v.is_finite() || v.abs() > crate::MAX_COORD) {
            return Err(EngineError::Other("result would exceed the canvas (coordinates out of range)".into()));
        }
        let st = self.doc_mut()?;
        if let Some(interaction) = &mut st.interaction {
            if !Arc::ptr_eq(&st.doc, &interaction.doc) {
                st.doc = interaction.doc.clone();
                st.selection = interaction.selection.clone();
            }
            interaction.preview = Some((command.to_string(), params.clone()));
            interaction.deferred_move = Some(DeferredMove { offset, copy });
        }
        Ok(Some(json!({"deferred": true})))
    }

    /// Apply the latest deferred command through the usual guarded transform implementation.
    /// Saves, recovery, external commands and release all see exact geometry.
    pub fn materialize_deferred_move(&mut self) -> Result<()> {
        let preview = self.active_mut().and_then(|st| {
            let interaction = st.interaction.as_mut()?;
            let movement = interaction.deferred_move.take()?;
            if !movement.copy && movement.offset == Vec2::ZERO {
                interaction.preview = None;
                return None;
            }
            interaction.preview.clone()
        });
        if let Some((command, params)) = preview
            && let Err(error) = self.preview(&command, &params)
        {
            self.cancel_interaction()?;
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Action;

    fn prepared() -> (Session, NodeId, Arc<Document>) {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
        let id = NodeId(s.execute("shape.rectangle", &json!({"x": 30, "y": 40, "width": 50, "height": 60})).unwrap()["id"].as_u64().unwrap());
        let st = s.doc_mut().unwrap();
        st.mark_saved();
        st.history.undo.clear();
        let snapshot = st.doc.clone();
        let bounds = snapshot.node(id).unwrap().visual_bounds().unwrap();
        s.journal.clear();
        s.prepare_move_preview(vec![id], bounds);
        s.begin_interaction("Move").unwrap();
        (s, id, snapshot)
    }

    fn preview(s: &mut Session, x: f64, y: f64, copy: bool) {
        s.apply_actions(vec![Action::Preview("object.transform".into(), json!({"matrix": [1, 0, 0, 1, x, y], "copy": copy}))]).unwrap();
    }

    #[test]
    fn deferred_move_reuses_geometry_and_commits_latest_delta_with_one_undo() {
        let (mut s, id, original) = prepared();
        for i in 1..=100 {
            preview(&mut s, i as f64, -10.0, false);
            assert!(Arc::ptr_eq(&s.doc().unwrap().doc, &original));
            assert!(s.doc().unwrap().is_dirty());
            assert!(s.journal.is_empty());
        }
        s.commit_interaction().unwrap();
        let moved = s.doc().unwrap().doc.node(id).unwrap().geometric_bounds().unwrap();
        assert_eq!((moved.x0, moved.y0), (130.0, 30.0));
        assert_eq!(s.doc().unwrap().history.undo.len(), 1);
        assert_eq!(s.journal.len(), 1);
        s.execute("edit.undo", &json!({})).unwrap();
        assert!(Arc::ptr_eq(&s.doc().unwrap().doc, &original));
        assert!(!s.doc().unwrap().is_dirty());
        s.execute("edit.redo", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().doc.node(id).unwrap().geometric_bounds().unwrap(), moved);
    }

    #[test]
    fn cancelling_and_zero_moves_leave_original_snapshot_clean() {
        for offset in [0.0, 50.0] {
            let (mut s, _, original) = prepared();
            preview(&mut s, offset, 0.0, false);
            if offset == 0.0 {
                assert!(!s.doc().unwrap().is_dirty());
            }
            s.cancel_interaction().unwrap();
            assert!(Arc::ptr_eq(&s.doc().unwrap().doc, &original));
            assert!(s.doc().unwrap().history.undo.is_empty());
            assert!(s.journal.is_empty());
        }
        let (mut s, _, original) = prepared();
        preview(&mut s, 0.0, 0.0, false);
        s.commit_interaction().unwrap();
        assert!(Arc::ptr_eq(&s.doc().unwrap().doc, &original));
        assert!(s.doc().unwrap().history.undo.is_empty());
        assert!(s.journal.is_empty());
    }

    #[test]
    fn copy_modifier_changes_never_accumulate_duplicate_paths() {
        let (mut s, id, original) = prepared();
        for copy in [true, false, true] {
            preview(&mut s, 70.0, 20.0, copy);
            assert!(Arc::ptr_eq(&s.doc().unwrap().doc, &original));
        }
        s.commit_interaction().unwrap();
        let st = s.doc().unwrap();
        assert_eq!(st.doc.node_count(), original.node_count() + 1);
        assert_eq!(st.doc.node(id).unwrap().geometric_bounds(), original.node(id).unwrap().geometric_bounds());
        let copied = st.selection.objects[0];
        assert_ne!(copied, id);
        assert_eq!(st.doc.node(copied).unwrap().geometric_bounds().unwrap().x0, 100.0);
        assert_eq!(st.history.undo.len(), 1);
    }

    #[test]
    fn external_inspection_and_recovery_materialize_exact_geometry() {
        let (mut s, id, original) = prepared();
        preview(&mut s, 25.0, 30.0, false);
        s.execute("document.inspect", &json!({})).unwrap();
        assert_eq!(s.doc().unwrap().doc.node(id).unwrap().geometric_bounds().unwrap().x0, 55.0);
        assert!(s.deferred_move().is_none());
        preview(&mut s, 40.0, 10.0, false);
        let store: Arc<dyn crate::cmd::recovery::RecoveryStore> = Arc::new(crate::cmd::recovery::MemoryStore::default());
        let (jobs, _) = crate::cmd::recovery::jobs(&mut s, &store).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(s.doc().unwrap().doc.node(id).unwrap().geometric_bounds().unwrap().x0, 70.0);
        jobs[0].write(store.as_ref()).unwrap();
        jobs[0].clone().finish(&mut s);
        s.cancel_interaction().unwrap();
        assert!(Arc::ptr_eq(&s.doc().unwrap().doc, &original));
    }

    #[test]
    fn invalid_offsets_keep_last_valid_preview_and_nontranslations_use_normal_commands() {
        let (mut s, id, _) = prepared();
        preview(&mut s, 10.0, 20.0, false);
        assert!(s.defer_move("object.transform", &json!({"matrix": [1, 0, 0, 1, 1e20, 0]})).is_err());
        assert_eq!(s.deferred_move().unwrap().offset, Vec2::new(10.0, 20.0));
        s.preview_tool_action("object.transform", &json!({"matrix": [2, 0, 0, 2, 0, 0]})).unwrap();
        assert!(s.deferred_move().is_none());
        s.commit_interaction().unwrap();
        assert_eq!(s.doc().unwrap().doc.node(id).unwrap().geometric_bounds().unwrap().width(), 100.0);
    }
}
