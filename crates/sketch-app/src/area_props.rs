//! Area properties and attributes (screen 7): `updateSelectedAreaProperties`,
//! `updateSelectedAreaAttributes` and `toggleSelectedAreaAutoSubtract`
//! (refs/sketch-app.js:1010-1050).
//!
//! The dialog itself is `showSelectedAreaProperties` (measured into
//! `refs/ui-spec/area-properties-spec.json`); this module owns what Apply does
//! to the model, which is the part every other area feature depends on.

use sketch_core::model::{AutoPost, Polygon};

use crate::drawing::Drawing;
use crate::selection::Selection;

/// The Area Properties dialog's fields. `None` means "leave as it is" for the
/// optional ones, matching the original's `properties.x != null` guards.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AreaProperties {
    /// `type`: the area code. Empty leaves the code alone.
    pub code: String,
    /// `name`. Empty leaves the name alone.
    pub name: String,
    /// `factor`. `None` is the original's `undefined` (`Number(undefined)` is
    /// NaN, so `factorOverride` clears); a **dialog passes `Some(0.0)` for an
    /// empty field**, because `Number('')` is 0 in the original — an empty
    /// Factor therefore means factor 0, which is the original's own quirk and
    /// is reproduced rather than corrected.
    pub factor: Option<f64>,
    /// `autoSubtract`: false means "None" (`subtractMode: 'never'`).
    pub auto_subtract: bool,
    /// `measureTo`: `center`, `interior` or `exterior`.
    pub measure_to: Option<String>,
    /// `autoPost`: only the switches the dialog touched.
    pub auto_post: Option<AutoPost>,
}

impl AreaProperties {
    /// The values the dialog opens with for the first selected area.
    #[allow(dead_code)] // the dialog opens with these values
    pub fn from_area(poly: &Polygon) -> AreaProperties {
        AreaProperties {
            code: poly.kind.clone(),
            name: poly.name.clone(),
            factor: poly.factor_override,
            auto_subtract: poly.subtract_mode != "never",
            measure_to: Some(poly.measure_to.clone()),
            auto_post: Some(poly.auto_post.clone()),
        }
    }
}

/// `updateSelectedAreaProperties(properties)`: apply to every selected area.
/// Returns false when nothing is selected, so the caller can skip the undo
/// step exactly as the original does.
#[allow(dead_code)] // the dialog widget lands next (spec measured)
pub fn apply_properties(d: &mut Drawing, sel: &Selection, props: &AreaProperties) -> bool {
    if sel.polys.is_empty() {
        return false;
    }
    for poly in d.polygons.iter_mut().filter(|p| sel.polys.contains(&p.id)) {
        if !props.code.is_empty() {
            poly.kind = props.code.clone();
        }
        if !props.name.is_empty() {
            poly.name = props.name.clone();
        }
        // `factorOverride`: a finite number sets it, anything else clears it.
        poly.factor_override = props.factor.filter(|f| f.is_finite());
        poly.factor = match poly.factor_override {
            Some(f) => f,
            None => {
                if poly.factor == 0.0 {
                    1.0
                } else {
                    poly.factor
                }
            }
        };
        poly.subtract_mode = if props.auto_subtract {
            "auto".to_string()
        } else {
            "never".to_string()
        };
        if let Some(measure) = &props.measure_to {
            poly.measure_to = if ["center", "interior", "exterior"].contains(&measure.as_str()) {
                measure.clone()
            } else {
                "center".to_string()
            };
        }
        if let Some(post) = &props.auto_post {
            poly.auto_post = post.clone();
        }
    }
    // The dims switch hides or shows every wall on the selected areas' edges.
    if let Some(post) = &props.auto_post {
        let edges: std::collections::HashSet<i64> = d
            .polygons
            .iter()
            .filter(|p| sel.polys.contains(&p.id))
            .flat_map(|p| p.edges.iter().filter_map(|e| e.line_id))
            .collect();
        for line in d.lines.iter_mut().filter(|l| edges.contains(&l.id)) {
            line.dimension_hidden = !post.dims;
        }
    }
    true
}

/// `updateSelectedAreaAttributes(attributes)`: merge into every selected
/// area's `attributes` bag.
#[allow(dead_code)] // the attributes dialog lands with the properties one
pub fn apply_attributes(d: &mut Drawing, sel: &Selection, attributes: &[(String, String)]) -> bool {
    if sel.polys.is_empty() {
        return false;
    }
    for poly in d.polygons.iter_mut().filter(|p| sel.polys.contains(&p.id)) {
        for (key, value) in attributes {
            poly.attributes.insert(key.clone(), value.clone());
        }
    }
    true
}

/// `toggleSelectedAreaAutoSubtract`: if any selected area is `never`, turn
/// them all on; otherwise turn them all off. Returns the toast the original
/// shows, or None when nothing is selected.
pub fn toggle_auto_subtract(d: &mut Drawing, sel: &Selection) -> Option<String> {
    if sel.polys.is_empty() {
        return None;
    }
    let enable = d
        .polygons
        .iter()
        .filter(|p| sel.polys.contains(&p.id))
        .any(|p| p.subtract_mode == "never");
    for poly in d.polygons.iter_mut().filter(|p| sel.polys.contains(&p.id)) {
        poly.subtract_mode = if enable { "auto" } else { "never" }.to_string();
    }
    let count = sel.polys.len();
    Some(format!(
        "Auto-Subtract {} for selected area{}.",
        if enable { "enabled" } else { "disabled" },
        if count == 1 { "" } else { "s" }
    ))
}

/// The Edit Tools area controls (`setSelectedAreaStyle(property, value)`).
#[derive(Debug, Clone, PartialEq)]
pub enum AreaStyle {
    FillPattern(String),
    FillColor(String),
    LineStyle(String),
    LineThickness(f64),
    LineColor(String),
}

/// Which select the ribbon cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaCycle {
    FillPattern,
    LineStyle,
    LineThickness,
}

pub const FILL_PATTERNS: [&str; 5] = ["solid", "none", "diagonal", "crosshatch", "dots"];
pub const LINE_STYLES: [&str; 3] = ["solid", "dashed", "dotted"];
pub const LINE_THICKNESSES: [f64; 5] = [1.0, 2.0, 3.0, 4.0, 6.0];

/// The values the controls show for an area (`syncContextualControls`).
pub fn shown_pattern(poly: &Polygon) -> &str {
    poly.fill_pattern
        .as_deref()
        .filter(|p| !p.is_empty())
        .unwrap_or("solid")
}

pub fn shown_line_style(poly: &Polygon) -> &str {
    poly.line_style
        .as_deref()
        .filter(|p| !p.is_empty())
        .unwrap_or("solid")
}

pub fn shown_thickness(poly: &Polygon) -> f64 {
    poly.line_thickness.filter(|t| *t != 0.0).unwrap_or(1.0)
}

/// The next value after `current` in `list` (the select's next option).
pub fn next_after<T: PartialEq + Copy>(list: &[T], current: T) -> T {
    let at = list.iter().position(|v| *v == current).unwrap_or(0);
    list[(at + 1) % list.len()]
}

/// `setSelectedAreaStyle`: the property goes on every selected area, and the
/// three outline properties also restyle the walls the areas' edges ride
/// (`lineColor` lands on the wall's `color`). Returns false with nothing
/// selected.
pub fn set_selected_area_style(d: &mut Drawing, sel: &Selection, style: &AreaStyle) -> bool {
    if sel.polys.is_empty() {
        return false;
    }
    let mut line_ids = std::collections::HashSet::new();
    for poly in d.polygons.iter_mut().filter(|p| sel.polys.contains(&p.id)) {
        match style {
            AreaStyle::FillPattern(v) => poly.fill_pattern = Some(v.clone()),
            AreaStyle::FillColor(v) => poly.fill_color = Some(v.clone()),
            AreaStyle::LineStyle(v) => poly.line_style = Some(v.clone()),
            AreaStyle::LineThickness(v) => poly.line_thickness = Some(*v),
            AreaStyle::LineColor(v) => poly.line_color = Some(v.clone()),
        }
        line_ids.extend(poly.edges.iter().filter_map(|e| e.line_id));
    }
    for line in d.lines.iter_mut().filter(|l| line_ids.contains(&l.id)) {
        match style {
            // "solid" is the absence of a style, as the walls store it.
            AreaStyle::LineStyle(v) => {
                line.line_style = if v == "solid" { None } else { Some(v.clone()) }
            }
            AreaStyle::LineThickness(v) => line.line_thickness = Some(*v),
            AreaStyle::LineColor(v) => line.color = Some(v.clone()),
            AreaStyle::FillPattern(_) | AreaStyle::FillColor(_) => {}
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use sketch_core::geometry::{Edge, Pt};
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

    fn area() -> (Drawing, Selection) {
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
        let edges: Vec<Edge> = (0..4)
            .map(|i| Edge {
                line_id: Some(i as i64 + 1),
                bow_angle: 0.0,
            })
            .collect();
        let mut poly = Polygon {
            id: 100,
            points,
            edges,
            kind: "GLA1".into(),
            name: "First Floor".into(),
            gross: 100.0,
            area: 100.0,
            factor_override: None,
            subtract_mode: "auto".into(),
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
        };
        poly.attributes.clear();
        d.polygons = vec![poly];
        let sel = Selection {
            polys: [100].into_iter().collect(),
            ..Selection::default()
        };
        (d, sel)
    }

    #[test]
    fn applying_properties_sets_every_field() {
        let (mut d, sel) = area();
        let props = AreaProperties {
            code: "GLA2".into(),
            name: "Second Floor".into(),
            factor: Some(0.5),
            auto_subtract: false,
            measure_to: Some("interior".into()),
            auto_post: Some(AutoPost {
                code: true,
                dims: false,
                suffix: "SF".into(),
                ..AutoPost::default()
            }),
        };
        assert!(apply_properties(&mut d, &sel, &props));
        let poly = &d.polygons[0];
        assert_eq!(poly.kind, "GLA2");
        assert_eq!(poly.name, "Second Floor");
        assert_eq!(poly.factor_override, Some(0.5));
        assert_eq!(poly.factor, 0.5);
        assert_eq!(poly.subtract_mode, "never");
        assert_eq!(poly.measure_to, "interior");
        assert!(poly.auto_post.code);
        assert_eq!(poly.auto_post.suffix, "SF");
        // `autoPost.dims === false` hides the area's boundary dimensions.
        assert!(d.lines.iter().all(|l| l.dimension_hidden));
    }

    #[test]
    fn an_empty_code_or_name_leaves_the_old_one() {
        let (mut d, sel) = area();
        let props = AreaProperties {
            auto_subtract: true,
            ..AreaProperties::default()
        };
        assert!(apply_properties(&mut d, &sel, &props));
        assert_eq!(d.polygons[0].kind, "GLA1", "an empty code is ignored");
        assert_eq!(d.polygons[0].name, "First Floor");
    }

    #[test]
    fn an_undefined_factor_clears_the_override_but_keeps_a_factor() {
        // The live original with `factor: undefined` -> `factorOverride: null`
        // and the applied factor untouched. (`factor: null` is different: JS
        // coerces it to 0, which the dialog sends as `Some(0.0)`.)
        let (mut d, sel) = area();
        d.polygons[0].factor = 1.25;
        let props = AreaProperties {
            factor: None,
            ..AreaProperties::default()
        };
        assert!(apply_properties(&mut d, &sel, &props));
        assert_eq!(d.polygons[0].factor_override, None);
        assert_eq!(d.polygons[0].factor, 1.25, "the applied factor survives");
        // The dialog's empty field: `Number('') === 0` in the original.
        let props = AreaProperties {
            factor: Some(0.0),
            ..AreaProperties::default()
        };
        assert!(apply_properties(&mut d, &sel, &props));
        assert_eq!(d.polygons[0].factor_override, Some(0.0));
        assert_eq!(
            d.polygons[0].factor, 0.0,
            "the original's quirk, reproduced"
        );
    }

    #[test]
    fn an_unknown_measure_to_falls_back_to_center() {
        let (mut d, sel) = area();
        let props = AreaProperties {
            measure_to: Some("diagonal".into()),
            ..AreaProperties::default()
        };
        assert!(apply_properties(&mut d, &sel, &props));
        assert_eq!(d.polygons[0].measure_to, "center");
    }

    #[test]
    fn toggling_auto_subtract_flips_the_whole_selection() {
        let (mut d, sel) = area();
        // All on -> all off.
        let toast = toggle_auto_subtract(&mut d, &sel).expect("toast");
        assert_eq!(toast, "Auto-Subtract disabled for selected area.");
        assert_eq!(d.polygons[0].subtract_mode, "never");
        // Any 'never' -> all on.
        let toast = toggle_auto_subtract(&mut d, &sel).expect("toast");
        assert_eq!(toast, "Auto-Subtract enabled for selected area.");
        assert_eq!(d.polygons[0].subtract_mode, "auto");
        // Nothing selected: no toast, no change.
        assert!(toggle_auto_subtract(&mut d, &Selection::default()).is_none());
        // The plural form.
        let mut sel2 = sel.clone();
        sel2.polys.insert(101);
        d.polygons.push(d.polygons[0].clone());
        d.polygons[1].id = 101;
        let toast = toggle_auto_subtract(&mut d, &sel2).expect("toast");
        assert_eq!(toast, "Auto-Subtract disabled for selected areas.");
    }

    #[test]
    fn attributes_merge_into_every_selected_area() {
        let (mut d, sel) = area();
        let attrs = vec![
            ("planRef".to_string(), "A-1".to_string()),
            ("floorCode".to_string(), "1".to_string()),
        ];
        assert!(apply_attributes(&mut d, &sel, &attrs));
        assert_eq!(
            d.polygons[0].attributes.get("planRef"),
            Some(&"A-1".to_string())
        );
        // A second apply merges rather than replaces.
        assert!(apply_attributes(
            &mut d,
            &sel,
            &[("yearBuilt".to_string(), "1998".to_string())]
        ));
        assert_eq!(d.polygons[0].attributes.len(), 3);
        assert!(!apply_attributes(&mut d, &Selection::default(), &attrs));
    }

    #[test]
    fn area_style_goes_on_the_area_and_restyles_its_walls() {
        let (mut d, sel) = area();
        d.lines.push(wall(99, 50.0, 0.0, 60.0, 0.0));
        assert!(set_selected_area_style(
            &mut d,
            &sel,
            &AreaStyle::LineStyle("dashed".into())
        ));
        assert!(set_selected_area_style(
            &mut d,
            &sel,
            &AreaStyle::LineThickness(3.0)
        ));
        assert!(set_selected_area_style(
            &mut d,
            &sel,
            &AreaStyle::LineColor("#112233".into())
        ));
        assert!(set_selected_area_style(
            &mut d,
            &sel,
            &AreaStyle::FillPattern("dots".into())
        ));
        let poly = &d.polygons[0];
        assert_eq!(poly.line_style.as_deref(), Some("dashed"));
        assert_eq!(poly.line_thickness, Some(3.0));
        assert_eq!(poly.fill_pattern.as_deref(), Some("dots"));
        for id in 1..=4 {
            let l = d.lines.iter().find(|l| l.id == id).unwrap();
            assert_eq!(l.line_style.as_deref(), Some("dashed"));
            assert_eq!(l.line_thickness, Some(3.0));
            assert_eq!(l.color.as_deref(), Some("#112233"));
        }
        let other = d.lines.iter().find(|l| l.id == 99).unwrap();
        assert!(
            other.line_style.is_none() && other.color.is_none(),
            "unrelated wall untouched"
        );
        // "solid" is stored as no style on the walls.
        set_selected_area_style(&mut d, &sel, &AreaStyle::LineStyle("solid".into()));
        assert!(d.lines[0].line_style.is_none());
        assert_eq!(d.polygons[0].line_style.as_deref(), Some("solid"));
        // Nothing selected: refused.
        assert!(!set_selected_area_style(
            &mut d,
            &Selection::default(),
            &AreaStyle::FillColor("#fff".into())
        ));
    }

    #[test]
    fn the_selects_cycle_through_their_options() {
        assert_eq!(next_after(&FILL_PATTERNS, "solid"), "none");
        assert_eq!(next_after(&FILL_PATTERNS, "dots"), "solid");
        assert_eq!(next_after(&LINE_STYLES, "dotted"), "solid");
        assert_eq!(next_after(&LINE_THICKNESSES, 4.0), 6.0);
        assert_eq!(next_after(&LINE_THICKNESSES, 6.0), 1.0);
        assert_eq!(
            next_after(&LINE_THICKNESSES, 99.0),
            2.0,
            "unknown starts from the first"
        );
    }
}
