//! The Calcs panel's numbers (screen 7, S7-002): `updateCalcsPanel`.
//!
//! The original builds a tree grouped by area code: each group shows the sum of
//! its areas' net values, and each area expands to Base Area / Factor / Total /
//! (Subtracted) / Net Area / Perimeter. Undefined (`UND`) areas are excluded
//! from the groups and counted separately in the summary.
//!
//! The computed values come from `sketch_core::area_calc::recalculate` (the
//! ported `AreaCalc.recalculate`); before the first pass the map is empty and
//! every area's net equals its total, exactly as the original behaves before
//! its first `recalcAutoSubtract`.

use sketch_core::geometry::GeometryEngine;
use sketch_core::model::Polygon;

/// One area's row in the tree.
///
/// The model is complete and verified against the original; the dock's tree
/// widget that consumes it is the next step, so until then only the tests
/// construct these.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaRow {
    pub id: i64,
    /// `poly.name || code`.
    pub name: String,
    pub base_area: f64,
    pub factor: f64,
    pub total: f64,
    /// The `Subtracted` row, when the area has a deduction.
    pub subtracted: Option<f64>,
    pub net_area: f64,
    pub perimeter: f64,
}

/// One area code's group.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeGroup {
    pub code: String,
    /// The sum of the group's net areas.
    pub total: f64,
    pub areas: Vec<AreaRow>,
}

/// The whole panel.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Calcs {
    pub groups: Vec<CodeGroup>,
    /// `state.polygons.filter(p => p.type === 'UND').length`.
    pub und_count: usize,
}

impl Calcs {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// The panel's own empty-state text.
    pub fn empty_note(&self) -> &'static str {
        "No defined areas yet."
    }
}

/// `updateCalcsPanel`'s grouping. Areas are grouped in the order their codes
/// first appear, as the original's `Map` does.
/// One polygon's computed values from the recalculation pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Computed {
    pub subtracted: f64,
    pub factor: f64,
    pub net: f64,
    pub perimeter: f64,
}

pub fn build(polygons: &[Polygon], computed: &std::collections::BTreeMap<i64, Computed>) -> Calcs {
    let geo = GeometryEngine::new();
    let defined: Vec<&Polygon> = polygons
        .iter()
        .filter(|p| !p.kind.is_empty() && p.kind != "UND")
        .collect();
    let und_count = polygons.iter().filter(|p| p.kind == "UND").count();

    let mut groups: Vec<CodeGroup> = Vec::new();
    for poly in &defined {
        let idx = match groups.iter().position(|g| g.code == poly.kind) {
            Some(i) => i,
            None => {
                groups.push(CodeGroup {
                    code: poly.kind.clone(),
                    total: 0.0,
                    areas: Vec::new(),
                });
                groups.len() - 1
            }
        };
        let base_area = poly.gross;
        let computed_row = computed.get(&poly.id).copied();
        let factor = if poly.factor.is_finite() && poly.factor != 0.0 {
            poly.factor
        } else {
            poly.factor_override.unwrap_or(1.0)
        };
        let factor = if factor.is_finite() { factor } else { 1.0 };
        let total = base_area * factor;
        // With a recalculation pass the Total is the raw product and Net Area
        // is the post-subtraction net; without one the net equals the total.
        let (total, net_area, subtracted) = match computed_row {
            Some(row) => (base_area * row.factor, row.net, Some(row.subtracted)),
            None => (total, total, None),
        };
        let perimeter = computed_row.map_or_else(
            || geo.calculate_perimeter(&poly.points, &poly.edges),
            |row| row.perimeter,
        );
        groups[idx].areas.push(AreaRow {
            id: poly.id,
            name: if poly.name.is_empty() {
                poly.kind.clone()
            } else {
                poly.name.clone()
            },
            base_area,
            factor,
            total,
            subtracted,
            net_area,
            perimeter,
        });
    }
    for group in groups.iter_mut() {
        group.total = group.areas.iter().map(|a| a.net_area).sum();
    }
    Calcs { groups, und_count }
}

/// `app.areaTotals`, which `recalculate()` produces. Absent means the totals
/// have not been computed, and the original falls back to zero:
/// `const totals = this.app.areaTotals || { totalGLA: 0, totalAllAreas: 0 }`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AreaTotals {
    pub total_gla: f64,
    /// `totalsByGroup.GBA`.
    pub gba: f64,
}

/// One summary line.
#[derive(Debug, Clone, PartialEq)]
pub struct SummaryRow {
    pub label: &'static str,
    pub value: String,
    /// The Undefined row prints in `#E53E3E`.
    pub alarm: bool,
}

/// `updateCalcsPanel`'s summary block: Defined Areas, Undefined Areas (only
/// when there are any), Total Gross Living Area and Total Gross Building Area,
/// the last two to one decimal place with the original's " sf" suffix.
pub fn summary(calcs: &Calcs, totals: Option<AreaTotals>) -> Vec<SummaryRow> {
    let totals = totals.unwrap_or_default();
    let defined: usize = calcs.groups.iter().map(|g| g.areas.len()).sum();
    let mut out = vec![SummaryRow {
        label: "Defined Areas",
        value: format!("{defined}"),
        alarm: false,
    }];
    if calcs.und_count > 0 {
        out.push(SummaryRow {
            label: "Undefined Areas",
            value: format!("{}", calcs.und_count),
            alarm: true,
        });
    }
    out.push(SummaryRow {
        label: "Total Gross Living Area",
        value: format!("{:.1} sf", totals.total_gla),
        alarm: false,
    });
    out.push(SummaryRow {
        label: "Total Gross Building Area",
        value: format!("{:.1} sf", totals.gba),
        alarm: false,
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketch_core::geometry::{Edge, Pt};

    fn area(id: i64, code: &str, name: &str, gross: f64, factor: f64) -> Polygon {
        let points = vec![
            Pt::new(0.0, 0.0),
            Pt::new(10.0, 0.0),
            Pt::new(10.0, 10.0),
            Pt::new(0.0, 10.0),
        ];
        Polygon {
            id,
            points,
            edges: vec![
                Edge {
                    line_id: None,
                    bow_angle: 0.0,
                },
                Edge {
                    line_id: None,
                    bow_angle: 0.0,
                },
                Edge {
                    line_id: None,
                    bow_angle: 0.0,
                },
                Edge {
                    line_id: None,
                    bow_angle: 0.0,
                },
            ],
            kind: code.into(),
            name: name.into(),
            gross,
            area: gross,
            factor_override: None,
            subtract_mode: "auto".into(),
            subtract_target_id: None,
            factor,
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
        }
    }

    #[test]
    fn areas_group_by_code_in_first_seen_order() {
        let polys = vec![
            area(1, "GLA1", "First Floor", 1000.0, 1.0),
            area(2, "GAR", "Garage", 400.0, 1.0),
            area(3, "GLA1", "Wing", 250.0, 1.0),
            area(4, "UND", "Undefined Area", 100.0, 1.0),
        ];
        let calcs = build(&polys, &std::collections::BTreeMap::new());
        assert_eq!(calcs.groups.len(), 2);
        assert_eq!(calcs.groups[0].code, "GLA1");
        assert_eq!(calcs.groups[0].areas.len(), 2);
        assert_eq!(calcs.groups[0].total, 1250.0);
        assert_eq!(calcs.groups[1].code, "GAR");
        assert_eq!(calcs.und_count, 1, "UND is counted, not grouped");
        assert!(!calcs.is_empty());
    }

    #[test]
    fn a_row_carries_base_factor_total_net_and_perimeter() {
        let polys = vec![area(1, "GLA1", "First Floor", 1000.0, 0.5)];
        let calcs = build(&polys, &std::collections::BTreeMap::new());
        let row = &calcs.groups[0].areas[0];
        assert_eq!(row.name, "First Floor");
        assert_eq!(row.base_area, 1000.0);
        assert_eq!(row.factor, 0.5);
        assert_eq!(row.total, 500.0);
        assert_eq!(row.net_area, 500.0, "no subtraction yet");
        assert_eq!(row.perimeter, 40.0, "a 10x10 square");
        assert_eq!(row.subtracted, None);
    }

    #[test]
    fn an_unnamed_area_uses_its_code() {
        let polys = vec![area(1, "GAR", "", 400.0, 1.0)];
        let calcs = build(&polys, &std::collections::BTreeMap::new());
        assert_eq!(calcs.groups[0].areas[0].name, "GAR");
    }

    #[test]
    fn an_empty_sketch_has_the_originals_note() {
        let calcs = build(&[], &std::collections::BTreeMap::new());
        assert!(calcs.is_empty());
        assert_eq!(calcs.empty_note(), "No defined areas yet.");
        // The summary still prints its four rows on an empty sketch — only the
        // tree shows the note.
        assert_eq!(summary(&calcs, None).len(), 3, "no UND row");
    }

    #[test]
    fn the_summary_matches_the_originals_four_rows() {
        // The live original with three defined areas and one UND prints
        // "Defined Areas 3 / Undefined Areas 1 / Total Gross Living Area 0.0 sf
        // / Total Gross Building Area 0.0 sf" when `areaTotals` is unset.
        let polys = vec![
            area(1, "GLA1", "First", 1000.0, 1.0),
            area(2, "GLA2", "Second", 500.0, 1.0),
            area(3, "GAR", "Garage", 400.0, 1.0),
            area(4, "UND", "", 100.0, 1.0),
        ];
        let calcs = build(&polys, &std::collections::BTreeMap::new());
        let rows = summary(&calcs, None);
        assert_eq!(rows.len(), 4);
        assert_eq!(
            (rows[0].label, rows[0].value.as_str()),
            ("Defined Areas", "3")
        );
        assert_eq!(
            (rows[1].label, rows[1].value.as_str(), rows[1].alarm),
            ("Undefined Areas", "1", true)
        );
        assert_eq!(rows[2].value, "0.0 sf", "no totals yet, as the original");
        assert_eq!(rows[3].value, "0.0 sf");
        // With totals the two area rows carry them to one decimal place.
        let rows = summary(
            &calcs,
            Some(AreaTotals {
                total_gla: 1500.0,
                gba: 1900.0,
            }),
        );
        assert_eq!(rows[2].value, "1500.0 sf");
        assert_eq!(rows[3].value, "1900.0 sf");
        // No UND areas: the Undefined row is absent, as the original's
        // conditional does.
        let rows = summary(
            &build(&polys[..3], &std::collections::BTreeMap::new()),
            None,
        );
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.label != "Undefined Areas"));
    }
}
