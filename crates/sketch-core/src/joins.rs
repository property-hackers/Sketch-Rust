//! The wall-graph primitives behind `healExteriorWallJoins` /
//! `closeJoinedExteriorLoops` (refs/sketch-app.js:3626-3720) — the pieces
//! that were missing from `geometry.rs`: proper segment crossings, the
//! point-key identity the original uses everywhere, and the closed-loop walk.
//!
//! These are transcribed from the browser source rather than invented: the
//! corner exclusion in `segment_cross_point`, the "pass straight through a
//! T-junction" rule in the walk, and the 5-decimal `pointKey` rounding are all
//! load-bearing — each one prevents the healer from fabricating or truncating
//! walls, which the original's own comments call out.

use crate::geometry::{distance, Pt};
use crate::model::{Polygon, SketchLine, WallType};

/// `minimumClosedArea()` — loops below this are not areas.
pub const MIN_CLOSED_AREA: f64 = 0.01;

/// `pointKey(point)`: `${x.toFixed(5)},${y.toFixed(5)}`.
pub fn point_key(point: Pt) -> String {
    format!("{:.5},{:.5}", point.x, point.y)
}

/// `segmentCrossPoint(a1, a2, b1, b2)` — the proper crossing of two
/// segments, or `None`. Two segments that merely share a corner are not a
/// crossing, and an extrapolated intersection is not one either.
pub fn segment_cross_point(a1: Pt, a2: Pt, b1: Pt, b2: Pt) -> Option<Pt> {
    let (r1x, r1y) = (a2.x - a1.x, a2.y - a1.y);
    let (r2x, r2y) = (b2.x - b1.x, b2.y - b1.y);
    let den = r1x * r2y - r1y * r2x;
    if den.abs() < 1e-9 {
        return None;
    }
    let t = ((b1.x - a1.x) * r2y - (b1.y - a1.y) * r2x) / den;
    let u = ((b1.x - a1.x) * r1y - (b1.y - a1.y) * r1x) / den;
    let eps = 1e-6;
    if t < -eps || t > 1.0 + eps || u < -eps || u > 1.0 + eps {
        return None;
    }
    if (t <= eps || t >= 1.0 - eps) && (u <= eps || u >= 1.0 - eps) {
        return None;
    }
    Some(Pt::new(a1.x + t * r1x, a1.y + t * r1y))
}

/// `linePassesThrough(point, line)` — the point is strictly inside the wall,
/// not at either end.
pub fn line_passes_through(point: Pt, line: &SketchLine) -> bool {
    point_key(point) != point_key(line.start) && point_key(point) != point_key(line.end)
}

/// `exitKeyFrom(nodePoint, fromPoint, line)` — which end of `line` is "ahead"
/// when arriving at `node` from `from`.
pub fn exit_key_from(node: Pt, from: Pt, line: &SketchLine) -> String {
    let (dir_x, dir_y) = (node.x - from.x, node.y - from.y);
    let dot_start = dir_x * (line.start.x - node.x) + dir_y * (line.start.y - node.y);
    let dot_end = dir_x * (line.end.x - node.x) + dir_y * (line.end.y - node.y);
    if dot_start > dot_end {
        point_key(line.start)
    } else {
        point_key(line.end)
    }
}

/// One node of the virtual wall graph: the point and the sub-edge ids meeting
/// there.
#[derive(Debug, Clone)]
pub struct Node {
    pub point: Pt,
    pub ids: Vec<String>,
}

/// A closed loop found in the graph.
#[derive(Debug, Clone, PartialEq)]
pub struct ClosedLoop {
    pub points: Vec<Pt>,
    /// `(real line id, bow angle)` per edge, in walk order.
    pub edges: Vec<(i64, f64)>,
    /// The virtual edge ids the loop used.
    pub used: Vec<String>,
}

/// The sine of the turn angle at a node — the walk prefers the straightest
/// continuation.
fn deviation(from: Pt, node: Pt, line: &SketchLine) -> f64 {
    let (in_x, in_y) = (node.x - from.x, node.y - from.y);
    let out_key = exit_key_from(node, from, line);
    let out = if point_key(line.start) == out_key {
        line.start
    } else {
        line.end
    };
    let (out_x, out_y) = (out.x - node.x, out.y - node.y);
    let in_len = in_x.hypot(in_y).max(f64::MIN_POSITIVE);
    let out_len = out_x.hypot(out_y).max(f64::MIN_POSITIVE);
    ((in_x / in_len) * (out_y / out_len) - (in_y / in_len) * (out_x / out_len)).abs()
}

/// `walkClosedLoop(nodes, byId, startKey, firstId)`: walk the graph from
/// `start_key` along `first_id`, preferring the straightest continuation, and
/// return the first closed loop of three or more edges. A wall may be passed
/// straight through where a junction sits on its body, so T-junctions and
/// healed crossings traverse correctly.
pub fn walk_closed_loop(
    nodes: &std::collections::HashMap<String, Node>,
    by_id: &std::collections::HashMap<String, SketchLine>,
    start_key: &str,
    first_id: &str,
) -> Option<ClosedLoop> {
    let path: Vec<Pt> = Vec::new();
    let edges: Vec<(i64, f64)> = Vec::new();
    let used: Vec<String> = Vec::new();
    rec(
        nodes, by_id, start_key, start_key, first_id, &used, &path, &edges,
    )
}

#[allow(clippy::too_many_arguments)]
fn rec(
    nodes: &std::collections::HashMap<String, Node>,
    by_id: &std::collections::HashMap<String, SketchLine>,
    loop_start: &str,
    current_key: &str,
    current_id: &str,
    used: &[String],
    path: &[Pt],
    edges: &[(i64, f64)],
) -> Option<ClosedLoop> {
    let node = nodes.get(current_key)?;
    let line = by_id.get(current_id)?;
    if used.iter().any(|id| id == current_id) {
        return None;
    }
    let start_k = point_key(line.start);
    let end_k = point_key(line.end);
    // Arriving mid-wall (a T-junction pass-through) either way is viable.
    let exits: Vec<&str> = if current_key == start_k {
        vec![end_k.as_str()]
    } else if current_key == end_k {
        vec![start_k.as_str()]
    } else {
        vec![start_k.as_str(), end_k.as_str()]
    };
    for next_key in exits {
        let mut next_points = path.to_vec();
        next_points.push(node.point);
        let bow = if next_key == end_k {
            line.bow_angle
        } else {
            -line.bow_angle
        };
        let mut next_edges = edges.to_vec();
        next_edges.push((line.id, bow));
        let mut next_used = used.to_vec();
        next_used.push(current_id.to_string());
        if next_key == loop_start {
            if next_used.len() >= 3 {
                return Some(ClosedLoop {
                    points: next_points,
                    edges: next_edges,
                    used: next_used,
                });
            }
            continue;
        }
        let Some(next_node) = nodes.get(next_key) else {
            continue;
        };
        let mut candidates: Vec<String> = next_node
            .ids
            .iter()
            .filter(|id| {
                if **id == current_id {
                    line.bow_angle.abs() < 1e-6 && line_passes_through(next_node.point, line)
                } else {
                    !next_used.iter().any(|u| u == *id)
                }
            })
            .cloned()
            .collect();
        candidates.sort_by(|a, b| {
            let da = by_id
                .get(a)
                .map(|l| deviation(node.point, next_node.point, l))
                .unwrap_or(f64::MAX);
            let db = by_id
                .get(b)
                .map(|l| deviation(node.point, next_node.point, l))
                .unwrap_or(f64::MAX);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        for cand in candidates {
            if let Some(result) = rec(
                nodes,
                by_id,
                loop_start,
                next_key,
                &cand,
                &next_used,
                &next_points,
                &next_edges,
            ) {
                return Some(result);
            }
        }
    }
    None
}

/// `pointsEqual(a, b, tolerance = 0.0001)`.
pub fn points_equal(a: Pt, b: Pt, tolerance: f64) -> bool {
    (a.x - b.x).abs() <= tolerance && (a.y - b.y).abs() <= tolerance
}

/// `polygonBoundaryLines`: the live exterior walls matching a polygon's
/// points edge by edge, or `None` when the boundary is stale.
fn polygon_boundary_lines<'a>(
    poly: &Polygon,
    by_id: &std::collections::HashMap<i64, &'a SketchLine>,
) -> Option<Vec<&'a SketchLine>> {
    let geo = crate::geometry::GeometryEngine::new();
    if poly.points.len() < 3 || poly.edges.len() != poly.points.len() {
        return None;
    }
    if geo.calculate_area(&poly.points, &poly.edges) < MIN_CLOSED_AREA {
        return None;
    }
    let mut used = std::collections::HashSet::new();
    let mut boundary = Vec::new();
    for index in 0..poly.points.len() {
        let edge = &poly.edges[index];
        let line = edge.line_id.and_then(|id| by_id.get(&id).copied());
        let start = poly.points[index];
        let end = poly.points[(index + 1) % poly.points.len()];
        let matches = line.is_some_and(|line| {
            line.kind == WallType::Exterior
                && ((points_equal(line.start, start, 0.0001)
                    && points_equal(line.end, end, 0.0001))
                    || (points_equal(line.start, end, 0.0001)
                        && points_equal(line.end, start, 0.0001)))
        });
        let line = line?;
        if !matches || !used.insert(line.id) {
            return None;
        }
        boundary.push(line);
    }
    Some(boundary)
}

/// `isPolygonBoundaryIntact`.
pub fn is_polygon_boundary_intact(
    poly: &Polygon,
    by_id: &std::collections::HashMap<i64, &SketchLine>,
) -> bool {
    polygon_boundary_lines(poly, by_id).is_some()
}

/// `closedAreaVertexKeys`: the live endpoints of genuinely closed areas never
/// attract new walls.
pub fn closed_area_vertex_keys(
    polygons: &[Polygon],
    by_id: &std::collections::HashMap<i64, &SketchLine>,
) -> std::collections::HashSet<String> {
    let mut keys = std::collections::HashSet::new();
    for poly in polygons {
        if let Some(boundary) = polygon_boundary_lines(poly, by_id) {
            for line in boundary {
                keys.insert(point_key(line.start));
                keys.insert(point_key(line.end));
            }
        }
    }
    keys
}

#[derive(Debug, Clone)]
struct Weld {
    /// Index into the exterior list.
    line: usize,
    at_start: bool,
    target: Pt,
    at_endpoint: bool,
    distance: f64,
}

impl Weld {
    fn distance_less_than(&self, other: &Weld) -> bool {
        self.distance < other.distance - 1e-9
    }

    fn distance_equal(&self, other: &Weld) -> bool {
        (self.distance - other.distance).abs() <= 1e-9
    }
}

#[derive(Debug, Clone)]
struct SubEdge {
    id: String,
    line_id: i64,
    a: String,
    b: String,
}

/// `healExteriorWallJoins`: weld free wall ends onto nearby endpoints or wall
/// bodies, split both walls at proper crossings, prune the dangling pieces,
/// and — only when a closed loop results — move the endpoints onto it.
/// Returns whether anything was committed.
pub fn heal_exterior_wall_joins(
    lines: &mut [SketchLine],
    polygons: &mut [Polygon],
    tolerance: f64,
) -> bool {
    use std::collections::{HashMap, HashSet};
    let geo = crate::geometry::GeometryEngine::new();

    // The exterior walls the healer considers (straight and non-degenerate).
    let exterior: Vec<usize> = (0..lines.len())
        .filter(|i| {
            let line = &lines[*i];
            line.kind == WallType::Exterior && is_drawable(line) && line.bow_angle.abs() < 0.5
        })
        .collect();
    if exterior.len() < 3 {
        return false;
    }
    let by_id: HashMap<i64, &SketchLine> = lines.iter().map(|l| (l.id, l)).collect();
    let protected = closed_area_vertex_keys(polygons, &by_id);
    let line_index: HashMap<i64, usize> = exterior
        .iter()
        .enumerate()
        .map(|(index, i)| (lines[*i].id, index))
        .collect();
    let existing: HashSet<String> = polygons
        .iter()
        .filter(|poly| is_polygon_boundary_intact(poly, &by_id))
        .map(|poly| signature(&poly.edges))
        .collect();

    let endpoint_shared = |line: usize, at_start: bool| -> bool {
        let point = if at_start {
            lines[line].start
        } else {
            lines[line].end
        };
        exterior.iter().any(|other| {
            *other != line
                && (point_key(lines[*other].start) == point_key(point)
                    || point_key(lines[*other].end) == point_key(point))
        })
    };

    // --- welds -------------------------------------------------------------
    let mut welds: Vec<Weld> = Vec::new();
    for line in &exterior {
        for at_start in [true, false] {
            let point = if at_start {
                lines[*line].start
            } else {
                lines[*line].end
            };
            if endpoint_shared(*line, at_start) || protected.contains(&point_key(point)) {
                continue;
            }
            // An end that already crosses another wall joins the graph there;
            // welding it elsewhere would pull the wall to a wrong node.
            let already_joined = exterior.iter().any(|other| {
                if other == line {
                    return false;
                }
                let Some(cross) = segment_cross_point(
                    lines[*line].start,
                    lines[*line].end,
                    lines[*other].start,
                    lines[*other].end,
                ) else {
                    return false;
                };
                let on_line = along_t(lines[*line].start, lines[*line].end, cross);
                let on_other = along_t(lines[*other].start, lines[*other].end, cross);
                on_line > 1e-6
                    && on_line < 1.0 - 1e-6
                    && on_other > 1e-6
                    && on_other < 1.0 - 1e-6
                    && distance(point, cross) <= tolerance
            });
            if already_joined {
                continue;
            }
            let mut best: Option<Weld> = None;
            let mut consider = |candidate: Weld| {
                let better = match &best {
                    None => true,
                    Some(b) => {
                        candidate.distance_less_than(b)
                            || (candidate.distance_equal(b)
                                && candidate.at_endpoint
                                && !b.at_endpoint)
                    }
                };
                if better {
                    best = Some(candidate);
                }
            };
            for host in &exterior {
                if host == line {
                    continue;
                }
                // Canonicalize near endpoint-to-endpoint joins onto the older
                // wall, and skip the reverse so a pair cannot weld to itself.
                if line_index[&lines[*host].id] < line_index[&lines[*line].id] {
                    for host_start in [true, false] {
                        let target = if host_start {
                            lines[*host].start
                        } else {
                            lines[*host].end
                        };
                        if protected.contains(&point_key(target)) {
                            continue;
                        }
                        let d = distance(point, target);
                        if d > 1e-9 && d <= tolerance {
                            consider(Weld {
                                line: *line,
                                at_start,
                                target,
                                at_endpoint: true,
                                distance: d,
                            });
                        }
                    }
                }
                let target =
                    geo.project_point_to_segment(point, lines[*host].start, lines[*host].end);
                if distance(target, lines[*host].start) <= 1e-4
                    || distance(target, lines[*host].end) <= 1e-4
                {
                    continue;
                }
                let d = distance(point, target);
                if d <= 1e-9 || d > tolerance {
                    continue;
                }
                consider(Weld {
                    line: *line,
                    at_start,
                    target,
                    at_endpoint: false,
                    distance: d,
                });
            }
            if let Some(best) = best {
                welds.push(best);
            }
        }
    }

    // --- splits ------------------------------------------------------------
    let mut splits: HashMap<i64, Vec<Pt>> = HashMap::new();
    let add_split = |splits: &mut HashMap<i64, Vec<Pt>>, line: usize, point: Pt| {
        splits.entry(lines[line].id).or_default().push(point);
    };
    for i in 0..exterior.len() {
        for j in (i + 1)..exterior.len() {
            if let Some(point) = segment_cross_point(
                lines[exterior[i]].start,
                lines[exterior[i]].end,
                lines[exterior[j]].start,
                lines[exterior[j]].end,
            ) {
                add_split(&mut splits, exterior[i], point);
                add_split(&mut splits, exterior[j], point);
            }
        }
    }
    for weld in &welds {
        if !weld.at_endpoint {
            let host = weld.line;
            add_split(&mut splits, host, weld.target);
        }
    }
    if welds.is_empty() && splits.is_empty() {
        return false;
    }

    // --- virtual graph -----------------------------------------------------
    let mut nodes: HashMap<String, Node> = HashMap::new();
    let mut sub_edges: Vec<SubEdge> = Vec::new();
    let mut sub_edge_count: HashMap<i64, usize> = HashMap::new();
    let mut weld_key: HashMap<(i64, bool), String> = HashMap::new();
    for weld in &welds {
        weld_key.insert((lines[weld.line].id, weld.at_start), point_key(weld.target));
    }
    let node_for = |nodes: &mut HashMap<String, Node>, point: Pt, override_key: Option<String>| {
        let key = override_key.unwrap_or_else(|| point_key(point));
        nodes.entry(key.clone()).or_insert(Node {
            point,
            ids: Vec::new(),
        });
        key
    };
    for weld in &welds {
        node_for(&mut nodes, weld.target, None);
    }
    for points in splits.values() {
        for point in points {
            node_for(&mut nodes, *point, None);
        }
    }
    for line in &exterior {
        let id = lines[*line].id;
        let mut ordered: Vec<(f64, Pt)> = splits
            .get(&id)
            .map(|points| {
                points
                    .iter()
                    .map(|p| (along_t(lines[*line].start, lines[*line].end, *p), *p))
                    .collect()
            })
            .unwrap_or_default();
        ordered.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut keys = vec![node_for(
            &mut nodes,
            lines[*line].start,
            weld_key.get(&(id, true)).cloned(),
        )];
        for (_, point) in ordered {
            keys.push(node_for(&mut nodes, point, None));
        }
        keys.push(node_for(
            &mut nodes,
            lines[*line].end,
            weld_key.get(&(id, false)).cloned(),
        ));
        for index in 0..keys.len().saturating_sub(1) {
            if keys[index] == keys[index + 1] {
                continue;
            }
            let edge_id = format!("{}#{}", id, sub_edges.len());
            sub_edges.push(SubEdge {
                id: edge_id.clone(),
                line_id: id,
                a: keys[index].clone(),
                b: keys[index + 1].clone(),
            });
            *sub_edge_count.entry(id).or_insert(0) += 1;
            nodes
                .get_mut(&keys[index])
                .unwrap()
                .ids
                .push(edge_id.clone());
            nodes.get_mut(&keys[index + 1]).unwrap().ids.push(edge_id);
        }
    }

    // --- prune dangling pieces --------------------------------------------
    let mut end_point_node: HashMap<(i64, bool), String> = HashMap::new();
    for line in &exterior {
        let id = lines[*line].id;
        for at_start in [true, false] {
            let point = if at_start {
                lines[*line].start
            } else {
                lines[*line].end
            };
            let key = weld_key
                .get(&(id, at_start))
                .cloned()
                .unwrap_or_else(|| point_key(point));
            end_point_node.insert((id, at_start), key);
        }
    }
    let mut queue: Vec<String> = nodes
        .iter()
        .filter(|(_, node)| node.ids.len() == 1)
        .map(|(key, _)| key.clone())
        .collect();
    let mut stuck: HashSet<String> = HashSet::new();
    while let Some(key) = queue.pop() {
        if stuck.contains(&key) || protected.contains(&key) {
            continue;
        }
        let node = match nodes.get(&key) {
            Some(node) if node.ids.len() == 1 => node.clone(),
            _ => continue,
        };
        let Some(edge) = sub_edges.iter().find(|e| e.id == node.ids[0]).cloned() else {
            continue;
        };
        if sub_edge_count.get(&edge.line_id).copied().unwrap_or(0) <= 1 {
            stuck.insert(key);
            continue;
        }
        let other_key = if edge.a == key {
            edge.b.clone()
        } else {
            edge.a.clone()
        };
        for at_start in [true, false] {
            if end_point_node.get(&(edge.line_id, at_start)) == Some(&key) {
                end_point_node.insert((edge.line_id, at_start), other_key.clone());
            }
        }
        nodes.get_mut(&key).unwrap().ids.clear();
        if let Some(other) = nodes.get_mut(&other_key) {
            other.ids.retain(|id| *id != edge.id);
            if other.ids.len() == 1 {
                queue.push(other_key.clone());
            }
        }
        let count = sub_edge_count.entry(edge.line_id).or_insert(0);
        *count = count.saturating_sub(1);
    }

    // --- look for a closed loop -------------------------------------------
    let mut virtual_nodes: HashMap<String, Node> = HashMap::new();
    let mut virtual_by_id: HashMap<String, SketchLine> = HashMap::new();
    for edge in &sub_edges {
        let (Some(a), Some(b)) = (nodes.get(&edge.a), nodes.get(&edge.b)) else {
            continue;
        };
        if !a.ids.contains(&edge.id) || !b.ids.contains(&edge.id) {
            continue;
        }
        let mut wall = SketchLine {
            id: edge.line_id,
            start: a.point,
            end: b.point,
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
        };
        wall.bow_angle = 0.0;
        virtual_by_id.insert(edge.id.clone(), wall);
    }
    for (key, node) in &nodes {
        if node.ids.is_empty() {
            continue;
        }
        virtual_nodes.insert(key.clone(), node.clone());
    }
    let mut new_loop: Option<ClosedLoop> = None;
    'outer: for edge in &sub_edges {
        if !virtual_by_id.contains_key(&edge.id) {
            continue;
        }
        for start_key in [&edge.a, &edge.b] {
            let Some(found) = walk_closed_loop(&virtual_nodes, &virtual_by_id, start_key, &edge.id)
            else {
                continue;
            };
            let mut real_ids: Vec<i64> = found
                .used
                .iter()
                .filter_map(|id| virtual_by_id.get(id).map(|l| l.id))
                .collect();
            real_ids.sort_unstable();
            real_ids.dedup();
            let signature = real_ids
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join("|");
            if existing.contains(&signature) {
                continue;
            }
            let edges: Vec<crate::geometry::Edge> = found
                .edges
                .iter()
                .map(|(id, bow)| crate::geometry::Edge {
                    line_id: Some(*id),
                    bow_angle: *bow,
                })
                .collect();
            if geo.calculate_area(&found.points, &edges) < MIN_CLOSED_AREA {
                continue;
            }
            new_loop = Some(found);
            break 'outer;
        }
    }
    let Some(new_loop) = new_loop else {
        return false;
    };

    // --- commit the heals -------------------------------------------------
    let mut changed = false;
    for line in &exterior {
        let id = lines[*line].id;
        for at_start in [true, false] {
            let Some(target_key) = end_point_node.get(&(id, at_start)).cloned() else {
                continue;
            };
            let point = if at_start {
                lines[*line].start
            } else {
                lines[*line].end
            };
            if target_key == point_key(point) {
                continue;
            }
            let used_at_target = sub_edges.iter().any(|edge| {
                new_loop.used.contains(&edge.id)
                    && edge.line_id == id
                    && (edge.a == target_key || edge.b == target_key)
            });
            if !used_at_target {
                continue;
            }
            let Some(target) = nodes.get(&target_key) else {
                continue;
            };
            if protected.contains(&point_key(point)) {
                continue;
            }
            let old = point;
            if at_start {
                lines[*line].start = target.point;
            } else {
                lines[*line].end = target.point;
            }
            changed = true;
            for poly in polygons.iter_mut() {
                for vertex in poly.points.iter_mut() {
                    if points_equal(*vertex, old, 1e-5) {
                        *vertex = target.point;
                    }
                }
            }
        }
    }
    changed
}

/// `polygonRingsEqual(first, second, tolerance = 0.02)` — the same ring,
/// allowing a different start vertex and either direction.
pub fn polygon_rings_equal(first: &[Pt], second: &[Pt], tolerance: f64) -> bool {
    if first.len() < 3 || second.len() < 3 || first.len() != second.len() {
        return false;
    }
    let count = first.len();
    for offset in 0..count {
        if !points_equal(first[0], second[offset], tolerance) {
            continue;
        }
        let forward =
            (0..count).all(|i| points_equal(first[i], second[(offset + i) % count], tolerance));
        let reverse = (0..count)
            .all(|i| points_equal(first[i], second[(offset + count - i) % count], tolerance));
        if forward || reverse {
            return true;
        }
    }
    false
}

/// What `closeJoinedExteriorLoops` needs from the app's state.
pub struct CloseOptions {
    /// `getScreenToleranceWorld(16)` in feet.
    pub tolerance: f64,
    /// `defineFirst && selectedAreaType` -> (type, name) for a new area.
    pub defined: Option<(String, String)>,
    /// `options.autoSubtractOverlapped !== false`.
    pub auto_subtract: bool,
}

/// `closeJoinedExteriorLoops`: heal the joins, then materialise an area for
/// every bounded face the exterior walls now enclose that no intact area
/// already covers. Returns how many areas were created or repaired.
///
/// NOT ported here, both off/unbuilt: `splitWallsAtJunctions` (undefined in
/// the original's defaults, i.e. off) and the dedicated-layer assignment
/// (layers are a later screen).
pub fn close_joined_exterior_loops(
    lines: &mut [SketchLine],
    polygons: &mut Vec<Polygon>,
    opts: &CloseOptions,
    next_id: &mut i64,
) -> usize {
    use std::collections::{HashMap, HashSet};
    let geo = crate::geometry::GeometryEngine::new();
    heal_exterior_wall_joins(lines, polygons, opts.tolerance);

    let exterior_idx: Vec<usize> = (0..lines.len())
        .filter(|i| lines[*i].kind == WallType::Exterior && is_drawable(&lines[*i]))
        .collect();
    let exterior: Vec<crate::geometry::Line> =
        exterior_idx.iter().map(|i| lines[*i].geometry()).collect();
    let by_id: HashMap<i64, &SketchLine> = lines.iter().map(|l| (l.id, l)).collect();
    let faces = geo.find_enclosed_faces(&exterior, 1e-9, 1e-5);

    let intact: Vec<&Polygon> = polygons
        .iter()
        .filter(|poly| is_polygon_boundary_intact(poly, &by_id))
        .collect();
    let intact_ids: HashSet<i64> = intact.iter().map(|poly| poly.id).collect();
    let mut repairable: Vec<Polygon> = polygons
        .iter()
        .filter(|poly| !intact_ids.contains(&poly.id))
        .cloned()
        .collect();
    let claimed: HashSet<i64> = intact
        .iter()
        .flat_map(|poly| poly.edges.iter().filter_map(|e| e.line_id))
        .collect();
    let mut existing: HashSet<String> = intact.iter().map(|poly| signature(&poly.edges)).collect();

    // --- candidate faces ---------------------------------------------------
    struct Candidate {
        points: Vec<Pt>,
        edges: Vec<crate::geometry::Edge>,
        line_ids: HashSet<i64>,
        signature: String,
        area: f64,
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for face in &faces {
        let mut materialized = true;
        let mut edges = Vec::with_capacity(face.edges.len());
        for (index, edge) in face.edges.iter().enumerate() {
            let Some(line) = edge.line_id.and_then(|id| by_id.get(&id).copied()) else {
                materialized = false;
                edges.push(crate::geometry::Edge {
                    line_id: edge.line_id,
                    bow_angle: 0.0,
                });
                continue;
            };
            let start = face.points[index];
            let end = face.points[(index + 1) % face.points.len()];
            let follows =
                points_equal(line.start, start, 0.0001) && points_equal(line.end, end, 0.0001);
            let reverses =
                points_equal(line.end, start, 0.0001) && points_equal(line.start, end, 0.0001);
            if !follows && !reverses {
                materialized = false;
            }
            edges.push(crate::geometry::Edge {
                line_id: edge.line_id,
                bow_angle: if follows {
                    line.bow_angle
                } else if reverses {
                    -line.bow_angle
                } else {
                    0.0
                },
            });
        }
        // A face whose edge is only part of a live wall could never validate
        // as a closed boundary, so it is not persisted.
        if !materialized {
            continue;
        }
        let line_ids: HashSet<i64> = edges.iter().filter_map(|e| e.line_id).collect();
        let sig = signature(&edges);
        if existing.contains(&sig) || seen.contains(&sig) {
            continue;
        }
        if line_ids.iter().all(|id| claimed.contains(id)) {
            continue;
        }
        let area = geo.calculate_area(&face.points, &edges);
        if area < MIN_CLOSED_AREA {
            continue;
        }
        seen.insert(sig.clone());
        candidates.push(Candidate {
            points: face.points.clone(),
            edges,
            line_ids,
            signature: sig,
            area,
        });
    }

    // Drop composites that merely wrap or subdivide an intact area, and
    // larger rings that reuse a smaller candidate's boundary.
    let shares = |a: &HashSet<i64>, b: &HashSet<i64>| a.iter().any(|id| b.contains(id));
    let accepted: Vec<&Candidate> = candidates
        .iter()
        .filter(|candidate| {
            let overlaps_intact = intact.iter().any(|poly| {
                let poly_ids: HashSet<i64> = poly.edges.iter().filter_map(|e| e.line_id).collect();
                if !shares(&candidate.line_ids, &poly_ids) {
                    return false;
                }
                geo.polygon_contains(
                    &candidate.points,
                    &candidate.edges,
                    &poly.points,
                    &poly.edges,
                    1e-4,
                    0.01,
                ) || geo.polygon_contains(
                    &poly.points,
                    &poly.edges,
                    &candidate.points,
                    &candidate.edges,
                    1e-4,
                    0.01,
                )
            });
            if overlaps_intact {
                return false;
            }
            !candidates.iter().any(|other| {
                !std::ptr::eq(other, *candidate)
                    && other.area < candidate.area - 1e-6
                    && shares(&candidate.line_ids, &other.line_ids)
                    && geo.polygon_contains(
                        &candidate.points,
                        &candidate.edges,
                        &other.points,
                        &other.edges,
                        1e-4,
                        0.01,
                    )
            })
        })
        .collect();
    let mut accepted: Vec<&Candidate> = accepted;
    accepted.sort_by(|a, b| {
        a.area
            .partial_cmp(&b.area)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.signature.cmp(&b.signature))
    });

    // --- materialise -------------------------------------------------------
    let mut created = 0;
    for candidate in accepted {
        let repair_index = repairable
            .iter()
            .position(|poly| polygon_rings_equal(&poly.points, &candidate.points, 0.02));
        if let Some(index) = repair_index {
            let mut repaired = repairable.remove(index);
            repaired.points = candidate.points.clone();
            repaired.edges = candidate.edges.clone();
            repaired.gross = candidate.area;
            repaired.area = candidate.area;
            if let Some(slot) = polygons.iter_mut().find(|p| p.id == repaired.id) {
                *slot = repaired;
            } else {
                polygons.push(repaired);
            }
        } else {
            *next_id += 1;
            let (kind, name) = match &opts.defined {
                Some((kind, name)) => (kind.clone(), name.clone()),
                None => ("UND".to_string(), "Undefined Area".to_string()),
            };
            polygons.push(Polygon {
                id: *next_id,
                points: candidate.points.clone(),
                edges: candidate.edges.clone(),
                kind,
                name,
                gross: candidate.area,
                area: candidate.area,
                factor_override: None,
                subtract_mode: if opts.auto_subtract { "auto" } else { "never" }.to_string(),
                subtract_target_id: None,
                factor: 1.0,
                measure_to: "center".into(),
                auto_post: Default::default(),
                attributes: Default::default(),
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
        existing.insert(candidate.signature.clone());
        created += 1;
    }
    created
}

/// `alongLineT(line, point)` on bare points.
fn along_t(start: Pt, end: Pt, point: Pt) -> f64 {
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    ((point.x - start.x) * dx + (point.y - start.y) * dy) / (dx * dx + dy * dy)
}

/// The edge-id signature the original uses to recognise an existing area.
fn signature(edges: &[crate::geometry::Edge]) -> String {
    let mut ids: Vec<String> = edges
        .iter()
        .filter_map(|e| e.line_id)
        .map(|id| id.to_string())
        .collect();
    ids.sort();
    ids.join("|")
}

/// A wall's chord length, the original's `distance(start, end) > 1e-6` filter.
pub fn is_drawable(line: &SketchLine) -> bool {
    distance(line.start, line.end) > 1e-6
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

    #[test]
    fn point_keys_use_five_decimals() {
        assert_eq!(point_key(Pt::new(-11.0, 3.0)), "-11.00000,3.00000");
        assert_eq!(point_key(Pt::new(0.000001, 0.0)), "0.00000,0.00000");
    }

    #[test]
    fn a_proper_crossing_is_found() {
        // Horizontal through vertical, crossing in the middle of both.
        let cross = segment_cross_point(
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            Pt::new(5.0, -5.0),
            Pt::new(5.0, 5.0),
        );
        assert_eq!(cross, Some(Pt::new(5.0, 0.0)));
    }

    #[test]
    fn a_shared_corner_is_not_a_crossing() {
        // Two walls meeting at (10,0) only share a corner.
        assert_eq!(
            segment_cross_point(
                Pt::new(0.0, 0.0),
                Pt::new(10.0, 0.0),
                Pt::new(10.0, 0.0),
                Pt::new(10.0, 10.0),
            ),
            None
        );
    }

    #[test]
    fn an_extrapolated_crossing_is_not_a_crossing() {
        // The lines would cross at (15,0) if extended.
        assert_eq!(
            segment_cross_point(
                Pt::new(0.0, 0.0),
                Pt::new(10.0, 0.0),
                Pt::new(15.0, -5.0),
                Pt::new(15.0, 5.0),
            ),
            None
        );
    }

    #[test]
    fn parallel_segments_never_cross() {
        assert_eq!(
            segment_cross_point(
                Pt::new(0.0, 0.0),
                Pt::new(10.0, 0.0),
                Pt::new(0.0, 1.0),
                Pt::new(10.0, 1.0),
            ),
            None
        );
    }

    #[test]
    fn a_tees_body_counts_as_passing_through() {
        let wall = line(1, 0.0, 0.0, 10.0, 0.0);
        assert!(line_passes_through(Pt::new(5.0, 0.0), &wall));
        assert!(!line_passes_through(Pt::new(0.0, 0.0), &wall));
        assert!(!line_passes_through(Pt::new(10.0, 0.0), &wall));
    }

    #[test]
    fn exit_keys_point_away_from_the_arrival() {
        let wall = line(1, 10.0, 0.0, 20.0, 0.0);
        // Arriving at (10,0) from the left: the wall continues to (20,0).
        assert_eq!(
            exit_key_from(Pt::new(10.0, 0.0), Pt::new(0.0, 0.0), &wall),
            point_key(Pt::new(20.0, 0.0))
        );
    }

    #[test]
    fn the_minimum_closed_area_is_the_originals() {
        assert_eq!(MIN_CLOSED_AREA, 0.01);
    }

    #[test]
    fn the_walk_finds_a_closed_square() {
        use std::collections::HashMap;
        let corners = [
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            Pt::new(10.0, 10.0),
            Pt::new(0.0, 10.0),
        ];
        let mut by_id: HashMap<String, SketchLine> = HashMap::new();
        let mut nodes: HashMap<String, Node> = HashMap::new();
        for i in 0..4 {
            let id = format!("{}#0", i + 1);
            let a = corners[i];
            let b = corners[(i + 1) % 4];
            let mut wall = line(i as i64 + 1, a.x, a.y, b.x, b.y);
            wall.id = i as i64 + 1;
            by_id.insert(id.clone(), wall);
            for point in [a, b] {
                nodes
                    .entry(point_key(point))
                    .or_insert(Node {
                        point,
                        ids: Vec::new(),
                    })
                    .ids
                    .push(id.clone());
            }
        }
        let start = point_key(corners[0]);
        let found = walk_closed_loop(&nodes, &by_id, &start, "1#0").expect("a loop");
        assert_eq!(found.used.len(), 4, "four edges");
        assert_eq!(found.points.len(), 4);
        // The loop's edge list carries the real wall ids.
        let ids: Vec<i64> = found.edges.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![1, 2, 3, 4]);
    }

    #[test]
    fn an_open_chain_has_no_loop() {
        use std::collections::HashMap;
        let mut by_id: HashMap<String, SketchLine> = HashMap::new();
        let mut nodes: HashMap<String, Node> = HashMap::new();
        for (i, (a, b)) in [
            (Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)),
            (Pt::new(10.0, 0.0), Pt::new(10.0, 10.0)),
        ]
        .into_iter()
        .enumerate()
        {
            let id = format!("{}#0", i + 1);
            let mut wall = line(i as i64 + 1, a.x, a.y, b.x, b.y);
            wall.id = i as i64 + 1;
            by_id.insert(id.clone(), wall);
            for point in [a, b] {
                nodes
                    .entry(point_key(point))
                    .or_insert(Node {
                        point,
                        ids: Vec::new(),
                    })
                    .ids
                    .push(id.clone());
            }
        }
        assert!(walk_closed_loop(&nodes, &by_id, &point_key(Pt::new(0.0, 0.0)), "1#0").is_none());
    }

    #[test]
    fn a_nearly_closed_square_is_healed_onto_its_corner() {
        // Three walls close a square; the fourth ends 0.1 ft short of (0,0).
        // The weld tolerance is 16 screen px (1.6 ft at 10 px/ft).
        let mut lines = vec![
            line(1, 0.0, 0.0, 10.0, 0.0),
            line(2, 10.0, 0.0, 10.0, 10.0),
            line(3, 10.0, 10.0, 0.0, 10.0),
            line(4, 0.0, 10.0, 0.0, 0.1),
        ];
        let mut polygons: Vec<Polygon> = Vec::new();
        let healed = heal_exterior_wall_joins(&mut lines, &mut polygons, 1.6);
        assert!(healed, "the loop completed, so the weld commits");
        // The loose end moved onto the corner.
        assert_eq!(lines[3].end, Pt::new(0.0, 0.0));
    }

    #[test]
    fn a_wall_far_from_its_neighbour_is_left_alone() {
        // A 3 ft gap is outside the 1.6 ft weld tolerance.
        let mut lines = vec![
            line(1, 0.0, 0.0, 10.0, 0.0),
            line(2, 10.0, 0.0, 10.0, 10.0),
            line(3, 10.0, 10.0, 0.0, 10.0),
            line(4, 0.0, 10.0, 0.0, 3.0),
        ];
        let mut polygons: Vec<Polygon> = Vec::new();
        assert!(!heal_exterior_wall_joins(&mut lines, &mut polygons, 1.6));
        assert_eq!(lines[3].end, Pt::new(0.0, 3.0), "untouched");
    }

    #[test]
    fn fewer_than_three_walls_never_heals() {
        let mut lines = vec![line(1, 0.0, 0.0, 10.0, 0.0), line(2, 10.0, 0.0, 10.0, 0.1)];
        let mut polygons: Vec<Polygon> = Vec::new();
        assert!(!heal_exterior_wall_joins(&mut lines, &mut polygons, 1.6));
    }

    fn square(id_base: i64) -> Vec<SketchLine> {
        vec![
            line(id_base, 0.0, 0.0, 10.0, 0.0),
            line(id_base + 1, 10.0, 0.0, 10.0, 10.0),
            line(id_base + 2, 10.0, 10.0, 0.0, 10.0),
            line(id_base + 3, 0.0, 10.0, 0.0, 0.0),
        ]
    }

    fn close_opts() -> CloseOptions {
        CloseOptions {
            tolerance: 0.8,
            defined: None,
            auto_subtract: true,
        }
    }

    #[test]
    fn four_separate_walls_close_into_one_undefined_area() {
        // Walls drawn as separate strokes (no chain): the face walk finds the
        // enclosure and materialises the original's UND area.
        let mut lines = square(1);
        let mut polygons: Vec<Polygon> = Vec::new();
        let mut next_id = 100;
        let created =
            close_joined_exterior_loops(&mut lines, &mut polygons, &close_opts(), &mut next_id);
        assert_eq!(created, 1);
        assert_eq!(polygons.len(), 1);
        assert_eq!(polygons[0].kind, "UND");
        assert_eq!(polygons[0].name, "Undefined Area");
        assert!(
            (polygons[0].area - 100.0).abs() < 1e-6,
            "area {}",
            polygons[0].area
        );
        assert_eq!(polygons[0].edges.len(), 4);
        assert!(polygons[0].edges.iter().all(|e| e.line_id.is_some()));
    }

    #[test]
    fn an_existing_area_is_not_duplicated() {
        let mut lines = square(1);
        let mut polygons: Vec<Polygon> = Vec::new();
        let mut next_id = 100;
        close_joined_exterior_loops(&mut lines, &mut polygons, &close_opts(), &mut next_id);
        assert_eq!(polygons.len(), 1);
        // Running again finds the same signature and creates nothing.
        let again =
            close_joined_exterior_loops(&mut lines, &mut polygons, &close_opts(), &mut next_id);
        assert_eq!(again, 0);
        assert_eq!(polygons.len(), 1);
    }

    #[test]
    fn define_first_names_the_materialised_area() {
        let mut lines = square(1);
        let mut polygons: Vec<Polygon> = Vec::new();
        let mut next_id = 100;
        let opts = CloseOptions {
            tolerance: 0.8,
            defined: Some(("GLA".into(), "Living".into())),
            auto_subtract: true,
        };
        assert_eq!(
            close_joined_exterior_loops(&mut lines, &mut polygons, &opts, &mut next_id),
            1
        );
        assert_eq!(polygons[0].kind, "GLA");
        assert_eq!(polygons[0].name, "Living");
    }

    #[test]
    fn an_open_shape_creates_no_area() {
        let mut lines = vec![
            line(1, 0.0, 0.0, 10.0, 0.0),
            line(2, 10.0, 0.0, 10.0, 10.0),
            line(3, 10.0, 10.0, 0.0, 10.0),
        ];
        let mut polygons: Vec<Polygon> = Vec::new();
        let mut next_id = 100;
        assert_eq!(
            close_joined_exterior_loops(&mut lines, &mut polygons, &close_opts(), &mut next_id),
            0
        );
        assert!(polygons.is_empty());
    }
}
