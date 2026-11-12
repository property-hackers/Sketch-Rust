//! Placed text labels on the canvas (Screen 9).

use serde::{Deserialize, Serialize};
use sketch_core::geometry::Pt;

/// `state.labels[]` — the save model uses the original's camelCase keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedText {
    pub id: i64,
    pub text: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub rotation: f64,
    #[serde(default = "default_font_family")]
    pub font_family: String,
    #[serde(default = "default_font_size")]
    pub font_size: f64,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub underline: bool,
    /// Empty = automatic (`color: null` in the original).
    #[serde(default = "default_color", deserialize_with = "null_as_empty")]
    pub color: String,
    // NOTE: numeric in this port; `build_save_model` maps it to the named
    // layer id the original stores (and `apply_model` maps back).
    #[serde(default)]
    pub layer_id: i64,
    /// Keys this build does not model (Sketchy residue such as `sketchy_id`
    /// or `sketchy_other`), kept so a save writes them back unchanged.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl PlacedText {
    /// Forget the Sketchy id: a copy of this label is a new element.
    pub fn forget_sketchy_id(&mut self) {
        self.extra.remove(sketch_core::model::SKETCHY_ID_KEY);
    }
}

fn default_font_family() -> String {
    "Arial".into()
}
fn default_font_size() -> f64 {
    12.0
}
fn default_color() -> String {
    "#000000".into()
}

/// A saved `color: null` (the automatic ink) reads as an empty string.
fn null_as_empty<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

impl PlacedText {
    #[allow(dead_code)]
    pub fn new(id: i64, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            font_family: "Arial".into(),
            font_size: 12.0,
            bold: false,
            italic: false,
            underline: false,
            color: "#000000".into(),
            layer_id: 0,
            extra: Default::default(),
        }
    }
}

#[allow(dead_code)]
pub fn point_in_text(t: &PlacedText, world: sketch_core::geometry::Pt, zoom: f64) -> bool {
    use sketch_core::geometry::PIXELS_PER_FOOT;
    let w = (t.text.len() as f64 * t.font_size * 0.6) / 12.0; // rough
    let h = t.font_size / 12.0;
    let dx = world.x - t.x;
    let dy = world.y - t.y;
    let rad = -t.rotation * std::f64::consts::PI / 180.0;
    let lx = dx * rad.cos() - dy * rad.sin();
    let ly = dx * rad.sin() + dy * rad.cos();
    let pad = 6.0 / (PIXELS_PER_FOOT * zoom.max(1e-9));
    lx.abs() <= w / 2.0 + pad && ly.abs() <= h / 2.0 + pad
}

pub fn place_text_at(drawing: &mut crate::drawing::Drawing, world: Pt, text: String) -> i64 {
    let id = drawing.gen_id();
    // `placeTextAt`: the Text Label options (`textFont`, size, Bold / Italic
    // / Underline, `textColor`); the old dark-theme `#e2e8f0` means
    // "automatic" (no colour of its own).
    let o = &drawing.options;
    let placed = PlacedText {
        id,
        text,
        x: world.x,
        y: world.y,
        rotation: 0.0,
        font_family: if o.text_font.is_empty() {
            "Arial".into()
        } else {
            o.text_font.clone()
        },
        font_size: if o.text_font_size > 0.0 {
            o.text_font_size
        } else {
            12.0
        },
        bold: o.text_bold,
        italic: o.text_italic,
        underline: o.text_underline,
        color: if o.text_color.is_empty() || o.text_color.eq_ignore_ascii_case("#e2e8f0") {
            String::new()
        } else {
            o.text_color.clone()
        },
        layer_id: 0,
        extra: Default::default(),
    };
    // store in drawing.texts if field exists; for now push to a vec on drawing
    drawing.texts.push(placed);
    id
}
