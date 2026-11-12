//! Wall drawing with the mouse (refs/sketch-app.js penDown / penUp /
//! snapPoint / structuralSnap / getDrawingTarget / handleCanvasClick /
//! toggleLastDimension, refs/canvas-engine.js pointermove HUD) and the HUD
//! strings (refs/ui-manager.js updateHUD). Closing a loop into an area,
//! wall-join healing and undo arrive with screens 5b-2 / 6.

use std::collections::HashSet;

use sketch_core::geometry::{
    distance, GeometryEngine, JsRound, Pt, MAX_BOW_ANGLE, PIXELS_PER_FOOT,
};
use sketch_core::model::{geometry_lines, Polygon, SketchLine, WallType};

/// What drawing needs from the view and mode.
#[derive(Debug, Clone, Copy)]
pub struct DrawCtx {
    /// `state.snapEnabled`.
    pub snap: bool,
    /// `state.gridSize` (ft).
    pub grid: f64,
    pub zoom: f64,
    /// `draw-exterior` / `draw-interior`, or None outside the draw modes.
    pub kind: Option<WallType>,
    /// `options.stepUnit` — `whole` (default), `half` or `tenths`: the arrow
    /// keys' step in feet.
    pub step_unit: StepUnit,
}

/// The wall-shape menu's shapes (UIManager.showWallShapeMenu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallShape {
    Rectangle,
    Circle,
    Polygon,
}

/// `options.stepUnit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepUnit {
    Tenths,
    Half,
    #[default]
    Whole,
}

impl StepUnit {
    /// The arrow keys' step (`Shift` is always 0.1).
    pub fn step(self) -> f64 {
        match self {
            StepUnit::Tenths => 0.1,
            StepUnit::Half => 0.5,
            StepUnit::Whole => 1.0,
        }
    }

    /// `options.stepUnit` as stored ("whole" by default). The Options dialog
    /// arrives with screen 12; until then the app carries the default.
    pub fn from_option(value: &str) -> Self {
        match value {
            "tenths" => StepUnit::Tenths,
            "half" => StepUnit::Half,
            _ => StepUnit::Whole,
        }
    }
}

/// `state.options.stepUnit`'s default.
pub const DEFAULT_STEP_UNIT: &str = "whole";

/// `#hud-len` / `#hud-ang` / `#hud-state`.
#[derive(Debug, Clone, PartialEq)]
pub struct Hud {
    pub len: String,
    pub ang: String,
    pub down: bool,
}

impl Default for Hud {
    fn default() -> Self {
        Hud {
            len: "0.0'".into(),
            ang: "0.0°".into(),
            down: false,
        }
    }
}

impl Hud {
    /// `updateHUD(distBuf, angleBuf, isDown)`: `None` leaves a field as is;
    /// an empty string shows "0.0".
    pub fn update(&mut self, dist: Option<&str>, angle: Option<&str>, down: bool) {
        if let Some(d) = dist {
            self.len = format!("{}'", if d.is_empty() { "0.0" } else { d });
        }
        if let Some(a) = angle {
            self.ang = format!("{}°", if a.is_empty() { "0.0" } else { a });
        }
        self.down = down;
    }
}

/// JS `Number.prototype.toFixed(d)`. Rust formats the exact binary value
/// with ties-to-even; JS picks the larger candidate on an exact tie. A tie
/// is exact only when the full decimal expansion after `d` digits is
/// "5000…", so test that, not `value * 10^d` (which itself rounds).
pub fn js_to_fixed(value: f64, digits: usize) -> String {
    let exact = format!("{:.*}", digits + 40, value.abs());
    let dot = exact.find('.').unwrap_or(exact.len());
    let tail = &exact[dot + 1 + digits..];
    if tail.starts_with('5') && tail[1..].bytes().all(|b| b == b'0') {
        let scale = 10f64.powi(digits as i32);
        let up = (value.abs() * scale).floor() + 1.0;
        let sign = if value < 0.0 { "-" } else { "" };
        return format!("{sign}{:.*}", digits, up / scale);
    }
    format!("{value:.digits$}")
}

/// JS `Math.round` (halves round toward +infinity).
pub fn js_round(value: f64) -> f64 {
    value.js_round()
}

/// JS `parseFloat`: the longest numeric PREFIX, so "10." is 10 and
/// "1.2.3" is 1.2 (Rust's `from_str` rejects both).
pub fn parse_float(s: &str) -> Option<f64> {
    let b = s.trim_start().as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let digits_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i == digits_start || (i == digits_start + 1 && b[digits_start] == b'.') {
        return None;
    }
    // An exponent is part of the number only if it is complete.
    let mut end = i;
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            end = j;
        }
    }
    s.trim_start()[..end].parse().ok()
}

/// `SketchApp.cleanCoord`: round to 1e-4, drop the binary residue through
/// `toFixed(4)`, and normalise -0.
pub fn clean_coord(value: f64) -> f64 {
    let rounded = (value / 1e-4).js_round() * 1e-4;
    let text = js_to_fixed(rounded, 4);
    match text.parse::<f64>() {
        Ok(v) if v != 0.0 => v,
        _ => 0.0,
    }
}

/// `InputController.parseBuffer`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BufferCmd {
    Distance(f64),
    /// `10<45`
    Angle {
        distance: f64,
        angle: f64,
    },
    /// `10x5` (rise × run)
    RiseRun {
        rise: f64,
        run: f64,
    },
    /// `10l45` / `10r` (the angle defaults to 90)
    Turn {
        distance: f64,
        left: bool,
        angle: f64,
    },
}

fn num(s: &str) -> Option<f64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    parse_float(s)
}

pub fn parse_buffer(buf: &str) -> Option<BufferCmd> {
    let buf = buf.trim();
    if buf.is_empty() {
        return None;
    }
    // Rise × Run.
    if let Some(i) = buf.find(['x', 'X', '×']) {
        let (a, b) = (
            &buf[..i],
            &buf[i + buf[i..].chars().next().map(char::len_utf8).unwrap_or(1)..],
        );
        if let (Some(rise), Some(run)) = (num(a), num(b)) {
            return Some(BufferCmd::RiseRun { rise, run });
        }
    }
    // Distance < Angle.
    if let Some(i) = buf.find('<') {
        if let (Some(distance), Some(angle)) = (num(&buf[..i]), num(&buf[i + 1..])) {
            return Some(BufferCmd::Angle { distance, angle });
        }
    }
    // Direct turn: "10l45" / "10r"; the angle defaults to 90.
    if let Some(i) = buf.find(['l', 'L', 'r', 'R']) {
        let (dist, rest) = (&buf[..i], &buf[i + 1..]);
        if rest.is_empty() || rest.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            if let Some(distance) = num(dist) {
                let angle = if rest.is_empty() {
                    90.0
                } else {
                    parse_float(rest).unwrap_or(90.0)
                };
                return Some(BufferCmd::Turn {
                    distance,
                    left: matches!(buf.as_bytes()[i], b'l' | b'L'),
                    angle,
                });
            }
        }
    }
    // Plain distance.
    parse_float(buf).map(BufferCmd::Distance)
}

/// `getDefaultLineStyle(type)` with the default options.
/// `getDefaultLineStyle(type)` returns the Options colours, which default to
/// these (`state-init.js:102`); a line's colour is `lineStyle.color ||
/// state.drawColor` (sketch-app.js:2717), so Draw Color only shows through
/// once an option colour is blanked.
pub fn default_line_style(kind: WallType) -> (&'static str, f64) {
    match kind {
        WallType::Exterior => ("#2B6CB0", 4.0),
        WallType::Interior => ("#63B3ED", 2.0),
    }
}

#[derive(Debug, Clone)]
pub struct Drawing {
    /// `state.lines`.
    pub lines: Vec<SketchLine>,
    /// `state.polygons` — closed areas (5b-2).
    pub polygons: Vec<Polygon>,
    pub reopened_areas: Vec<crate::area_repair::ReopenedArea>,
    /// `this.nextId`.
    next_id: i64,
    pub pen_down: bool,
    /// `state.cursor` — the committed drawing anchor.
    pub cursor: Pt,
    /// `state.currentPolyPoints`.
    pub points: Vec<Pt>,
    /// `state.currentLineIds`.
    pub line_ids: Vec<i64>,
    /// `state.pointerWorld` — the hovered point.
    pub pointer: Pt,
    pub suppress_dimension_next: bool,
    /// `state.drawColor` (sketch-app.js:21) — the fallback line colour.
    pub draw_color: String,
    /// `state.options` (screen 12 will edit them).
    pub options: crate::options::Options,
    pub auto_post: sketch_core::model::AutoPost,
    /// `state.selectedAreaType` / `selectedAreaName` — what Define First
    /// stamps on a closed loop (the dock's area panel sets them).
    pub selected_area_type: Option<String>,
    pub selected_area_name: Option<String>,
    /// Placed symbol instances (screen 8).
    pub symbols: Vec<crate::placed_symbol::PlacedSymbol>,
    /// Placed text labels (screen 9).
    pub texts: Vec<crate::placed_text::PlacedText>,
    /// `state.dimensionChains` — chained dimension records (screen 9); the
    /// spans recompute from the live walls on every render.
    pub dimension_chains: Vec<crate::chains::ChainRecord>,
    /// Measure tool run (screen 9).
    pub measure_points: Vec<Pt>,
    pub measure_runs: Vec<Vec<Pt>>,
    /// A message the UI should toast (e.g. Define First blocking the pen).
    pub pending_notice: Option<String>,
    /// `InputController.buffer` — the typed wall command.
    pub buffer: String,
    /// `state.keyboardPointer` — the arrow keys drive the cursor, so the
    /// visible pointer parks at it until the mouse moves >3px.
    pub keyboard_pointer: bool,
    pub hud: Hud,
    pub(crate) geo: GeometryEngine,
}

impl Default for Drawing {
    fn default() -> Self {
        Drawing {
            lines: Vec::new(),
            polygons: Vec::new(),
            reopened_areas: Vec::new(),
            next_id: 1,
            pen_down: false,
            cursor: Pt::new(0.0, 0.0),
            points: Vec::new(),
            line_ids: Vec::new(),
            pointer: Pt::new(0.0, 0.0),
            suppress_dimension_next: false,
            draw_color: crate::color_dialog::DEFAULT_COLOR.to_string(),
            options: crate::options::Options::default(),
            auto_post: Default::default(),
            selected_area_type: None,
            selected_area_name: None,
            symbols: Vec::new(),
            texts: Vec::new(),
            dimension_chains: Vec::new(),
            measure_points: Vec::new(),
            measure_runs: Vec::new(),
            pending_notice: None,
            buffer: String::new(),
            keyboard_pointer: false,
            hud: Hud::default(),
            geo: GeometryEngine::new(),
        }
    }
}

impl Drawing {
    /// `this.nextId` — the save model's id cursor.
    pub fn next_id(&self) -> i64 {
        self.next_id
    }

    /// The save model's id cursor. Callers pass `Math.max(saved, live+1)`
    /// already — the setter only clamps the floor.
    pub fn set_next_id(&mut self, next: i64) {
        self.next_id = next.max(1);
    }

    /// `snapshotModel(label)`.
    pub fn snapshot(&self, label: &str) -> crate::undo::Snapshot {
        crate::undo::Snapshot {
            label: label.to_string(),
            lines: self.lines.clone(),
            polygons: self.polygons.clone(),
            reopened_areas: self.reopened_areas.clone(),
            texts: self.texts.clone(),
            symbols: self.symbols.clone(),
            cursor: self.cursor,
            points: self.points.clone(),
            line_ids: self.line_ids.clone(),
            pen_down: self.pen_down,
            dimension_chains: self.dimension_chains.clone(),
            // The App's snapshot_model stamps the layer stack.
            layers: None,
            active_layer_id: None,
            north_angle: None,
        }
    }

    /// `restoreDrawingChain` + the model restore: the chain is only restored
    /// while it is still being drawn (a finished chain's pen-down state would
    /// leave an invisible start point behind).
    pub fn restore(&mut self, snap: &crate::undo::Snapshot) {
        self.lines = snap.lines.clone();
        self.polygons = snap.polygons.clone();
        self.reopened_areas = snap.reopened_areas.clone();
        self.texts = snap.texts.clone();
        self.symbols = snap.symbols.clone();
        // Chained dimension records undo/redo with the walls they wrap.
        self.dimension_chains = snap.dimension_chains.clone();
        self.cursor = snap.cursor;
        if self.pen_down && snap.pen_down {
            self.points = snap.points.clone();
            self.line_ids = snap.line_ids.clone();
            self.pen_down = true;
        } else {
            self.points.clear();
            self.line_ids.clear();
        }
    }

    /// `setSelectedLineLength(length)`: move the active endpoint along the
    /// wall's own direction so the wall becomes `length` long. The endpoint is
    /// the one the click picked (`activeLineEndpoint`), else the end.
    pub fn set_selected_line_length(
        &mut self,
        selection: &crate::selection::Selection,
        text: &str,
    ) -> bool {
        let Some(line) = self
            .lines
            .iter()
            .find(|line| selection.is_line_selected(line.id))
            .cloned()
        else {
            return false;
        };
        let Ok(next_length) = text.trim().parse::<f64>() else {
            return false;
        };
        if !next_length.is_finite() || next_length <= 0.0 {
            return false;
        }
        let current = distance(line.start, line.end);
        if current < 1e-8 {
            return false;
        }
        let at_start = selection
            .active_endpoint
            .filter(|(id, _)| *id == line.id)
            .map(|(_, start)| start)
            .unwrap_or(false);
        let (fixed, moving) = if at_start {
            (line.end, line.start)
        } else {
            (line.start, line.end)
        };
        let next = Pt::new(
            fixed.x + (moving.x - fixed.x) / current * next_length,
            fixed.y + (moving.y - fixed.y) / current * next_length,
        );
        if let Some(slot) = self.lines.iter_mut().find(|l| l.id == line.id) {
            if at_start {
                slot.start = next;
            } else {
                slot.end = next;
            }
        }
        // Area vertices sitting on the moved endpoint follow, as they do for
        // a dragged endpoint.
        for poly in self.polygons.iter_mut() {
            for vertex in poly.points.iter_mut() {
                if sketch_core::joins::points_equal(*vertex, moving, 1e-5) {
                    *vertex = next;
                }
            }
        }
        true
    }

    /// `setSelectedLineAppearance({ lineStyle })` / `{ color }`.
    pub fn set_selected_line_appearance(
        &mut self,
        selection: &crate::selection::Selection,
        style: Option<&str>,
        color: Option<&str>,
    ) -> bool {
        let mut changed = false;
        for line in self.lines.iter_mut() {
            if !selection.is_line_selected(line.id) {
                continue;
            }
            if let Some(style) = style {
                line.line_style = if style == "solid" {
                    None
                } else {
                    Some(style.to_string())
                };
                changed = true;
            }
            if let Some(color) = color {
                line.color = Some(color.to_string());
                changed = true;
            }
        }
        changed
    }

    /// The first selected wall, for the Edit Tools panel's values.
    pub fn first_selected_line(
        &self,
        selection: &crate::selection::Selection,
    ) -> Option<&SketchLine> {
        self.lines
            .iter()
            .find(|line| selection.is_line_selected(line.id))
    }

    /// `getScreenToleranceWorld(16)` in feet: 0.8 ft at 100% zoom.
    pub fn weld_tolerance(zoom: f64) -> f64 {
        16.0 / (PIXELS_PER_FOOT * zoom.max(1e-9))
    }

    /// `closeJoinedExteriorLoops`: heal the joins, then materialise an area
    /// for every bounded face the exterior walls enclose. Returns how many
    /// areas were created or repaired.
    pub fn close_loops(&mut self, zoom: f64) -> usize {
        let tolerance = Self::weld_tolerance(zoom);
        let defined = if self.options.define_first {
            self.selected_area_type.clone().map(|kind| {
                let name = self
                    .selected_area_name
                    .clone()
                    .unwrap_or_else(|| kind.clone());
                (kind, name)
            })
        } else {
            None
        };
        let opts = sketch_core::joins::CloseOptions {
            tolerance,
            defined,
            // `options.autoSubtractOverlapped === false` -> 'never'.
            auto_subtract: self.options.auto_subtract_overlapped,
        };
        // The core port follows JS: its counter is the last allocated ID.
        // Drawing stores the next unused ID; translate at this boundary so
        // the next wall cannot reuse an area ID allocated during closure.
        let already_created: HashSet<_> = self.polygons.iter().map(|p| p.id).collect();
        let mut next_id = self.next_id.saturating_sub(1);
        let created = sketch_core::joins::close_joined_exterior_loops(
            &mut self.lines,
            &mut self.polygons,
            &opts,
            &mut next_id,
        );
        self.next_id = next_id.saturating_add(1);
        let restored = crate::area_repair::restore(
            &mut self.lines,
            &mut self.polygons,
            &mut self.reopened_areas,
        );
        created
            + restored
                .iter()
                .filter(|id| already_created.contains(id))
                .count()
    }

    /// Original finishCommittedWall: detect exterior faces after any input
    /// path, but end the run only when this new wall closed one of them.
    pub fn finish_committed_wall(&mut self, line_id: i64, zoom: f64) -> usize {
        if !self
            .lines
            .iter()
            .any(|line| line.id == line_id && line.kind == WallType::Exterior)
        {
            return 0;
        }
        let created = self.close_loops(zoom);
        if created == 0 {
            return 0;
        }
        let by_id = self.lines.iter().map(|line| (line.id, line)).collect();
        let committed_wall_closed = self.polygons.iter().any(|poly| {
            poly.edges.iter().any(|edge| edge.line_id == Some(line_id))
                && sketch_core::joins::is_polygon_boundary_intact(poly, &by_id)
        });
        if !committed_wall_closed {
            return 0;
        }
        self.pen_up();
        self.keyboard_pointer = false;
        created
    }

    /// Shift+wheel while drawing: bow the last wall (CanvasEngine's wheel
    /// handler). In angle mode `delta` is degrees and the arc height is
    /// derived; in height mode `delta` is feet of rise and the angle is
    /// derived (`4 * atan2(2h, chord)`). Both clamp at +-359 degrees.
    pub fn bow_last_wall(&mut self, delta: f64, height_mode: bool) -> bool {
        let Some(line) = self.lines.last_mut() else {
            return false;
        };
        let chord = distance(line.start, line.end);
        if height_mode {
            line.arc_height += delta;
            let deg = 4.0 * (2.0 * line.arc_height).atan2(chord).to_degrees();
            line.bow_angle = deg.clamp(-MAX_BOW_ANGLE, MAX_BOW_ANGLE);
        } else {
            line.bow_angle = (line.bow_angle + delta).clamp(-MAX_BOW_ANGLE, MAX_BOW_ANGLE);
            line.arc_height = chord / 2.0 * (line.bow_angle * std::f64::consts::PI / 720.0).tan();
        }
        true
    }

    /// `commitCurveBow(lineId, bowAngle)`: clamps the bow, rebuilds the arc
    /// height and re-signs the bow of every area edge that rides the wall
    /// (positive when the area's path follows the wall's own direction).
    pub fn set_line_bow(&mut self, line_id: i64, bow: f64) -> bool {
        let clamped = bow.clamp(-MAX_BOW_ANGLE, MAX_BOW_ANGLE);
        let Some(line) = self.lines.iter_mut().find(|l| l.id == line_id) else {
            return false;
        };
        line.bow_angle = clamped;
        line.arc_height =
            distance(line.start, line.end) / 2.0 * (clamped * std::f64::consts::PI / 720.0).tan();
        let line_start = line.start;
        for poly in &mut self.polygons {
            let points = poly.points.clone();
            for (index, edge) in poly.edges.iter_mut().enumerate() {
                if edge.line_id != Some(line_id) {
                    continue;
                }
                let follows = points
                    .get(index)
                    .is_some_and(|p| sketch_core::model::same(*p, line_start));
                edge.bow_angle = if follows { clamped } else { -clamped };
            }
        }
        true
    }

    /// `adjustSelectedCurves(delta)`: bows every listed wall by `delta`
    /// degrees, or by `delta` feet of rise in height mode.
    pub fn adjust_curves(&mut self, ids: &[i64], delta: f64, height_mode: bool) {
        for id in ids {
            let Some(line) = self.lines.iter().find(|l| l.id == *id) else {
                continue;
            };
            let bow = if height_mode {
                let chord = distance(line.start, line.end);
                let height = arc_height_of(line) + delta;
                4.0 * (2.0 * height).atan2(chord).to_degrees()
            } else {
                line.bow_angle + delta
            };
            self.set_line_bow(*id, bow);
        }
    }

    /// `insertVertexAt(worldPt)`: splits the wall under the point in two and
    /// adds the vertex to every area whose boundary runs along it. The error
    /// is the original's notice; nothing changes in that case.
    pub fn insert_vertex(&mut self, world: Pt, zoom: f64) -> Result<(), &'static str> {
        let tolerance = Self::tolerance(10.0, zoom);
        let mut best: Option<(usize, Pt)> = None;
        let mut best_distance = tolerance;
        for (index, line) in self.lines.iter().enumerate() {
            let projection = self
                .geo
                .project_point_to_segment(world, line.start, line.end);
            let d = distance(world, projection);
            if d < best_distance {
                best = Some((index, projection));
                best_distance = d;
            }
        }
        let Some((index, projection)) = best else {
            return Err("Click directly on a wall to insert a vertex.");
        };
        let line = &self.lines[index];
        if distance(projection, line.start) < 0.05 || distance(projection, line.end) < 0.05 {
            return Err("That point is already a vertex.");
        }
        if line.bow_angle.abs() > 0.5 {
            return Err("Straighten the curved wall before inserting a vertex.");
        }

        let (line_start, original_end, line_id) = (line.start, line.end, line.id);
        let second_id = self.gen_id();
        let first = &mut self.lines[index];
        // Each new wall gets its own automatic dimension; a pinned label must
        // not be copied onto both halves.
        first.dimension_position = None;
        first.dimension_anchor = None;
        first.end = projection;
        let mut second = first.clone();
        second.id = second_id;
        second.forget_sketchy_id();
        second.start = projection;
        second.end = original_end;
        self.lines.insert(index + 1, second);

        use sketch_core::geometry::Edge;
        use sketch_core::model::same;
        for poly in &mut self.polygons {
            let n = poly.points.len();
            for i in 0..n {
                let a = poly.points[i];
                let b = poly.points[(i + 1) % n];
                let follows = same(a, line_start) && same(b, original_end);
                let reverses = same(a, original_end) && same(b, line_start);
                if !follows && !reverses {
                    continue;
                }
                poly.points.insert(i + 1, projection);
                if poly.edges.len() != n {
                    poly.edges = vec![Edge::default(); n];
                }
                let edge = |id: i64| Edge {
                    line_id: Some(id),
                    bow_angle: 0.0,
                };
                let split = if follows {
                    [edge(line_id), edge(second_id)]
                } else {
                    [edge(second_id), edge(line_id)]
                };
                poly.edges.splice(i..i + 1, split);
                break;
            }
        }
        Ok(())
    }

    /// `wallThicknessOf(line)`: per-line `thickness` (inches) wins over the
    /// option thickness for the wall's type; the result is in FEET.
    pub fn wall_thickness_of(&self, line: &SketchLine) -> f64 {
        if let Some(t) = line.thickness {
            if t > 0.0 {
                return t / 12.0;
            }
        }
        let inches = if line.kind == WallType::Interior {
            self.options.interior_thickness
        } else {
            self.options.exterior_thickness
        };
        inches / 12.0
    }

    /// Shared face-measure calculation for the live page and exported pages.
    pub fn recalculate_areas(
        &self,
        polygons: &[Polygon],
        lines: &[SketchLine],
        codes: &[sketch_core::geometry::AreaCode],
    ) -> sketch_core::area_calc::RecalcResult {
        let measured: Vec<_> = polygons
            .iter()
            .map(|p| {
                sketch_core::area_calc::face_measure_polygon(
                    p,
                    lines,
                    self.options.wall_solids_enabled(),
                    &|line| {
                        line.map_or(self.options.exterior_thickness / 12.0, |line| {
                            self.wall_thickness_of(line)
                        })
                    },
                )
            })
            .collect();
        sketch_core::area_calc::recalculate(&measured, codes, self.engine())
    }

    /// `lineStyle.color || state.drawColor`.
    fn line_style(&self, kind: WallType) -> (String, f64) {
        let (color, thickness) = default_line_style(kind);
        let color = if color.is_empty() {
            self.draw_color.clone()
        } else {
            color.to_string()
        };
        (color, thickness)
    }

    pub(crate) fn gen_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// A test hook: start generated ids above a fixture's hand-written ones.
    #[cfg(test)]
    pub(crate) fn next_id_for_tests(&mut self, next: i64) {
        self.next_id = next;
    }

    /// `getScreenToleranceWorld(px)`.
    pub fn tolerance(px: f64, zoom: f64) -> f64 {
        px / (PIXELS_PER_FOOT * zoom).max(0.0001)
    }

    /// `pointKey`.
    fn point_key(p: Pt) -> String {
        format!("{:.5},{:.5}", p.x, p.y)
    }

    /// `closedAreaVertexKeys()` — the vertices of intact closed areas never
    /// attract new walls.
    fn closed_area_vertex_keys(&self) -> HashSet<String> {
        self.polygons
            .iter()
            .flat_map(Polygon::vertex_keys)
            .collect()
    }

    /// The geometry engine, for the recalculation pass.
    pub fn engine(&self) -> &GeometryEngine {
        &self.geo
    }

    fn nearest_endpoint(&self, pt: Pt, tolerance: f64) -> Option<Pt> {
        let keys = self.closed_area_vertex_keys();
        let skip = |p: &Pt| keys.contains(&Self::point_key(*p));
        self.geo
            .find_nearest_endpoint(pt, &geometry_lines(&self.lines), tolerance, Some(&skip))
    }

    /// `structuralSnap` — wall endpoints within 10 screen px (wall faces
    /// join when wall solids are enabled; they are off by default).
    pub fn structural_snap(&self, pt: Pt, ctx: DrawCtx) -> Option<Pt> {
        self.nearest_endpoint(pt, Self::tolerance(10.0, ctx.zoom))
    }

    /// `snapPoint`.
    pub fn snap_point(&self, pt: Pt, ctx: DrawCtx) -> Pt {
        if !ctx.snap {
            return pt;
        }
        self.structural_snap(pt, ctx)
            .unwrap_or_else(|| self.geo.snap_to_grid(pt, ctx.grid))
    }

    /// `getDrawingTarget`: snap; while a chain is live, polar steps from the
    /// anchor (5 degrees, whole grid lengths) unless an endpoint is near; an
    /// endpoint within 16px always joins; the chain's start captures when it
    /// has 3+ points.
    pub fn drawing_target(&self, pt: Pt, ctx: DrawCtx) -> Pt {
        let mut target = self.snap_point(pt, ctx);
        if ctx.snap
            && self.pen_down
            && ctx.kind.is_some()
            && self.structural_snap(pt, ctx).is_none()
        {
            if let Some(last) = self.points.last() {
                if let Some(polar) = self.geo.polar_snap(*last, pt, ctx.grid, 5.0) {
                    target = polar;
                }
            }
        }
        let join = Self::tolerance(16.0, ctx.zoom);
        if let Some(endpoint) = self.nearest_endpoint(pt, join) {
            target = endpoint;
        }
        if self.pen_down && self.points.len() >= 3 {
            let start = self.points[0];
            if distance(pt, start) <= join || distance(target, start) <= join {
                target = start;
            }
        }
        target
    }

    /// `penDown(worldPt)`.
    pub fn pen_down_at(&mut self, pt: Pt, ctx: DrawCtx) {
        if self.pen_down {
            return;
        }
        let start = self.snap_point(pt, ctx);
        self.pen_down = true;
        self.cursor = start;
        self.points = vec![start];
        self.line_ids.clear();
        self.hud.update(Some(""), None, true);
    }

    /// `penUp()`.
    pub fn pen_up(&mut self) {
        if !self.pen_down {
            return;
        }
        self.pen_down = false;
        self.points.clear();
        self.line_ids.clear();
        self.hud.update(Some(""), Some(""), false);
    }

    /// `handleCanvasClick(worldPt)` in a draw mode. Returns true when the
    /// click closed the chain at its start (autoClose).
    pub fn click(&mut self, world: Pt, ctx: DrawCtx) -> bool {
        let Some(kind) = ctx.kind else { return false };
        self.pointer = world;
        let target = self.drawing_target(world, ctx);
        if !self.pen_down {
            // `defineFirst` blocks exterior drawing until an area type is
            // chosen (handleCanvasClick): the dock's area panel opens and
            // the pen stays up.
            if self.options.define_first
                && kind == WallType::Exterior
                && self.selected_area_type.is_none()
            {
                self.pending_notice = Some(
                    "Define First is enabled. Choose an area type before drawing.".to_string(),
                );
                return false;
            }
            self.pen_down_at(target, ctx);
            return false;
        }
        if self.points.len() >= 3 && distance(target, self.points[0]) < 0.0001 {
            self.auto_close(kind);
            return true;
        }
        let dist = distance(self.cursor, target);
        if dist < 0.1 {
            return false;
        }
        let id = self.gen_id();
        let (color, thickness) = self.line_style(kind);
        self.lines.push(SketchLine {
            id,
            start: self.cursor,
            end: target,
            kind,
            color: Some(color),
            line_thickness: Some(thickness),
            thickness: None,
            dimension_hidden: self.suppress_dimension_next,
            layer_id: None,
            bow_angle: 0.0,
            arc_height: 0.0,
            line_style: None,
            dimension_position: None,
            dimension_anchor: None,
            extra: Default::default(),
        });
        self.suppress_dimension_next = false;
        self.line_ids.push(id);
        self.points.push(target);
        self.cursor = target;
        self.hud.update(Some(&js_to_fixed(dist, 1)), Some(""), true);
        false
    }

    /// `autoClose()` — the closing wall back to the start, then pen up. An
    /// EXTERIOR loop becomes an undefined area (interior chains are
    /// illustration lines and may loop without creating an appraisal area).
    pub fn auto_close(&mut self, kind: WallType) {
        if self.points.len() < 3 {
            return;
        }
        let start = self.points[0];
        if distance(self.cursor, start) > 0.1 {
            let id = self.gen_id();
            let (color, thickness) = self.line_style(kind);
            self.lines.push(SketchLine {
                id,
                start: self.cursor,
                end: start,
                kind,
                color: Some(color),
                line_thickness: Some(thickness),
                thickness: None,
                dimension_hidden: self.suppress_dimension_next,
                layer_id: None,
                bow_angle: 0.0,
                arc_height: 0.0,
                line_style: None,
                dimension_position: None,
                dimension_anchor: None,
                extra: Default::default(),
            });
            self.suppress_dimension_next = false;
            self.line_ids.push(id);
        }
        // The area's edges follow the chain's walls in order, each with its
        // bow angle signed by the path's direction (autoClose's positional
        // mapping, not a geometric lookup).
        if kind == WallType::Exterior {
            let edges: Vec<sketch_core::geometry::Edge> = self
                .line_ids
                .iter()
                .enumerate()
                .map(|(index, line_id)| {
                    let point = self.points[index];
                    let line = self.lines.iter().find(|l| l.id == *line_id);
                    sketch_core::geometry::Edge {
                        line_id: Some(*line_id),
                        bow_angle: line
                            .map(|l| {
                                if sketch_core::model::same(l.start, point) {
                                    l.bow_angle
                                } else {
                                    -l.bow_angle
                                }
                            })
                            .unwrap_or(0.0),
                    }
                })
                .collect();
            let area = self.geo.calculate_area(&self.points, &edges);
            if area >= 0.01 {
                let id = self.gen_id();
                // `defineFirst ? selectedAreaType : 'UND'` and the name
                // falls back to the type code (handleCanvasClick).
                let (kind, name) = if self.options.define_first {
                    match &self.selected_area_type {
                        Some(t) => (
                            t.clone(),
                            self.selected_area_name.clone().unwrap_or_else(|| t.clone()),
                        ),
                        None => ("UND".to_string(), "Undefined Area".to_string()),
                    }
                } else {
                    ("UND".to_string(), "Undefined Area".to_string())
                };
                self.polygons.push(Polygon {
                    id,
                    points: self.points.clone(),
                    edges,
                    kind,
                    name,
                    gross: area,
                    area,
                    factor_override: None,
                    factor: 1.0,
                    measure_to: "center".into(),
                    auto_post: self.auto_post.clone(),
                    attributes: Default::default(),
                    // `options.autoSubtractOverlapped === false` -> 'never'.
                    subtract_mode: if self.options.auto_subtract_overlapped {
                        "auto"
                    } else {
                        "never"
                    }
                    .into(),
                    subtract_target_id: None,
                    layer_id: None,
                    fill_color: None,
                    fill_pattern: None,
                    fill_opacity: None,
                    line_color: None,
                    line_style: None,
                    line_thickness: None,
                    label_rotation: None,
                    area_size_rotation: None,
                    label_position: None,
                    area_size_position: None,
                    label_scale: None,
                    area_size_scale: None,
                    label_font_size: None,
                    label_font_family: None,
                    label_bold: None,
                    label_italic: None,
                    label_underline: None,
                    label_color: None,
                    extra: Default::default(),
                });
            }
        }
        self.cursor = start;
        self.pen_down = false;
        self.points.clear();
        self.line_ids.clear();
        self.hud.update(Some(""), Some(""), false);
    }

    /// `InputController.handleKeyDown`'s buffer characters: digits and '.'
    /// always; 'x' only after a digit; l/r only when the buffer is all
    /// digits/dots; '<' (or ',') only after a digit; Tab is a decimal point.
    /// Returns true when the key was consumed.
    pub fn buffer_key(&mut self, key: &str, ctx: DrawCtx) -> bool {
        let all_numeric =
            !self.buffer.is_empty() && self.buffer.bytes().all(|b| b.is_ascii_digit() || b == b'.');
        let ends_digit = self.buffer.ends_with(|c: char| c.is_ascii_digit());
        let append = |d: &mut Drawing, s: &str, angle: Option<&str>| {
            d.buffer.push_str(s);
            d.hud.update(Some(&d.buffer.clone()), angle, d.pen_down);
        };
        let _ = ctx;
        match key {
            k if k.len() == 1 && (k.as_bytes()[0].is_ascii_digit() || k == ".") => {
                append(self, k, None);
                true
            }
            "x" | "X" if ends_digit => {
                append(self, "x", None);
                true
            }
            "l" | "L" | "r" | "R" if all_numeric => {
                let lower = key.to_ascii_lowercase();
                append(self, &lower, Some("90"));
                true
            }
            "<" | "," if ends_digit => {
                append(self, "<", None);
                true
            }
            "Tab" => {
                append(self, ".", None);
                true
            }
            _ => false,
        }
    }

    /// Backspace while drawing with a buffer: drop the last character.
    pub fn buffer_backspace(&mut self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        self.buffer.pop();
        let text = self.buffer.clone();
        self.hud.update(Some(&text), None, self.pen_down);
        true
    }

    /// `Enter` while drawing: pen down when up; otherwise commit a wall from
    /// the last point to the cursor (applying the typed command when the
    /// cursor has not moved), or pen up on a second Enter with no buffer.
    pub fn enter_key(&mut self, ctx: DrawCtx) -> bool {
        if !self.pen_down {
            self.pen_down_at(self.cursor, ctx);
            return true;
        }
        let Some(&last) = self.points.last() else {
            return false;
        };
        let mut cursor = self.cursor;
        if cursor == last {
            if let Some(cmd) = parse_buffer(&self.buffer) {
                let heading = self
                    .line_ids
                    .last()
                    .and_then(|id| self.lines.iter().find(|l| l.id == *id))
                    .map(|l| (l.end.y - l.start.y).atan2(l.end.x - l.start.x))
                    .unwrap_or(std::f64::consts::FRAC_PI_2); // the first wall points up
                cursor = match cmd {
                    BufferCmd::Distance(distance) => Pt::new(
                        cursor.x + distance * heading.cos(),
                        cursor.y + distance * heading.sin(),
                    ),
                    BufferCmd::Angle { distance, angle } => {
                        let rad = angle.to_radians();
                        Pt::new(
                            cursor.x + distance * rad.cos(),
                            cursor.y + distance * rad.sin(),
                        )
                    }
                    BufferCmd::RiseRun { rise, run } => Pt::new(cursor.x + run, cursor.y + rise),
                    BufferCmd::Turn {
                        distance,
                        left,
                        angle,
                    } => {
                        let turn = if left { angle } else { -angle };
                        let rad = heading + turn.to_radians();
                        Pt::new(
                            cursor.x + distance * rad.cos(),
                            cursor.y + distance * rad.sin(),
                        )
                    }
                };
                cursor = Pt::new(clean_coord(cursor.x), clean_coord(cursor.y));
            }
        }
        let unchanged = cursor == last;
        if unchanged && self.buffer.is_empty() && !self.line_ids.is_empty() {
            self.pen_up();
        } else if !unchanged {
            let dist = distance(last, cursor);
            let angle = js_round((cursor.y - last.y).atan2(cursor.x - last.x).to_degrees());
            // Arrow keys move the keyboard cursor before Enter. The new wall
            // still starts at the committed chain anchor, not at its endpoint.
            self.cursor = last;
            self.commit_to(cursor, dist, angle, ctx.kind.unwrap_or(WallType::Exterior));
        }
        self.buffer.clear();
        self.hud.update(Some(""), None, self.pen_down);
        true
    }

    /// `bayWindowMirror`: reflect the last segment about the preceding
    /// segment's bearing, continuing from the committed drawing cursor.
    pub fn bay_window_mirror(&mut self) -> bool {
        if !self.pen_down || self.lines.len() < 2 {
            return false;
        }
        let previous = &self.lines[self.lines.len() - 2];
        let last = self.lines.last().expect("two walls");
        let base = (previous.end.y - previous.start.y).atan2(previous.end.x - previous.start.x);
        let dx = last.end.x - last.start.x;
        let dy = last.end.y - last.start.y;
        let length = dx.hypot(dy);
        let angle = 2.0 * base - dy.atan2(dx);
        let target = Pt::new(
            self.cursor.x + length * angle.cos(),
            self.cursor.y + length * angle.sin(),
        );
        let mut mirrored = last.clone();
        mirrored.id = self.gen_id();
        mirrored.forget_sketchy_id();
        mirrored.start = self.cursor;
        mirrored.end = target;
        mirrored.color = mirrored.color.or_else(|| Some(self.draw_color.clone()));
        mirrored.line_thickness = None;
        mirrored.thickness = None;
        mirrored.bow_angle = 0.0;
        mirrored.arc_height = 0.0;
        mirrored.line_style = None;
        mirrored.dimension_hidden = false;
        mirrored.dimension_position = None;
        mirrored.dimension_anchor = None;
        self.line_ids.push(mirrored.id);
        self.lines.push(mirrored);
        self.points.push(target);
        self.cursor = target;
        self.hud.update(
            Some(&js_to_fixed(length, 1)),
            Some(&format!("{}", js_round(angle.to_degrees()))),
            true,
        );
        true
    }

    /// `drawLine(newPos, dist, angle)`: commit a wall from the cursor to an
    /// exact (already cleaned) point.
    pub fn commit_to(&mut self, target: Pt, dist: f64, angle: f64, kind: WallType) -> bool {
        if !self.pen_down || dist <= 0.0 {
            self.cursor = target;
            return false;
        }
        let id = self.gen_id();
        let (color, thickness) = self.line_style(kind);
        self.lines.push(SketchLine {
            id,
            start: self.cursor,
            end: target,
            kind,
            color: Some(color),
            line_thickness: Some(thickness),
            thickness: None,
            dimension_hidden: self.suppress_dimension_next,
            layer_id: None,
            bow_angle: 0.0,
            arc_height: 0.0,
            line_style: None,
            dimension_position: None,
            dimension_anchor: None,
            extra: Default::default(),
        });
        self.suppress_dimension_next = false;
        self.line_ids.push(id);
        self.points.push(target);
        self.cursor = target;
        // drawLine ends with updateHUD(dist.toFixed(1), angle, penDown) —
        // unlike the mouse path, which clears the angle. Enter then re-sets
        // the length only, so the committed wall's angle stays on screen.
        self.hud.update(
            Some(&js_to_fixed(dist, 1)),
            Some(&format!("{}", angle as i64)),
            self.pen_down,
        );
        true
    }

    /// The arrow keys / diagonals: move the cursor without committing.
    /// `key` is one of `ArrowUp` / `ArrowDown` / `ArrowLeft` / `ArrowRight` /
    /// `PageUp` / `Home` / `End` / `PageDown`.
    pub fn arrow_move(&mut self, key: &str, ctrl: bool, shift: bool, ctx: DrawCtx) -> bool {
        let parsed = parse_buffer(&self.buffer);
        let (mut dx, mut dy) = (0.0, 0.0);
        if let Some(BufferCmd::RiseRun { rise, run }) = parsed {
            match key {
                "ArrowUp" | "ArrowRight" => (dx, dy) = (run, rise),
                "ArrowDown" => (dx, dy) = (run, -rise),
                "ArrowLeft" => (dx, dy) = (-run, rise),
                _ => return false,
            }
            let target = Pt::new(
                clean_coord(self.cursor.x + dx),
                clean_coord(self.cursor.y + dy),
            );
            self.cursor = target;
            self.buffer.clear();
            self.enter_keyboard_pointer();
            self.update_keyboard_hud();
            return true;
        }
        let dist = match parsed {
            Some(BufferCmd::Distance(d)) => d,
            _ => {
                if shift {
                    0.1
                } else {
                    ctx.step_unit.step()
                }
            }
        };
        let diagonal = match key {
            "PageUp" => Some((1.0, 1.0)),
            "Home" => Some((-1.0, 1.0)),
            "End" => Some((-1.0, -1.0)),
            "PageDown" => Some((1.0, -1.0)),
            _ => None,
        };
        match key {
            "ArrowUp" => dy = dist,
            "ArrowDown" => dy = -dist,
            "ArrowLeft" => dx = -dist,
            "ArrowRight" => dx = dist,
            _ => {
                if let Some((sx, sy)) = diagonal {
                    dx = sx * dist * std::f64::consts::FRAC_1_SQRT_2;
                    dy = sy * dist * std::f64::consts::FRAC_1_SQRT_2;
                } else {
                    return false;
                }
            }
        }
        let mut new_pos = Pt::new(self.cursor.x + dx, self.cursor.y + dy);
        if ctrl && diagonal.is_none() {
            let snap_dir = match key {
                "ArrowUp" => 0.0,
                "ArrowDown" => 180.0,
                "ArrowLeft" => 270.0,
                "ArrowRight" => 90.0,
                _ => -1.0,
            };
            let snapped = if shift {
                self.geo.find_intersecting_alignment(
                    self.cursor,
                    &geometry_lines(&self.lines),
                    snap_dir,
                )
            } else {
                self.geo
                    .find_alignment(self.cursor, &geometry_lines(&self.lines), snap_dir)
            };
            if let Some(pos) = snapped {
                new_pos = pos;
            }
        }
        self.cursor = Pt::new(clean_coord(new_pos.x), clean_coord(new_pos.y));
        self.buffer.clear();
        self.enter_keyboard_pointer();
        self.update_keyboard_hud();
        true
    }

    /// `enterKeyboardPointerMode`: only while a chain is live.
    pub fn enter_keyboard_pointer(&mut self) {
        if self.pen_down {
            self.keyboard_pointer = true;
        }
    }

    /// `updateKeyboardHUD`: distance and the MATH angle (not the mouse path's
    /// compass bearing) from the last committed point to the cursor.
    pub fn update_keyboard_hud(&mut self) {
        if let Some(&last) = self.points.last() {
            let (dx, dy) = (self.cursor.x - last.x, self.cursor.y - last.y);
            let dist = dx.hypot(dy);
            let angle = format!("{}", js_round(dy.atan2(dx).to_degrees()) as i64);
            self.hud
                .update(Some(&js_to_fixed(dist, 1)), Some(&angle), self.pen_down);
        }
    }

    /// `J`: jump the cursor to the nearest wall endpoint (no limit).
    pub fn jump_to_endpoint(&mut self) -> bool {
        match self.geo.find_nearest_endpoint(
            self.cursor,
            &geometry_lines(&self.lines),
            f64::INFINITY,
            None,
        ) {
            Some(pt) => {
                self.cursor = pt;
                true
            }
            None => false,
        }
    }

    /// `addWallShape(shape, wallType, settings)` — a closed shape centred on
    /// the canvas, added as walls (and, for exterior, as an undefined area).
    /// `centre` is the canvas centre in world coordinates.
    pub fn add_wall_shape(
        &mut self,
        shape: WallShape,
        kind: WallType,
        width: f64,
        height: Option<f64>,
        sides: Option<f64>,
        centre: Pt,
    ) {
        let width = width.max(0.5);
        let height = height.unwrap_or(width).max(0.5);
        let count = match shape {
            WallShape::Rectangle => 4,
            WallShape::Circle => sides.unwrap_or(36.0).max(24.0) as usize,
            WallShape::Polygon => sides.unwrap_or(8.0).clamp(3.0, 24.0) as usize,
        };
        let points: Vec<Pt> = if shape == WallShape::Rectangle {
            vec![
                Pt::new(centre.x - width / 2.0, centre.y - height / 2.0),
                Pt::new(centre.x + width / 2.0, centre.y - height / 2.0),
                Pt::new(centre.x + width / 2.0, centre.y + height / 2.0),
                Pt::new(centre.x - width / 2.0, centre.y + height / 2.0),
            ]
        } else {
            let (rx, ry) = (width / 2.0, height / 2.0);
            (0..count)
                .map(|i| {
                    // Starts at the top (-90 degrees) and runs clockwise in
                    // world coordinates.
                    let angle = -std::f64::consts::FRAC_PI_2
                        + i as f64 * std::f64::consts::TAU / count as f64;
                    Pt::new(centre.x + angle.cos() * rx, centre.y + angle.sin() * ry)
                })
                .collect()
        };
        let (color, thickness) = self.line_style(kind);
        let mut ids = Vec::new();
        for (i, point) in points.iter().enumerate() {
            let id = self.gen_id();
            self.lines.push(SketchLine {
                id,
                start: *point,
                end: points[(i + 1) % points.len()],
                kind,
                color: Some(color.clone()),
                line_thickness: Some(thickness),
                thickness: None,
                dimension_hidden: false,
                layer_id: None,
                bow_angle: 0.0,
                arc_height: 0.0,
                line_style: None,
                dimension_position: None,
                dimension_anchor: None,
                extra: Default::default(),
            });
            ids.push(id);
        }
        if kind == WallType::Exterior {
            let edges: Vec<sketch_core::geometry::Edge> = ids
                .iter()
                .map(|line_id| sketch_core::geometry::Edge {
                    line_id: Some(*line_id),
                    bow_angle: 0.0,
                })
                .collect();
            let area = self.geo.calculate_area(&points, &edges);
            let id = self.gen_id();
            self.polygons.push(Polygon {
                id,
                points,
                edges,
                kind: "UND".into(),
                name: "Undefined Area".into(),
                gross: area,
                area,
                factor_override: None,
                factor: 1.0,
                measure_to: "center".into(),
                auto_post: self.auto_post.clone(),
                attributes: Default::default(),
                // `options.autoSubtractOverlapped === false` -> 'never'.
                subtract_mode: if self.options.auto_subtract_overlapped {
                    "auto"
                } else {
                    "never"
                }
                .into(),
                subtract_target_id: None,
                layer_id: None,
                fill_color: None,
                fill_pattern: None,
                fill_opacity: None,
                line_color: None,
                line_style: None,
                line_thickness: None,
                label_rotation: None,
                area_size_rotation: None,
                label_position: None,
                area_size_position: None,
                label_scale: None,
                area_size_scale: None,
                label_font_size: None,
                label_font_family: None,
                label_bold: None,
                label_italic: None,
                label_underline: None,
                label_color: None,
                extra: Default::default(),
            });
        }
    }

    /// Pointer move over the canvas: `pointerWorld`, and while a chain is
    /// live the HUD shows the target's length and compass bearing.
    pub fn hover(&mut self, world: Pt, ctx: DrawCtx) {
        self.pointer = world;
        if ctx.kind.is_some() && self.pen_down && !self.points.is_empty() {
            let target = self.drawing_target(world, ctx);
            let (dx, dy) = (target.x - self.cursor.x, target.y - self.cursor.y);
            let dist = dx.hypot(dy);
            let math_angle = dy.atan2(dx).to_degrees();
            let apex = (90.0 - math_angle + 360.0) % 360.0;
            let apex = format!("{}", js_round(apex) as i64);
            self.hud
                .update(Some(&js_to_fixed(dist, 1)), Some(&apex), true);
        }
    }

    /// `toggleLastDimension()` (Space tapped while drawing); returns the
    /// toast.
    pub fn toggle_last_dimension(&mut self) -> &'static str {
        let active = self.line_ids.last().copied();
        if let Some(line) = active.and_then(|id| self.lines.iter_mut().find(|l| l.id == id)) {
            line.dimension_hidden = !line.dimension_hidden;
            if line.dimension_hidden {
                "Dimension suppressed for the last wall."
            } else {
                "Dimension restored."
            }
        } else {
            self.suppress_dimension_next = !self.suppress_dimension_next;
            if self.suppress_dimension_next {
                "The next wall dimension will be suppressed."
            } else {
                "Next wall dimension restored."
            }
        }
    }

    /// Is this wall part of the live chain (drawn in `wallActive`)?
    pub fn is_active(&self, id: i64) -> bool {
        self.pen_down && self.line_ids.contains(&id)
    }
}

/// A bare 10 ft wall (0,0)-(10,0) for tests elsewhere.
#[cfg(test)]
pub fn tests_wall() -> SketchLine {
    SketchLine {
        id: 1,
        start: Pt::new(0.0, 0.0),
        end: Pt::new(10.0, 0.0),
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
    }
}

/// `Geometry.pinnedDimensionPoint(line)`: where a moved measurement label
/// draws - its pin, carried along by however far the wall's midpoint has
/// travelled from the midpoint it was pinned against (`dimensionAnchor`).
pub fn pinned_dimension_point(line: &SketchLine) -> Option<Pt> {
    let pin = line
        .dimension_position
        .filter(|p| p.x.is_finite() && p.y.is_finite())?;
    let mid = Pt::new(
        (line.start.x + line.end.x) / 2.0,
        (line.start.y + line.end.y) / 2.0,
    );
    let anchor = line
        .dimension_anchor
        .filter(|a| a.x.is_finite() && a.y.is_finite())
        .unwrap_or(mid);
    Some(Pt::new(pin.x + mid.x - anchor.x, pin.y + mid.y - anchor.y))
}

impl Drawing {
    /// `isDimensionVisible(line)`: not hidden, the Dimensions filter on, the
    /// toggle on (or the wall being drawn), at least 0.1 ft long and over the
    /// minimum-dimension option.
    pub fn dimension_visible(&self, line: &SketchLine, enabled: bool, filter_on: bool) -> bool {
        if line.dimension_hidden || !filter_on {
            return false;
        }
        if !enabled && !self.is_active(line.id) {
            return false;
        }
        let length = distance(line.start, line.end);
        if length < 0.1 {
            return false;
        }
        !(self.options.minimum_dimension_enabled && length < self.options.minimum_dimension)
    }
}

/// A wall's rise at the chord's midpoint (feet, signed): `arcHeight` when it
/// is stored, else derived from the bow angle.
pub fn arc_height_of(line: &SketchLine) -> f64 {
    distance(line.start, line.end) / 2.0 * (line.bow_angle * std::f64::consts::PI / 720.0).tan()
}

/// `getCurveHandleWorld(line)`: the grab point on the wall's arc (the chord
/// midpoint of a straight wall).
pub fn curve_handle(line: &SketchLine) -> Pt {
    let mid = Pt::new(
        (line.start.x + line.end.x) / 2.0,
        (line.start.y + line.end.y) / 2.0,
    );
    let (dx, dy) = (line.end.x - line.start.x, line.end.y - line.start.y);
    let chord = dx.hypot(dy);
    if chord < 0.001 {
        return mid;
    }
    let height = arc_height_of(line);
    Pt::new(
        mid.x + (-dy / chord) * height,
        mid.y + (dx / chord) * height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAN: (f64, f64) = (523.5, 351.5);

    /// Canvas px -> world, at the startup pan and zoom 1.
    fn w(x: f64, y: f64) -> Pt {
        Pt::new((x - PAN.0) / 20.0, (PAN.1 - y) / 20.0)
    }

    fn ctx(kind: WallType) -> DrawCtx {
        DrawCtx {
            snap: true,
            grid: 1.0,
            zoom: 1.0,
            kind: Some(kind),
            step_unit: StepUnit::default(),
        }
    }

    #[test]
    fn pen_down_and_hover_match_the_original() {
        // Live original: Draw Exterior, click canvas (300,300) -> pen down at
        // (-11, 3); hover (500,303) -> HUD "10.0'" / "95°" / DOWN.
        let mut d = Drawing::default();
        let c = ctx(WallType::Exterior);
        assert!(!d.click(w(300.0, 300.0), c));
        assert!(d.pen_down);
        assert_eq!(d.points, vec![Pt::new(-11.0, 3.0)]);
        assert_eq!(
            d.hud,
            Hud {
                len: "0.0'".into(),
                ang: "0.0°".into(),
                down: true
            }
        );
        d.hover(w(500.0, 303.0), c);
        assert_eq!((d.hud.len.as_str(), d.hud.ang.as_str()), ("10.0'", "95°"));
    }

    #[test]
    fn two_walls_follow_the_polar_snap_and_style() {
        // Live original: clicks (300,300) (500,300) (500,500) -> walls
        // (-11,3)->(-1,3)->(-1,-7), exterior #2B6CB0 width 4; hover (350,450)
        // -> HUD "8.0'" / "285°".
        let mut d = Drawing::default();
        let c = ctx(WallType::Exterior);
        for (x, y) in [(300.0, 300.0), (500.0, 300.0), (500.0, 500.0)] {
            d.click(w(x, y), c);
        }
        let ends: Vec<(Pt, Pt)> = d.lines.iter().map(|l| (l.start, l.end)).collect();
        assert_eq!(
            ends,
            vec![
                (Pt::new(-11.0, 3.0), Pt::new(-1.0, 3.0)),
                (Pt::new(-1.0, 3.0), Pt::new(-1.0, -7.0))
            ]
        );
        assert!(d
            .lines
            .iter()
            .all(|l| l.color.as_deref() == Some("#2B6CB0") && l.line_thickness == Some(4.0)));
        assert_eq!(d.hud.len, "10.0'");
        d.hover(w(350.0, 450.0), c);
        assert_eq!((d.hud.len.as_str(), d.hud.ang.as_str()), ("8.0'", "285°"));
        assert!(d.is_active(1) && d.is_active(2));
        d.pen_up();
        assert!(!d.is_active(1));
        assert_eq!(d.hud, Hud::default());
    }

    #[test]
    fn duplicate_clicks_joins_closing_and_dimension_toggles() {
        let mut d = Drawing::default();
        let c = ctx(WallType::Interior);
        d.click(w(300.0, 300.0), c);
        d.click(w(300.0, 300.0), c);
        assert!(d.lines.is_empty(), "a click on the anchor adds nothing");
        d.click(w(500.0, 300.0), c);
        assert_eq!(d.lines[0].color.as_deref(), Some("#63B3ED"));
        assert_eq!(
            d.toggle_last_dimension(),
            "Dimension suppressed for the last wall."
        );
        assert!(d.lines[0].dimension_hidden);
        d.click(w(500.0, 500.0), c);
        // Back near the start (within 16px) closes the chain.
        assert!(d.click(w(305.0, 296.0), c));
        assert!(!d.pen_down);
        assert_eq!(d.lines.len(), 3);
        assert_eq!(d.lines[2].end, Pt::new(-11.0, 3.0));
        // A new chain started near an existing endpoint joins it.
        d.click(w(497.0, 302.0), c);
        assert_eq!(d.points[0], Pt::new(-1.0, 3.0));
        // With no live wall, Space arms the next one.
        let mut e = Drawing::default();
        assert_eq!(
            e.toggle_last_dimension(),
            "The next wall dimension will be suppressed."
        );
        e.click(w(300.0, 300.0), c);
        e.click(w(500.0, 300.0), c);
        assert!(e.lines[0].dimension_hidden && !e.suppress_dimension_next);
    }

    #[test]
    fn an_exterior_loop_becomes_an_undefined_area() {
        // A 10x10 ft square drawn as a chain, closed at the start.
        let mut d = Drawing::default();
        let c = ctx(WallType::Exterior);
        for (x, y) in [
            (300.0, 300.0),
            (500.0, 300.0),
            (500.0, 500.0),
            (300.0, 500.0),
        ] {
            d.click(w(x, y), c);
        }
        assert!(d.click(w(301.0, 299.0), c), "closes at the start");
        assert_eq!(d.polygons.len(), 1);
        let area = &d.polygons[0];
        assert_eq!(area.kind, "UND");
        assert_eq!(area.name, "Undefined Area");
        assert!((area.area - 100.0).abs() < 1e-9, "{}", area.area);
        assert!((area.gross - 100.0).abs() < 1e-9);
        assert_eq!(area.edges.len(), 4);
        assert!(area.edges.iter().all(|e| e.line_id.is_some()));
        // A closed area's vertices stop attracting new walls
        // (closedAreaVertexKeys): the same click that joins the corner while
        // the area is absent now only grid-snaps.
        let mut e = Drawing {
            lines: d.lines.clone(),
            polygons: d.polygons.clone(),
            ..Drawing::default()
        };
        e.click(w(497.0, 302.0), c);
        assert_eq!(e.points[0], Pt::new(-1.0, 2.0), "grid snap, no corner join");
        let mut plain = Drawing {
            lines: d.lines.clone(),
            ..Drawing::default()
        };
        plain.click(w(497.0, 302.0), c);
        assert_eq!(plain.points[0], Pt::new(-1.0, 3.0), "the corner joins");

        // An interior loop creates no area.
        let mut i = Drawing::default();
        let ci = ctx(WallType::Interior);
        for (x, y) in [(300.0, 300.0), (500.0, 300.0), (500.0, 500.0)] {
            i.click(w(x, y), ci);
        }
        i.click(w(301.0, 299.0), ci);
        assert!(i.polygons.is_empty(), "interior chains do not make areas");
    }

    #[test]
    fn typed_entry_commits_walls_like_the_original() {
        // The live original: Enter with the pen up drops the first point;
        // "20" Enter with the cursor unmoved keeps the current heading (the
        // first wall points up); "10<45" Enter draws at 45 degrees; "10x5"
        // Enter is +10 run, +5 rise; "10r" Enter turns 90 degrees right of
        // the last heading.
        let mut d = Drawing::default();
        let c = ctx(WallType::Exterior);
        d.click(w(300.0, 300.0), c);
        assert!(d.pen_down);
        for k in ["2", "0"] {
            assert!(d.buffer_key(k, c));
        }
        assert_eq!(d.buffer, "20");
        assert_eq!(d.hud.len, "20'", "the buffer echoes into the HUD");
        assert!(d.enter_key(c));
        assert_eq!(d.buffer, "");
        assert_eq!(d.lines.len(), 1);
        assert_eq!(d.lines[0].start, Pt::new(-11.0, 3.0));
        assert_eq!(d.lines[0].end, Pt::new(-11.0, 23.0), "20 ft straight up");
        // The live original after Enter: "0.0' | 90° | DOWN" — the length
        // resets and the committed wall's angle stays.
        assert_eq!(d.hud.len, "0.0'");
        assert_eq!(d.hud.ang, "90°");

        // 10<45 from the new cursor.
        for k in ["1", "0", "<", "4", "5"] {
            d.buffer_key(k, c);
        }
        d.enter_key(c);
        let end = d.lines[1].end;
        assert!(
            (end.x - (-11.0 + 10.0 * std::f64::consts::FRAC_1_SQRT_2)).abs() < 1e-4,
            "{end:?}"
        );
        assert!((end.y - (23.0 + 10.0 * std::f64::consts::FRAC_1_SQRT_2)).abs() < 1e-4);

        // "10x5" is RISE x RUN (the first number is the rise), so the
        // endpoint is +5 on x and +10 on y.
        let before = d.cursor;
        for k in ["1", "0", "x", "5"] {
            d.buffer_key(k, c);
        }
        assert_eq!(d.buffer, "10x5");
        d.enter_key(c);
        assert_eq!(d.lines[2].end, Pt::new(before.x + 5.0, before.y + 10.0));

        // 10r: 90 degrees right of the heading (which is now 10x5's diagonal).
        let before = d.cursor;
        let heading =
            (d.lines[2].end.y - d.lines[2].start.y).atan2(d.lines[2].end.x - d.lines[2].start.x);
        for k in ["1", "0", "r"] {
            d.buffer_key(k, c);
        }
        assert_eq!(d.buffer, "10r");
        d.enter_key(c);
        let rad = heading - std::f64::consts::FRAC_PI_2;
        assert!((d.lines[3].end.x - (before.x + 10.0 * rad.cos())).abs() < 1e-4);
        assert!((d.lines[3].end.y - (before.y + 10.0 * rad.sin())).abs() < 1e-4);

        // A second Enter with no buffer and an unmoved cursor lifts the pen.
        assert!(d.enter_key(c));
        assert!(!d.pen_down);
        assert_eq!(d.lines.len(), 4);
    }

    #[test]
    fn arrow_keys_move_the_cursor_without_committing() {
        // The live original: with no buffer an arrow steps 1 ft (step unit
        // whole), Shift steps 0.1; a typed distance replaces the step; the
        // diagonals move distance/sqrt(2) on both axes; the HUD shows the
        // MATH angle (unlike the mouse path's compass bearing).
        let mut d = Drawing::default();
        let c = ctx(WallType::Exterior);
        d.click(w(300.0, 300.0), c);
        assert_eq!(d.cursor, Pt::new(-11.0, 3.0));
        assert!(d.arrow_move("ArrowUp", false, false, c));
        assert_eq!(d.cursor, Pt::new(-11.0, 4.0));
        assert!(d.lines.is_empty(), "arrows never commit");
        assert!(d.keyboard_pointer, "the keyboard pointer takes over");
        assert_eq!((d.hud.len.as_str(), d.hud.ang.as_str()), ("1.0'", "90°"));
        assert!(d.arrow_move("ArrowRight", false, true, c));
        assert_eq!(d.cursor, Pt::new(-10.9, 4.0));
        // A typed distance drives the next step.
        d.buffer_key("2", c);
        d.buffer_key("0", c);
        assert!(d.arrow_move("ArrowLeft", false, false, c));
        assert_eq!(d.cursor, Pt::new(-30.9, 4.0));
        assert_eq!(d.buffer, "", "an arrow clears the buffer");
        // Diagonal: 10 ft along the diagonal is 7.07 on each axis.
        d.buffer_key("1", c);
        d.buffer_key("0", c);
        assert!(d.arrow_move("PageUp", false, false, c));
        let p = d.cursor;
        assert!((p.x - (-30.9 + 10.0 * std::f64::consts::FRAC_1_SQRT_2)).abs() < 1e-3);
        assert!((p.y - (4.0 + 10.0 * std::f64::consts::FRAC_1_SQRT_2)).abs() < 1e-3);
        // Shift+distance ignores the buffer? No: the buffer wins when typed.
        assert!(d.arrow_move("ArrowUp", false, true, c));
        assert!((d.cursor.y - (p.y + 0.1)).abs() < 1e-9);
    }

    #[test]
    fn buffer_rules_backspace_jump_and_clean_coord() {
        let mut d = Drawing::default();
        let c = ctx(WallType::Exterior);
        d.click(w(300.0, 300.0), c);
        // 'x' only after a digit, l/r only when the buffer is all numeric,
        // '<' only after a digit, Tab is a decimal point.
        assert!(!d.buffer_key("x", c));
        assert!(!d.buffer_key("l", c));
        assert!(!d.buffer_key("<", c));
        assert!(d.buffer_key("1", c));
        assert!(d.buffer_key("x", c));
        assert_eq!(d.buffer, "1x");
        assert!(d.buffer_backspace());
        assert_eq!(d.buffer, "1");
        assert!(d.buffer_key("l", c), "\"1\" is all numeric");
        assert_eq!(d.buffer, "1l");
        assert!(!d.buffer_key("r", c), "1l is not all numeric");
        d.buffer.clear();
        assert!(d.buffer_key("Tab", c));
        assert_eq!(d.buffer, ".");
        // Commit one wall, then J jumps to its nearest endpoint.
        d.arrow_move("ArrowUp", false, false, c);
        assert!(d.enter_key(c));
        assert_eq!(d.lines.len(), 1);
        assert!(d.arrow_move("ArrowUp", false, false, c));
        assert_eq!(d.cursor, Pt::new(-11.0, 5.0));
        assert!(d.jump_to_endpoint());
        assert_eq!(d.cursor, Pt::new(-11.0, 4.0), "the wall's end is nearer");
        assert_eq!(clean_coord(24.1422), 24.1422);
        assert_eq!(clean_coord(0.00001), 0.0);
        assert_eq!(clean_coord(-0.0), 0.0);
    }

    #[test]
    fn step_units_scale_the_arrow_step() {
        // options.stepUnit: whole (default) 1 ft, half 0.5, tenths 0.1;
        // Shift always 0.1. The Options dialog arrives with screen 12.
        assert_eq!(StepUnit::default(), StepUnit::Whole);
        assert_eq!(
            (
                StepUnit::Whole.step(),
                StepUnit::Half.step(),
                StepUnit::Tenths.step()
            ),
            (1.0, 0.5, 0.1)
        );
        for (unit, expected) in [
            (StepUnit::Whole, 1.0),
            (StepUnit::Half, 0.5),
            (StepUnit::Tenths, 0.1),
        ] {
            let mut d = Drawing::default();
            let c = DrawCtx {
                step_unit: unit,
                ..ctx(WallType::Exterior)
            };
            d.click(w(300.0, 300.0), c);
            d.arrow_move("ArrowUp", false, false, c);
            assert_eq!(d.cursor.y, 3.0 + expected);
            d.arrow_move("ArrowUp", false, true, c);
            assert_eq!(d.cursor.y, 3.0 + expected + 0.1, "Shift is always 0.1");
        }
    }

    #[test]
    fn wall_shapes_follow_add_wall_shape() {
        // addWallShape: a 20x20 rectangle centred on the canvas, walls in
        // order, and an UND area for exterior; a circle uses >= 24 sides and
        // starts at the top; a polygon clamps 3..24.
        let centre = Pt::new(0.0, 0.0);
        let mut d = Drawing::default();
        d.add_wall_shape(
            WallShape::Rectangle,
            WallType::Exterior,
            20.0,
            None,
            None,
            centre,
        );
        assert_eq!(d.lines.len(), 4);
        let corners: Vec<Pt> = d.lines.iter().map(|l| l.start).collect();
        assert_eq!(
            corners,
            vec![
                Pt::new(-10.0, -10.0),
                Pt::new(10.0, -10.0),
                Pt::new(10.0, 10.0),
                Pt::new(-10.0, 10.0)
            ]
        );
        assert!(d.lines.iter().all(
            |l| l.end == corners[(corners.iter().position(|c| *c == l.start).unwrap() + 1) % 4]
        ));
        assert_eq!(d.polygons.len(), 1);
        assert!((d.polygons[0].area - 400.0).abs() < 1e-9);
        assert_eq!(d.polygons[0].edges.len(), 4);
        assert!(!d.pen_down, "shapes do not touch the pen");

        // A circle: 36 sides by default; the original starts at -90 degrees,
        // which in world coordinates (y up) is the bottom of the screen.
        let mut c = Drawing::default();
        c.add_wall_shape(
            WallShape::Circle,
            WallType::Interior,
            20.0,
            None,
            None,
            centre,
        );
        assert_eq!(c.lines.len(), 36);
        let first = c.lines[0].start;
        assert!(
            first.x.abs() < 1e-9 && (first.y + 10.0).abs() < 1e-9,
            "{first:?}"
        );
        assert!(c.polygons.is_empty(), "interior shapes make no area");
        // The radius is right: every point is 10 ft from the centre.
        for l in &c.lines {
            assert!((l.start.x.hypot(l.start.y) - 10.0).abs() < 1e-9);
        }
        // Sides are clamped.
        let mut p = Drawing::default();
        p.add_wall_shape(
            WallShape::Polygon,
            WallType::Exterior,
            10.0,
            None,
            Some(2.0),
            centre,
        );
        assert_eq!(p.lines.len(), 3, "at least 3 sides");
        let mut p = Drawing::default();
        p.add_wall_shape(
            WallShape::Polygon,
            WallType::Exterior,
            10.0,
            None,
            Some(99.0),
            centre,
        );
        assert_eq!(p.lines.len(), 24, "at most 24 sides");
    }

    #[test]
    fn js_number_formatting() {
        assert_eq!(js_to_fixed(10.25, 1), "10.3");
        assert_eq!(js_to_fixed(9.95, 1), "9.9"); // 9.95 is below the tie in binary
        assert_eq!(js_to_fixed(7.95, 1), "8.0");
        assert_eq!(js_to_fixed(0.25, 1), "0.3"); // exact tie: JS takes the larger
        assert_eq!(js_to_fixed(-10.25, 1), "-10.3");
        assert_eq!(js_round(284.5), 285.0);
        assert_eq!(js_round(-2.5), -2.0);
    }
}

/// Shift+wheel bowing (S5-040). The expected numbers are the live original's
/// own response to dispatched `wheel` events with `shiftKey` (10 ft chord,
/// `app.state.curveMode` 'degree' / 'height').
#[cfg(test)]
mod bow_tests {
    use super::*;

    fn drawn() -> Drawing {
        let mut d = Drawing::default();
        d.lines.push(SketchLine {
            id: 1,
            start: Pt::new(-11.0, 3.0),
            end: Pt::new(-1.0, 3.0),
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
        });
        d
    }

    #[test]
    fn angle_mode_steps_match_the_live_original() {
        let mut d = drawn();
        // Shift+wheel up: +15 degrees, arc height derived from the chord.
        assert!(d.bow_last_wall(15.0, false));
        assert!((d.lines[0].bow_angle - 15.0).abs() < 1e-9);
        assert!(
            (d.lines[0].arc_height - 0.327717).abs() < 1e-6,
            "arc {}",
            d.lines[0].arc_height
        );
        d.bow_last_wall(15.0, false);
        assert!((d.lines[0].bow_angle - 30.0).abs() < 1e-9);
        assert!((d.lines[0].arc_height - 0.658262).abs() < 1e-6);
        // Shift+wheel down: -15.
        d.bow_last_wall(-15.0, false);
        assert!((d.lines[0].bow_angle - 15.0).abs() < 1e-9);
        // Ctrl+Shift+wheel: +1 degree.
        d.bow_last_wall(1.0, false);
        assert!((d.lines[0].bow_angle - 16.0).abs() < 1e-9);
        assert!((d.lines[0].arc_height - 0.349634).abs() < 1e-6);
    }

    #[test]
    fn height_mode_steps_match_the_live_original() {
        let mut d = drawn();
        // Shift+wheel up: +1 ft of rise, angle derived (4*atan2(2h, chord)).
        assert!(d.bow_last_wall(1.0, true));
        assert!((d.lines[0].arc_height - 1.0).abs() < 1e-9);
        assert!(
            (d.lines[0].bow_angle - 45.23973).abs() < 1e-5,
            "bow {}",
            d.lines[0].bow_angle
        );
        // Ctrl+Shift+wheel: +0.1 ft.
        d.bow_last_wall(0.1, true);
        assert!((d.lines[0].arc_height - 1.1).abs() < 1e-9);
        assert!((d.lines[0].bow_angle - 49.629674).abs() < 1e-5);
        // Shift+wheel down: -1 ft (back to 0.1).
        d.bow_last_wall(-1.0, true);
        assert!((d.lines[0].arc_height - 0.1).abs() < 1e-9);
        assert!((d.lines[0].bow_angle - 4.583051).abs() < 1e-5);
    }

    #[test]
    fn the_bow_clamps_at_359_degrees() {
        let mut d = drawn();
        for _ in 0..40 {
            d.bow_last_wall(15.0, false);
        }
        assert_eq!(d.lines[0].bow_angle, MAX_BOW_ANGLE);
        for _ in 0..80 {
            d.bow_last_wall(-15.0, false);
        }
        assert_eq!(d.lines[0].bow_angle, -MAX_BOW_ANGLE);
    }

    #[test]
    fn bowing_needs_a_wall() {
        let mut d = Drawing::default();
        assert!(!d.bow_last_wall(15.0, false));
    }
}

/// `closeJoinedExteriorLoops`' heal, through the app's own tolerance.
#[cfg(test)]
mod heal_tests {
    use super::*;

    fn wall(id: i64, sx: f64, sy: f64, ex: f64, ey: f64) -> SketchLine {
        SketchLine {
            id,
            start: Pt::new(sx, sy),
            end: Pt::new(ex, ey),
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
        }
    }

    #[test]
    fn the_weld_tolerance_is_16_screen_pixels() {
        // `getScreenToleranceWorld(16)` at 20 px/ft: 0.8 ft at 100%, 0.4 at 200%.
        let mut d = Drawing {
            lines: vec![
                wall(1, 0.0, 0.0, 10.0, 0.0),
                wall(2, 10.0, 0.0, 10.0, 10.0),
                wall(3, 10.0, 10.0, 0.0, 10.0),
                // 0.5 ft short: inside the 0.8 ft tolerance at zoom 1.
                wall(4, 0.0, 10.0, 0.0, 0.5),
            ],
            ..Drawing::default()
        };
        assert_eq!(Drawing::weld_tolerance(1.0), 0.8);
        assert!(sketch_core::joins::heal_exterior_wall_joins(
            &mut d.lines,
            &mut d.polygons,
            Drawing::weld_tolerance(1.0)
        ));
        assert_eq!(d.lines[3].end, Pt::new(0.0, 0.0));

        let mut d = Drawing {
            lines: vec![
                wall(1, 0.0, 0.0, 10.0, 0.0),
                wall(2, 10.0, 0.0, 10.0, 10.0),
                wall(3, 10.0, 10.0, 0.0, 10.0),
                wall(4, 0.0, 10.0, 0.0, 0.5),
            ],
            ..Drawing::default()
        };
        assert_eq!(Drawing::weld_tolerance(2.0), 0.4);
        assert!(!sketch_core::joins::heal_exterior_wall_joins(
            &mut d.lines,
            &mut d.polygons,
            Drawing::weld_tolerance(2.0)
        ));
        assert_eq!(d.lines[3].end, Pt::new(0.0, 0.5));
    }
}

/// Edit Tools (screen 6b-2). The expected endpoints are the live original's
/// own response to `setSelectedLineLength` / `setSelectedLineAppearance` on a
/// 10 ft wall from (-11,3) to (-1,3).
#[cfg(test)]
mod edit_tools_tests {
    use super::*;
    use crate::selection::Selection;

    fn wall() -> Drawing {
        Drawing {
            lines: vec![SketchLine {
                id: 1,
                start: Pt::new(-11.0, 3.0),
                end: Pt::new(-1.0, 3.0),
                kind: WallType::Exterior,
                color: Some("#2B6CB0".into()),
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
            }],
            ..Drawing::default()
        }
    }

    fn selected(at_start: bool) -> Selection {
        Selection {
            lines: [1].into_iter().collect(),
            polys: Default::default(),
            symbols: Default::default(),
            texts: Default::default(),
            area_labels: Default::default(),
            area_sizes: Default::default(),
            dimensions: Default::default(),
            chains: Default::default(),
            active_endpoint: Some((1, at_start)),
        }
    }

    #[test]
    fn the_start_handle_moves_when_it_is_the_active_one() {
        // Original: start -> (-21, 3), end stays at (-1, 3).
        let mut d = wall();
        assert!(d.set_selected_line_length(&selected(true), "20"));
        assert_eq!(d.lines[0].start, Pt::new(-21.0, 3.0));
        assert_eq!(d.lines[0].end, Pt::new(-1.0, 3.0));
    }

    #[test]
    fn the_end_handle_moves_when_it_is_the_active_one() {
        let mut d = wall();
        assert!(d.set_selected_line_length(&selected(false), "20"));
        assert_eq!(d.lines[0].start, Pt::new(-11.0, 3.0));
        assert_eq!(d.lines[0].end, Pt::new(9.0, 3.0));
    }

    #[test]
    fn a_bad_length_is_rejected() {
        let mut d = wall();
        assert!(!d.set_selected_line_length(&selected(true), "0"));
        assert!(!d.set_selected_line_length(&selected(true), "-5"));
        assert!(!d.set_selected_line_length(&selected(true), "abc"));
        assert_eq!(d.lines[0].start, Pt::new(-11.0, 3.0));
    }

    #[test]
    fn appearance_applies_to_every_selected_wall() {
        let mut d = wall();
        assert!(d.set_selected_line_appearance(&selected(false), Some("dashed"), None));
        assert_eq!(d.lines[0].line_style.as_deref(), Some("dashed"));
        assert!(d.set_selected_line_appearance(&selected(false), None, Some("#ff0000")));
        assert_eq!(d.lines[0].color.as_deref(), Some("#ff0000"));
        // "solid" is the absence of a style, as the original stores it.
        assert!(d.set_selected_line_appearance(&selected(false), Some("solid"), None));
        assert_eq!(d.lines[0].line_style, None);
    }
}

/// Insert Vertex and the curve handle (`insertVertexAt`, `commitCurveBow`,
/// `adjustSelectedCurves`).
#[cfg(test)]
mod curve_tests {
    use super::*;

    fn wall(id: i64, a: (f64, f64), b: (f64, f64)) -> SketchLine {
        SketchLine {
            id,
            start: Pt::new(a.0, a.1),
            end: Pt::new(b.0, b.1),
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
        }
    }

    /// A 10 ft square: walls 1..4 counter-clockwise, area 100 over them.
    fn square() -> Drawing {
        let mut d = Drawing::default();
        d.next_id_for_tests(100);
        d.lines = vec![
            wall(1, (0.0, 0.0), (10.0, 0.0)),
            wall(2, (10.0, 0.0), (10.0, 10.0)),
            wall(3, (10.0, 10.0), (0.0, 10.0)),
            wall(4, (0.0, 10.0), (0.0, 0.0)),
        ];
        let points: Vec<Pt> = d.lines.iter().map(|l| l.start).collect();
        let edges = Polygon::build_edges(&points, &d.lines);
        let mut poly: Polygon = serde_json::from_value(serde_json::json!({
            "id": 9,
            "points": points.iter().map(|p| serde_json::json!({"x": p.x, "y": p.y})).collect::<Vec<_>>(),
        }))
        .unwrap();
        poly.edges = edges;
        d.polygons.push(poly);
        d
    }

    #[test]
    fn insert_vertex_splits_the_wall_and_the_area_boundary() {
        let mut d = square();
        d.lines[0].dimension_position = Some(Pt::new(5.0, -2.0));
        d.insert_vertex(Pt::new(4.0, 0.1), 1.0).unwrap();
        assert_eq!(d.lines.len(), 5);
        let (a, b) = (&d.lines[0], &d.lines[1]);
        assert_eq!((a.start, a.end), (Pt::new(0.0, 0.0), Pt::new(4.0, 0.0)));
        assert_eq!((b.start, b.end), (Pt::new(4.0, 0.0), Pt::new(10.0, 0.0)));
        assert_ne!(a.id, b.id);
        assert!(a.dimension_position.is_none() && b.dimension_position.is_none());
        let poly = &d.polygons[0];
        assert_eq!(poly.points.len(), 5);
        assert_eq!(poly.points[1], Pt::new(4.0, 0.0));
        let ids: Vec<Option<i64>> = poly.edges.iter().map(|e| e.line_id).collect();
        assert_eq!(ids, vec![Some(1), Some(b.id), Some(2), Some(3), Some(4)]);
    }

    #[test]
    fn insert_vertex_on_a_reversed_edge_orders_the_halves_the_other_way() {
        let mut d = square();
        // The area walks wall 3 backwards if its points run the other way.
        d.polygons[0].points.reverse();
        d.polygons[0].edges = Polygon::build_edges(&d.polygons[0].points, &d.lines);
        d.insert_vertex(Pt::new(4.0, 0.0), 1.0).unwrap();
        let poly = &d.polygons[0];
        assert_eq!(poly.points.len(), 5);
        let second = d.lines[1].id;
        let at = poly
            .edges
            .iter()
            .position(|e| e.line_id == Some(second))
            .unwrap();
        // Reversed: the second half comes first along the path.
        assert_eq!(poly.edges[at + 1].line_id, Some(1));
    }

    #[test]
    fn insert_vertex_refuses_bad_clicks_without_changing_anything() {
        let mut d = square();
        let before = d.lines.len();
        assert_eq!(
            d.insert_vertex(Pt::new(5.0, 5.0), 1.0),
            Err("Click directly on a wall to insert a vertex.")
        );
        assert_eq!(
            d.insert_vertex(Pt::new(10.0, 0.01), 1.0),
            Err("That point is already a vertex.")
        );
        d.set_line_bow(3, 40.0);
        assert_eq!(
            d.insert_vertex(Pt::new(5.0, 10.0), 1.0),
            Err("Straighten the curved wall before inserting a vertex.")
        );
        assert_eq!(d.lines.len(), before);
        assert_eq!(d.polygons[0].points.len(), 4);
    }

    #[test]
    fn bowing_a_wall_re_signs_the_edges_that_ride_it() {
        let mut d = square();
        // Wall 3 runs against the area's walking direction? No: the area
        // follows each wall's own direction, so every edge takes +bow.
        assert!(d.set_line_bow(1, 30.0));
        assert_eq!(d.polygons[0].edges[0].bow_angle, 30.0);
        assert!((arc_height_of(&d.lines[0]) - d.lines[0].arc_height).abs() < 1e-12);
        // Reverse the area's path: the same wall now counts as -bow.
        d.polygons[0].points.reverse();
        d.polygons[0].edges = Polygon::build_edges(&d.polygons[0].points, &d.lines);
        d.set_line_bow(1, 20.0);
        let e = d.polygons[0]
            .edges
            .iter()
            .find(|e| e.line_id == Some(1))
            .unwrap();
        assert_eq!(e.bow_angle, -20.0);
        // The clamp holds, and an unknown wall is refused.
        d.set_line_bow(1, 1000.0);
        assert_eq!(d.lines[0].bow_angle, MAX_BOW_ANGLE);
        assert!(!d.set_line_bow(77, 5.0));
    }

    #[test]
    fn the_curve_handle_rides_the_arc_and_height_mode_steps_the_rise() {
        let mut d = square();
        let straight = curve_handle(&d.lines[0]);
        assert_eq!(straight, Pt::new(5.0, 0.0));
        d.adjust_curves(&[1], 15.0, false);
        assert_eq!(d.lines[0].bow_angle, 15.0);
        let h = curve_handle(&d.lines[0]);
        assert!(h.y > 0.0, "positive bow rises along the wall's left normal");
        // Height mode: +1 ft of rise from the current arc.
        let rise = arc_height_of(&d.lines[0]);
        d.adjust_curves(&[1], 1.0, true);
        assert!((arc_height_of(&d.lines[0]) - (rise + 1.0)).abs() < 1e-9);
        d.adjust_curves(&[1], -1.0, true);
        assert!((d.lines[0].bow_angle - 15.0).abs() < 1e-9);
    }
}
