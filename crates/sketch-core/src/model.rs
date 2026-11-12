//! The sketch document model — the persisted shapes of refs/sketch-app.js
//! (`state.lines`, later polygons / labels / symbols / chains), with the
//! original's JSON field names so .skx files round-trip. Geometry work goes
//! through `geometry::GeometryEngine`, which takes the slimmer
//! `geometry::Line`; `SketchLine::geometry` converts.

use serde::{Deserialize, Serialize};

use crate::geometry::{Edge, Line, Pt};

/// `line.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WallType {
    #[default]
    Exterior,
    Interior,
}

/// A wall (`state.lines[]`), as handleCanvasClick / drawLine / autoClose
/// create it: `{ start, end, type, color, lineThickness, dimensionHidden,
/// layerId, id }`, plus the optional bow / style / pinned-label fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SketchLine {
    pub id: i64,
    pub start: Pt,
    pub end: Pt,
    #[serde(rename = "type", default)]
    pub kind: WallType,
    /// Stamped at creation (`getDefaultLineStyle(type).color || drawColor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Stroke width in px (`lineThickness`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_thickness: Option<f64>,
    /// Per-line wall thickness in INCHES (`thickness`) — wins over the
    /// options' exterior/interior thickness for wall solids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thickness: Option<f64>,
    #[serde(default)]
    pub dimension_hidden: bool,
    /// `layerId` — the original's layers are named ids ("layer-1"), not
    /// numbers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub bow_angle: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub arc_height: f64,
    /// `solid` (absent) / `dashed` / `dotted`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension_position: Option<Pt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimension_anchor: Option<Pt>,
    /// Keys this build does not model (Sketchy residue such as `sketchy_id`
    /// or `sketchy_other`), kept so a save writes them back unchanged.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

/// Residual key naming an element's id in an opened Sketchy file.
pub const SKETCHY_ID_KEY: &str = "sketchy_id";

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl SketchLine {
    /// Forget the Sketchy id: a copy of this wall is a new element and must
    /// not reuse the original's id when written back.
    pub fn forget_sketchy_id(&mut self) {
        self.extra.remove(SKETCHY_ID_KEY);
    }

    /// The geometry engine's view of this wall.
    pub fn geometry(&self) -> Line {
        Line {
            id: self.id,
            start: self.start,
            end: self.end,
            bow_angle: self.bow_angle,
            dimension_position: self.dimension_position,
            dimension_anchor: self.dimension_anchor,
        }
    }
}

/// Geometry views of a set of walls.
pub fn geometry_lines(lines: &[SketchLine]) -> Vec<Line> {
    lines.iter().map(SketchLine::geometry).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_json_uses_the_original_field_names() {
        let line = SketchLine {
            id: 1,
            start: Pt::new(-11.0, 3.0),
            end: Pt::new(-1.0, 3.0),
            kind: WallType::Exterior,
            color: Some("#2B6CB0".into()),
            line_thickness: Some(4.0),
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
        let json = serde_json::to_value(&line).unwrap();
        assert_eq!(json["type"], "exterior");
        assert!(json.get("layerId").is_none());
        assert_eq!(json["lineThickness"], 4.0);
        assert_eq!(json["dimensionHidden"], false);
        assert!(json.get("bowAngle").is_none());
        let back: SketchLine = serde_json::from_value(serde_json::json!({
            "id": 2, "start": {"x": 0, "y": 0}, "end": {"x": 5, "y": 0},
            "type": "interior", "bowAngle": 30, "someFutureField": true
        }))
        .unwrap();
        assert_eq!(
            (back.kind, back.bow_angle, back.color.clone()),
            (WallType::Interior, 30.0, None)
        );
        assert_eq!(back.geometry().end, Pt::new(5.0, 0.0));

        // Layer ids are strings in the original ("layer-1").
        let layered: SketchLine = serde_json::from_value(serde_json::json!({
            "id": 3, "start": {"x": 0, "y": 0}, "end": {"x": 1, "y": 0},
            "type": "exterior", "layerId": "layer-1"
        }))
        .unwrap();
        assert_eq!(layered.layer_id.as_deref(), Some("layer-1"));
        let area: Polygon = serde_json::from_value(serde_json::json!({
            "id": 9, "points": [{"x":0,"y":0},{"x":1,"y":0},{"x":1,"y":1}],
            "type": "UND", "name": "Undefined Area", "area": 0.5,
            "edges": [{"lineId": 1, "bowAngle": 0}], "layerId": "layer-6",
            "subtractMode": "auto"
        }))
        .unwrap();
        assert_eq!(
            (
                area.kind.as_str(),
                area.name.as_str(),
                area.layer_id.as_deref()
            ),
            ("UND", "Undefined Area", Some("layer-6"))
        );
        assert_eq!(area.edges[0].line_id, Some(1));
    }
}

/// A closed area (`state.polygons[]`) as autoClose creates it: the chain's
/// points, one edge per point (the wall it follows, with the bow angle in
/// the path's direction), the area code, the computed area and the layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Polygon {
    pub id: i64,
    pub points: Vec<Pt>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// `type`: an area code (`UND` until one is applied).
    #[serde(rename = "type", default = "undefined_code")]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    /// The area as drawn (before any factor).
    #[serde(default)]
    pub gross: f64,
    #[serde(default)]
    pub area: f64,
    #[serde(default)]
    pub factor_override: Option<f64>,
    /// `auto` (default) or `never`.
    #[serde(default = "auto_subtract")]
    pub subtract_mode: String,
    #[serde(default)]
    pub subtract_target_id: Option<i64>,
    /// `factor` — the applied multiplier (`factorOverride` when set, else 1).
    #[serde(default = "one_f64")]
    pub factor: f64,
    /// `measureTo`: `center` (default), `interior` or `exterior`.
    #[serde(default = "measure_to_default")]
    pub measure_to: String,
    /// `autoPost`: which labels the area prints (`updateSelectedAreaProperties`).
    #[serde(default)]
    pub auto_post: AutoPost,
    /// `attributes`: the Area Attributes dialog's bag (plan ref, floor code,
    /// year built…), merged rather than replaced.
    #[serde(default)]
    pub attributes: std::collections::BTreeMap<String, String>,
    /// `layerId` (a layer name like "layer-6"; autoClose assigns the area to
    /// a dedicated "Area" layer — screen 8/9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_opacity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_color: Option<String>,
    /// `lineStyle`: `solid` (absent) / `dashed` / `dotted` outline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_style: Option<String>,
    /// `lineThickness`: the outline's stroke width in px.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_thickness: Option<f64>,
    /// `labelRotation`: the area label's rotation in degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_rotation: Option<f64>,
    /// `areaSizeRotation`: the size text's rotation (falls back to the label's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_size_rotation: Option<f64>,
    /// `labelPosition`: where the area's label block sits (world); absent =
    /// the area's label anchor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_position: Option<Pt>,
    /// `areaSizePosition`: where the size block sits; absent = below the label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_size_position: Option<Pt>,
    /// `labelScale` (0.5..4) and `areaSizeScale` (falls back to the label's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_scale: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area_size_scale: Option<f64>,
    /// The label's own typography (`labelFontSize`, `labelFontFamily`,
    /// `labelBold`, `labelItalic`, `labelUnderline`, `labelColor`); absent =
    /// the Options auto-post settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_font_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_font_family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_bold: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_color: Option<String>,
    /// Keys this build does not model (Sketchy residue such as `sketchy_id`
    /// or `sketchy_other`), kept so a save writes them back unchanged.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Polygon {
    /// Forget the Sketchy id: a copy of this area is a new element and must
    /// not reuse the original's id when written back.
    pub fn forget_sketchy_id(&mut self) {
        self.extra.remove(SKETCHY_ID_KEY);
    }
}

fn one_f64() -> f64 {
    1.0
}

fn measure_to_default() -> String {
    "center".into()
}

/// `poly.autoPost`: the label switches the Area Properties dialog sets. The
/// defaults are the original's own (Name, Dims and Calcs on; Code, Base and
/// Factor off; suffix "sf").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoPost {
    #[serde(default)]
    pub code: bool,
    #[serde(default = "yes")]
    pub name: bool,
    #[serde(default = "yes")]
    pub dims: bool,
    #[serde(default = "yes")]
    pub calcs: bool,
    #[serde(default)]
    pub base: bool,
    #[serde(default)]
    pub factor: bool,
    #[serde(default = "suffix_default")]
    pub suffix: String,
}

fn yes() -> bool {
    true
}

fn suffix_default() -> String {
    "sf".into()
}

impl Default for AutoPost {
    fn default() -> Self {
        AutoPost {
            code: false,
            name: true,
            dims: true,
            calcs: true,
            base: false,
            factor: false,
            suffix: suffix_default(),
        }
    }
}

fn undefined_code() -> String {
    "UND".into()
}

fn auto_subtract() -> String {
    "auto".into()
}

impl Polygon {
    /// `buildPolygonEdges(poly, lines)`: for each point pair, the wall that
    /// joins them (either direction) with its bow angle signed by the path's
    /// direction; `{ lineId: null }` when no wall joins them.
    pub fn build_edges(points: &[Pt], lines: &[SketchLine]) -> Vec<Edge> {
        points
            .iter()
            .enumerate()
            .map(|(i, start)| {
                let end = points[(i + 1) % points.len()];
                let line = lines.iter().find(|l| {
                    (same(l.start, *start) && same(l.end, end))
                        || (same(l.start, end) && same(l.end, *start))
                });
                match line {
                    Some(l) => Edge {
                        line_id: Some(l.id),
                        bow_angle: if same(l.start, *start) {
                            l.bow_angle
                        } else {
                            -l.bow_angle
                        },
                    },
                    None => Edge::default(),
                }
            })
            .collect()
    }

    /// The `closedAreaVertexKeys()` this area contributes: every vertex of an
    /// intact closed area, so new walls never snap onto it.
    pub fn vertex_keys(&self) -> Vec<String> {
        self.points
            .iter()
            .map(|p| format!("{:.5},{:.5}", p.x, p.y))
            .collect()
    }
}

/// `pointsEqual` (tolerance 0.0001).
pub fn same(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() <= 0.0001 && (a.y - b.y).abs() <= 0.0001
}
