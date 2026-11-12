//! Defining an area from a point (`resolveRegionAt`, `regionAreaAt`,
//! `ownBoundaryLine`, `defineAreaTarget`): what an area-code drop on the
//! canvas does. The point means the tightest existing area under it, or the
//! tightest region the exterior walls enclose around it (which the drop turns
//! into a new area).

use sketch_core::geometry::{EnclosedFace, Pt};
use sketch_core::model::{Polygon, SketchLine, WallType};

use crate::drawing::Drawing;
use crate::layers::LayerGates;

/// What a point resolves to.
#[derive(Debug, Clone, PartialEq)]
pub enum Region {
    /// An area that already exists (by id).
    Area(i64),
    /// A wall-enclosed face no area covers yet.
    Face(EnclosedFace),
}

/// The enclosed faces of the exterior walls, kept until a wall changes (the
/// hover asks on every move).
#[derive(Debug, Default)]
pub struct RegionCache {
    key: u64,
    faces: Vec<EnclosedFace>,
}

fn same_point(a: Pt, b: Pt, tolerance: f64) -> bool {
    (a.x - b.x).abs() <= tolerance && (a.y - b.y).abs() <= tolerance
}

/// `polygonRingsEqual`: the same ring from any start, either way round.
fn rings_equal(a: &[Pt], b: &[Pt], tolerance: f64) -> bool {
    if a.len() < 3 || b.len() < 3 || a.len() != b.len() {
        return false;
    }
    let n = a.len();
    (0..n).any(|offset| {
        if !same_point(a[0], b[offset], tolerance) {
            return false;
        }
        let forward = (1..n).all(|i| same_point(a[i], b[(offset + i) % n], tolerance));
        let reverse = (1..n).all(|i| same_point(a[i], b[(offset + n - i) % n], tolerance));
        forward || reverse
    })
}

/// A polygon's ring with its curved edges flattened.
pub fn ring_of(d: &Drawing, poly: &Polygon) -> Vec<Pt> {
    d.engine().flatten_ring(&poly.points, &poly.edges, 0.05)
}

fn exterior_faces<'a>(
    d: &Drawing,
    gates: &LayerGates,
    cache: &'a mut RegionCache,
) -> &'a [EnclosedFace] {
    use std::hash::{Hash, Hasher};
    let walls: Vec<&SketchLine> = d
        .lines
        .iter()
        .filter(|l| {
            l.kind == WallType::Exterior
                && gates.shows(l.id)
                && sketch_core::geometry::distance(l.start, l.end) > 1e-6
        })
        .collect();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for l in &walls {
        (l.id, l.start.x.to_bits(), l.start.y.to_bits()).hash(&mut hasher);
        (l.end.x.to_bits(), l.end.y.to_bits(), l.bow_angle.to_bits()).hash(&mut hasher);
    }
    let key = hasher.finish() | 1;
    if cache.key != key {
        let geometry: Vec<_> = walls.iter().map(|l| l.geometry()).collect();
        cache.faces = d.engine().find_enclosed_faces(&geometry, 1.0, 0.02);
        cache.key = key;
    }
    &cache.faces
}

/// `resolveRegionAt(worldPt)`: the existing area to use, or the wall face to
/// materialize, or neither.
pub fn resolve(
    d: &Drawing,
    gates: &LayerGates,
    cache: &mut RegionCache,
    world: Pt,
) -> Option<Region> {
    let by_area = |p: &Polygon| p.area.abs();
    let smallest = d
        .polygons
        .iter()
        .filter(|p| p.points.len() >= 3 && gates.shows(p.id))
        .filter(|p| d.engine().point_in_polygon(world, &ring_of(d, p)))
        .min_by(|a, b| by_area(a).total_cmp(&by_area(b)));
    let face = exterior_faces(d, gates, cache)
        .iter()
        .filter(|f| d.engine().point_in_polygon(world, &f.points))
        .min_by(|a, b| a.area.total_cmp(&b.area))
        .cloned();
    let Some(face) = face else {
        return smallest.map(|p| Region::Area(p.id));
    };
    if let Some(same) = d
        .polygons
        .iter()
        .filter(|p| gates.shows(p.id))
        .find(|p| rings_equal(&p.points, &face.points, 0.02))
    {
        return Some(Region::Area(same.id));
    }
    if let Some(p) = smallest {
        if by_area(p) <= face.area + 1e-6 || !gates.selectable(p.id) {
            return Some(Region::Area(p.id));
        }
    }
    Some(Region::Face(face))
}

/// The ring a region would be painted with.
pub fn region_ring(d: &Drawing, region: &Region) -> Vec<Pt> {
    match region {
        Region::Area(id) => d
            .polygons
            .iter()
            .find(|p| p.id == *id)
            .map(|p| ring_of(d, p))
            .unwrap_or_default(),
        Region::Face(face) => face.points.clone(),
    }
}

/// `ownBoundaryLine(poly, index)`: give edge `index` a wall of its own running
/// exactly along it, styled like the wall it borrowed.
fn own_boundary_line(d: &mut Drawing, poly_index: usize, index: usize) {
    let (edge, start, end, poly_layer) = {
        let poly = &d.polygons[poly_index];
        (
            poly.edges[index],
            poly.points[index],
            poly.points[(index + 1) % poly.points.len()],
            poly.layer_id.clone(),
        )
    };
    let source = edge
        .line_id
        .and_then(|id| d.lines.iter().find(|l| l.id == id))
        .cloned();
    let id = d.gen_id();
    let mut line = source.unwrap_or(SketchLine {
        id,
        start,
        end,
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
    line.id = id;
    line.start = start;
    line.end = end;
    line.bow_angle = edge.bow_angle;
    line.dimension_position = None;
    line.dimension_anchor = None;
    line.arc_height = if edge.bow_angle != 0.0 {
        let chord = sketch_core::geometry::distance(start, end);
        chord / 2.0 * (edge.bow_angle * std::f64::consts::PI / 720.0).tan()
    } else {
        0.0
    };
    if poly_layer.is_some() {
        line.layer_id = poly_layer;
    }
    d.polygons[poly_index].edges[index] = sketch_core::geometry::Edge {
        line_id: Some(id),
        bow_angle: line.bow_angle,
    };
    d.lines.push(line);
}

/// `edgeBorrowsPartOfWall(poly, index)`.
fn edge_borrows_part_of_wall(d: &Drawing, poly: &Polygon, index: usize) -> bool {
    let Some(line) = poly
        .edges
        .get(index)
        .and_then(|e| e.line_id)
        .and_then(|id| d.lines.iter().find(|l| l.id == id))
    else {
        return false;
    };
    let (start, end) = (
        poly.points[index],
        poly.points[(index + 1) % poly.points.len()],
    );
    let exact = (same_point(line.start, start, 1e-4) && same_point(line.end, end, 1e-4))
        || (same_point(line.start, end, 1e-4) && same_point(line.end, start, 1e-4));
    !exact
}

/// `regionAreaAt`'s materialization: the face becomes an undefined area, and an
/// edge that runs along only part of a wall gets a wall of its own. Returns the
/// new area's id.
pub fn materialize(d: &mut Drawing, face: &EnclosedFace) -> i64 {
    let id = d.gen_id();
    let edges: Vec<_> = face
        .edges
        .iter()
        .map(|e| sketch_core::geometry::Edge {
            line_id: e.line_id,
            bow_angle: 0.0,
        })
        .collect();
    let mut poly = Polygon {
        id,
        points: face.points.clone(),
        edges,
        kind: "UND".into(),
        name: "Undefined Area".into(),
        gross: face.area,
        area: face.area,
        factor_override: None,
        factor: 1.0,
        measure_to: "center".into(),
        auto_post: d.auto_post.clone(),
        attributes: Default::default(),
        subtract_mode: if d.options.auto_subtract_overlapped {
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
    };
    // A face always names the wall under each side; pad a short list.
    poly.edges
        .resize(poly.points.len(), sketch_core::geometry::Edge::default());
    d.polygons.push(poly);
    let index = d.polygons.len() - 1;
    for edge in 0..d.polygons[index].points.len() {
        if edge_borrows_part_of_wall(d, &d.polygons[index].clone(), edge) {
            own_boundary_line(d, index, edge);
        }
    }
    id
}

/// An auto-generated area layer ("Area", "Area 2") picks up the area's name
/// once it is defined (`/^Area( \d+)?$/`).
pub fn is_generated_layer_name(name: &str) -> bool {
    match name.strip_prefix("Area") {
        Some("") => true,
        Some(rest) => rest
            .strip_prefix(' ')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
        None => false,
    }
}

/// `defineAreaTarget`'s data half: the code, the name and a factor of at
/// least the default.
pub fn define(d: &mut Drawing, id: i64, code: &str, name: &str) -> bool {
    let Some(poly) = d.polygons.iter_mut().find(|p| p.id == id) else {
        return false;
    };
    poly.kind = code.to_string();
    poly.name = name.to_string();
    if poly.factor == 0.0 {
        poly.factor = 1.0;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_match_from_any_start_and_direction() {
        let a = [Pt::new(0.0, 0.0), Pt::new(4.0, 0.0), Pt::new(4.0, 3.0)];
        let b = [Pt::new(4.0, 3.0), Pt::new(4.0, 0.0), Pt::new(0.0, 0.0)];
        let c = [Pt::new(4.0, 0.0), Pt::new(4.0, 3.0), Pt::new(0.0, 0.0)];
        assert!(rings_equal(&a, &b, 0.02), "reversed");
        assert!(rings_equal(&a, &c, 0.02), "rotated");
        assert!(!rings_equal(&a, &[a[0], a[1], Pt::new(4.0, 3.5)], 0.02));
        assert!(!rings_equal(&a, &a[..2], 0.02));
    }

    #[test]
    fn only_generated_layer_names_are_renamed() {
        for yes in ["Area", "Area 2", "Area 12"] {
            assert!(is_generated_layer_name(yes), "{yes}");
        }
        for no in ["Areas", "Area x", "Area ", "Kitchen", "area"] {
            assert!(!is_generated_layer_name(no), "{no}");
        }
    }
}
