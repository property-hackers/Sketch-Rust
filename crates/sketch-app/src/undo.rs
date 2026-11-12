//! Undo / redo (screen 6c): `snapshotModel`, `pushUndo`, `undo`, `redo` and
//! `undoTo` (refs/sketch-app.js:4656-4760).
//!
//! Every snapshot is a labelled copy of the model plus the live drawing chain
//! (`restoreDrawingChain` is deliberately conservative: the chain is restored
//! only while it is still being drawn, because restoring a finished chain's
//! pen-down state used to leave an invisible start point behind).
//!
//! Like the original's, a snapshot carries walls, areas, text labels, symbols,
//! the layer stack, the live drawing chain and the dimension chains.

use sketch_core::geometry::Pt;
use sketch_core::model::{Polygon, SketchLine};

/// One labelled model snapshot (`snapshotModel`).
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub label: String,
    pub lines: Vec<SketchLine>,
    pub polygons: Vec<Polygon>,
    pub reopened_areas: Vec<crate::area_repair::ReopenedArea>,
    /// `labels`: the free text labels.
    pub texts: Vec<crate::placed_text::PlacedText>,
    pub symbols: Vec<crate::placed_symbol::PlacedSymbol>,
    pub cursor: Pt,
    pub points: Vec<Pt>,
    pub line_ids: Vec<i64>,
    pub pen_down: bool,
    /// `state.dimensionChains` (screen 9).
    pub dimension_chains: Vec<crate::chains::ChainRecord>,
    /// `state.layers` + `state.activeLayerId` — the layer stack rides with
    /// every snapshot (the original's `snapshotModel`), so Move Layer /
    /// layer toggles undo as a whole. `None` for a snapshot built without
    /// the App (all App-side builders stamp it).
    pub layers: Option<Vec<crate::layers::Layer>>,
    /// `northAngle` of the page the snapshot was taken on, so a rotate / flip
    /// of the sketch (which turns the compass with it) undoes whole. `None`
    /// for a snapshot built without the App.
    pub north_angle: Option<f64>,
    pub active_layer_id: Option<String>,
}

/// The two stacks. `push` clears redo, as `pushUndo` does.
#[derive(Debug, Clone, Default)]
pub struct UndoStack {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Bumped by every push / undo / redo, so a caller can tell cheaply
    /// that the document may have changed since it last looked.
    revision: u64,
}

impl UndoStack {
    /// Points placed symbols of a retired id at the id that replaced it.
    pub fn remap_symbols(&mut self, map: &std::collections::BTreeMap<String, String>) {
        for snapshot in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            for s in &mut snapshot.symbols {
                if let Some(to) = map.get(&s.symbol_type) {
                    s.symbol_type = to.clone();
                }
            }
        }
    }
    pub fn purge_symbols(&mut self, ids: &std::collections::BTreeSet<String>) {
        for snapshot in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            snapshot.symbols.retain(|s| !ids.contains(&s.symbol_type));
        }
    }
    /// `pushUndo(label)`: record `snapshot` and drop the redo history.
    pub fn push(&mut self, snapshot: Snapshot) {
        self.revision += 1;
        self.undo.push(snapshot);
        self.redo.clear();
    }

    /// `undo()`: the state to restore, having banked `current` for redo. The
    /// redo entry carries the label of the action being undone, which is what
    /// the history list shows.
    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let label = self
            .undo
            .last()
            .map(|s| s.label.clone())
            .unwrap_or_else(|| "Edit".to_string());
        let prev = self.undo.pop()?;
        self.revision += 1;
        self.redo.push(Snapshot { label, ..current });
        Some(prev)
    }

    /// `redo()`.
    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let label = self
            .redo
            .last()
            .map(|s| s.label.clone())
            .unwrap_or_else(|| "Edit".to_string());
        let next = self.redo.pop()?;
        self.revision += 1;
        self.undo.push(Snapshot { label, ..current });
        Some(next)
    }

    /// See the `revision` field.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// `undoTo(index)`: rewind until `index` entries remain, restoring each
    /// intermediate state in turn (`while (undoStack.length > target) undo()`).
    pub fn undo_to(&mut self, index: usize, current: Snapshot) -> Vec<Snapshot> {
        let target = index.min(self.undo.len());
        let mut out = Vec::new();
        let mut current = current;
        let mut guard = self.undo.len() + 1;
        while self.undo.len() > target && guard > 0 {
            guard -= 1;
            let Some(state) = self.undo(current.clone()) else {
                break;
            };
            out.push(state.clone());
            current = state;
        }
        out
    }

    /// The history list, newest first (`showUndoHistory` renders `N. <label>`).
    pub fn labels(&self) -> Vec<String> {
        self.undo.iter().rev().map(|s| s.label.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketch_core::model::WallType;

    fn snap(label: &str, walls: usize) -> Snapshot {
        Snapshot {
            label: label.to_string(),
            lines: (0..walls)
                .map(|i| SketchLine {
                    id: i as i64 + 1,
                    start: Pt::new(i as f64 * 10.0, 0.0),
                    end: Pt::new(i as f64 * 10.0 + 10.0, 0.0),
                    kind: WallType::Exterior,
                    color: None,
                    line_thickness: None,
                    thickness: None,
                    dimension_hidden: false,
                    layer_id: None,
                    bow_angle: 0.0,
                    arc_height: 0.0,
                    line_style: None,
                    dimension_position: None,
                    dimension_anchor: None,
                    extra: Default::default(),
                })
                .collect(),
            polygons: Vec::new(),
            reopened_areas: Vec::new(),
            texts: Vec::new(),
            symbols: Vec::new(),
            north_angle: None,
            cursor: Pt::new(0.0, 0.0),
            points: Vec::new(),
            line_ids: Vec::new(),
            pen_down: false,
            dimension_chains: Vec::new(),
            layers: None,
            active_layer_id: None,
        }
    }

    #[test]
    fn undo_and_redo_walk_the_stack() {
        let mut stack = UndoStack::default();
        assert!(!stack.can_undo() && !stack.can_redo());
        stack.push(snap("Draw", 1));
        stack.push(snap("Draw", 2));
        assert_eq!(stack.labels().len(), 2);

        // Undo back to one wall.
        let prev = stack.undo(snap("current", 3)).expect("undo");
        assert_eq!(prev.lines.len(), 2);
        assert!(stack.can_redo());
        // And again to none.
        let prev = stack.undo(snap("current", 2)).expect("undo");
        assert_eq!(prev.lines.len(), 1);
        assert!(!stack.can_undo());
        assert_eq!(stack.undo(snap("current", 1)), None, "nothing left");

        // Redo restores what was undone.
        let next = stack.redo(snap("current", 1)).expect("redo");
        assert_eq!(next.lines.len(), 2);
        assert!(stack.can_undo());
    }

    #[test]
    fn a_new_action_clears_the_redo_history() {
        let mut stack = UndoStack::default();
        stack.push(snap("Draw", 1));
        stack.undo(snap("current", 2));
        assert!(stack.can_redo());
        stack.push(snap("Draw", 5));
        assert!(!stack.can_redo(), "pushUndo clears redo");
    }

    #[test]
    fn the_history_lists_labels_newest_first() {
        let mut stack = UndoStack::default();
        stack.push(snap("Draw", 1));
        stack.push(snap("Wall Appearance", 2));
        stack.push(snap("Set Length", 3));
        assert_eq!(
            stack.labels(),
            vec!["Set Length", "Wall Appearance", "Draw"]
        );
    }

    #[test]
    fn undo_to_walks_back_to_the_requested_step() {
        let mut stack = UndoStack::default();
        stack.push(snap("Draw", 1));
        stack.push(snap("Draw", 2));
        stack.push(snap("Draw", 3));
        // `undoTo(0)` walks back while the stack is longer than the target, so
        // three banked states take three steps and leave none.
        let states = stack.undo_to(0, snap("current", 4));
        assert_eq!(states.len(), 3);
        assert_eq!(states.last().map(|s| s.lines.len()), Some(1));
        assert_eq!(stack.labels().len(), 0);
        // `undoTo(1)` with three banked states leaves exactly one.
        let mut stack = UndoStack::default();
        stack.push(snap("Draw", 1));
        stack.push(snap("Draw", 2));
        stack.push(snap("Draw", 3));
        assert_eq!(stack.undo_to(1, snap("current", 4)).len(), 2);
        assert_eq!(stack.labels().len(), 1);
        // `undoTo(2)` rewinds just the newest action.
        let mut stack = UndoStack::default();
        stack.push(snap("Draw", 1));
        stack.push(snap("Draw", 2));
        stack.push(snap("Draw", 3));
        assert_eq!(stack.undo_to(2, snap("current", 4)).len(), 1);
        assert_eq!(stack.labels().len(), 2);
    }

    #[test]
    fn undo_banks_the_undone_action_label_for_redo() {
        let mut stack = UndoStack::default();
        stack.push(snap("Draw", 1));
        stack.undo(snap("current", 2));
        // The redo entry describes the action it will replay.
        assert_eq!(stack.redo.last().map(|s| s.label.as_str()), Some("Draw"));
    }
}
