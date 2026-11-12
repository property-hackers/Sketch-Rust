//! Rotate and flip (screen 6g): `rotateSelected`, `flipSelectedAreas`,
//! `rotateSketch` and `flipSketch` (refs/sketch-app.js:914-975, 1071-1105,
//! 3388-3528).
//!
//! Two ideas carry the behaviour:
//!
//! * Selection transforms work on a **point key set**, not on ids: every
//!   endpoint of a selected wall and every point of a selected area is keyed
//!   at five decimal places, and any point in the sketch with that key moves
//!   with it. Shared corners therefore stay welded.
//! * A reflection reverses handedness, so every signed bow angle is negated —
//!   otherwise a curved wall's arc jumps to the wrong side of its chord.
//!
//! Text, symbols and area label positions do not exist in this port yet, so
//! their rotations are absent rather than approximated.

use std::collections::HashSet;

use sketch_core::geometry::Pt;
use sketch_core::model::Polygon;

use crate::drawing::Drawing;
use crate::pages;
use crate::selection::Selection;

/// The two mirror axes, named as the context menu names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// `'horizontal'`: mirror x about the selection's centre.
    Horizontal,
    /// `'vertical'`: mirror y.
    Vertical,
}

/// `pointKey`: five decimal places, the same key the join pass uses.
fn point_key(p: Pt) -> String {
    format!("{:.5},{:.5}", p.x, p.y)
}

/// `getSelectedGeometryRotationModel`: the keys that move, and the centre of
/// their bounding box.
pub struct Model {
    pub keys: HashSet<String>,
    pub center: Pt,
    /// The bounding box's top edge (largest y).
    pub max_y: f64,
}

pub fn selected_model(d: &Drawing, sel: &Selection) -> Option<Model> {
    let mut keys = HashSet::new();
    for line in d.lines.iter().filter(|l| sel.lines.contains(&l.id)) {
        keys.insert(point_key(line.start));
        keys.insert(point_key(line.end));
    }
    for poly in d.polygons.iter().filter(|p| sel.polys.contains(&p.id)) {
        for p in &poly.points {
            keys.insert(point_key(*p));
        }
    }
    if keys.is_empty() {
        return None;
    }
    let mut points: Vec<Pt> = Vec::new();
    let mut seen = HashSet::new();
    for line in &d.lines {
        for p in [line.start, line.end] {
            let k = point_key(p);
            if keys.contains(&k) && seen.insert(k) {
                points.push(p);
            }
        }
    }
    for poly in &d.polygons {
        for p in &poly.points {
            let k = point_key(*p);
            if keys.contains(&k) && seen.insert(k) {
                points.push(*p);
            }
        }
    }
    if points.is_empty() {
        return None;
    }
    let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    Some(Model {
        keys,
        center: Pt::new((min_x + max_x) / 2.0, (min_y + max_y) / 2.0),
        max_y,
    })
}

/// `rotateSelected(degrees)`: rotate every point whose key is selected. The
/// centre is the selection's bounding box, and clockwise on screen is
/// positive (y grows downwards).
pub fn rotate_selected(d: &mut Drawing, sel: &Selection, degrees: f64) -> bool {
    let Some(model) = selected_model(d, sel) else {
        return false;
    };
    let radians = degrees * std::f64::consts::PI / 180.0;
    let (cos, sin) = (radians.cos(), radians.sin());
    let c = model.center;
    let turn = |p: Pt| {
        let (dx, dy) = (p.x - c.x, p.y - c.y);
        Pt::new(c.x + dx * cos - dy * sin, c.y + dx * sin + dy * cos)
    };
    // Selected free labels turn about the centre and against the rotation
    // (their `rotation` reads clockwise).
    for t in &mut d.texts {
        if sel.texts.contains(&t.id) {
            let p = turn(Pt::new(t.x, t.y));
            t.x = p.x;
            t.y = p.y;
            t.rotation -= degrees;
        }
    }
    // Area text: an area turning with its selection carries its label and
    // size blocks (when they have a position of their own); a label / size
    // picked on its own turns alone, from wherever it sits. Read before the
    // points move, as the anchors derive from them.
    let positions: Vec<(i64, Pt, Pt)> = d
        .polygons
        .iter()
        .map(|p| {
            (
                p.id,
                crate::handles::label_position(d, p),
                crate::handles::size_position(d, p),
            )
        })
        .collect();
    for poly in &mut d.polygons {
        let with_area = sel.polys.contains(&poly.id);
        let label_only = sel.area_labels.contains(&poly.id) && !with_area;
        let size_only = sel.area_sizes.contains(&poly.id) && !with_area;
        let label_rotation = poly.label_rotation.unwrap_or(0.0);
        let size_rotation = poly.area_size_rotation.unwrap_or(label_rotation);
        let (_, label_at, size_at) = *positions
            .iter()
            .find(|(id, _, _)| *id == poly.id)
            .expect("positions cover every area");
        if with_area {
            poly.label_position = poly.label_position.map(&turn);
            poly.area_size_position = poly.area_size_position.map(&turn);
            poly.label_rotation = Some(label_rotation - degrees);
            poly.area_size_rotation = Some(size_rotation - degrees);
        } else {
            if label_only {
                poly.label_position = Some(turn(label_at));
                poly.label_rotation = Some(label_rotation - degrees);
            }
            if size_only {
                poly.area_size_position = Some(turn(size_at));
                poly.area_size_rotation = Some(size_rotation - degrees);
            }
        }
    }
    let rotate = |p: &mut Pt, keys: &HashSet<String>| {
        if !keys.contains(&point_key(*p)) {
            return;
        }
        let dx = p.x - c.x;
        let dy = p.y - c.y;
        *p = Pt::new(c.x + dx * cos - dy * sin, c.y + dx * sin + dy * cos);
    };
    for line in &mut d.lines {
        rotate(&mut line.start, &model.keys);
        rotate(&mut line.end, &model.keys);
    }
    for poly in &mut d.polygons {
        for p in &mut poly.points {
            rotate(p, &model.keys);
        }
    }
    true
}

/// `flipSelectedAreas(axis)`: mirror the selected areas (and the walls that
/// bound them) about the selection's centre, reversing every bow.
pub fn flip_selected(d: &mut Drawing, sel: &Selection, axis: Axis) -> bool {
    let selected: Vec<usize> = d
        .polygons
        .iter()
        .enumerate()
        .filter(|(_, p)| sel.polys.contains(&p.id))
        .map(|(i, _)| i)
        .collect();
    if selected.is_empty() {
        return false;
    }
    let Some(model) = selected_model(d, sel) else {
        return false;
    };
    let c = model.center;
    // The walls that bound the selected areas.
    let line_ids: HashSet<i64> = selected
        .iter()
        .flat_map(|i| d.polygons[*i].edges.iter().filter_map(|e| e.line_id))
        .collect();

    let flip = |p: &mut Pt, keys: &HashSet<String>| {
        if !keys.contains(&point_key(*p)) {
            return;
        }
        match axis {
            Axis::Horizontal => p.x = 2.0 * c.x - p.x,
            Axis::Vertical => p.y = 2.0 * c.y - p.y,
        }
    };

    for line in d.lines.iter_mut().filter(|l| line_ids.contains(&l.id)) {
        flip(&mut line.start, &model.keys);
        flip(&mut line.end, &model.keys);
        line.bow_angle = -line.bow_angle;
        line.arc_height = -line.arc_height;
    }
    for i in &selected {
        let poly: &mut Polygon = &mut d.polygons[*i];
        for p in &mut poly.points {
            flip(p, &model.keys);
        }
        for edge in &mut poly.edges {
            edge.bow_angle = -edge.bow_angle;
        }
    }
    true
}

/// `moveSelected(dx, dy)`: translate the selection by a delta.
///
/// Two regimes, exactly as the original's:
///
/// * **Lines selected, no areas** — a connected wall stretches: only the
///   endpoint it shares with the selection follows, its far end stays put.
/// * **Areas selected** — only the selection's own boundary walls move (the
///   selected walls plus the walls of the selected areas' edges), and any
///   other area that genuinely *shares one of those walls* is glued to it.
///   A merely coincident vertex is left alone, so a subtraction area drawn
///   against another area's corner can move without grabbing it.
///
/// Not ported: `ownBoundaryLine`'s detachment of a wall a moving area shares
/// with a staying area — that needs the edge-borrowing test.
pub fn move_selected(d: &mut Drawing, sel: &Selection, dx: f64, dy: f64) -> bool {
    if sel.lines.is_empty()
        && sel.polys.is_empty()
        && sel.symbols.is_empty()
        && sel.texts.is_empty()
        && sel.area_labels.is_empty()
        && sel.area_sizes.is_empty()
    {
        return false;
    }
    let selection_has_areas = !sel.polys.is_empty();
    // Area label / size blocks picked on their own move from wherever they
    // sit; read before anything moves (the default anchors follow the points).
    let block_positions: Vec<(i64, Pt, Pt)> = d
        .polygons
        .iter()
        .map(|p| {
            (
                p.id,
                crate::handles::label_position(d, p),
                crate::handles::size_position(d, p),
            )
        })
        .collect();

    let mut moving_line_ids: HashSet<i64> = sel.lines.clone();
    for poly in d.polygons.iter().filter(|p| sel.polys.contains(&p.id)) {
        for edge in &poly.edges {
            if let Some(id) = edge.line_id {
                moving_line_ids.insert(id);
            }
        }
    }
    let mut moving_points: HashSet<String> = HashSet::new();
    for line in d.lines.iter().filter(|l| sel.lines.contains(&l.id)) {
        moving_points.insert(point_key(line.start));
        moving_points.insert(point_key(line.end));
    }
    for poly in d.polygons.iter().filter(|p| sel.polys.contains(&p.id)) {
        for p in &poly.points {
            moving_points.insert(point_key(*p));
        }
    }

    // Non-selected areas glued to a wall they share with the selection.
    let mut glue: Vec<(usize, HashSet<String>)> = Vec::new();
    if selection_has_areas {
        for (i, poly) in d.polygons.iter().enumerate() {
            if sel.polys.contains(&poly.id) {
                continue;
            }
            let mut keys = HashSet::new();
            for edge in &poly.edges {
                let Some(id) = edge.line_id else { continue };
                if !moving_line_ids.contains(&id) {
                    continue;
                }
                if let Some(line) = d.lines.iter().find(|l| l.id == id) {
                    keys.insert(point_key(line.start));
                    keys.insert(point_key(line.end));
                }
            }
            if !keys.is_empty() {
                glue.push((i, keys));
            }
        }
    }

    for line in &mut d.lines {
        if moving_line_ids.contains(&line.id) {
            line.start = Pt::new(line.start.x + dx, line.start.y + dy);
            line.end = Pt::new(line.end.x + dx, line.end.y + dy);
            continue;
        }
        if selection_has_areas {
            continue;
        }
        if moving_points.contains(&point_key(line.start)) {
            line.start = Pt::new(line.start.x + dx, line.start.y + dy);
        }
        if moving_points.contains(&point_key(line.end)) {
            line.end = Pt::new(line.end.x + dx, line.end.y + dy);
        }
    }

    for (i, poly) in d.polygons.iter_mut().enumerate() {
        let move_with_area = sel.polys.contains(&poly.id);
        let glue_keys = glue.iter().find(|(idx, _)| *idx == i).map(|(_, keys)| keys);
        for p in &mut poly.points {
            let key = point_key(*p);
            if move_with_area
                || glue_keys.is_some_and(|keys| keys.contains(&key))
                || (!selection_has_areas && moving_points.contains(&key))
            {
                *p = Pt::new(p.x + dx, p.y + dy);
            }
        }
        let shift = |p: Pt| Pt::new(p.x + dx, p.y + dy);
        let label_only = sel.area_labels.contains(&poly.id) && !move_with_area;
        let size_only = sel.area_sizes.contains(&poly.id) && !move_with_area;
        if move_with_area {
            poly.label_position = poly.label_position.map(shift);
            poly.area_size_position = poly.area_size_position.map(shift);
        } else {
            let (_, label_at, size_at) = block_positions[i];
            if label_only {
                // The name and its figure travel as one block; a figure
                // placed on its own keeps its offset.
                poly.label_position = Some(shift(label_at));
                if !size_only {
                    poly.area_size_position = poly.area_size_position.map(shift);
                }
            }
            if size_only {
                poly.area_size_position = Some(shift(size_at));
            }
        }
    }
    for t in &mut d.texts {
        if sel.texts.contains(&t.id) {
            t.x += dx;
            t.y += dy;
        }
    }

    // Move selected symbols (their hosted positions follow, but a manual
    // drag moves the instance; reposition happens on next refresh).
    for sym in &mut d.symbols {
        if sel.symbols.contains(&sym.id) {
            sym.x += dx;
            sym.y += dy;
        }
    }

    true
}

/// `moveLineEndpoint(lineId, endpoint, nextPoint)`: a wall endpoint is a
/// shared vertex, so it moves everywhere it occurs and adjoining walls stretch
/// while their opposite ends stay put.
pub fn move_line_endpoint(
    d: &mut Drawing,
    sel: &mut Selection,
    line_id: i64,
    start: bool,
    next: Pt,
) -> bool {
    let Some(line) = d.lines.iter().find(|l| l.id == line_id) else {
        return false;
    };
    let old = if start { line.start } else { line.end };
    if (next.x - old.x).hypot(next.y - old.y) < 1e-8 {
        return false;
    }
    let matches = |p: Pt| (p.x - old.x).hypot(p.y - old.y) < 1e-5;

    for line in &mut d.lines {
        if matches(line.start) {
            line.start = next;
        }
        if matches(line.end) {
            line.end = next;
        }
    }
    for poly in &mut d.polygons {
        for p in &mut poly.points {
            if matches(*p) {
                *p = next;
            }
        }
    }
    sel.active_endpoint = Some((line_id, start));

    // A bowed wall keeps its angle: the arc height follows the new chord.
    if let Some(line) = d.lines.iter_mut().find(|l| l.id == line_id) {
        if line.bow_angle.abs() > 0.001 {
            let chord = (line.end.x - line.start.x).hypot(line.end.y - line.start.y);
            line.arc_height = chord / 2.0 * (line.bow_angle * std::f64::consts::PI / 720.0).tan();
        }
    }
    true
}

/// Every point in the sketch (`getAllSketchPoints`, minus text and symbols).
fn all_points(d: &Drawing) -> Vec<Pt> {
    let mut out = Vec::new();
    for line in &d.lines {
        out.push(line.start);
        out.push(line.end);
    }
    for poly in &d.polygons {
        out.extend(poly.points.iter().copied());
    }
    // `getAllSketchPoints` also counts the positioned label / size blocks,
    // the free labels and the symbols.
    for poly in &d.polygons {
        out.extend(poly.label_position);
        out.extend(poly.area_size_position);
    }
    out.extend(d.texts.iter().map(|t| Pt::new(t.x, t.y)));
    out.extend(d.symbols.iter().map(|s| Pt::new(s.x, s.y)));
    out
}

fn bounds_center(points: &[Pt]) -> Pt {
    let min_x = points.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    Pt::new((min_x + max_x) / 2.0, (min_y + max_y) / 2.0)
}

/// `transformSketch`: apply `f` to every point, and turn the north arrow with
/// it.
fn transform_sketch(d: &mut Drawing, f: impl Fn(Pt) -> Pt, rotation_delta: f64) {
    for line in &mut d.lines {
        line.start = f(line.start);
        line.end = f(line.end);
        line.dimension_position = line.dimension_position.map(&f);
        line.dimension_anchor = line.dimension_anchor.map(&f);
    }
    for poly in &mut d.polygons {
        poly.points = poly.points.iter().map(|p| f(*p)).collect();
        // A positioned block moves with the sketch and counter-turns its
        // rotation (the polygons' sign; labels and symbols below add it).
        let label_rotation = poly.label_rotation.unwrap_or(0.0);
        let size_rotation = poly.area_size_rotation.unwrap_or(label_rotation);
        if let Some(p) = poly.label_position {
            poly.label_position = Some(f(p));
            poly.label_rotation = Some(label_rotation - rotation_delta);
        }
        if let Some(p) = poly.area_size_position {
            poly.area_size_position = Some(f(p));
            poly.area_size_rotation = Some(size_rotation - rotation_delta);
        }
    }
    for t in &mut d.texts {
        let p = f(Pt::new(t.x, t.y));
        t.x = p.x;
        t.y = p.y;
        t.rotation += rotation_delta;
    }
    for s in &mut d.symbols {
        let p = f(Pt::new(s.x, s.y));
        s.x = p.x;
        s.y = p.y;
        s.rotation += rotation_delta;
    }
}

/// `rotateSketch(degrees)`: the whole sketch about its own centre, north
/// included.
///
/// Its ribbon buttons (`rotate-sketch`, `flip-sketch`) are **hidden in the
/// original's own build** — they exist in the command registry with zero-sized
/// rects, so nothing on screen can reach them. Kept because the behaviour is
/// verified against the original and a command palette or the buttons
/// reappearing would make it live; the selection transforms below are the
/// reachable ones.
pub fn rotate_sketch(d: &mut Drawing, degrees: f64, north: &mut f64) -> bool {
    let points = all_points(d);
    if points.is_empty() {
        return false;
    }
    let c = bounds_center(&points);
    let radians = degrees * std::f64::consts::PI / 180.0;
    let (cos, sin) = (radians.cos(), radians.sin());
    transform_sketch(
        d,
        |p| {
            let dx = p.x - c.x;
            let dy = p.y - c.y;
            Pt::new(c.x + dx * cos - dy * sin, c.y + dx * sin + dy * cos)
        },
        degrees,
    );
    *north = pages::normalize_north(*north + degrees);
    true
}

/// `flipSketch(axis)`: the whole sketch mirrored about its centre. A mirror
/// reverses the compass: across the vertical axis a bearing `a` becomes
/// `360 - a`, across the horizontal axis `180 - a`.
pub fn flip_sketch(d: &mut Drawing, axis: Axis, north: &mut f64) -> bool {
    let points = all_points(d);
    if points.is_empty() {
        return false;
    }
    let c = bounds_center(&points);
    for line in &mut d.lines {
        line.bow_angle = -line.bow_angle;
        line.arc_height = -line.arc_height;
    }
    for poly in &mut d.polygons {
        for edge in &mut poly.edges {
            edge.bow_angle = -edge.bow_angle;
        }
    }
    transform_sketch(
        d,
        |p| match axis {
            Axis::Horizontal => Pt::new(2.0 * c.x - p.x, p.y),
            Axis::Vertical => Pt::new(p.x, 2.0 * c.y - p.y),
        },
        0.0,
    );
    *north = pages::normalize_north(match axis {
        Axis::Horizontal => -*north,
        Axis::Vertical => 180.0 - *north,
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketch_core::model::{SketchLine, WallType};

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

    fn square() -> Drawing {
        let mut d = Drawing::default();
        d.lines = vec![
            wall(1, 0.0, 0.0, 10.0, 0.0),
            wall(2, 10.0, 0.0, 10.0, 10.0),
            wall(3, 10.0, 10.0, 0.0, 10.0),
            wall(4, 0.0, 10.0, 0.0, 0.0),
        ];
        let points = vec![
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            Pt::new(10.0, 10.0),
            Pt::new(0.0, 10.0),
        ];
        let mut poly = Polygon {
            id: 100,
            points,
            edges: Vec::new(),
            kind: "UND".into(),
            name: String::new(),
            gross: 0.0,
            area: 100.0,
            factor_override: None,
            factor: 1.0,
            measure_to: "center".into(),
            auto_post: Default::default(),
            attributes: Default::default(),
            subtract_mode: "auto".into(),
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
        };
        poly.edges = Polygon::build_edges(&poly.points, &d.lines);
        d.polygons = vec![poly];
        d
    }

    /// Rotations leave a `-3e-16` residue exactly as the original's JS does,
    /// so comparisons are made to a tolerance rather than bit-for-bit.
    fn approx(p: Pt, x: f64, y: f64) -> bool {
        (p.x - x).abs() < 1e-9 && (p.y - y).abs() < 1e-9
    }

    fn select_all(d: &Drawing) -> Selection {
        Selection {
            lines: d.lines.iter().map(|l| l.id).collect(),
            polys: d.polygons.iter().map(|p| p.id).collect(),
            ..Selection::default()
        }
    }

    #[test]
    fn rotating_a_selected_square_turns_it_about_its_centre() {
        let mut d = square();
        let sel = select_all(&d);
        // The centre of a 10x10 square at the origin is (5,5); a quarter turn
        // clockwise takes (0,0) to (10,0).
        assert!(rotate_selected(&mut d, &sel, 90.0));
        let mut got: Vec<(f64, f64)> = d.polygons[0]
            .points
            .iter()
            .map(|p| ((p.x * 1e6).round() / 1e6, (p.y * 1e6).round() / 1e6))
            .collect();
        got.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(
            got,
            vec![(0.0, 0.0), (0.0, 10.0), (10.0, 0.0), (10.0, 10.0)],
            "a square maps onto itself"
        );
        // A corner wall's endpoints follow the same keys.
        assert!(
            approx(d.lines[0].start, 10.0, 0.0),
            "{:?}",
            d.lines[0].start
        );
        assert!(approx(d.lines[0].end, 10.0, 10.0), "{:?}", d.lines[0].end);
    }

    #[test]
    fn only_selected_points_move() {
        let mut d = square();
        // Select wall 1 alone: its endpoints are the only keys.
        let sel = Selection {
            lines: [1].into_iter().collect(),
            ..Selection::default()
        };
        assert!(rotate_selected(&mut d, &sel, 90.0));
        // The centre of (0,0)-(10,0) is (5,0): (0,0) -> (5,-5).
        assert!(
            approx(d.lines[0].start, 5.0, -5.0),
            "{:?}",
            d.lines[0].start
        );
        assert!(approx(d.lines[0].end, 5.0, 5.0), "{:?}", d.lines[0].end);
        // The live original moves everything that SHARES those keys: walls 2
        // and 4 (each touching an endpoint) and the area's first two corners.
        assert!(approx(d.lines[1].start, 5.0, 5.0), "{:?}", d.lines[1].start);
        assert!(approx(d.lines[3].end, 5.0, -5.0), "{:?}", d.lines[3].end);
        assert!(approx(d.polygons[0].points[0], 5.0, -5.0));
        assert!(approx(d.polygons[0].points[1], 5.0, 5.0));
        // The corners that share no key with the wall stay put.
        assert!(approx(d.polygons[0].points[2], 10.0, 10.0));
        assert!(approx(d.polygons[0].points[3], 0.0, 10.0));
    }

    #[test]
    fn flipping_the_sketch_mirrors_it_and_turns_north() {
        let mut d = square();
        let mut north = 30.0;
        assert!(flip_sketch(&mut d, Axis::Horizontal, &mut north));
        // The square spans 0..10 so the mirror about x=5 maps 0 <-> 10.
        let mut xs: Vec<f64> = d.polygons[0].points.iter().map(|p| p.x).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(xs, vec![0.0, 0.0, 10.0, 10.0]);
        // Across the vertical axis a bearing a becomes 360 - a.
        assert_eq!(north, 330.0);
        // And across the horizontal axis 180 - a.
        let mut north = 30.0;
        assert!(flip_sketch(&mut d, Axis::Vertical, &mut north));
        assert_eq!(north, 150.0);
    }

    #[test]
    fn rotating_the_sketch_turns_north_with_it() {
        let mut d = square();
        let mut north = 350.0;
        assert!(rotate_sketch(&mut d, 90.0, &mut north));
        assert_eq!(north, 80.0, "350 + 90 wraps");
        // The sketch's own centre is (5,5): the corner at (0,0) lands on
        // (10,0), so the square maps onto itself corner for corner.
        assert!(
            approx(d.polygons[0].points[0], 10.0, 0.0),
            "{:?}",
            d.polygons[0].points[0]
        );
        let mut pts: Vec<(i64, i64)> = d.polygons[0]
            .points
            .iter()
            .map(|p| (p.x.round() as i64, p.y.round() as i64))
            .collect();
        pts.sort();
        assert_eq!(pts, vec![(0, 0), (0, 10), (10, 0), (10, 10)]);
    }

    /// The live original's own output for the whole-sketch transforms on the
    /// 10x10 square with north at 30 degrees.
    #[test]
    fn whole_sketch_transforms_match_the_original() {
        let mut d = square();
        let mut north = 30.0;
        assert!(rotate_sketch(&mut d, 90.0, &mut north));
        let lines: Vec<(f64, f64, f64, f64)> = d
            .lines
            .iter()
            .map(|l| {
                (
                    l.start.x.round(),
                    l.start.y.round(),
                    l.end.x.round(),
                    l.end.y.round(),
                )
            })
            .collect();
        assert_eq!(
            lines,
            vec![
                (10.0, 0.0, 10.0, 10.0),
                (10.0, 10.0, 0.0, 10.0),
                (0.0, 10.0, 0.0, 0.0),
                (0.0, 0.0, 10.0, 0.0),
            ]
        );
        assert_eq!(north, 120.0);

        let mut d = square();
        let mut north = 30.0;
        assert!(flip_sketch(&mut d, Axis::Vertical, &mut north));
        let lines: Vec<(f64, f64, f64, f64)> = d
            .lines
            .iter()
            .map(|l| {
                (
                    l.start.x.round(),
                    l.start.y.round(),
                    l.end.x.round(),
                    l.end.y.round(),
                )
            })
            .collect();
        assert_eq!(
            lines,
            vec![
                (0.0, 10.0, 10.0, 10.0),
                (10.0, 10.0, 10.0, 0.0),
                (10.0, 0.0, 0.0, 0.0),
                (0.0, 0.0, 0.0, 10.0),
            ]
        );
        assert_eq!(north, 150.0);
    }

    #[test]
    fn a_flip_reverses_every_bow() {
        let mut d = square();
        d.lines[0].bow_angle = 30.0;
        d.lines[0].arc_height = 3.0;
        d.polygons[0].edges[0].bow_angle = 30.0;
        assert!(flip_sketch(&mut d, Axis::Horizontal, &mut 0.0));
        assert_eq!(d.lines[0].bow_angle, -30.0);
        assert_eq!(d.lines[0].arc_height, -3.0);
        assert_eq!(d.polygons[0].edges[0].bow_angle, -30.0);
    }

    #[test]
    fn flipping_an_area_takes_its_boundary_walls() {
        let mut d = square();
        let sel = Selection {
            polys: [100].into_iter().collect(),
            ..Selection::default()
        };
        assert!(flip_selected(&mut d, &sel, Axis::Horizontal));
        // Every corner moved to its mirror image, and the walls with them.
        assert!(
            approx(d.lines[0].start, 10.0, 0.0),
            "{:?}",
            d.lines[0].start
        );
        assert!(approx(d.lines[0].end, 0.0, 0.0), "{:?}", d.lines[0].end);
        // Nothing selected: nothing happens.
        assert!(!flip_selected(
            &mut d,
            &Selection::default(),
            Axis::Horizontal
        ));
        assert!(!rotate_selected(&mut d, &Selection::default(), 90.0));
    }

    #[test]
    fn rotating_or_flipping_the_sketch_takes_labels_symbols_and_pinned_blocks() {
        let mut d = square();
        d.texts
            .push(crate::placed_text::PlacedText::new(7, "Porch"));
        d.texts[0].x = 10.0;
        d.texts[0].y = 5.0;
        d.symbols.push(
            serde_json::from_value(serde_json::json!({
                "id": 8, "symbolType": "windows/36in", "x": 0.0, "y": 5.0
            }))
            .unwrap(),
        );
        d.symbols[0].x = 0.0;
        d.symbols[0].y = 5.0;
        d.polygons[0].label_position = Some(Pt::new(7.0, 5.0));
        d.polygons[0].label_rotation = Some(30.0);
        let mut north = 0.0;
        // 90 degrees about the centre (5, 5): (10, 5) -> (5, 10).
        assert!(rotate_sketch(&mut d, 90.0, &mut north));
        assert!((d.texts[0].x - 5.0).abs() < 1e-9 && (d.texts[0].y - 10.0).abs() < 1e-9);
        assert_eq!(d.texts[0].rotation, 90.0, "labels add the turn");
        assert!((d.symbols[0].x - 5.0).abs() < 1e-9 && (d.symbols[0].y - 0.0).abs() < 1e-9);
        assert_eq!(d.symbols[0].rotation, 90.0);
        let at = d.polygons[0].label_position.unwrap();
        assert!(
            (at.x - 5.0).abs() < 1e-9 && (at.y - 7.0).abs() < 1e-9,
            "{at:?}"
        );
        assert_eq!(
            d.polygons[0].label_rotation,
            Some(-60.0),
            "areas subtract it"
        );
        // A mirror moves them without touching their rotation.
        let mut d = square();
        d.texts
            .push(crate::placed_text::PlacedText::new(7, "Porch"));
        d.texts[0].x = 8.0;
        d.polygons[0].label_position = Some(Pt::new(8.0, 5.0));
        d.polygons[0].label_rotation = Some(30.0);
        assert!(flip_sketch(&mut d, Axis::Horizontal, &mut north));
        assert!((d.texts[0].x - 2.0).abs() < 1e-9);
        assert!((d.polygons[0].label_position.unwrap().x - 2.0).abs() < 1e-9);
        assert_eq!(d.polygons[0].label_rotation, Some(30.0));
        assert_eq!(d.texts[0].rotation, 0.0);
    }
}

#[cfg(test)]
mod move_tests {
    use super::*;
    use sketch_core::model::{SketchLine, WallType};

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

    /// Two walls in a line (1: 0..10, 2: 10..20) plus an area on wall 1.
    fn sketch() -> Drawing {
        let mut d = Drawing::default();
        d.lines = vec![wall(1, 0.0, 0.0, 10.0, 0.0), wall(2, 10.0, 0.0, 20.0, 0.0)];
        let mut poly = Polygon {
            id: 100,
            points: vec![
                Pt::new(0.0, 0.0),
                Pt::new(10.0, 0.0),
                Pt::new(10.0, 10.0),
                Pt::new(0.0, 10.0),
            ],
            edges: Vec::new(),
            kind: "UND".into(),
            name: String::new(),
            gross: 0.0,
            area: 100.0,
            factor_override: None,
            factor: 1.0,
            measure_to: "center".into(),
            auto_post: Default::default(),
            attributes: Default::default(),
            subtract_mode: "auto".into(),
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
        };
        poly.edges = Polygon::build_edges(&poly.points, &d.lines);
        d.polygons = vec![poly];
        d
    }

    #[test]
    fn moving_a_wall_stretches_its_neighbour() {
        let mut d = sketch();
        let sel = Selection {
            lines: [1].into_iter().collect(),
            ..Selection::default()
        };
        assert!(move_selected(&mut d, &sel, 5.0, 0.0));
        // Wall 1 moves whole.
        assert_eq!(d.lines[0].start, Pt::new(5.0, 0.0));
        assert_eq!(d.lines[0].end, Pt::new(15.0, 0.0));
        // Wall 2 keeps its far end and follows at the shared vertex.
        assert_eq!(d.lines[1].start, Pt::new(15.0, 0.0));
        assert_eq!(d.lines[1].end, Pt::new(20.0, 0.0));
        // The area's two points on wall 1 follow too.
        assert_eq!(d.polygons[0].points[0], Pt::new(5.0, 0.0));
        assert_eq!(d.polygons[0].points[1], Pt::new(15.0, 0.0));
        assert_eq!(d.polygons[0].points[2], Pt::new(10.0, 10.0));
    }

    #[test]
    fn moving_an_area_moves_only_its_own_boundary() {
        let mut d = sketch();
        let sel = Selection {
            polys: [100].into_iter().collect(),
            ..Selection::default()
        };
        assert!(move_selected(&mut d, &sel, 0.0, 3.0));
        // Every area point moves.
        assert_eq!(d.polygons[0].points[0], Pt::new(0.0, 3.0));
        assert_eq!(d.polygons[0].points[2], Pt::new(10.0, 13.0));
        // Wall 2 is not part of the area, so it does not stretch.
        assert_eq!(d.lines[1].start, Pt::new(10.0, 0.0));
        assert_eq!(d.lines[1].end, Pt::new(20.0, 0.0));
    }

    #[test]
    fn dragging_an_endpoint_moves_the_shared_vertex_everywhere() {
        let mut d = sketch();
        let mut sel = Selection::default();
        assert!(move_line_endpoint(
            &mut d,
            &mut sel,
            1,
            false,
            Pt::new(12.0, 4.0)
        ));
        // Wall 1's end and wall 2's start are the same vertex.
        assert_eq!(d.lines[0].end, Pt::new(12.0, 4.0));
        assert_eq!(d.lines[1].start, Pt::new(12.0, 4.0));
        // The area corner on that vertex follows; the opposite end stays.
        assert_eq!(d.polygons[0].points[1], Pt::new(12.0, 4.0));
        assert_eq!(d.lines[0].start, Pt::new(0.0, 0.0));
        assert_eq!(sel.active_endpoint, Some((1, false)));
        // A no-op drag reports false.
        assert!(!move_line_endpoint(
            &mut d,
            &mut sel,
            1,
            false,
            Pt::new(12.0, 4.0)
        ));
    }

    #[test]
    fn a_bowed_wall_keeps_its_angle_when_an_endpoint_moves() {
        let mut d = sketch();
        d.lines[0].bow_angle = 30.0;
        let mut sel = Selection::default();
        assert!(move_line_endpoint(
            &mut d,
            &mut sel,
            1,
            false,
            Pt::new(20.0, 0.0)
        ));
        // chord 20 ft at 30 degrees: 20/2 * tan(30*pi/720)
        let want = 10.0 * (30.0 * std::f64::consts::PI / 720.0).tan();
        assert!(
            (d.lines[0].arc_height - want).abs() < 1e-9,
            "{}",
            d.lines[0].arc_height
        );
    }
}
