//! Selection (screen 6): `hitTestAt`, `selectLineAt`, `selectAll` and the
//! selection sets (refs/sketch-app.js:711-830).
//!
//! The hit order is the original's, restricted to the entity kinds this port
//! has so far — walls (a 4 px "on the wall" pass that wins over dimension
//! text, then a 9 px pass) and polygon interiors. Area labels, text labels,
//! symbols and dimension text boxes join when those screens land; the
//! original tests them ahead of the wall passes, so adding them later must
//! keep that order.

use std::collections::HashSet;

use sketch_core::geometry::{distance, GeometryEngine, Pt, PIXELS_PER_FOOT};
use sketch_core::model::{Polygon, SketchLine};

/// `getScreenToleranceWorld(px)` — screen pixels in world feet.
pub fn tolerance(zoom: f64, px: f64) -> f64 {
    px / (PIXELS_PER_FOOT * zoom.max(1e-9))
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Selection {
    /// `state.selectedLineIds`.
    pub lines: HashSet<i64>,
    /// `state.selectedPolyIds`.
    pub polys: HashSet<i64>,
    /// `state.selectedSymbolIds`.
    pub symbols: HashSet<i64>,
    /// `state.selectedTextIds`.
    pub texts: HashSet<i64>,
    /// `state.selectedAreaLabelIds`: areas whose name / code block is picked.
    pub area_labels: HashSet<i64>,
    /// `state.selectedAreaSizeIds`: areas whose size block is picked.
    pub area_sizes: HashSet<i64>,
    /// `state.selectedChainIds` (screen 9).
    pub chains: HashSet<i64>,
    /// `state.selectedDimensionIds`: walls whose measurement label is picked.
    pub dimensions: HashSet<i64>,
    /// `state.activeLineEndpoint` — the handle a Length edit or a drag moves.
    pub active_endpoint: Option<(i64, bool)>,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
            && self.polys.is_empty()
            && self.symbols.is_empty()
            && self.texts.is_empty()
            && self.area_labels.is_empty()
            && self.area_sizes.is_empty()
            && self.dimensions.is_empty()
            && self.chains.is_empty()
    }

    /// `clearSelection`.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.polys.clear();
        self.symbols.clear();
        self.texts.clear();
        self.area_labels.clear();
        self.area_sizes.clear();
        self.dimensions.clear();
        self.chains.clear();
        self.active_endpoint = None;
    }

    /// Adds everything `other` holds (a fence's position-picked entities).
    pub fn absorb(&mut self, other: Selection) {
        self.texts.extend(other.texts);
        self.symbols.extend(other.symbols);
        self.area_labels.extend(other.area_labels);
        self.area_sizes.extend(other.area_sizes);
        self.dimensions.extend(other.dimensions);
    }

    pub fn is_chain_selected(&self, id: i64) -> bool {
        self.chains.contains(&id)
    }

    #[allow(dead_code)]
    pub fn is_symbol_selected(&self, id: i64) -> bool {
        self.symbols.contains(&id)
    }

    pub fn is_line_selected(&self, id: i64) -> bool {
        self.lines.contains(&id)
    }

    pub fn is_poly_selected(&self, id: i64) -> bool {
        self.polys.contains(&id)
    }
}

/// What a click landed on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Line(i64),
    Polygon(i64),
}

/// `distanceToWall(worldPt, line)` — the chord for a straight wall, the
/// flattened arc for a bowed one.
pub fn distance_to_wall(point: Pt, line: &SketchLine, zoom: f64) -> f64 {
    let geo = GeometryEngine::new();
    if line.bow_angle.abs() < 0.5 {
        return geo.distance_to_segment(point, line.start, line.end);
    }
    let ring = geo.flatten_ring(
        &[line.start, line.end],
        &[
            sketch_core::geometry::Edge {
                line_id: None,
                bow_angle: line.bow_angle,
            },
            sketch_core::geometry::Edge::default(),
        ],
        (tolerance(zoom, 1.0) / 2.0).max(0.005),
    );
    let mut best = f64::INFINITY;
    for pair in ring.windows(2) {
        best = best.min(geo.distance_to_segment(point, pair[0], pair[1]));
    }
    best
}

/// `hitTestAt(worldPt)`.
pub fn hit_test_at(
    point: Pt,
    lines: &[SketchLine],
    polygons: &[Polygon],
    zoom: f64,
    selectable: &dyn Fn(i64) -> bool,
) -> Option<Hit> {
    // A click right on a wall selects the wall even where its dimension text
    // overlaps it.
    let mut on_wall: Option<(i64, f64)> = None;
    let on_wall_threshold = tolerance(zoom, 4.0);
    for line in lines.iter().rev() {
        if !selectable(line.id) {
            continue;
        }
        let d = distance_to_wall(point, line, zoom);
        if d <= on_wall_threshold && on_wall.is_none_or(|(_, best)| d <= best) {
            on_wall = Some((line.id, d));
        }
    }
    if let Some((id, _)) = on_wall {
        return Some(Hit::Line(id));
    }

    let line_threshold = tolerance(zoom, 9.0);
    for line in lines.iter().rev() {
        if !selectable(line.id) {
            continue;
        }
        if distance_to_wall(point, line, zoom) <= line_threshold {
            return Some(Hit::Line(line.id));
        }
    }

    let geo = GeometryEngine::new();
    for poly in polygons.iter().rev() {
        if !selectable(poly.id) {
            continue;
        }
        let ring = geo.flatten_ring(&poly.points, &poly.edges, 0.01);
        if ring.len() >= 3 && geo.point_in_polygon(point, &ring) {
            return Some(Hit::Polygon(poly.id));
        }
    }
    None
}

/// `selectLineAt(worldPt, additive)`: clear unless additive, toggle when
/// additive, and remember which end of a picked wall is nearer.
pub fn select_at(
    selection: &mut Selection,
    point: Pt,
    lines: &[SketchLine],
    polygons: &[Polygon],
    zoom: f64,
    additive: bool,
    selectable: &dyn Fn(i64) -> bool,
) -> Option<Hit> {
    let hit = hit_test_at(point, lines, polygons, zoom, selectable);
    if !additive {
        selection.clear();
    }
    let hit = hit?;
    match hit {
        Hit::Line(id) => {
            if additive && selection.lines.contains(&id) {
                selection.lines.remove(&id);
            } else {
                selection.lines.insert(id);
            }
            if let Some(line) = lines.iter().find(|l| l.id == id) {
                let at_start = distance(point, line.start) <= distance(point, line.end);
                selection.active_endpoint = Some((id, at_start));
            }
        }
        Hit::Polygon(id) => {
            if additive && selection.polys.contains(&id) {
                selection.polys.remove(&id);
            } else {
                selection.polys.insert(id);
            }
        }
    }
    Some(hit)
}

/// `selectAll()`: the `selectable` gate (hidden/locked layers) is the
/// layer system's (sketch-app.js:831 `const selectable = ...`).
pub fn select_all(
    selection: &mut Selection,
    lines: &[SketchLine],
    polygons: &[Polygon],
    texts: &[crate::placed_text::PlacedText],
    symbols: &[crate::placed_symbol::PlacedSymbol],
    selectable: &dyn Fn(i64) -> bool,
) {
    selection.clear();
    selection
        .lines
        .extend(lines.iter().filter(|l| selectable(l.id)).map(|l| l.id));
    selection
        .polys
        .extend(polygons.iter().filter(|p| selectable(p.id)).map(|p| p.id));
    // `selectAll` also takes the label and size block of every defined area.
    for poly in polygons
        .iter()
        .filter(|p| selectable(p.id) && !p.kind.is_empty() && p.kind != "UND")
    {
        selection.area_labels.insert(poly.id);
        selection.area_sizes.insert(poly.id);
    }
    // ...and every free text label and symbol.
    selection
        .texts
        .extend(texts.iter().filter(|t| selectable(t.id)).map(|t| t.id));
    selection
        .symbols
        .extend(symbols.iter().filter(|s| selectable(s.id)).map(|s| s.id));
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketch_core::model::WallType;

    fn line(id: i64, sx: f64, sy: f64, ex: f64, ey: f64) -> SketchLine {
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
    fn a_click_on_a_wall_hits_it() {
        let lines = vec![line(1, 0.0, 0.0, 10.0, 0.0)];
        // 4 px = 0.2 ft at 20 px/ft, zoom 1: 0.1 ft off the wall is a hit.
        assert_eq!(
            hit_test_at(Pt::new(5.0, 0.1), &lines, &[], 1.0, &|_| true),
            Some(Hit::Line(1))
        );
        // Far away is nothing.
        assert_eq!(
            hit_test_at(Pt::new(5.0, 5.0), &lines, &[], 1.0, &|_| true),
            None
        );
    }

    #[test]
    fn the_tolerance_follows_the_zoom() {
        let lines = vec![line(1, 0.0, 0.0, 10.0, 0.0)];
        // 0.3 ft is inside 9 px at zoom 1 (0.45 ft) but outside at zoom 2.
        assert_eq!(
            hit_test_at(Pt::new(5.0, 0.3), &lines, &[], 1.0, &|_| true),
            Some(Hit::Line(1))
        );
        assert_eq!(
            hit_test_at(Pt::new(5.0, 0.3), &lines, &[], 2.0, &|_| true),
            None
        );
    }

    #[test]
    fn a_click_inside_a_polygon_selects_the_area() {
        let poly = Polygon {
            id: 7,
            points: vec![
                Pt::new(0.0, 0.0),
                Pt::new(10.0, 0.0),
                Pt::new(10.0, 10.0),
                Pt::new(0.0, 10.0),
            ],
            edges: vec![
                sketch_core::geometry::Edge::default(),
                sketch_core::geometry::Edge::default(),
                sketch_core::geometry::Edge::default(),
                sketch_core::geometry::Edge::default(),
            ],
            kind: "UND".into(),
            name: "Undefined Area".into(),
            gross: 100.0,
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
        assert_eq!(
            hit_test_at(Pt::new(5.0, 5.0), &[], &[poly], 1.0, &|_| true),
            Some(Hit::Polygon(7))
        );
    }

    #[test]
    fn a_wall_wins_over_the_area_under_it() {
        let lines = vec![line(1, 0.0, 0.0, 10.0, 0.0)];
        let poly = Polygon {
            id: 7,
            points: vec![
                Pt::new(0.0, 0.0),
                Pt::new(10.0, 0.0),
                Pt::new(10.0, 10.0),
                Pt::new(0.0, 10.0),
            ],
            edges: vec![
                sketch_core::geometry::Edge::default(),
                sketch_core::geometry::Edge::default(),
                sketch_core::geometry::Edge::default(),
                sketch_core::geometry::Edge::default(),
            ],
            kind: "UND".into(),
            name: "Undefined Area".into(),
            gross: 100.0,
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
        // Right on the wall: the wall.
        assert_eq!(
            hit_test_at(
                Pt::new(5.0, 0.0),
                &lines,
                std::slice::from_ref(&poly),
                1.0,
                &|_| true
            ),
            Some(Hit::Line(1))
        );
        // Inside the area but off the wall: the area.
        assert_eq!(
            hit_test_at(Pt::new(5.0, 5.0), &lines, &[poly], 1.0, &|_| true),
            Some(Hit::Polygon(7))
        );
    }

    #[test]
    fn additive_clicks_toggle_and_plain_clicks_replace() {
        let lines = vec![line(1, 0.0, 0.0, 10.0, 0.0), line(2, 0.0, 5.0, 10.0, 5.0)];
        let mut sel = Selection::default();
        select_at(
            &mut sel,
            Pt::new(5.0, 0.0),
            &lines,
            &[],
            1.0,
            false,
            &|_| true,
        );
        assert!(sel.is_line_selected(1));
        assert_eq!(sel.active_endpoint.map(|(id, _)| id), Some(1));
        // Additive adds.
        select_at(&mut sel, Pt::new(5.0, 5.0), &lines, &[], 1.0, true, &|_| {
            true
        });
        assert!(sel.is_line_selected(1) && sel.is_line_selected(2));
        // Additive on a selected wall removes it.
        select_at(&mut sel, Pt::new(5.0, 5.0), &lines, &[], 1.0, true, &|_| {
            true
        });
        assert!(sel.is_line_selected(1) && !sel.is_line_selected(2));
        // A plain click replaces the selection.
        select_at(
            &mut sel,
            Pt::new(5.0, 5.0),
            &lines,
            &[],
            1.0,
            false,
            &|_| true,
        );
        assert!(!sel.is_line_selected(1) && sel.is_line_selected(2));
        // A plain click on empty canvas clears.
        select_at(
            &mut sel,
            Pt::new(5.0, 50.0),
            &lines,
            &[],
            1.0,
            false,
            &|_| true,
        );
        assert!(sel.is_empty());
    }

    #[test]
    fn the_nearer_end_of_a_picked_wall_becomes_active() {
        let lines = vec![line(1, 0.0, 0.0, 10.0, 0.0)];
        let mut sel = Selection::default();
        select_at(
            &mut sel,
            Pt::new(1.0, 0.0),
            &lines,
            &[],
            1.0,
            false,
            &|_| true,
        );
        assert_eq!(sel.active_endpoint, Some((1, true)));
        select_at(
            &mut sel,
            Pt::new(9.0, 0.0),
            &lines,
            &[],
            1.0,
            false,
            &|_| true,
        );
        assert_eq!(sel.active_endpoint, Some((1, false)));
    }

    #[test]
    fn select_all_takes_walls_and_areas() {
        let lines = vec![line(1, 0.0, 0.0, 10.0, 0.0)];
        let mut sel = Selection::default();
        select_all(&mut sel, &lines, &[], &[], &[], &|_| true);
        assert_eq!(sel.lines.len(), 1);
        assert!(!sel.is_empty());
    }
}

/// `fenceSelect(min, max, additive)`: a wall needs BOTH endpoints inside the
/// rectangle, an area ALL of its points — containment, not intersection.
pub fn fence_select(
    sel: &mut Selection,
    lines: &[sketch_core::model::SketchLine],
    polygons: &[sketch_core::model::Polygon],
    min: sketch_core::geometry::Pt,
    max: sketch_core::geometry::Pt,
    additive: bool,
    selectable: &dyn Fn(i64) -> bool,
) {
    if !additive {
        sel.clear();
    }
    let inside =
        |p: sketch_core::geometry::Pt| p.x >= min.x && p.x <= max.x && p.y >= min.y && p.y <= max.y;
    for line in lines {
        if selectable(line.id) && inside(line.start) && inside(line.end) {
            sel.lines.insert(line.id);
        }
    }
    for poly in polygons {
        if selectable(poly.id) && poly.points.iter().all(|p| inside(*p)) {
            sel.polys.insert(poly.id);
        }
    }
}

/// `lassoSelect(points, additive)`: the same containment rules against a
/// closed freehand path (`Geometry.pointInPolygon`).
pub fn lasso_select(
    sel: &mut Selection,
    lines: &[sketch_core::model::SketchLine],
    polygons: &[sketch_core::model::Polygon],
    points: &[sketch_core::geometry::Pt],
    additive: bool,
    selectable: &dyn Fn(i64) -> bool,
) {
    if points.len() < 3 {
        return;
    }
    if !additive {
        sel.clear();
    }
    let geo = sketch_core::geometry::GeometryEngine::new();
    let inside = |p: sketch_core::geometry::Pt| geo.point_in_polygon(p, points);
    for line in lines {
        if selectable(line.id) && inside(line.start) && inside(line.end) {
            sel.lines.insert(line.id);
        }
    }
    for poly in polygons {
        if selectable(poly.id) && poly.points.iter().all(|p| inside(*p)) {
            sel.polys.insert(poly.id);
        }
    }
}

/// The entities a fence or lasso picks by where they sit rather than by their
/// outline (`fenceSelect` / `lassoSelect`): free labels, symbols, the label
/// and size blocks of defined areas, and wall measurement labels.
#[derive(Default)]
pub struct Pickable<'a> {
    pub texts: &'a [crate::placed_text::PlacedText],
    pub symbols: &'a [crate::placed_symbol::PlacedSymbol],
    /// `(id, label position, size position)` of every defined area.
    pub area_text: Vec<(i64, Pt, Pt)>,
    /// `(wall id, label position)` of every visible measurement label.
    pub dimensions: Vec<(i64, Pt)>,
}

/// The position-picked half of a fence / lasso: whatever sits inside joins the
/// selection (the caller has already picked walls and areas by containment).
pub fn pick_by_position(
    sel: &mut Selection,
    pick: &Pickable,
    inside: &dyn Fn(Pt) -> bool,
    selectable: &dyn Fn(i64) -> bool,
) {
    for (id, label, size) in &pick.area_text {
        if !selectable(*id) {
            continue;
        }
        if inside(*label) {
            sel.area_labels.insert(*id);
        }
        if inside(*size) {
            sel.area_sizes.insert(*id);
        }
    }
    for t in pick.texts {
        if selectable(t.id) && inside(Pt::new(t.x, t.y)) {
            sel.texts.insert(t.id);
        }
    }
    for s in pick.symbols {
        if selectable(s.id) && inside(Pt::new(s.x, s.y)) {
            sel.symbols.insert(s.id);
        }
    }
    for (id, at) in &pick.dimensions {
        if selectable(*id) && inside(*at) {
            sel.dimensions.insert(*id);
        }
    }
}

/// Whether a wall lies within 4 screen px of the point: `hitTestAt`'s first
/// wall pass, which wins over a measurement label drawn across it.
pub fn on_wall(
    point: Pt,
    lines: &[SketchLine],
    zoom: f64,
    selectable: &dyn Fn(i64) -> bool,
) -> bool {
    let threshold = tolerance(zoom, 4.0);
    lines
        .iter()
        .any(|l| selectable(l.id) && distance_to_wall(point, l, zoom) <= threshold)
}

/// `selectByType(type)` (refs/sketch-app.js:842-869). The original also has
/// text, dimension and symbol selection sets; this port has no text, symbol or
/// dimension-selection model yet, so those kinds clear the selection and leave
/// it empty rather than pretending to select something.
pub fn select_by_type(
    sel: &mut Selection,
    lines: &[sketch_core::model::SketchLine],
    polygons: &[sketch_core::model::Polygon],
    texts: &[crate::placed_text::PlacedText],
    symbols: &[crate::placed_symbol::PlacedSymbol],
    kind: crate::select_menu::SelectKind,
    selectable: &dyn Fn(i64) -> bool,
) {
    use crate::select_menu::SelectKind;
    sel.clear();
    match kind {
        // `else { this.selectAll(); }` — the original's fallback.
        SelectKind::All => select_all(sel, lines, polygons, texts, symbols, selectable),
        SelectKind::Areas => {
            sel.polys = polygons
                .iter()
                .filter(|p| selectable(p.id))
                .map(|p| p.id)
                .collect();
        }
        SelectKind::Freeform => {
            // `l.type === 'interior'`.
            sel.lines = lines
                .iter()
                .filter(|l| l.kind == sketch_core::model::WallType::Interior && selectable(l.id))
                .map(|l| l.id)
                .collect();
        }
        // `type === 'text'`: the free text labels plus every defined area's
        // label and size blocks.
        SelectKind::Text => {
            let defined = polygons
                .iter()
                .filter(|p| selectable(p.id) && !p.kind.is_empty() && p.kind != "UND")
                .map(|p| p.id);
            let defined: Vec<i64> = defined.collect();
            sel.area_labels = defined.iter().copied().collect();
            sel.area_sizes = defined.into_iter().collect();
            sel.texts = texts
                .iter()
                .filter(|t| selectable(t.id))
                .map(|t| t.id)
                .collect();
        }
        SelectKind::Symbols => {
            sel.symbols = symbols
                .iter()
                .filter(|s| selectable(s.id))
                .map(|s| s.id)
                .collect();
        }
        // Dimension selection (`selectedDimensionIds`) has no set here yet.
        SelectKind::Dimensions => {}
    }
}

#[cfg(test)]
mod by_type_tests {
    use super::*;
    use crate::select_menu::SelectKind;
    use sketch_core::geometry::Pt;
    use sketch_core::model::{SketchLine, WallType};

    fn line(id: i64, kind: WallType) -> SketchLine {
        SketchLine {
            id,
            start: Pt::new(0.0, 0.0),
            end: Pt::new(10.0, 0.0),
            kind,
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

    fn poly(id: i64) -> sketch_core::model::Polygon {
        sketch_core::model::Polygon {
            id,
            points: Vec::new(),
            edges: Vec::new(),
            kind: "UND".into(),
            name: String::new(),
            gross: 0.0,
            area: 0.0,
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
        }
    }

    #[test]
    fn areas_select_only_polygons() {
        let lines = vec![line(1, WallType::Exterior), line(2, WallType::Interior)];
        let polys = vec![poly(100), poly(101)];
        let mut sel = Selection::default();
        select_by_type(
            &mut sel,
            &lines,
            &polys,
            &[],
            &[],
            SelectKind::Areas,
            &|_| true,
        );
        assert_eq!(sel.polys.len(), 2);
        assert!(sel.lines.is_empty());
    }

    #[test]
    fn freeform_selects_interior_walls_only() {
        let lines = vec![line(1, WallType::Exterior), line(2, WallType::Interior)];
        let mut sel = Selection::default();
        select_by_type(
            &mut sel,
            &lines,
            &[],
            &[],
            &[],
            SelectKind::Freeform,
            &|_| true,
        );
        assert_eq!(sel.lines.into_iter().collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn all_selects_everything_and_the_rest_clear_it() {
        let lines = vec![line(1, WallType::Exterior), line(2, WallType::Interior)];
        let polys = vec![poly(100)];
        let mut sel = Selection::default();
        select_by_type(&mut sel, &lines, &polys, &[], &[], SelectKind::All, &|_| {
            true
        });
        assert_eq!((sel.lines.len(), sel.polys.len()), (2, 1));
        // Dimensions have no selection set yet; the selection clears first.
        select_by_type(
            &mut sel,
            &lines,
            &polys,
            &[],
            &[],
            SelectKind::Dimensions,
            &|_| true,
        );
        assert!(
            sel.is_empty(),
            "the selection is cleared first, as the original does"
        );
    }

    #[test]
    fn text_and_symbols_select_their_own_kind_respecting_locks() {
        let symbol = |id: i64, kind: &str| -> crate::placed_symbol::PlacedSymbol {
            serde_json::from_value(serde_json::json!({
                "id": id, "symbolType": kind, "x": 0.0, "y": 0.0
            }))
            .unwrap()
        };
        let texts = vec![
            crate::placed_text::PlacedText::new(7, "a"),
            crate::placed_text::PlacedText::new(8, "b"),
        ];
        let symbols = vec![symbol(20, "door-single"), symbol(21, "window-single")];
        let lines = vec![line(1, WallType::Exterior)];
        let mut sel = Selection::default();
        select_by_type(
            &mut sel,
            &lines,
            &[],
            &texts,
            &symbols,
            SelectKind::Text,
            &|_| true,
        );
        assert_eq!(sel.texts.len(), 2);
        assert!(sel.symbols.is_empty() && sel.lines.is_empty());
        // A locked / hidden entity is skipped (`selectable`).
        select_by_type(
            &mut sel,
            &lines,
            &[],
            &texts,
            &symbols,
            SelectKind::Symbols,
            &|id| id != 21,
        );
        assert_eq!(sel.symbols.into_iter().collect::<Vec<_>>(), vec![20]);
        assert!(sel.texts.is_empty(), "the other kinds clear");
    }
}

#[cfg(test)]
mod fence_tests {
    use super::*;
    use sketch_core::geometry::Pt;
    use sketch_core::model::{Polygon, SketchLine, WallType};

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

    fn poly(id: i64, pts: &[(f64, f64)]) -> Polygon {
        Polygon {
            id,
            points: pts.iter().map(|(x, y)| Pt::new(*x, *y)).collect(),
            edges: Vec::new(),
            kind: "UND".into(),
            name: String::new(),
            gross: 0.0,
            area: 0.0,
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
        }
    }

    #[test]
    fn a_fence_needs_whole_entities_inside() {
        let lines = vec![
            wall(1, 0.0, 0.0, 5.0, 0.0),  // fully inside
            wall(2, 5.0, 0.0, 20.0, 0.0), // sticks out
            wall(3, 0.0, 9.0, 5.0, 9.0),  // fully inside
        ];
        let polys = vec![
            poly(100, &[(0.0, 0.0), (5.0, 0.0), (5.0, 5.0)]),
            poly(101, &[(0.0, 0.0), (5.0, 0.0), (5.0, 30.0)]),
        ];
        let mut sel = Selection::default();
        fence_select(
            &mut sel,
            &lines,
            &polys,
            Pt::new(-1.0, -1.0),
            Pt::new(10.0, 10.0),
            false,
            &|_| true,
        );
        assert_eq!(sel.lines.len(), 2, "the wall crossing the edge is out");
        assert!(!sel.lines.contains(&2));
        assert_eq!(sel.polys.len(), 1, "an area with a corner outside is out");
        assert!(sel.polys.contains(&100));
    }

    #[test]
    fn additive_fences_keep_what_was_selected() {
        let lines = vec![wall(1, 0.0, 0.0, 5.0, 0.0), wall(2, 100.0, 0.0, 105.0, 0.0)];
        let mut sel = Selection {
            lines: [2].into_iter().collect(),
            ..Selection::default()
        };
        fence_select(
            &mut sel,
            &lines,
            &[],
            Pt::new(-1.0, -1.0),
            Pt::new(10.0, 10.0),
            true,
            &|_| true,
        );
        assert_eq!(sel.lines.len(), 2, "Ctrl keeps the earlier pick");
    }

    #[test]
    fn a_lasso_uses_the_same_containment_rules() {
        let lines = vec![wall(1, 0.0, 0.0, 5.0, 0.0), wall(2, 5.0, 0.0, 20.0, 0.0)];
        // A triangle around the first wall only.
        let path = vec![Pt::new(-1.0, -1.0), Pt::new(10.0, -1.0), Pt::new(5.0, 6.0)];
        let mut sel = Selection::default();
        lasso_select(&mut sel, &lines, &[], &path, false, &|_| true);
        assert_eq!(sel.lines.into_iter().collect::<Vec<_>>(), vec![1]);
        // Fewer than three points selects nothing at all.
        let mut sel = Selection::default();
        lasso_select(&mut sel, &lines, &[], &path[..2], false, &|_| true);
        assert!(sel.is_empty());
    }
}
