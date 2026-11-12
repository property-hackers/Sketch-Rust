//! Placed symbol instances on the canvas (Screen 8).
//!
//! Mirrors the browser's `state.symbols` array: position, rotation, scale,
//! wall-snap metadata, and flip state. The save format uses camelCase; the
//! Rust model uses snake_case and converts at the boundaries.

use serde::{Deserialize, Serialize};
use sketch_core::geometry::Pt;
use sketch_core::model::{Polygon, SketchLine, WallType};
use sketch_core::symbols::WallSnap;

/// One placed symbol instance (`state.symbols[]`), camelCase for the save
/// model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedSymbol {
    pub id: i64,
    pub symbol_type: String,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub rotation: f64,
    #[serde(default = "default_scale")]
    pub scale: f64,
    #[serde(default)]
    pub flip_h: bool,
    /// The host wall's id, when wall-snapped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_id: Option<i64>,
    #[serde(default)]
    pub wall_snap: WallSnap,
    /// Gap width in feet (definition inches / 12).
    #[serde(default)]
    pub wall_gap_width: f64,
    /// Along-wall position 0..1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_fraction: Option<f64>,
    /// Which side of the wall the symbol faces.
    #[serde(default)]
    pub wall_side_flip: bool,
    /// True when the side was explicitly chosen (not inherited).
    #[serde(default)]
    pub wall_side_explicit: bool,
    // NOTE: numeric in this port; `build_save_model` maps it to the named
    // layer id the original stores (and `apply_model` maps back).
    #[serde(default)]
    pub layer_id: i64,
    /// Keys this build does not model (Sketchy residue such as `sketchy_id`
    /// or `sketchy_other`), kept so a save writes them back unchanged.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

fn default_scale() -> f64 {
    1.0
}

/// Hosted openings passed to the shared wall-solid renderer and exporters.
pub fn openings(symbols: &[PlacedSymbol]) -> Vec<sketch_core::solids::Opening> {
    symbols
        .iter()
        .filter_map(|s| {
            Some(sketch_core::solids::Opening {
                wall_id: s.wall_id?,
                x: s.x,
                y: s.y,
                gap_width: s.wall_gap_width,
                snap: if s.wall_snap == WallSnap::Side {
                    sketch_core::solids::WallSnap::Side
                } else {
                    sketch_core::solids::WallSnap::Center
                },
                flip_h: s.flip_h,
                side_flip: s.wall_side_flip,
            })
        })
        .collect()
}

impl PlacedSymbol {
    /// Forget the Sketchy id: a copy of this symbol is a new element.
    pub fn forget_sketchy_id(&mut self) {
        self.extra.remove(sketch_core::model::SKETCHY_ID_KEY);
    }

    /// Keep the hosted opening in step with geometry, including repeated drags.
    pub fn set_scale(&mut self, scale: f64) {
        if !scale.is_finite() || scale <= 0.0 {
            return;
        }
        let previous = if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        };
        self.wall_gap_width *= scale / previous;
        self.scale = scale;
    }
    /// `ensurePageModel`'s one-time hosted-opening migration recomputes the
    /// side flip (and rotation) of a saved pre-explicit symbol from its host
    /// wall (sketch-app.js:4461).
    pub fn migrate_saved_swing(&mut self, line: &SketchLine, polygons: &[Polygon]) {
        if self.wall_side_explicit {
            return;
        }
        self.wall_side_explicit = true;
        if self.wall_snap != WallSnap::Side || line.kind != WallType::Exterior {
            return;
        }
        let flip = exterior_swing_flip(line, polygons);
        self.wall_side_flip = flip;
        let dx = line.end.x - line.start.x;
        let dy = line.end.y - line.start.y;
        if dx.hypot(dy) > 1e-6 {
            self.rotation =
                -dy.atan2(dx) * 180.0 / std::f64::consts::PI + if flip { 180.0 } else { 0.0 };
        }
    }
}

/// Result of `resolve_symbol_placement` (browser's `resolveSymbolPlacement`).
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolPlacement {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub wall_id: Option<i64>,
    pub wall_snap: WallSnap,
    pub wall_gap_width: f64,
    pub wall_fraction: Option<f64>,
    pub wall_side_flip: bool,
}

// (constructor inlined at call sites; kept as documentation of defaults)

fn points_equal(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() <= 1e-4 && (a.y - b.y).abs() <= 1e-4
}

fn exterior_swing_flip(line: &SketchLine, polygons: &[Polygon]) -> bool {
    // Find a polygon that has this line as an edge.
    let poly = polygons.iter().find(|p| {
        let pts = &p.points;
        if pts.len() < 3 {
            return false;
        }
        for i in 0..pts.len() {
            let next = pts[(i + 1) % pts.len()];
            if (points_equal(pts[i], line.start) && points_equal(next, line.end))
                || (points_equal(pts[i], line.end) && points_equal(next, line.start))
            {
                return true;
            }
        }
        false
    });
    let poly = match poly {
        Some(p) => p,
        None => return false,
    };
    let pts = &poly.points;
    // signed area (shoelace), world coords.
    let mut area = 0.0;
    for i in 0..pts.len() {
        let next = pts[(i + 1) % pts.len()];
        area += pts[i].x * next.y - next.x * pts[i].y;
    }
    let signed_area = area / 2.0;

    let mut follows_path = true;
    for i in 0..pts.len() {
        let next = &pts[(i + 1) % pts.len()];
        if points_equal(pts[i], line.start) && points_equal(*next, line.end) {
            follows_path = true;
            break;
        }
        if points_equal(pts[i], line.end) && points_equal(*next, line.start) {
            follows_path = false;
            break;
        }
    }

    let mut exterior_on_swing = signed_area < 0.0;
    if !follows_path {
        exterior_on_swing = !exterior_on_swing;
    }
    exterior_on_swing
}

/// Port of the browser's `resolveSymbolPlacement`.
pub fn resolve_symbol_placement(
    world: Pt,
    def_wall_snap: WallSnap,
    def_wall_gap_width_inches: f64,
    lines: &[SketchLine],
    polygons: &[Polygon],
    current_side_flip: Option<bool>,
    zoom: f64,
) -> SymbolPlacement {
    let mut placement = SymbolPlacement {
        x: world.x,
        y: world.y,
        rotation: 0.0,
        wall_id: None,
        wall_snap: def_wall_snap,
        wall_gap_width: def_wall_gap_width_inches / 12.0,
        wall_fraction: None,
        wall_side_flip: false,
    };

    if def_wall_snap == WallSnap::None {
        return placement;
    }

    let tolerance = (28.0 / (sketch_core::geometry::PIXELS_PER_FOOT * zoom.max(1e-9))).max(1.5);

    let mut nearest: Option<(usize, Pt, f64, f64, f64)> = None; // (index, point, dist, angle, t)
    for (idx, line) in lines.iter().enumerate() {
        if line.bow_angle.abs() > 0.001 || line.arc_height.abs() > 0.001 {
            continue;
        }
        let dx = line.end.x - line.start.x;
        let dy = line.end.y - line.start.y;
        let length_sq = dx * dx + dy * dy;
        if length_sq < 1e-8 {
            continue;
        }
        let t = (((world.x - line.start.x) * dx + (world.y - line.start.y) * dy) / length_sq)
            .clamp(0.0, 1.0);
        let point = Pt {
            x: line.start.x + dx * t,
            y: line.start.y + dy * t,
        };
        let distance = ((world.x - point.x).powi(2) + (world.y - point.y).powi(2)).sqrt();
        if distance <= tolerance {
            let better = match &nearest {
                None => true,
                Some(n) => distance < n.2,
            };
            if better {
                let angle = dy.atan2(dx);
                nearest = Some((idx, point, distance, angle, t));
            }
        }
    }

    if let Some((idx, point, _dist, angle, t)) = nearest {
        let line = &lines[idx];
        placement.x = point.x;
        placement.y = point.y;
        placement.rotation = -angle * 180.0 / std::f64::consts::PI;
        placement.wall_id = Some(line.id);
        placement.wall_fraction = Some(t);

        let wall_length =
            ((line.end.x - line.start.x).powi(2) + (line.end.y - line.start.y).powi(2)).sqrt();
        let offset = if wall_length > 0.0 {
            ((line.end.x - line.start.x) * (world.y - line.start.y)
                - (line.end.y - line.start.y) * (world.x - line.start.x))
                / wall_length
        } else {
            0.0
        };

        let mut side_flip = false;
        if def_wall_snap == WallSnap::Side && offset.abs() > 0.05 {
            side_flip = offset < 0.0;
        } else if def_wall_snap == WallSnap::Side && current_side_flip.is_some() {
            side_flip = current_side_flip.unwrap();
        } else if line.kind == WallType::Exterior
            && def_wall_snap == WallSnap::Side
            && exterior_swing_flip(line, polygons)
        {
            side_flip = true;
        }

        if side_flip {
            placement.rotation += 180.0;
        }
        placement.wall_side_flip = side_flip;
    }

    placement
}

/// Recompute a hosted symbol's position/rotation from its wall + stored fraction.
/// Returns true if updated.
pub fn reposition_hosted_symbol(sym: &mut PlacedSymbol, lines: &[SketchLine]) -> bool {
    let wall_id = match sym.wall_id {
        Some(id) => id,
        None => return false,
    };
    let frac = match sym.wall_fraction {
        Some(f) => f,
        None => return false,
    };
    let line = match lines.iter().find(|l| l.id == wall_id) {
        Some(l) => l,
        None => return false,
    };
    let dx = line.end.x - line.start.x;
    let dy = line.end.y - line.start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return false;
    }
    let t = frac.clamp(0.0, 1.0);
    sym.x = line.start.x + dx * t;
    sym.y = line.start.y + dy * t;
    let base = -dy.atan2(dx) * 180.0 / std::f64::consts::PI;
    sym.rotation = base + if sym.wall_side_flip { 180.0 } else { 0.0 };
    true
}

/// Hit-test a placed symbol (rotated local bounds, 6 px pad) — P4.
pub fn point_in_symbol(
    sym: &PlacedSymbol,
    def_bounds: Option<(f64, f64, f64, f64)>, // (min_x, min_y, max_x, max_y)
    world: sketch_core::geometry::Pt,
    zoom: f64,
) -> bool {
    let (min_x, min_y, max_x, max_y) = def_bounds.unwrap_or((-6.0, -6.0, 6.0, 6.0));
    let bw = (max_x - min_x).max(1.0) * sym.scale / 12.0;
    let bh = (max_y - min_y).max(1.0) * sym.scale / 12.0;
    let cx = (min_x + max_x) / 2.0 * sym.scale / 12.0;
    let cy = (min_y + max_y) / 2.0 * sym.scale / 12.0;

    let dx = world.x - sym.x;
    let dy = world.y - sym.y;
    // `pointInSymbol`: the offset into the symbol's own frame turns by
    // `+rotation` (rotation is degrees clockwise on screen, like `ctx.rotate`).
    let rad = sym.rotation * std::f64::consts::PI / 180.0;
    let lx0 = dx * rad.cos() - dy * rad.sin();
    let ly0 = dx * rad.sin() + dy * rad.cos();
    let lx = if sym.flip_h { -lx0 } else { lx0 };
    let lx = lx - cx;
    let ly = ly0 - cy;

    let pad = 6.0 / (sketch_core::geometry::PIXELS_PER_FOOT * zoom.max(1e-9));
    (lx.abs() <= bw / 2.0 + pad) && (ly.abs() <= bh / 2.0 + pad)
}

/// Follow-along: keep non-selected hosted openings glued to their walls.
pub fn refresh_hosted_symbols(
    symbols: &mut [PlacedSymbol],
    selected: &std::collections::HashSet<i64>,
    lines: &[SketchLine],
) {
    for sym in symbols.iter_mut() {
        if sym.wall_id.is_some() && sym.wall_fraction.is_some() && !selected.contains(&sym.id) {
            reposition_hosted_symbol(sym, lines);
        }
    }
}

/// Swing flip: mirror the selected wall-hosted openings across the wall axis.
#[allow(dead_code)]
pub fn flip_opening(symbols: &mut [PlacedSymbol], selected: &std::collections::HashSet<i64>) {
    for sym in symbols.iter_mut() {
        if selected.contains(&sym.id) && sym.wall_id.is_some() {
            sym.flip_h = !sym.flip_h;
        }
    }
}

/// Per-instance opening width: grow/shrink the gap cut in the host wall.
#[allow(dead_code)]
pub fn adjust_opening_width(
    symbols: &mut [PlacedSymbol],
    selected: &std::collections::HashSet<i64>,
    delta_feet: f64,
) {
    for sym in symbols.iter_mut() {
        if selected.contains(&sym.id) && sym.wall_id.is_some() {
            let cur = sym.wall_gap_width;
            sym.wall_gap_width = (cur + delta_feet).max(0.0);
        }
    }
}

/// Resnap selected symbols (uses resolve_symbol_placement with current geometry).
#[allow(dead_code)]
pub fn resnap_selected_symbols(
    symbols: &mut [PlacedSymbol],
    selected: &std::collections::HashSet<i64>,
    lines: &[SketchLine],
    polygons: &[Polygon],
    zoom: f64,
) {
    // Need definition info; caller must supply? We only have symbol_type here.
    // Resolve using stored wall_snap as def_wall_snap (best-effort) and current gap.
    for sym in symbols.iter_mut() {
        if !selected.contains(&sym.id) {
            continue;
        }
        if sym.wall_id.is_none() && sym.wall_snap == WallSnap::None {
            // nothing to resnap for non-hosted? still resolve position
        }
        let current_side_flip = if sym.wall_id.is_some() {
            Some(sym.wall_side_flip)
        } else {
            None
        };
        let placement = resolve_symbol_placement(
            Pt { x: sym.x, y: sym.y },
            sym.wall_snap,
            sym.wall_gap_width * 12.0,
            lines,
            polygons,
            current_side_flip,
            zoom,
        );
        sym.x = placement.x;
        sym.y = placement.y;
        sym.wall_id = placement.wall_id;
        if placement.wall_id.is_some() {
            sym.rotation = placement.rotation;
        }
        if placement.wall_id.is_some() && placement.wall_fraction.is_some() {
            sym.wall_fraction = placement.wall_fraction;
            sym.wall_side_flip = placement.wall_side_flip;
            sym.wall_side_explicit = true;
            if sym.wall_gap_width == 0.0 && placement.wall_gap_width > 0.0 {
                sym.wall_gap_width = placement.wall_gap_width;
            }
        }
    }
}
