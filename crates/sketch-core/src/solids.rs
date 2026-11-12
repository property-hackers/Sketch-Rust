//! Wall solids — `window.WallSolids.computeWallSolids` and the junction
//! classification behind it (refs/area-breakdown.js:737-930).
//!
//! A wall solid is the wall's full geometric thickness drawn as a filled
//! quad instead of a centreline stroke. Each end is classified so corners
//! fill, collinear runs continue, and butt walls stop at the through wall's
//! face:
//!
//! * `Free`   — unconnected end, no extension.
//! * `T`      — the endpoint lands inside another wall's span, so the solid
//!   stops at that wall's near face (negative extension,
//!   `-(host/2)/sin(angle)` clamped at 0.25).
//! * `Inline` — collinear continuation, extends over the mate's half
//!   thickness.
//! * `Corner` — L/X node, extends over the largest mate half thickness so the
//!   corner fills.
//!
//! Hosted openings (doors/windows with a `wallGapWidth`) cut gaps out of the
//! solid, so the opening passes through the full wall thickness.

use crate::geometry::{distance, Pt};
use crate::model::SketchLine;

/// `EPS` — lengths below this are nothing.
const EPS: f64 = 1e-6;
/// `CLUSTER_TOL` — endpoints closer than this (feet) share a node.
const CLUSTER_TOL: f64 = 0.02;

/// `WallSolids.offsetRing`: per-edge outward offsets with bounded miter joins.
/// Invalid or collapsed rings fall back to the caller's original geometry.
pub fn offset_ring(points: &[Pt], offsets: &[f64]) -> Option<Vec<Pt>> {
    let n = points.len();
    if n < 3 || offsets.len() != n {
        return None;
    }
    if offsets.iter().all(|o| o.abs() < EPS) {
        return Some(points.to_vec());
    }
    let area = |ring: &[Pt]| -> f64 {
        (0..ring.len())
            .map(|i| {
                let a = ring[i];
                let b = ring[(i + 1) % ring.len()];
                a.x * b.y - b.x * a.y
            })
            .sum::<f64>()
            / 2.0
    };
    let clockwise = area(points) < 0.0;
    let mut ring = points.to_vec();
    if clockwise {
        ring.reverse();
    }
    let mut edges = Vec::with_capacity(n);
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let length = distance(a, b);
        if length < EPS {
            return None;
        }
        let ux = (b.x - a.x) / length;
        let uy = (b.y - a.y) / length;
        let offset = offsets[if clockwise { (2 * n - 2 - i) % n } else { i }];
        edges.push((ux, uy, uy, -ux, offset));
    }
    let limit = 4.0 * offsets.iter().fold(0.01_f64, |m, o| m.max(o.abs()));
    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        let prev = edges[(i + n - 1) % n];
        let next = edges[i];
        let origin = ring[i];
        let a = ring[(i + n - 1) % n];
        let p = Pt::new(a.x + prev.2 * prev.4, a.y + prev.3 * prev.4);
        let q = Pt::new(origin.x + next.2 * next.4, origin.y + next.3 * next.4);
        let cross = prev.0 * next.1 - prev.1 * next.0;
        let vertex = if cross.abs() < 1e-7 {
            let t = (origin.x - p.x) * prev.0 + (origin.y - p.y) * prev.1;
            Pt::new(p.x + prev.0 * t, p.y + prev.1 * t)
        } else {
            let t = ((q.x - p.x) * next.2 + (q.y - p.y) * next.3) / cross;
            let mut v = Pt::new(p.x + prev.0 * t, p.y + prev.1 * t);
            let length = distance(v, origin);
            if length > limit && length > EPS {
                v = Pt::new(
                    origin.x + (v.x - origin.x) * limit / length,
                    origin.y + (v.y - origin.y) * limit / length,
                );
            }
            v
        };
        // Keep malformed imported coordinates out of the calculation pipeline.
        if !vertex.x.is_finite() || !vertex.y.is_finite() {
            return None;
        }
        result.push(vertex);
    }
    if area(&result).signum() != area(&ring).signum() || area(&result).abs() < EPS {
        return None;
    }
    for i in 0..n {
        let j = (i + 1) % n;
        if (result[j].x - result[i].x) * (ring[j].x - ring[i].x)
            + (result[j].y - result[i].y) * (ring[j].y - ring[i].y)
            <= 0.0
        {
            return None;
        }
    }
    if clockwise {
        result.reverse();
    }
    Some(result)
}

/// How a hosted opening anchors along its wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WallSnap {
    /// Anchored at the hinge (single doors).
    Side,
    /// Anchored at the middle of the opening.
    #[default]
    Center,
}

/// A symbol hosted on a wall that cuts a gap in its solid. Only the fields
/// the solid geometry needs (`wallId`, `wallGapWidth`, snap/flip).
#[derive(Debug, Clone, PartialEq)]
pub struct Opening {
    pub wall_id: i64,
    pub x: f64,
    pub y: f64,
    /// `wallGapWidth` in feet; 0 or absent cuts nothing.
    pub gap_width: f64,
    pub snap: WallSnap,
    /// `flipH` — Flip Swing.
    pub flip_h: bool,
    /// `wallSideFlip`.
    pub side_flip: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JunctionKind {
    Free,
    T,
    Inline,
    Corner,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Junction {
    pub line_id: i64,
    /// `true` for the wall's start end.
    pub start: bool,
    pub kind: JunctionKind,
}

/// One wall's solid: `pieces` are filled quads (4 points each, in order).
#[derive(Debug, Clone, PartialEq)]
pub struct WallSolid {
    pub line_id: i64,
    pub interior: bool,
    pub pieces: Vec<Vec<Pt>>,
}

/// A side face of a solid, used as a snap target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Face {
    pub line_id: i64,
    /// +1 / -1 (the normal's sign).
    pub side: i32,
    pub start: Pt,
    pub end: Pt,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WallSolids {
    pub solids: Vec<WallSolid>,
    pub faces: Vec<Face>,
    pub junctions: Vec<Junction>,
}

#[derive(Debug, Clone, Copy)]
struct Metrics {
    dx: f64,
    dy: f64,
    length: f64,
}

fn segment_metrics(line: &SketchLine) -> Option<Metrics> {
    let dx = line.end.x - line.start.x;
    let dy = line.end.y - line.start.y;
    let length = dx.hypot(dy);
    if length < EPS {
        return None;
    }
    Some(Metrics {
        dx: dx / length,
        dy: dy / length,
        length,
    })
}

/// `isStraight`: a bow beyond half a degree is a curve (its solid is drawn
/// as an annular sector by the renderer instead).
pub fn is_straight(line: &SketchLine) -> bool {
    !(line.bow_angle != 0.0 && line.bow_angle.abs() > 0.5)
}

/// Signed projection of `point` onto a segment's interior; `None` outside.
fn interior_projection(line: &SketchLine, m: &Metrics, point: Pt) -> Option<(f64, f64)> {
    let px = point.x - line.start.x;
    let py = point.y - line.start.y;
    let along = px * m.dx + py * m.dy;
    if along <= CLUSTER_TOL || along >= m.length - CLUSTER_TOL {
        return None;
    }
    let across = px * -m.dy + py * m.dx;
    if across.abs() > CLUSTER_TOL {
        return None;
    }
    Some((along, across))
}

fn classify_junction(
    line: &SketchLine,
    m: &Metrics,
    at_start: bool,
    thickness: f64,
    lines: &[&SketchLine],
    metrics_by_line: &[(i64, Metrics)],
    thickness_of: &dyn Fn(&SketchLine) -> f64,
) -> (JunctionKind, f64) {
    let point = if at_start { line.start } else { line.end };

    // T: the endpoint lands inside another wall's span.
    let mut host: Option<(usize, f64)> = None;
    for (i, other) in lines.iter().enumerate() {
        if other.id == line.id {
            continue;
        }
        let Some(om) = metrics_by_line
            .iter()
            .find(|(id, _)| *id == other.id)
            .map(|(_, m)| *m)
        else {
            continue;
        };
        let Some((_, across)) = interior_projection(other, &om, point) else {
            continue;
        };
        if host.is_none_or(|(_, best)| across.abs() < best.abs()) {
            host = Some((i, across));
        }
    }
    if let Some((i, _)) = host {
        let other = lines[i];
        let om = metrics_by_line
            .iter()
            .find(|(id, _)| *id == other.id)
            .map(|(_, m)| *m)
            .unwrap();
        let host_thickness = thickness_of(other);
        let sine = (m.dx * om.dy - m.dy * om.dx).abs();
        let clamped = sine.clamp(0.25, 1.0);
        return (JunctionKind::T, -(host_thickness / 2.0) / clamped);
    }

    // Endpoint cluster: every other wall sharing this node.
    let mates: Vec<&SketchLine> = lines
        .iter()
        .copied()
        .filter(|other| {
            other.id != line.id
                && (distance(other.start, point) <= CLUSTER_TOL
                    || distance(other.end, point) <= CLUSTER_TOL)
        })
        .collect();
    if mates.is_empty() {
        return (JunctionKind::Free, 0.0);
    }
    let mut extension = thickness / 2.0;
    for mate in &mates {
        extension = extension.max(thickness_of(mate) / 2.0);
    }
    let collinear_only = mates.iter().all(|mate| {
        metrics_by_line
            .iter()
            .find(|(id, _)| *id == mate.id)
            .map(|(_, mm)| (m.dx * mm.dy - m.dy * mm.dx).abs() < 0.02)
            .unwrap_or(false)
    });
    (
        if collinear_only {
            JunctionKind::Inline
        } else {
            JunctionKind::Corner
        },
        extension,
    )
}

/// `gapSpan` — the axial span an opening cuts, clipped to the wall.
fn gap_span(opening: &Opening, along: f64, length: f64) -> (f64, f64) {
    let width = opening.gap_width;
    match opening.snap {
        WallSnap::Side => {
            let dir = if opening.side_flip { -1.0 } else { 1.0 }
                * if opening.flip_h { -1.0 } else { 1.0 };
            if dir > 0.0 {
                (along.max(0.0), (along + width).min(length))
            } else {
                ((along - width).max(0.0), along.min(length))
            }
        }
        WallSnap::Center => {
            let half = width / 2.0;
            ((along - half).max(0.0), (along + half).min(length))
        }
    }
}

fn wall_gaps(line: &SketchLine, m: &Metrics, openings: &[Opening]) -> Vec<(f64, f64)> {
    let mut gaps: Vec<(f64, f64)> = openings
        .iter()
        .filter(|o| o.wall_id == line.id && o.gap_width > 0.0)
        .map(|o| {
            let along = (o.x - line.start.x) * m.dx + (o.y - line.start.y) * m.dy;
            gap_span(o, along, m.length)
        })
        .filter(|(s, e)| *e > *s)
        .collect();
    gaps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    gaps
}

/// One filled quad of a wall solid between two axial positions.
fn push_piece(
    pieces: &mut Vec<Vec<Pt>>,
    point_at: &dyn Fn(f64, f64) -> Pt,
    a0: f64,
    a1: f64,
    half: f64,
) {
    if a1 - a0 < EPS {
        return;
    }
    pieces.push(vec![
        point_at(a0, half),
        point_at(a1, half),
        point_at(a1, -half),
        point_at(a0, -half),
    ]);
}

/// Visible fractions of a straight wall after hosted doors/windows cut gaps.
/// Empty means an uninterrupted wall, matching the export model convention.
pub fn visible_runs(line: &SketchLine, openings: &[Opening]) -> Vec<(f64, f64)> {
    if !is_straight(line) {
        return Vec::new();
    }
    let Some(m) = segment_metrics(line) else {
        return Vec::new();
    };
    let gaps = wall_gaps(line, &m, openings);
    if gaps.is_empty() {
        return Vec::new();
    }
    let mut runs = Vec::new();
    let mut cursor = 0.0_f64;
    for (a, b) in gaps {
        if a > cursor {
            runs.push((cursor / m.length, a / m.length));
        }
        cursor = cursor.max(b);
    }
    if cursor < m.length {
        runs.push((cursor / m.length, 1.0));
    }
    if runs.is_empty() {
        runs.push((0.0, 0.0));
    }
    runs
}

/// `computeWallSolids(lines, thicknessOf, symbols)`.
pub fn compute_wall_solids(
    lines: &[SketchLine],
    thickness_of: &dyn Fn(&SketchLine) -> f64,
    openings: &[Opening],
) -> WallSolids {
    let straight: Vec<&SketchLine> = lines
        .iter()
        .filter(|line| is_straight(line) && thickness_of(line) > 0.0)
        .collect();
    let metrics_by_line: Vec<(i64, Metrics)> = straight
        .iter()
        .filter_map(|line| segment_metrics(line).map(|m| (line.id, m)))
        .collect();

    let mut out = WallSolids::default();
    for line in &straight {
        let Some(m) = metrics_by_line
            .iter()
            .find(|(id, _)| *id == line.id)
            .map(|(_, m)| *m)
        else {
            continue;
        };
        let thickness = thickness_of(line);
        let half = thickness / 2.0;
        let (nx, ny) = (-m.dy, m.dx);

        let (start_kind, start_ext) = classify_junction(
            line,
            &m,
            true,
            thickness,
            &straight,
            &metrics_by_line,
            thickness_of,
        );
        let (end_kind, end_ext) = classify_junction(
            line,
            &m,
            false,
            thickness,
            &straight,
            &metrics_by_line,
            thickness_of,
        );
        out.junctions.push(Junction {
            line_id: line.id,
            start: true,
            kind: start_kind,
        });
        out.junctions.push(Junction {
            line_id: line.id,
            start: false,
            kind: end_kind,
        });

        let solid_start = -start_ext;
        let solid_end = m.length + end_ext.max(0.0);
        if solid_end - solid_start < EPS {
            continue;
        }

        let point_at = |along: f64, across: f64| {
            Pt::new(
                line.start.x + m.dx * along + nx * across,
                line.start.y + m.dy * along + ny * across,
            )
        };

        let gaps: Vec<(f64, f64)> = wall_gaps(line, &m, openings)
            .into_iter()
            .map(|(s, e)| (s.max(solid_start), e.min(solid_end)))
            .filter(|(s, e)| e - s > EPS)
            .collect();

        let mut pieces: Vec<Vec<Pt>> = Vec::new();
        let mut cursor = solid_start;
        for (gs, ge) in &gaps {
            push_piece(&mut pieces, &point_at, cursor, *gs, half);
            cursor = cursor.max(*ge);
        }
        push_piece(&mut pieces, &point_at, cursor, solid_end, half);

        out.solids.push(WallSolid {
            line_id: line.id,
            interior: line.kind == crate::model::WallType::Interior,
            pieces,
        });
        out.faces.push(Face {
            line_id: line.id,
            side: 1,
            start: point_at(solid_start, half),
            end: point_at(solid_end, half),
        });
        out.faces.push(Face {
            line_id: line.id,
            side: -1,
            start: point_at(solid_start, -half),
            end: point_at(solid_end, -half),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::WallType;

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

    /// 6 inch walls (0.5 ft).
    fn six() -> impl Fn(&SketchLine) -> f64 {
        |_: &SketchLine| 0.5
    }

    #[test]
    fn a_free_wall_is_one_quad_of_its_thickness() {
        let walls = vec![line(1, 0.0, 0.0, 10.0, 0.0)];
        let out = compute_wall_solids(&walls, &six(), &[]);
        assert_eq!(out.solids.len(), 1);
        let solid = &out.solids[0];
        assert_eq!(solid.pieces.len(), 1);
        // 0.5 ft thick -> +/- 0.25 either side of the centreline.
        assert_eq!(
            solid.pieces[0],
            vec![
                Pt::new(0.0, 0.25),
                Pt::new(10.0, 0.25),
                Pt::new(10.0, -0.25),
                Pt::new(0.0, -0.25),
            ]
        );
        assert_eq!(out.junctions.len(), 2);
        assert_eq!(out.junctions[0].kind, JunctionKind::Free);
        assert_eq!(out.faces.len(), 2);
        assert_eq!(out.faces[0].side, 1);
        assert_eq!(out.faces[1].side, -1);
    }

    #[test]
    fn an_l_corner_extends_over_the_mates_half_thickness() {
        // (0,0)->(10,0) and (10,0)->(10,10) meet at a corner.
        let walls = vec![line(1, 0.0, 0.0, 10.0, 0.0), line(2, 10.0, 0.0, 10.0, 10.0)];
        let out = compute_wall_solids(&walls, &six(), &[]);
        let kinds: Vec<JunctionKind> = out.junctions.iter().map(|j| j.kind).collect();
        assert_eq!(
            kinds,
            vec![
                JunctionKind::Free,
                JunctionKind::Corner,
                JunctionKind::Corner,
                JunctionKind::Free
            ]
        );
        // The first wall's solid runs from 0 to 10.25 (extended at its end).
        let first = &out.solids[0];
        assert_eq!(first.pieces[0][1], Pt::new(10.25, 0.25));
        // The second wall starts 0.25 before its start point (its normal
        // points -x, so the +half face is at x 9.75).
        let second = &out.solids[1];
        assert_eq!(second.pieces[0][0], Pt::new(9.75, -0.25));
    }

    #[test]
    fn a_t_junction_stops_at_the_through_walls_face() {
        // A through wall along y=0, a butt wall arriving at (5,0) from below.
        let walls = vec![line(1, 0.0, 0.0, 10.0, 0.0), line(2, 5.0, 5.0, 5.0, 0.0)];
        let out = compute_wall_solids(&walls, &six(), &[]);
        // The butt wall's END lands inside wall 1's span: T with
        // -(0.25)/sin(90deg). The original only shortens at the START
        // (`solidEnd = length + Math.max(0, endJoin.extension)`), so at the
        // end the solid still reaches the centreline of the through wall.
        let butt = out.solids.iter().find(|s| s.line_id == 2).unwrap();
        let end = out
            .junctions
            .iter()
            .find(|j| j.line_id == 2 && !j.start)
            .unwrap();
        assert_eq!(end.kind, JunctionKind::T);
        assert_eq!(butt.pieces[0][2], Pt::new(4.75, 0.0));
        assert_eq!(butt.pieces[0][3], Pt::new(4.75, 5.0));

        // A T at the START does shorten: the same butt wall reversed.
        let walls = vec![line(1, 0.0, 0.0, 10.0, 0.0), line(2, 5.0, 0.0, 5.0, 5.0)];
        let out = compute_wall_solids(&walls, &six(), &[]);
        let start = out
            .junctions
            .iter()
            .find(|j| j.line_id == 2 && j.start)
            .unwrap();
        assert_eq!(start.kind, JunctionKind::T);
        let butt = out.solids.iter().find(|s| s.line_id == 2).unwrap();
        // solid_start = +0.25 -> the quad begins 0.25 up from (5,0).
        assert_eq!(butt.pieces[0][0], Pt::new(4.75, 0.25));
        assert_eq!(butt.pieces[0][3], Pt::new(5.25, 0.25));
    }

    #[test]
    fn a_collinear_run_extends_and_is_inline() {
        let walls = vec![line(1, 0.0, 0.0, 10.0, 0.0), line(2, 10.0, 0.0, 20.0, 0.0)];
        let out = compute_wall_solids(&walls, &six(), &[]);
        let kinds: Vec<JunctionKind> = out.junctions.iter().map(|j| j.kind).collect();
        assert_eq!(
            kinds,
            vec![
                JunctionKind::Free,
                JunctionKind::Inline,
                JunctionKind::Inline,
                JunctionKind::Free
            ]
        );
        assert_eq!(out.solids[0].pieces[0][1], Pt::new(10.25, 0.25));
        assert_eq!(out.solids[1].pieces[0][0], Pt::new(9.75, 0.25));
    }

    #[test]
    fn visible_runs_merge_overlaps_and_a_full_opening_stays_empty() {
        let wall = line(1, 0.0, 0.0, 10.0, 0.0);
        let mut opening = Opening {
            wall_id: 1,
            x: 5.0,
            y: 0.0,
            gap_width: 3.0,
            snap: WallSnap::Center,
            flip_h: false,
            side_flip: false,
        };
        assert_eq!(
            visible_runs(&wall, std::slice::from_ref(&opening)),
            vec![(0.0, 0.35), (0.65, 1.0)]
        );
        let mut second = opening.clone();
        second.x = 6.0;
        assert_eq!(
            visible_runs(&wall, &[opening.clone(), second]),
            vec![(0.0, 0.35), (0.75, 1.0)]
        );
        opening.gap_width = 20.0;
        assert_eq!(
            visible_runs(&wall, std::slice::from_ref(&opening)),
            vec![(0.0, 0.0)]
        );
        assert!(
            compute_wall_solids(&[wall], &six(), std::slice::from_ref(&opening)).solids[0]
                .pieces
                .is_empty()
        );
    }

    #[test]
    fn a_centred_opening_cuts_two_pieces() {
        let walls = vec![line(1, 0.0, 0.0, 10.0, 0.0)];
        let openings = vec![Opening {
            wall_id: 1,
            x: 5.0,
            y: 0.0,
            gap_width: 3.0,
            snap: WallSnap::Center,
            flip_h: false,
            side_flip: false,
        }];
        let out = compute_wall_solids(&walls, &six(), &openings);
        let solid = &out.solids[0];
        assert_eq!(solid.pieces.len(), 2, "a gap splits the wall");
        // First piece ends at 5 - 1.5, the second starts at 5 + 1.5.
        assert_eq!(solid.pieces[0][1], Pt::new(3.5, 0.25));
        assert_eq!(solid.pieces[1][0], Pt::new(6.5, 0.25));
    }

    #[test]
    fn a_side_snapped_opening_runs_from_the_hinge() {
        let walls = vec![line(1, 0.0, 0.0, 10.0, 0.0)];
        let opening = Opening {
            wall_id: 1,
            x: 4.0,
            y: 0.0,
            gap_width: 3.0,
            snap: WallSnap::Side,
            flip_h: false,
            side_flip: false,
        };
        let out = compute_wall_solids(&walls, &six(), std::slice::from_ref(&opening));
        let solid = &out.solids[0];
        assert_eq!(solid.pieces[0][1], Pt::new(4.0, 0.25));
        assert_eq!(solid.pieces[1][0], Pt::new(7.0, 0.25));
        // Flip Swing mirrors the run to the other side of the hinge.
        let flipped = Opening {
            flip_h: true,
            ..opening
        };
        let out = compute_wall_solids(&walls, &six(), &[flipped]);
        let solid = &out.solids[0];
        assert_eq!(solid.pieces[0][1], Pt::new(1.0, 0.25));
        assert_eq!(solid.pieces[1][0], Pt::new(4.0, 0.25));
    }

    #[test]
    fn zero_thickness_and_bowed_walls_produce_no_solid() {
        let walls = vec![line(1, 0.0, 0.0, 10.0, 0.0)];
        let none = |_: &SketchLine| 0.0;
        assert!(compute_wall_solids(&walls, &none, &[]).solids.is_empty());

        let mut bowed = line(2, 0.0, 0.0, 10.0, 0.0);
        bowed.bow_angle = 12.0;
        let out = compute_wall_solids(&[bowed], &six(), &[]);
        assert!(out.solids.is_empty(), "bowed walls are the renderer's job");
    }

    #[test]
    fn interior_walls_are_flagged() {
        let mut wall = line(1, 0.0, 0.0, 10.0, 0.0);
        wall.kind = WallType::Interior;
        let out = compute_wall_solids(&[wall], &six(), &[]);
        assert!(out.solids[0].interior);
    }

    /// The full piece lists read off the live original's
    /// `app.getWallSolids()` for the four junction cases (exterior 6in,
    /// `wallSolids: true`). This is the parity check: same quads, same order.
    #[test]
    fn matches_the_live_originals_solids_exactly() {
        let p = |pts: &[(f64, f64)]| pts.iter().map(|(x, y)| Pt::new(*x, *y)).collect::<Vec<_>>();

        // free: one wall, both ends free.
        let out = compute_wall_solids(&[line(1, 0.0, 0.0, 10.0, 0.0)], &six(), &[]);
        assert_eq!(
            out.solids[0].pieces,
            vec![p(&[(0.0, 0.25), (10.0, 0.25), (10.0, -0.25), (0.0, -0.25)])]
        );

        // corner: L at (10,0); both walls extend over the mate's half.
        let walls = vec![line(1, 0.0, 0.0, 10.0, 0.0), line(2, 10.0, 0.0, 10.0, 10.0)];
        let out = compute_wall_solids(&walls, &six(), &[]);
        assert_eq!(
            out.solids[0].pieces,
            vec![p(&[
                (0.0, 0.25),
                (10.25, 0.25),
                (10.25, -0.25),
                (0.0, -0.25)
            ])]
        );
        assert_eq!(
            out.solids[1].pieces,
            vec![p(&[
                (9.75, -0.25),
                (9.75, 10.0),
                (10.25, 10.0),
                (10.25, -0.25)
            ])]
        );

        // t: the butt wall arrives from below; the end extension is clamped,
        // so it reaches the through wall's centreline.
        let walls = vec![line(1, 0.0, 0.0, 10.0, 0.0), line(2, 5.0, 5.0, 5.0, 0.0)];
        let out = compute_wall_solids(&walls, &six(), &[]);
        assert_eq!(
            out.solids[1].pieces,
            vec![p(&[(5.25, 5.0), (5.25, 0.0), (4.75, 0.0), (4.75, 5.0)])]
        );

        // inline: collinear continuation, both walls extend by the half.
        let walls = vec![line(1, 0.0, 0.0, 10.0, 0.0), line(2, 10.0, 0.0, 20.0, 0.0)];
        let out = compute_wall_solids(&walls, &six(), &[]);
        assert_eq!(
            out.solids[0].pieces,
            vec![p(&[
                (0.0, 0.25),
                (10.25, 0.25),
                (10.25, -0.25),
                (0.0, -0.25)
            ])]
        );
        assert_eq!(
            out.solids[1].pieces,
            vec![p(&[
                (9.75, 0.25),
                (20.0, 0.25),
                (20.0, -0.25),
                (9.75, -0.25)
            ])]
        );
    }
}
