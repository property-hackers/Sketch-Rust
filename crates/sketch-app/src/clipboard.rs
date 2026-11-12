//! Clipboard and delete (screen 6d): `copySelected`, `cutSelected`, `paste`
//! and `deleteSelected` (refs/sketch-app.js:2248-2340, 2526-2600).
//!
//! `promptOnDeletion` defaults to false (refs/state-init.js:88), so Delete
//! never asks first — the confirm dialog belongs to the Options screen.

use std::collections::{HashMap, HashSet};

use sketch_core::geometry::{Edge, Pt};
use sketch_core::model::{Polygon, SketchLine};

use crate::drawing::Drawing;
use crate::selection::Selection;

/// `state.clipboard`: deep copies of what was copied.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Clipboard {
    pub lines: Vec<SketchLine>,
    pub polygons: Vec<Polygon>,
    /// `labels`: the free text labels.
    pub texts: Vec<crate::placed_text::PlacedText>,
    pub symbols: Vec<crate::placed_symbol::PlacedSymbol>,
}

/// `pointKey`: five decimal places, so coincident points compare equal.
fn point_key(p: Pt) -> String {
    format!("{:.5},{:.5}", p.x, p.y)
}

/// `edgeKey`: the two endpoints sorted, so direction does not matter.
fn edge_key(a: Pt, b: Pt) -> String {
    let (ka, kb) = (point_key(a), point_key(b));
    if ka <= kb {
        format!("{ka}|{kb}")
    } else {
        format!("{kb}|{ka}")
    }
}

/// `copySelected`: the selected walls and areas, plus the boundary walls of
/// any selected area (so a copied area keeps its outline), and the selected
/// text labels and symbols.
pub fn copy(d: &Drawing, sel: &Selection) -> Option<Clipboard> {
    let polygons: Vec<Polygon> = d
        .polygons
        .iter()
        .filter(|p| sel.polys.contains(&p.id))
        .cloned()
        .collect();
    let boundary: HashSet<i64> = polygons
        .iter()
        .flat_map(|p| p.edges.iter().filter_map(|e| e.line_id))
        .collect();
    let lines: Vec<SketchLine> = d
        .lines
        .iter()
        .filter(|l| sel.lines.contains(&l.id) || boundary.contains(&l.id))
        .cloned()
        .collect();
    let texts: Vec<crate::placed_text::PlacedText> = d
        .texts
        .iter()
        .filter(|t| sel.texts.contains(&t.id))
        .cloned()
        .collect();
    let symbols: Vec<crate::placed_symbol::PlacedSymbol> = d
        .symbols
        .iter()
        .filter(|s| sel.symbols.contains(&s.id))
        .cloned()
        .collect();
    if lines.is_empty() && polygons.is_empty() && texts.is_empty() && symbols.is_empty() {
        return None;
    }
    Some(Clipboard {
        lines,
        polygons,
        texts,
        symbols,
    })
}

/// `paste`: everything lands 2 ft down-right of the original, keeps its
/// internal wall references, and comes back selected.
pub fn paste(d: &mut Drawing, clip: &Clipboard) -> Selection {
    // `const offset = 2;` — feet, not pixels.
    const OFFSET: f64 = 2.0;
    let mut out = Selection::default();

    let mut line_ids: HashMap<i64, i64> = HashMap::new();
    for line in &clip.lines {
        let new_id = d.gen_id();
        line_ids.insert(line.id, new_id);
        let mut copy = line.clone();
        copy.id = new_id;
        copy.forget_sketchy_id();
        copy.start = Pt::new(line.start.x + OFFSET, line.start.y + OFFSET);
        copy.end = Pt::new(line.end.x + OFFSET, line.end.y + OFFSET);
        d.lines.push(copy);
        out.lines.insert(new_id);
    }

    let mut poly_ids: HashMap<i64, i64> = HashMap::new();
    for poly in &clip.polygons {
        poly_ids.insert(poly.id, d.gen_id());
    }
    for poly in &clip.polygons {
        let mut copy = poly.clone();
        copy.id = poly_ids[&poly.id];
        copy.forget_sketchy_id();
        copy.points = poly
            .points
            .iter()
            .map(|p| Pt::new(p.x + OFFSET, p.y + OFFSET))
            .collect();
        copy.edges = poly
            .edges
            .iter()
            .map(|e| Edge {
                line_id: e.line_id.and_then(|id| line_ids.get(&id).copied()),
                bow_angle: e.bow_angle,
            })
            .collect();
        // A pasted area only keeps a subtraction target that was copied with
        // it (`containerId`/`subtractedFromId` are not modelled yet).
        copy.subtract_target_id = poly
            .subtract_target_id
            .and_then(|id| poly_ids.get(&id).copied());
        // Any edge that lost its wall is rebuilt from the copied outline.
        if copy.edges.len() != copy.points.len() || copy.edges.iter().any(|e| e.line_id.is_none()) {
            copy.edges = Polygon::build_edges(&copy.points, &d.lines);
        }
        // Blocks that sit where they were put move with the copy.
        let shift = |p: Pt| Pt::new(p.x + OFFSET, p.y + OFFSET);
        copy.label_position = copy.label_position.map(shift);
        copy.area_size_position = copy.area_size_position.map(shift);
        out.polys.insert(copy.id);
        d.polygons.push(copy);
    }

    for text in &clip.texts {
        let mut copy = text.clone();
        copy.id = d.gen_id();
        copy.forget_sketchy_id();
        copy.x += OFFSET;
        copy.y += OFFSET;
        out.texts.insert(copy.id);
        d.texts.push(copy);
    }
    for symbol in &clip.symbols {
        let mut copy = symbol.clone();
        copy.id = d.gen_id();
        copy.forget_sketchy_id();
        copy.x += OFFSET;
        copy.y += OFFSET;
        // A copied opening follows the copy of its wall when that was copied
        // too; otherwise it detaches rather than snapping back onto the
        // original wall the next time something moves.
        match copy.wall_id.and_then(|id| line_ids.get(&id).copied()) {
            Some(new_wall) => copy.wall_id = Some(new_wall),
            None => {
                copy.wall_id = None;
                copy.wall_fraction = None;
                copy.wall_side_explicit = false;
            }
        }
        out.symbols.insert(copy.id);
        d.symbols.push(copy);
    }
    out
}

/// `deleteSelected`: removes the selected walls and areas under the
/// original's rules.
///
/// * An explicitly selected wall always goes — deleting a boundary wall
///   reopens its area.
/// * An area goes if it is selected, if one of its own walls is explicitly
///   deleted, or if it runs along an explicitly deleted wall.
/// * An area is never removed merely because a *deleted area's* outline
///   coincides with it: deleting a second floor drawn over the first floor
///   used to wipe out the first floor too.
/// * A wall a surviving area still uses is kept.
/// * A selected dimension chain (screen 9) goes, and it alone doesn't touch
///   the walls/areas under it (the original filters `dimensionChains` first).
pub fn delete(d: &mut Drawing, sel: &Selection) -> bool {
    let selected_chains: HashSet<i64> = sel.chains.iter().copied().collect();
    if selected_chains.is_empty() && sel.lines.is_empty() && sel.polys.is_empty() {
        return false;
    }
    let selected_edges: HashSet<String> = d
        .lines
        .iter()
        .filter(|l| sel.lines.contains(&l.id))
        .map(|l| edge_key(l.start, l.end))
        .collect();

    let mut area_line_ids: HashSet<i64> = HashSet::new();
    let mut area_edges: HashSet<String> = HashSet::new();
    for poly in d.polygons.iter().filter(|p| sel.polys.contains(&p.id)) {
        for edge in &poly.edges {
            if let Some(id) = edge.line_id {
                area_line_ids.insert(id);
            }
        }
        for i in 0..poly.points.len() {
            let a = poly.points[i];
            let b = poly.points[(i + 1) % poly.points.len()];
            area_edges.insert(edge_key(a, b));
        }
    }

    let removed: HashSet<i64> = d
        .polygons
        .iter()
        .filter(|poly| {
            if sel.polys.contains(&poly.id) {
                return true;
            }
            if poly
                .edges
                .iter()
                .any(|e| e.line_id.is_some_and(|id| sel.lines.contains(&id)))
            {
                return true;
            }
            (0..poly.points.len()).any(|i| {
                let a = poly.points[i];
                let b = poly.points[(i + 1) % poly.points.len()];
                selected_edges.contains(&edge_key(a, b))
            })
        })
        .map(|p| p.id)
        .collect();

    d.reopened_areas
        .retain(|record| !sel.polys.contains(&record.original.id));
    for original in d
        .polygons
        .iter()
        .filter(|p| removed.contains(&p.id) && !sel.polys.contains(&p.id))
    {
        if !d
            .reopened_areas
            .iter()
            .any(|record| record.original.id == original.id)
        {
            d.reopened_areas.push(crate::area_repair::ReopenedArea {
                original: original.clone(),
                first_replacement_id: d.next_id(),
            });
        }
    }

    // Walls a surviving area still uses.
    let kept: HashSet<i64> = d
        .polygons
        .iter()
        .filter(|p| !removed.contains(&p.id))
        .flat_map(|p| p.edges.iter().filter_map(|e| e.line_id))
        .collect();

    d.lines.retain(|line| {
        if sel.lines.contains(&line.id) {
            return false;
        }
        if kept.contains(&line.id) {
            return true;
        }
        !(area_line_ids.contains(&line.id) || area_edges.contains(&edge_key(line.start, line.end)))
    });
    d.polygons.retain(|p| !removed.contains(&p.id));
    d.dimension_chains
        .retain(|chain| !selected_chains.contains(&chain.id));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketch_core::model::WallType;

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

    /// A closed 10x10 square as four walls plus its area, the shape the
    /// original's own auto-close produces.
    fn square() -> (Drawing, i64) {
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
        let edges = Polygon::build_edges(&points, &d.lines);
        let poly = Polygon {
            id: 100,
            points,
            edges,
            kind: "UND".into(),
            name: "Undefined Area".into(),
            area: 100.0,
            ..Polygon {
                id: 0,
                points: Vec::new(),
                edges: Vec::new(),
                kind: String::new(),
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
        };
        d.polygons = vec![poly];
        d.next_id_for_tests(200);
        (d, 100)
    }

    fn select_lines(ids: &[i64]) -> Selection {
        Selection {
            lines: ids.iter().copied().collect(),
            ..Selection::default()
        }
    }

    #[test]
    fn copying_an_area_takes_its_boundary_walls() {
        let (d, poly_id) = square();
        let sel = Selection {
            polys: [poly_id].into_iter().collect(),
            ..Selection::default()
        };
        let clip = copy(&d, &sel).expect("clipboard");
        assert_eq!(clip.polygons.len(), 1);
        assert_eq!(clip.lines.len(), 4, "the outline comes along");
        // Copying nothing yields nothing.
        assert!(copy(&d, &Selection::default()).is_none());
    }

    #[test]
    fn paste_offsets_by_two_feet_and_selects_the_copies() {
        let (mut d, poly_id) = square();
        let sel = Selection {
            polys: [poly_id].into_iter().collect(),
            ..Selection::default()
        };
        let clip = copy(&d, &sel).expect("clipboard");
        let pasted = paste(&mut d, &clip);
        assert_eq!(d.lines.len(), 8);
        assert_eq!(d.polygons.len(), 2);
        assert_eq!(pasted.lines.len(), 4);
        assert_eq!(pasted.polys.len(), 1);
        // The copy sits 2 ft down-right.
        let new_poly = d.polygons.iter().find(|p| p.id != poly_id).expect("copy");
        assert_eq!(new_poly.points[0], Pt::new(2.0, 2.0));
        // Its edges point at the copied walls, not the originals.
        for edge in &new_poly.edges {
            let id = edge.line_id.expect("edge keeps a wall");
            assert!(pasted.lines.contains(&id), "edge {id} is a copied wall");
        }
        assert_eq!(d.lines.len(), 8, "no extra walls were invented");
    }

    #[test]
    fn pasted_copies_do_not_reuse_sketchy_ids() {
        let (mut d, poly_id) = square();
        for line in &mut d.lines {
            line.extra.insert("sketchy_id".into(), format!("wall-{}", line.id).into());
            line.extra.insert("sketchy_display".into(), true.into());
        }
        d.polygons[0].extra.insert("sketchy_id".into(), "space-a".into());
        d.polygons[0].extra.insert("sketchy_index".into(), 3.into());
        let sel = Selection {
            polys: [poly_id].into_iter().collect(),
            ..Selection::default()
        };
        let clip = copy(&d, &sel).expect("clipboard");
        let pasted = paste(&mut d, &clip);
        let copies = d.lines.iter().filter(|l| pasted.lines.contains(&l.id));
        for line in copies {
            assert!(!line.extra.contains_key("sketchy_id"), "line {}", line.id);
            assert!(line.extra.contains_key("sketchy_display"), "other residue stays");
        }
        let copy = d.polygons.iter().find(|p| p.id != poly_id).expect("copy");
        assert!(!copy.extra.contains_key("sketchy_id"));
        assert!(copy.extra.contains_key("sketchy_index"));
        let original = d.polygons.iter().find(|p| p.id == poly_id).expect("original");
        assert_eq!(original.extra["sketchy_id"], "space-a", "the original keeps its id");
    }

    #[test]
    fn deleting_a_boundary_wall_reopens_its_area() {
        let (mut d, poly_id) = square();
        assert!(delete(&mut d, &select_lines(&[1])));
        assert_eq!(d.lines.len(), 3);
        assert!(d.polygons.is_empty(), "the area goes with its wall");
        assert_eq!(poly_id, 100);
    }

    #[test]
    fn deleting_an_area_takes_its_outline_but_keeps_walls_another_area_uses() {
        let (mut d, poly_id) = square();
        // A second area sharing wall 1 (the "second floor" case).
        let points = vec![
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            Pt::new(10.0, -10.0),
            Pt::new(0.0, -10.0),
        ];
        let edges = Polygon::build_edges(&points, &d.lines);
        let second = Polygon {
            id: 101,
            points,
            edges,
            kind: "UND".into(),
            ..d.polygons[0].clone()
        };
        d.polygons.push(second);

        let sel = Selection {
            polys: [poly_id].into_iter().collect(),
            ..Selection::default()
        };
        assert!(delete(&mut d, &sel));
        assert_eq!(d.polygons.len(), 1, "only the selected area goes");
        assert_eq!(d.polygons[0].id, 101);
        // Wall 1 is shared by the survivor, so it stays even though the
        // deleted area ran along it; walls only the deleted area used go.
        let ids: Vec<i64> = d.lines.iter().map(|l| l.id).collect();
        assert!(ids.contains(&1), "the shared wall survives");
        assert!(!ids.contains(&2) && !ids.contains(&3) && !ids.contains(&4));
    }

    #[test]
    fn deleting_nothing_reports_nothing_to_do() {
        let (mut d, _) = square();
        assert!(!delete(&mut d, &Selection::default()));
        assert_eq!(d.lines.len(), 4);
    }
}
