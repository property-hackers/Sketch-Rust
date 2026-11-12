//! `AreaCalc.recalculate` — the shared recalculation pipeline from
//! `refs/area-breakdown.js` (the module `window.AreaCalc` actually is; the
//! app's `recalcAutoSubtract`/`recalculate` call straight into it).
//!
//! No polygon boolean operations: a "subtraction" is a per-container roll-up
//! of the children's gross areas, and a net area is `(gross - subtracted) ×
//! factor × sign`. The one geometric requirement is containment (which area
//! sits inside which), via the engine's `polygon_contains`.
//!
//! The original runs per page (`page: this.currentPage`); the port's model is
//! single-page, so every polygon is on page 0.

use crate::geometry::{AreaCode, Edge, GeometryEngine, Pt};
use crate::model::Polygon;
use std::collections::BTreeMap;

/// `faceMeasurePolygon`: calculate on wall faces while retaining the editable
/// centerline ring in the caller. A face measurement treats bows as chords.
pub fn face_measure_polygon(
    poly: &Polygon,
    lines: &[crate::model::SketchLine],
    wall_solids: bool,
    thickness_of: &dyn Fn(Option<&crate::model::SketchLine>) -> f64,
) -> Polygon {
    let mut measured = poly.clone();
    let sign = match poly.measure_to.as_str() {
        "interior" => -1.0,
        "exterior" => 1.0,
        _ => return measured,
    };
    if !wall_solids {
        return measured;
    }
    let by_id: BTreeMap<_, _> = lines.iter().map(|l| (l.id, l)).collect();
    let offsets: Vec<_> = poly
        .edges
        .iter()
        .map(|e| {
            let line = e.line_id.and_then(|id| by_id.get(&id).copied());
            sign * thickness_of(line) / 2.0
        })
        .collect();
    if let Some(points) = crate::solids::offset_ring(&poly.points, &offsets) {
        measured.points = points;
        for edge in &mut measured.edges {
            edge.bow_angle = 0.0;
        }
    }
    measured
}

/// Building level an area code describes: GLA2/GBA2 are the second floor,
/// BSMT codes the basement, other defined codes (garage, porch, deck,
/// patio...) the ground floor, and an undefined outline any level. An area is
/// only deducted from a container on its own level, so a first-floor garage
/// is never taken out of the second floor drawn over it.
pub fn area_level(area_type: Option<&str>) -> Option<u32> {
    let code = area_type.unwrap_or("").to_uppercase();
    if code.is_empty() || code == "UND" {
        return None;
    }
    if code.starts_with("BSMT") {
        return Some(0);
    }
    // `code.match(/^(?:GLA|GBA)(\d+)$/)`
    let digits: String = code
        .strip_prefix("GLA")
        .or_else(|| code.strip_prefix("GBA"))
        .unwrap_or("")
        .to_string();
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Some(1);
    }
    Some(digits.parse::<u32>().unwrap_or(1))
}

/// `sameLevel` — an area is only deducted from a container on its own level;
/// `null` levels (undefined outlines) pair with anything.
pub fn same_level(a: Option<&str>, b: Option<&str>) -> bool {
    match (area_level(a), area_level(b)) {
        (None, _) | (_, None) => true,
        (Some(first), Some(second)) => first == second,
    }
}

/// `idOrder` — numeric when both ids are numeric, otherwise lexicographic.
fn id_order(a: i64, b: i64) -> std::cmp::Ordering {
    a.cmp(&b)
}

/// One polygon's computed state, mirroring the fields `recalculate` writes.
#[derive(Debug, Clone, PartialEq)]
pub struct CalcPoly {
    /// The source polygon (geometry unchanged — the original stores the
    /// centerline ring and only computes against it).
    pub id: i64,
    pub kind: String,
    pub name: String,
    pub points: Vec<Pt>,
    pub edges: Vec<Edge>,
    pub gross: f64,
    pub area: f64,
    pub perimeter: f64,
    pub container_id: Option<i64>,
    pub subtracted_from_id: Option<i64>,
    pub subtracted: f64,
    pub factor_override: Option<f64>,
    pub factor: f64,
    pub sign: f64,
    pub net: f64,
}

impl CalcPoly {
    fn from(poly: &Polygon, geo: &GeometryEngine) -> CalcPoly {
        let (gross, area, perimeter) = if poly.points.len() < 3 {
            (0.0, 0.0, 0.0)
        } else {
            (
                geo.calculate_area(&poly.points, &poly.edges),
                geo.calculate_area(&poly.points, &poly.edges),
                geo.calculate_perimeter(&poly.points, &poly.edges),
            )
        };
        CalcPoly {
            id: poly.id,
            kind: poly.kind.clone(),
            name: poly.name.clone(),
            points: poly.points.clone(),
            edges: poly.edges.clone(),
            gross,
            area,
            perimeter,
            container_id: None,
            subtracted_from_id: None,
            subtracted: 0.0,
            factor_override: poly.factor_override,
            factor: 1.0,
            sign: 1.0,
            net: 0.0,
        }
    }
}

/// The roll-up totals `recalculate` produces.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Totals {
    pub totals_by_code: BTreeMap<String, f64>,
    pub totals_by_group: BTreeMap<String, f64>,
    pub total_gla: f64,
    pub total_all_areas: f64,
}

/// The recalculation result: computed polygons, totals and warnings.
#[derive(Debug, Clone, PartialEq)]
pub struct RecalcResult {
    pub polygons: Vec<CalcPoly>,
    pub totals: Totals,
    pub warnings: Vec<String>,
}

/// `buildContainmentTree` — each polygon's container is the smallest polygon
/// on the same page, same level, that contains it (ties broken by id).
fn build_containment_tree(polys: &mut [CalcPoly], geo: &GeometryEngine) {
    let n = polys.len();
    for inner_index in 0..n {
        if polys[inner_index].points.len() < 3 {
            continue;
        }
        let mut best: Option<(i64, f64)> = None;
        for outer_index in 0..n {
            if outer_index == inner_index {
                continue;
            }
            let (inner, outer) = (&polys[inner_index], &polys[outer_index]);
            if outer.points.len() < 3
                || outer.gross <= inner.gross + 1e-6
                || !same_level(Some(&outer.kind), Some(&inner.kind))
            {
                continue;
            }
            if !geo.polygon_contains(
                &outer.points,
                &outer.edges,
                &inner.points,
                &inner.edges,
                1e-4,
                0.01,
            ) {
                continue;
            }
            // `candidates.sort((a, b) => a.gross - b.gross || idOrder(a, b))`
            // — smallest gross wins, then the lower id.
            let better = match best {
                None => true,
                Some((best_id, best_gross)) => {
                    outer.gross < best_gross
                        || (outer.gross == best_gross
                            && id_order(outer.id, best_id) == std::cmp::Ordering::Less)
                }
            };
            if better {
                best = Some((outer.id, outer.gross));
            }
        }
        polys[inner_index].container_id = best.map(|(id, _)| id);
    }
}

/// `cycleMembers` — the ids on any cycle of the proposed subtraction map.
/// Walks each start until it revisits its own path (a cycle) or terminates.
fn cycle_members(proposed: &BTreeMap<i64, Option<i64>>) -> Vec<i64> {
    // color: 1 = on the current path, 2 = fully processed.
    let mut color: BTreeMap<i64, u8> = BTreeMap::new();
    let mut cycles: Vec<i64> = Vec::new();
    for start in proposed.keys().copied().collect::<Vec<_>>() {
        if color.get(&start) == Some(&2) {
            continue;
        }
        let mut path: Vec<i64> = Vec::new();
        let mut position: BTreeMap<i64, usize> = BTreeMap::new();
        let mut current: Option<i64> = Some(start);
        while let Some(node) = current {
            if !proposed.contains_key(&node) || color.get(&node) == Some(&2) {
                break;
            }
            if let Some(&at) = position.get(&node) {
                cycles.extend(path[at..].iter().copied());
                break;
            }
            if color.get(&node) == Some(&1) {
                break;
            }
            color.insert(node, 1);
            position.insert(node, path.len());
            path.push(node);
            current = proposed.get(&node).copied().flatten();
        }
        for id in &path {
            color.insert(*id, 2);
        }
    }
    cycles
}

/// `recalculate` — the full pipeline.
pub fn recalculate(
    polygons: &[Polygon],
    code_table: &[AreaCode],
    geo: &GeometryEngine,
) -> RecalcResult {
    let mut polys: Vec<CalcPoly> = polygons.iter().map(|p| CalcPoly::from(p, geo)).collect();
    build_containment_tree(&mut polys, geo);
    let mut resolved = resolve_subtraction(&mut polys, polygons);
    apply_subtractions(&mut polys);
    apply_factors(&mut polys, code_table);
    resolved.extend(apply_native_calculations(&mut polys, polygons));
    let totals = rollup(&polys, code_table, polygons);
    RecalcResult {
        polygons: polys,
        totals,
        warnings: resolved,
    }
}

/// Native AXF areas use signed multipliers independently of the code table and
/// deduct the child's recursively calculated net, including its own factor.
fn apply_native_calculations(polys: &mut [CalcPoly], originals: &[Polygon]) -> Vec<String> {
    let native: Vec<bool> = originals
        .iter()
        .map(|p| {
            p.attributes
                .get("apex.calculation")
                .is_some_and(|v| v == "net")
        })
        .collect();
    for (i, calc) in polys.iter_mut().enumerate() {
        if native[i] {
            let multiplier = calc.factor_override.unwrap_or(calc.factor * calc.sign);
            calc.factor = multiplier.abs();
            calc.sign = if multiplier < 0. { -1. } else { 1. };
        }
    }
    let invalid: Vec<_> = polys
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            native[*i]
                && p.subtracted_from_id.is_some()
                && (p.sign < 0.
                    || polys
                        .iter()
                        .any(|parent| Some(parent.id) == p.subtracted_from_id && parent.sign < 0.))
        })
        .map(|(_, p)| p.id)
        .collect();
    for poly in polys.iter_mut().filter(|p| invalid.contains(&p.id)) {
        poly.subtracted_from_id = None;
    }
    fn net(i: usize, polys: &[CalcPoly], native: &[bool], cache: &mut BTreeMap<usize, f64>) -> f64 {
        if let Some(value) = cache.get(&i) {
            return *value;
        }
        let value = if native[i] {
            polys[i].gross * polys[i].factor * polys[i].sign
                - polys
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.subtracted_from_id == Some(polys[i].id))
                    .map(|(j, _)| net(j, polys, native, cache))
                    .sum::<f64>()
        } else {
            polys[i].net
        };
        cache.insert(i, value);
        value
    }
    let mut cache = BTreeMap::new();
    for i in 0..polys.len() {
        net(i, polys, &native, &mut cache);
    }
    for (i, calc) in polys.iter_mut().enumerate() {
        if native[i] {
            calc.net = cache[&i];
            let multiplier = calc.factor * calc.sign;
            calc.subtracted = if multiplier == 0. {
                0.
            } else {
                calc.gross - calc.net / multiplier
            };
        }
    }
    invalid.iter().map(|id|format!("Native area {id} uses a negative multiplier; negative areas cannot participate in a native subtraction link.")).collect()
}

#[cfg(test)]
mod native_tests {
    use super::*;
    #[test]
    fn native_nested_deductions_use_each_child_net_and_signed_factor() {
        let make = |id, width, factor, target| -> Polygon {
            serde_json::from_value(serde_json::json!({
                "id":id,"type":"GLA1","points":[{"x":0,"y":0},{"x":width,"y":0},{"x":width,"y":10},{"x":0,"y":10}],
                "factorOverride":factor,"subtractMode":if target==0 {"never"} else {"always"},"subtractTargetId":if target==0 {None} else {Some(target)},
                "attributes":{"apex.calculation":"net"}
            })).unwrap()
        };
        let mut codes = crate::geometry::area_codes::default_area_codes();
        codes
            .iter_mut()
            .find(|c| c.code == "GLA1")
            .unwrap()
            .positive = false;
        let source = [
            make(1, 20., 0.5, 0),
            make(2, 5., 0.25, 1),
            make(3, 1., 2., 2),
            make(4, 3., -0.5, 0),
        ];
        let result = recalculate(&source, &codes, &GeometryEngine::default());
        assert_eq!(
            result.polygons.iter().map(|p| p.net).collect::<Vec<_>>(),
            vec![107.5, -7.5, 20., -15.]
        );
        assert_eq!(result.totals.total_gla, 105.);
    }
}

/// `resolveSubtraction` — proposed targets from `subtractMode`, with cycle
/// detection. Returns the warnings; the targets land in `subtracted_from_id`.
fn resolve_subtraction(polys: &mut [CalcPoly], originals: &[Polygon]) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();
    let mut proposed: BTreeMap<i64, Option<i64>> = BTreeMap::new();
    for (poly, calc) in originals.iter().zip(polys.iter()) {
        let mode = if poly.subtract_mode.is_empty() {
            "auto"
        } else {
            poly.subtract_mode.as_str()
        };
        let mode = if mode != "auto" && mode != "always" && mode != "never" {
            "auto"
        } else {
            mode
        };
        let target: Option<i64> = match mode {
            "auto" => calc.container_id,
            "always" => {
                let explicit = poly.subtract_target_id;
                match explicit {
                    Some(id) if id == poly.id => {
                        warnings.push(format!(
                            "Invalid subtraction target for {}; using spatial containment.",
                            if poly.name.is_empty() {
                                poly.kind.clone()
                            } else {
                                poly.name.clone()
                            }
                        ));
                        calc.container_id
                    }
                    Some(id) => {
                        // The original warns when the target is missing or on
                        // another page; the port is single-page, so only the
                        // missing case can occur.
                        match polys.iter().find(|c| c.id == id) {
                            Some(_) => Some(id),
                            None => {
                                warnings.push(format!(
                                    "Invalid subtraction target for {}; using spatial containment.",
                                    if poly.name.is_empty() {
                                        poly.kind.clone()
                                    } else {
                                        poly.name.clone()
                                    }
                                ));
                                calc.container_id
                            }
                        }
                    }
                    None => calc.container_id,
                }
            }
            _ => None,
        };
        proposed.insert(poly.id, target);
    }
    let cyclic = cycle_members(&proposed);
    for id in &cyclic {
        warnings.push(format!(
            "Cyclic subtraction involving area {id} was ignored."
        ));
    }
    for calc in polys {
        let proposed_target = proposed.get(&calc.id).copied().flatten();
        calc.subtracted_from_id = if cyclic.contains(&calc.id) {
            None
        } else {
            proposed_target
        };
    }
    warnings
}

/// `applySubtractions` — each container's `subtracted` is the sum of the
/// gross areas deducted into it.
fn apply_subtractions(polys: &mut [CalcPoly]) {
    let mut deducted: BTreeMap<i64, f64> = BTreeMap::new();
    for calc in polys.iter() {
        if let Some(target) = calc.subtracted_from_id {
            *deducted.entry(target).or_insert(0.0) += calc.gross;
        }
    }
    for calc in polys.iter_mut() {
        calc.subtracted = deducted.get(&calc.id).copied().unwrap_or(0.0);
    }
}

/// `applyFactors` — net = (gross - subtracted) × effective multiplier × sign.
fn apply_factors(polys: &mut [CalcPoly], code_table: &[AreaCode]) {
    for calc in polys.iter_mut() {
        let definition = code_table.iter().find(|c| c.code == calc.kind);
        let (multiplier, positive) = match definition {
            Some(d) => (d.multiplier, d.positive),
            None => (1.0, true),
        };
        let effective_multiplier = match calc.factor_override {
            Some(override_value) if override_value.is_finite() => override_value,
            _ => multiplier,
        };
        let sign = if positive { 1.0 } else { -1.0 };
        calc.factor = effective_multiplier;
        calc.sign = sign;
        calc.net = (calc.gross - calc.subtracted) * effective_multiplier * sign;
    }
}

/// `rollup` — the totals the Calcs summary and the GLA event read.
fn rollup(polys: &[CalcPoly], code_table: &[AreaCode], originals: &[Polygon]) -> Totals {
    let mut totals = Totals::default();
    for calc in polys {
        if calc.kind.is_empty() || calc.kind == "UND" {
            continue;
        }
        // A deducted area still counts as what it is: a garage inside the
        // house outline is garage area, and a room divided off a floor keeps
        // the floor's total. Only a negative (hole) area stays out, because
        // deducting it from its container already cuts the hole.
        if calc.subtracted_from_id.is_some()
            && calc.net <= 0.0
            && !originals.iter().any(|p| {
                p.id == calc.id
                    && p.attributes
                        .get("apex.calculation")
                        .is_some_and(|v| v == "net")
            })
        {
            continue;
        }
        let net = calc.net;
        *totals
            .totals_by_code
            .entry(calc.kind.clone())
            .or_insert(0.0) += net;
        let group = code_table
            .iter()
            .find(|c| c.code == calc.kind)
            .map(|c| c.group.clone())
            .unwrap_or_else(|| "OTH".to_string());
        *totals.totals_by_group.entry(group).or_insert(0.0) += net;
        let include_in_gla = code_table
            .iter()
            .find(|c| c.code == calc.kind)
            .map(|c| c.include_in_gla)
            .unwrap_or(false);
        if include_in_gla {
            totals.total_gla += net;
        }
        totals.total_all_areas += net;
    }
    totals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::area_codes::default_area_codes;
    use crate::geometry::Pt;
    use crate::model::Polygon;

    fn geo() -> GeometryEngine {
        GeometryEngine::default()
    }

    #[test]
    fn face_measurement_changes_area_and_perimeter_without_moving_centerlines() {
        let mut poly = square(1, 0.0, 0.0, 10.0, "GLA1");
        for (mode, expected_area, expected_perimeter) in [
            ("center", 100.0, 40.0),
            ("interior", 90.25, 38.0),
            ("exterior", 110.25, 42.0),
        ] {
            poly.measure_to = mode.into();
            let measured = face_measure_polygon(&poly, &[], true, &|_| 0.5);
            let result = recalculate(&[measured], &default_area_codes(), &geo());
            assert!((result.totals.total_gla - expected_area).abs() < 1e-9);
            assert!((result.polygons[0].perimeter - expected_perimeter).abs() < 1e-9);
            assert_eq!(poly.points[0], Pt::new(0.0, 0.0));
        }
    }

    #[test]
    fn face_offsets_follow_each_edge_in_either_winding() {
        let points = square(1, 0.0, 0.0, 10.0, "GLA1").points;
        let offsets = [-0.5, -1.0, -1.5, -2.0];
        let forward = crate::solids::offset_ring(&points, &offsets).unwrap();
        assert!((geo().calculate_area(&forward, &[]) - 56.0).abs() < 1e-9);
        let reverse_points: Vec<_> = points.iter().rev().copied().collect();
        let reverse_offsets = [-1.5, -1.0, -0.5, -2.0];
        let mut reverse = crate::solids::offset_ring(&reverse_points, &reverse_offsets).unwrap();
        reverse.reverse();
        assert_eq!(forward, reverse);
    }

    #[test]
    fn face_measurement_falls_back_when_disabled_malformed_or_collapsed() {
        let mut poly = square(1, 0.0, 0.0, 10.0, "GLA1");
        poly.measure_to = "interior".into();
        assert_eq!(face_measure_polygon(&poly, &[], false, &|_| 0.5), poly);
        assert_eq!(face_measure_polygon(&poly, &[], true, &|_| 12.0), poly);
        poly.edges.pop();
        assert_eq!(face_measure_polygon(&poly, &[], true, &|_| 0.5), poly);
        assert!(crate::solids::offset_ring(&[Pt::new(0.0, 0.0); 3], &[0.5; 3]).is_none());
    }

    #[test]
    fn face_measurement_uses_chords_for_bows_only_when_offset_succeeds() {
        let mut poly = square(1, 0.0, 0.0, 10.0, "GLA1");
        poly.edges[0].bow_angle = 30.0;
        poly.measure_to = "interior".into();
        let measured = face_measure_polygon(&poly, &[], true, &|_| 0.5);
        assert!(measured.edges.iter().all(|e| e.bow_angle == 0.0));
        assert!((geo().calculate_area(&measured.points, &measured.edges) - 90.25).abs() < 1e-9);
        assert_eq!(poly.edges[0].bow_angle, 30.0);
        assert_eq!(face_measure_polygon(&poly, &[], false, &|_| 0.5), poly);
    }

    #[test]
    fn face_measured_children_drive_subtraction_and_breakdown_geometry() {
        let outer = square(1, 0.0, 0.0, 20.0, "GLA1");
        let mut inner = square(2, 5.0, 5.0, 10.0, "GAR");
        inner.measure_to = "interior".into();
        let measured = face_measure_polygon(&inner, &[], true, &|_| 0.5);
        let result = recalculate(&[outer, measured], &default_area_codes(), &geo());
        assert!((result.polygons[0].subtracted - 90.25).abs() < 1e-9);
        assert!((result.totals.total_gla - 309.75).abs() < 1e-9);
        assert_eq!(result.polygons[1].points[0], Pt::new(5.25, 5.25));
    }

    fn square(id: i64, x: f64, y: f64, size: f64, kind: &str) -> Polygon {
        Polygon {
            id,
            points: vec![
                Pt { x, y },
                Pt { x: x + size, y },
                Pt {
                    x: x + size,
                    y: y + size,
                },
                Pt { x, y: y + size },
            ],
            edges: vec![Edge::default(); 4],
            kind: kind.into(),
            name: kind.into(),
            gross: 0.0,
            area: 0.0,
            factor_override: None,
            factor: 1.0,
            measure_to: "center".into(),
            subtract_mode: "auto".into(),
            subtract_target_id: None,
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
        }
    }

    #[test]
    fn area_levels_follow_the_code_shapes() {
        assert_eq!(area_level(Some("GLA1")), Some(1));
        assert_eq!(area_level(Some("GLA2")), Some(2));
        assert_eq!(area_level(Some("GBA10")), Some(10));
        assert_eq!(area_level(Some("BSMT")), Some(0));
        assert_eq!(area_level(Some("BSMT2")), Some(0));
        assert_eq!(area_level(Some("GAR")), Some(1));
        assert_eq!(area_level(Some("UND")), None);
        assert_eq!(area_level(Some("")), None);
        assert_eq!(area_level(None), None);
        // Same-level pairing: a garage on level 1 cannot deduct from floor 2.
        assert!(same_level(Some("GLA1"), Some("GAR")));
        assert!(!same_level(Some("GLA1"), Some("GLA2")));
        assert!(same_level(Some("GLA1"), Some("UND")));
    }

    #[test]
    fn a_nested_area_deducts_from_its_container() {
        let g = geo();
        // 40x40 house (1600 sf) with a 10x10 garage (100 sf) inside it.
        let polygons = vec![
            square(1, 0.0, 0.0, 40.0, "GLA1"),
            square(2, 5.0, 5.0, 10.0, "GAR"),
        ];
        let result = recalculate(&polygons, &default_area_codes(), &g);
        assert_eq!(result.warnings, Vec::<String>::new());
        let house = &result.polygons[0];
        let garage = &result.polygons[1];
        assert_eq!(garage.container_id, Some(1), "the garage sits in the house");
        assert_eq!(house.subtracted_from_id, None);
        assert!(
            (house.subtracted - 100.0).abs() < 1e-6,
            "house subtracts the garage"
        );
        assert!((house.net - 1500.0).abs() < 1e-6);
        assert!(
            (garage.net - 100.0).abs() < 1e-6,
            "the garage still counts as garage"
        );
        // GLA1 includes in GLA; GAR does not.
        assert!((result.totals.total_gla - 1500.0).abs() < 1e-6);
        assert!((result.totals.totals_by_code["GAR"] - 100.0).abs() < 1e-6);
        assert!((result.totals.totals_by_group["GAR"] - 100.0).abs() < 1e-6);
        assert!((result.totals.total_all_areas - 1600.0).abs() < 1e-6);
    }

    #[test]
    fn a_cross_level_area_does_not_deduct() {
        let g = geo();
        // A second-floor outline over the house is never taken out of floor 1.
        let polygons = vec![
            square(1, 0.0, 0.0, 40.0, "GLA1"),
            square(2, 10.0, 10.0, 10.0, "GLA2"),
        ];
        let result = recalculate(&polygons, &default_area_codes(), &g);
        assert_eq!(result.polygons[0].subtracted, 0.0);
        assert_eq!(result.polygons[1].container_id, None, "different level");
    }

    #[test]
    fn cyclic_subtraction_is_ignored_with_a_warning() {
        let g = geo();
        let mut a = square(1, 0.0, 0.0, 40.0, "GLA1");
        a.subtract_mode = "always".into();
        a.subtract_target_id = Some(2);
        let mut b = square(2, 5.0, 5.0, 10.0, "GAR");
        b.subtract_mode = "always".into();
        b.subtract_target_id = Some(1);
        let result = recalculate(&[a, b], &default_area_codes(), &g);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.starts_with("Cyclic subtraction involving area")));
        assert_eq!(result.polygons[0].subtracted_from_id, None);
        assert_eq!(result.polygons[0].subtracted, 0.0);
    }

    #[test]
    fn a_missing_always_target_warns_and_falls_back_to_spatial() {
        let g = geo();
        let mut a = square(1, 0.0, 0.0, 40.0, "GLA1");
        a.subtract_mode = "always".into();
        a.subtract_target_id = Some(99);
        let result = recalculate(&[a], &default_area_codes(), &g);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.starts_with("Invalid subtraction target for")));
        assert_eq!(result.polygons[0].subtracted_from_id, None, "no container");
    }

    #[test]
    fn the_factor_override_and_negative_codes_shape_the_net() {
        let g = geo();
        let mut poly = square(1, 0.0, 0.0, 10.0, "GLA1");
        poly.factor_override = Some(2.5);
        let result = recalculate(&[poly], &default_area_codes(), &g);
        assert!((result.polygons[0].net - 250.0).abs() < 1e-6);
        assert!((result.polygons[0].factor - 2.5).abs() < 1e-6);
    }

    #[test]
    fn a_negative_code_flips_the_sign() {
        let g = geo();
        // A hole: negative codes (positive: false) subtract.
        let hole = square(1, 0.0, 0.0, 10.0, "GLA1");
        let mut codes = default_area_codes();
        codes.push(AreaCode {
            code: "HOLE".into(),
            name: "Hole".into(),
            positive: false,
            group: "OTH".into(),
            ..codes.iter().find(|c| c.code == "OTH").expect("OTH").clone()
        });
        let result = recalculate(&[hole, square(2, 0.0, 0.0, 5.0, "HOLE")], &codes, &g);
        assert_eq!(result.polygons[1].sign, -1.0);
    }

    #[test]
    fn rollup_skips_und_and_negative_deducted_areas() {
        let g = geo();
        let polygons = vec![
            square(1, 0.0, 0.0, 40.0, "GLA1"),
            square(2, 5.0, 5.0, 10.0, "UND"),
        ];
        let result = recalculate(&polygons, &default_area_codes(), &g);
        assert!(!result.totals.totals_by_code.contains_key("UND"));
        // Verified live: sameLevel treats a null level (UND) as a wildcard, so
        // an undefined outline DOES deduct from its container — the house nets
        // 1500 — while UND itself stays out of the totals.
        assert!((result.totals.totals_by_code["GLA1"] - 1500.0).abs() < 1e-6);
        assert_eq!(result.polygons[1].container_id, Some(1));
        assert!(
            (result.polygons[1].net - 0.0).abs() < 1e-6,
            "UND's multiplier is 0"
        );
    }
}
