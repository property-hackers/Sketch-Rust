//! The sketch renderer (refs/canvas-engine.js render → drawLines,
//! drawDimension, drawWitnessLines, drawVertices, drawRubberBand,
//! drawRiseRun, drawLiveAngle, drawReturnGuide, drawSnapPreview), in the
//! original's paint order. The palette is CanvasEngine's constructor colors
//! with the light-theme overrides (`applyOptions` is never called by the
//! original, so the Options colours do not reach these).

use iced::alignment::Vertical;
use iced::widget::canvas::{path::Arc, Frame, LineCap, LineDash, LineJoin, Path, Stroke, Text};
use iced::widget::text::{Alignment, LineHeight, Shaping};
use iced::{Color, Font, Pixels, Point, Radians, Rectangle, Size, Vector};
use sketch_core::geometry::{distance, GeometryEngine, Pt, MAX_BOW_ANGLE, PIXELS_PER_FOOT};
use sketch_core::model::{SketchLine, WallType};

use crate::drawing::{js_round, DrawCtx, Drawing};
use crate::theme::{ARIAL, CONSOLAS, CONSOLAS_BOLD};

const fn hex(rgb: u32) -> Color {
    Color::from_rgb(
        ((rgb >> 16) & 255) as f32 / 255.0,
        ((rgb >> 8) & 255) as f32 / 255.0,
        (rgb & 255) as f32 / 255.0,
    )
}

const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::from_rgba(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
}

/// `colors.wallExt` / `wallInt` (fallbacks when a line has no colour).
const WALL_EXT: Color = hex(0x2B6CB0);
const WALL_INT: Color = hex(0x63B3ED);
/// `colors.wallActive` — the live chain.
const WALL_ACTIVE: Color = hex(0xD946EF);
/// `colors.selectLine` — the purple highlight on selected walls.
const SELECT_LINE: Color = hex(0xC05AFA);
/// `colors.polyOutline` / `colors.fillGLA` — a selected area's outline and
/// fill (`rgba(43,108,176,.08)`).
const POLY_OUTLINE: Color = hex(0x2B6CB0);
const POLY_FILL: Color = Color::from_rgba(43.0 / 255.0, 108.0 / 255.0, 176.0 / 255.0, 0.08);
/// The curve handle and curve guides' green (`#22C55E`).
const CURVE_GUIDE: Color = hex(0x22C55E);

/// `colors.wallPreview` — rubber band, live angle, rise/run labels.
const WALL_PREVIEW: Color = hex(0xEF4444);
const RISE_RUN: Color = rgba(239, 68, 68, 0.65);
/// `dimLabel` / `witnessLine` follow the canvas theme (`canvas_theme`).
fn dim_label() -> Color {
    crate::canvas_theme::current().dim_label()
}
fn witness() -> Color {
    crate::canvas_theme::current().witness()
}
const VERTEX: Color = hex(0xE53E3E);
const VERTEX_STROKE: Color = hex(0xFC8181);
const SNAP_DOT: Color = hex(0x22C55E);
/// `wallWidths` (exterior twice interior).
const fn wall_width(kind: WallType) -> f32 {
    match kind {
        WallType::Exterior => 4.0,
        WallType::Interior => 2.0,
    }
}

/// Parse `#RRGGBB` (line colours are stamped as hex).
pub fn parse_hex(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    u32::from_str_radix(h, 16).ok().map(hex)
}

/// `canvas.applyOptions` (refs/canvas-engine.js:103): the Drawing Options
/// panel supplies the live fallback colours; a missing/broken hex keeps the
/// measured constant (the state-init default).
fn wall_color(options: &crate::options::Options, ext: bool) -> Color {
    if ext {
        parse_hex(&options.exterior_color).unwrap_or(WALL_EXT)
    } else {
        parse_hex(&options.interior_color).unwrap_or(WALL_INT)
    }
}

/// `colors.wallPreview = options.witnessCurveColor || …`.
fn preview_color(options: &crate::options::Options) -> Color {
    parse_hex(&options.witness_curve_color).unwrap_or(WALL_PREVIEW)
}

/// `colors.vertex = options.witnessBaseColor || …`.
fn vertex_color(options: &crate::options::Options) -> Color {
    parse_hex(&options.witness_base_color).unwrap_or(VERTEX)
}

/// `colors.snapDot = options.witnessInteriorColor || …`.
fn snap_color(options: &crate::options::Options) -> Color {
    parse_hex(&options.witness_interior_color).unwrap_or(SNAP_DOT)
}

/// What the renderer reads.
#[derive(Clone)]
pub struct Scene<'a> {
    /// Latest physical cursor, independent of throttled application HUD updates.
    pub live_pointer: Option<Pt>,
    pub drawing: &'a Drawing,
    pub ctx: DrawCtx,
    /// `state.pan`, `state.zoom`.
    pub pan: (f64, f64),
    pub zoom: f64,
    /// `state.filters`, in view_state::FILTERS order.
    pub filters: [bool; 6],
    /// `state.dimensionsEnabled`.
    pub dimensions: bool,
    /// `state.dimensionPlacement` — 'in' puts every label on the path's
    /// inside (the fallback when a wall belongs to no polygon).
    pub placement: crate::view_state::DimensionPlacement,
    /// `state.selectedLineIds` / `selectedPolyIds` (screen 6).
    pub selection: &'a crate::selection::Selection,
    /// The reference image (screen 10): drawn beneath the drawing, the
    /// scale/orient markers on top.
    pub photo: &'a crate::reference_image::BgImage,
    /// `isEntityVisible`'s layer half (screen 10): entity ids the render
    /// skips because their layer chain hides them.
    pub gates: &'a crate::layers::LayerGates,
    /// `app.areaCodes`: each area's fill, outline and group.
    pub area_codes: &'a [sketch_core::geometry::AreaCode],
    /// The last recalculation's net areas and factors, by polygon id.
    pub computed: &'a std::collections::BTreeMap<i64, crate::calcs::Computed>,
    /// Placed symbols (screen 8).
    pub symbols: &'a [crate::placed_symbol::PlacedSymbol],
    /// Symbol library (screen 8).
    pub symbol_lib: Option<&'a crate::symlib::SymIndex>,
    /// Placed text labels (screen 9).
    pub texts: &'a [crate::placed_text::PlacedText],
    /// Measure tool points (screen 9).
    pub measure_points: &'a [Pt],
    pub measure_runs: &'a [Vec<Pt>],
    /// `getUnderlayPage()` (screen 10): the ghosted underlay page, None when
    /// unset or the same as the current page.
    pub underlay: Option<&'a crate::pages::Page>,
    /// The Curve Wall tool is armed: selected walls paint in the preview ink.
    pub curve_tool: bool,
    /// A curve-handle drag: the wall's id and whether it has started to move
    /// (the live guides show only then).
    pub curve_drag: Option<(i64, bool)>,
    /// Select mode: the selected entities carry their transform handles.
    pub select_mode: bool,
    /// The drag in progress and whether it has started to move (the rotation
    /// handles preview it).
    pub drag: Option<&'a crate::view_state::Drag>,
    pub drag_moved: bool,
    /// `exportTextZoom`: the least zoom text is sized by (0 = none; an
    /// export fits the sketch small, so its text is held up to this).
    pub text_zoom_floor: f64,
}

impl Scene<'_> {
    /// `Geometry.toScreen`.
    fn screen(&self, p: Pt) -> Point {
        let k = PIXELS_PER_FOOT * self.zoom;
        Point::new((self.pan.0 + p.x * k) as f32, (self.pan.1 - p.y * k) as f32)
    }

    /// `getTextZoom()` (outside export).
    fn text_zoom(&self) -> f32 {
        let zoom = if self.zoom == 0.0 { 1.0 } else { self.zoom };
        zoom.max(self.text_zoom_floor) as f32
    }
}

fn points_match(a: Pt, b: Pt) -> bool {
    (a.x - b.x).abs() <= 0.0001 && (a.y - b.y).abs() <= 0.0001
}

/// `getPathDimensionSide(points, edgeStart, edgeEnd)`: the label goes on the
/// path's outside, or its inside when `state.dimensionPlacement` is 'in'.
pub fn path_dimension_side(
    points: &[Pt],
    start: Pt,
    end: Pt,
    placement: crate::view_state::DimensionPlacement,
) -> f32 {
    if points.len() < 3 {
        return placement.side();
    }
    let signed = GeometryEngine::new().calculate_signed_area(points);
    let mut follows = true;
    for (i, p) in points.iter().enumerate() {
        let next = points[(i + 1) % points.len()];
        if points_match(*p, start) && points_match(next, end) {
            follows = true;
            break;
        }
        if points_match(*p, end) && points_match(next, start) {
            follows = false;
            break;
        }
    }
    let outside = if signed >= 0.0 { 1.0 } else { -1.0 };
    let side = if follows { outside } else { -outside };
    match placement {
        crate::view_state::DimensionPlacement::Out => side,
        crate::view_state::DimensionPlacement::In => -side,
    }
}

/// `getLineDimensionSide(line)`: the wall's own area decides when it belongs
/// to one (areas' label sides arrive with screen 7); otherwise the
/// placement's default.
pub fn line_dimension_side(
    d: &Drawing,
    line: &SketchLine,
    placement: crate::view_state::DimensionPlacement,
) -> f32 {
    // The first area one of whose edges is this wall decides.
    let polygon = d.polygons.iter().find(|poly| {
        let n = poly.points.len();
        (0..n).any(|i| {
            let (a, b) = (poly.points[i], poly.points[(i + 1) % n]);
            (points_match(a, line.start) && points_match(b, line.end))
                || (points_match(a, line.end) && points_match(b, line.start))
        })
    });
    if let Some(poly) = polygon {
        return path_dimension_side(&poly.points, line.start, line.end, placement);
    }
    if d.is_active(line.id) {
        return path_dimension_side(&d.points, line.start, line.end, placement);
    }
    placement.side()
}

fn dashed(segments: &[f32]) -> LineDash<'_> {
    LineDash {
        segments,
        offset: 0,
    }
}

/// `drawGuideLabel` — centred, `middle` baseline.
fn guide_label(
    frame: &mut Frame,
    text: String,
    at: Point,
    color: Color,
    font: Font,
    size: f32,
    paths: bool,
) {
    let text = Text {
        content: text,
        position: at,
        color,
        size: Pixels(size),
        line_height: LineHeight::Relative(1.2),
        font,
        align_x: Alignment::Center,
        align_y: Vertical::Center,
        shaping: Shaping::Basic,
        ..Text::default()
    };
    // Iced queues ordinary text above all geometry. Export glyph paths keep
    // text at its actual position in the layer's paint order.
    if paths {
        text.draw_with(|path, color| frame.fill(&path, color));
    } else {
        frame.fill_text(text);
    }
}

/// `traceArc` from `start` (the path is already at `start`).
fn trace_arc(p: &mut iced::widget::canvas::path::Builder, start: Point, end: Point, bow_deg: f64) {
    let bow = bow_deg.to_radians() as f32;
    let (mx, my) = ((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
    let (dx, dy) = (end.x - start.x, end.y - start.y);
    let chord = dx.hypot(dy);
    let half = bow.abs() / 2.0;
    if chord < 1.0 || half < 0.01 {
        p.line_to(end);
        return;
    }
    let radius = (chord / 2.0) / half.sin();
    let (nx, ny) = (-dy / chord, dx / chord);
    let h = radius * half.cos();
    let sign = if bow > 0.0 { 1.0 } else { -1.0 };
    let (cx, cy) = (mx + nx * h * sign, my + ny * h * sign);
    let a0 = (start.y - cy).atan2(start.x - cx);
    p.arc(Arc {
        center: Point::new(cx, cy),
        radius: radius.abs(),
        start_angle: Radians(a0),
        end_angle: Radians(a0 + bow),
    });
}

/// `getCurveMetrics(line)` in WORLD coordinates — the circle a bowed wall
/// lies on, its radius, and the arc's apex.
struct CurveMetrics {
    center: Pt,
    radius: f64,
    apex: Pt,
}

fn curve_metrics(line: &SketchLine) -> Option<CurveMetrics> {
    let chord = distance(line.start, line.end);
    let bow = line.bow_angle.clamp(-MAX_BOW_ANGLE, MAX_BOW_ANGLE);
    let theta = bow.abs().to_radians();
    if chord < 0.001 || theta < 1e-6 {
        return None;
    }
    let radius = (chord / 2.0) / (theta / 2.0).sin();
    let rise = chord / 2.0 * (theta / 4.0).tan();
    let (dx, dy) = (line.end.x - line.start.x, line.end.y - line.start.y);
    let (nx, ny) = (-dy / chord, dx / chord);
    let sign = if bow < 0.0 { -1.0 } else { 1.0 };
    let (mx, my) = (
        (line.start.x + line.end.x) / 2.0,
        (line.start.y + line.end.y) / 2.0,
    );
    let center_distance = radius * (theta / 2.0).cos();
    Some(CurveMetrics {
        center: Pt::new(
            mx - nx * center_distance * sign,
            my - ny * center_distance * sign,
        ),
        radius,
        apex: Pt::new(mx + nx * rise * sign, my + ny * rise * sign),
    })
}

/// `drawCurvedWallSolid`: a bowed wall's solid is the annular sector between
/// `radius -+ thickness/2`, swept from the start angle to the end angle the
/// way that passes through the apex.
fn draw_curved_solid(
    frame: &mut Frame,
    scene: &Scene,
    line: &SketchLine,
    thickness: f64,
    color: Color,
) {
    let Some(m) = curve_metrics(line) else {
        return;
    };
    let half = thickness / 2.0;
    let tau = std::f64::consts::TAU;
    let modulo = |v: f64| ((v % tau) + tau) % tau;
    let a_start = (line.start.y - m.center.y).atan2(line.start.x - m.center.x);
    let a_end = (line.end.y - m.center.y).atan2(line.end.x - m.center.x);
    let a_apex = (m.apex.y - m.center.y).atan2(m.apex.x - m.center.x);
    let ccw = modulo(a_apex - a_start) <= modulo(a_end - a_start);
    let sweep = if ccw {
        modulo(a_end - a_start)
    } else {
        -modulo(a_start - a_end)
    };
    let steps = ((sweep.abs() / (std::f64::consts::PI / 24.0)).ceil() as usize).max(8);
    let outer = m.radius + half;
    let inner = (m.radius - half).max(0.01);
    let at = |radius: f64, angle: f64| {
        scene.screen(Pt::new(
            m.center.x + angle.cos() * radius,
            m.center.y + angle.sin() * radius,
        ))
    };
    let path = Path::new(|p| {
        for i in 0..=steps {
            let point = at(outer, a_start + sweep * i as f64 / steps as f64);
            if i == 0 {
                p.move_to(point);
            } else {
                p.line_to(point);
            }
        }
        for i in (0..=steps).rev() {
            p.line_to(at(inner, a_start + sweep * i as f64 / steps as f64));
        }
        p.close();
    });
    frame.fill(&path, color);
}

#[allow(clippy::too_many_arguments)]
/// `drawDimension(worldStart, worldEnd, screenStart, screenEnd, color,
/// side, overrideWorld)` — unselected, default dimension options.
fn draw_dimension(
    frame: &mut Frame,
    scene: &Scene,
    world: (Pt, Pt),
    screen: (Point, Point),
    side: f32,
    pinned: Option<Pt>,
    ink: Color,
    selected: bool,
) {
    let dist = distance(world.0, world.1);
    if dist < 0.1 {
        return;
    }
    let (a, b) = screen;
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if len < 1.0 {
        return;
    }
    let mut angle = dy.atan2(dx);
    let half_pi = std::f32::consts::FRAC_PI_2;
    if angle > half_pi {
        angle -= std::f32::consts::PI;
    }
    if angle < -half_pi {
        angle += std::f32::consts::PI;
    }
    let label = match pinned {
        Some(p) if p.x.is_finite() && p.y.is_finite() => scene.screen(p),
        _ => {
            let (nx, ny) = (-dy / len, dx / len);
            let offset = (10.0 * scene.text_zoom()).max(7.0);
            Point::new(
                (a.x + b.x) / 2.0 + nx * offset * side,
                (a.y + b.y) / 2.0 + ny * offset * side,
            )
        }
    };
    // `dimensionFontSize || 11`, `dimensionFont` / Bold / Italic.
    let opts = &scene.drawing.options;
    let size = (opts.dimension_font_size as f32 * scene.text_zoom()).max(9.0);
    let font = dimension_font(opts);
    // `sameColorAsLine` paints the label in the wall's own colour.
    let label_color = if opts.same_color_as_line {
        ink
    } else {
        dim_label()
    };
    let text = scene.drawing.options.units.length(dist);
    if selected {
        // A dashed leader back to the wall's middle, then a highlight box.
        let mid = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        frame.stroke(
            &Path::line(mid, label),
            Stroke {
                line_dash: dashed(&[3.0, 3.0]),
                ..Stroke::default().with_color(SELECT_LINE).with_width(1.0)
            },
        );
    }
    let label_color = if selected { SELECT_LINE } else { label_color };
    frame.with_save(|f| {
        f.translate(Vector::new(label.x, label.y));
        if angle != 0.0 {
            f.rotate(angle);
        }
        if selected {
            // `measureText(text).width + 8` by `fontSize + 8`; the width is
            // estimated (no text metrics in a canvas draw).
            let width = text.chars().count() as f32 * size * 0.52 + 8.0;
            let height = size + 8.0;
            let rect = Path::rectangle(
                Point::new(-width / 2.0, -height / 2.0),
                Size::new(width, height),
            );
            f.fill(
                &rect,
                Color::from_rgba(192.0 / 255.0, 90.0 / 255.0, 250.0 / 255.0, 0.15),
            );
            f.stroke(
                &rect,
                Stroke::default().with_color(SELECT_LINE).with_width(1.0),
            );
        }
        guide_label(
            f,
            text,
            Point::ORIGIN,
            label_color,
            font,
            size,
            scene.text_zoom_floor > 0.0,
        );
    });
}

/// A wall's measurement label, when the dimensions are on, the Dimensions
/// filter is, the label is not hidden and no chain already covers the wall.
fn draw_line_dimension(
    frame: &mut Frame,
    scene: &Scene,
    line: &SketchLine,
    color: Color,
    covered: &std::collections::HashSet<i64>,
    active: bool,
) {
    if !((scene.dimensions || active)
        && scene.filters[3]
        && !line.dimension_hidden
        && !covered.contains(&line.id))
    {
        return;
    }
    draw_dimension(
        frame,
        scene,
        (line.start, line.end),
        (scene.screen(line.start), scene.screen(line.end)),
        line_dimension_side(scene.drawing, line, scene.placement),
        crate::drawing::pinned_dimension_point(line),
        color,
        scene.selection.dimensions.contains(&line.id),
    );
}

/// The region an area-code drop would define: the code's colour at 20% over
/// the ring with a 2px outline (the live palette's drop preview).
pub fn draw_drop_hint(frame: &mut Frame, scene: &Scene, ring: &[Pt], color: Color) {
    if ring.len() < 3 {
        return;
    }
    let path = Path::new(|p| {
        p.move_to(scene.screen(ring[0]));
        for pt in &ring[1..] {
            p.line_to(scene.screen(*pt));
        }
        p.close();
    });
    frame.fill(&path, Color { a: 0.2, ..color });
    frame.stroke(&path, Stroke::default().with_color(color).with_width(2.0));
}

/// Clean render uses the canvas passes in the original per-layer order. Drawing
/// aids and selection handles never enter exported content.
pub fn draw_clean(frame: &mut Frame, scene: &Scene, layers: &[crate::layers::LayerGates]) {
    draw_bg_image(frame, scene);
    for gates in layers {
        let layer = Scene {
            gates,
            ..scene.clone()
        };
        draw_areas(frame, &layer);
        draw_lines(frame, &layer);
        if scene.filters[4] {
            draw_texts(frame, &layer);
        }
        if scene.filters[5] {
            draw_symbols(frame, &layer);
        }
    }
    draw_chains(frame, scene);
}

/// `ctx.font` for a dimension label (`dimensionFont`, Bold, Italic).
fn dimension_font(opts: &crate::options::Options) -> Font {
    crate::theme::font_named(
        &opts.dimension_font,
        opts.dimension_bold,
        opts.dimension_italic,
    )
}

/// `drawLines(layerId)` — unselected walls, no wall solids (off by default).
fn draw_lines(frame: &mut Frame, scene: &Scene) {
    let d = scene.drawing;
    // Chain coverage: walls a dimension chain covers skip their own labels
    // (canvas-engine.js:1660's `!this.chainCoverage?.has(line.id)`).
    let covered = if d.dimension_chains.is_empty() {
        std::collections::HashSet::new()
    } else {
        d.chain_coverage()
    };
    // Geometric wall thickness: filled solids replace the centreline strokes.
    // `drawWallSolids` runs before the stroke pass, which then skips every
    // line that has a solid (a pen-down line keeps its stroke preview).
    let solids = if d.options.wall_solids_enabled() {
        Some(sketch_core::solids::compute_wall_solids(
            &d.lines,
            &|line| d.wall_thickness_of(line),
            &crate::placed_symbol::openings(&d.symbols),
        ))
    } else {
        None
    };
    if let Some(solids) = &solids {
        for interior_pass in [true, false] {
            for solid in &solids.solids {
                if solid.interior != interior_pass {
                    continue;
                }
                let Some(line) = d.lines.iter().find(|l| l.id == solid.line_id) else {
                    continue;
                };
                // A hidden layer's walls do not paint (canvas-engine.js:1527).
                if !scene.gates.shows(line.id) {
                    continue;
                }
                let ext = line.kind == WallType::Exterior;
                if !scene.filters[if ext { 1 } else { 2 }] || d.is_active(line.id) {
                    continue;
                }
                let color = line
                    .color
                    .as_deref()
                    .and_then(parse_hex)
                    .unwrap_or(wall_color(&scene.drawing.options, ext));
                for piece in &solid.pieces {
                    let pts: Vec<Point> = piece.iter().map(|p| scene.screen(*p)).collect();
                    let path = Path::new(|p| {
                        if let Some((first, rest)) = pts.split_first() {
                            p.move_to(*first);
                            for pt in rest {
                                p.line_to(*pt);
                            }
                            p.close();
                        }
                    });
                    frame.fill(&path, color);
                }
            }
            // Bowed walls: an annular sector instead of a quad (hosted
            // openings are not cut in bowed walls, the original's v1 limit).
            for line in &d.lines {
                let bowed = line.bow_angle != 0.0 && line.bow_angle.abs() > 0.5;
                if !bowed || (line.kind == WallType::Interior) != interior_pass {
                    continue;
                }
                if !scene.gates.shows(line.id) {
                    continue;
                }
                let thickness = d.wall_thickness_of(line);
                // `if (!(thickness > 0)) return;` — NaN must skip too.
                if !thickness.is_finite() || thickness <= 0.0 {
                    continue;
                }
                let ext = line.kind == WallType::Exterior;
                if !scene.filters[if ext { 1 } else { 2 }] || d.is_active(line.id) {
                    continue;
                }
                let color = line
                    .color
                    .as_deref()
                    .and_then(parse_hex)
                    .unwrap_or(wall_color(&scene.drawing.options, ext));
                draw_curved_solid(frame, scene, line, thickness, color);
            }
        }
    }
    for line in &d.lines {
        let ext = line.kind == WallType::Exterior;
        if !scene.filters[if ext { 1 } else { 2 }] {
            continue;
        }
        // `if (!this.app.isEntityVisible(line)) return;` — hidden layers
        // drop out of every wall pass, dimensions included.
        if !scene.gates.shows(line.id) {
            continue;
        }
        let active = d.is_active(line.id);
        let selected = scene.selection.is_line_selected(line.id);
        let curve_target =
            selected && (scene.curve_tool || scene.curve_drag.is_some_and(|(id, _)| id == line.id));
        let is_curve_drag = scene.curve_drag == Some((line.id, true));
        let color = if active {
            WALL_ACTIVE
        } else if curve_target {
            preview_color(&scene.drawing.options)
        } else if selected {
            SELECT_LINE
        } else {
            line.color
                .as_deref()
                .and_then(parse_hex)
                .unwrap_or(wall_color(&scene.drawing.options, ext))
        };
        let base_width = line
            .line_thickness
            .filter(|w| *w != 0.0)
            .map(|w| w as f32)
            .unwrap_or(wall_width(line.kind));
        // `lineWidth = isSelected ? Math.max(5, baseWidth + 1) : baseWidth`.
        let width = if selected {
            (base_width + 1.0).max(5.0)
        } else {
            base_width
        };
        let dash: &[f32] = match line.line_style.as_deref() {
            Some("dashed") => &[9.0, 6.0],
            Some("dotted") => &[2.0, 5.0],
            _ => &[],
        };
        let (start, end) = (scene.screen(line.start), scene.screen(line.end));
        let bowed = line.bow_angle.abs() > 0.5;
        let solid_handled = solids.as_ref().is_some_and(|s| {
            !active
                && (s.solids.iter().any(|x| x.line_id == line.id)
                    || (bowed && d.wall_thickness_of(line) > 0.0))
        });
        let path = Path::new(|p| {
            p.move_to(start);
            if bowed {
                trace_arc(p, start, end, line.bow_angle);
            } else {
                let runs = sketch_core::solids::visible_runs(
                    line,
                    &crate::placed_symbol::openings(&d.symbols),
                );
                if runs.is_empty() {
                    p.line_to(end);
                } else {
                    for (a, b) in runs {
                        if b <= a {
                            continue;
                        }
                        p.move_to(scene.screen(Pt::new(
                            line.start.x + (line.end.x - line.start.x) * a,
                            line.start.y + (line.end.y - line.start.y) * a,
                        )));
                        p.line_to(scene.screen(Pt::new(
                            line.start.x + (line.end.x - line.start.x) * b,
                            line.start.y + (line.end.y - line.start.y) * b,
                        )));
                    }
                }
            }
        });
        if !solid_handled {
            frame.stroke(
                &path,
                Stroke {
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    line_dash: dashed(dash),
                    ..Stroke::default().with_color(color).with_width(width)
                },
            );
        }
        if selected {
            // The curve handle: a green square on the wall's arc.
            let at = scene.screen(crate::drawing::curve_handle(line));
            let top_left = Point::new(at.x - 4.0, at.y - 4.0);
            let size = iced::Size::new(8.0, 8.0);
            frame.fill_rectangle(top_left, size, CURVE_GUIDE);
            frame.stroke(
                &Path::rectangle(top_left, size),
                Stroke::default().with_color(hex(0xECFDF5)).with_width(1.0),
            );
        }
        if !is_curve_drag {
            draw_line_dimension(frame, scene, line, color, &covered, active);
        }
        if is_curve_drag {
            draw_curve_guides(frame, scene, line);
        }
    }
}

/// `drawCurveGuides`: while a wall is being bowed, the chord, rise, radii and
/// the arc's measurements.
fn draw_curve_guides(frame: &mut Frame, scene: &Scene, line: &SketchLine) {
    let Some(m) = curve_metrics(line) else {
        return;
    };
    let chord = distance(line.start, line.end);
    let bow = line.bow_angle.clamp(-MAX_BOW_ANGLE, MAX_BOW_ANGLE);
    let theta = bow.abs().to_radians();
    let arc_length = m.radius * theta;
    let rise = chord / 2.0 * (theta / 4.0).tan();
    let (start, end) = (scene.screen(line.start), scene.screen(line.end));
    let mid = scene.screen(Pt::new(
        (line.start.x + line.end.x) / 2.0,
        (line.start.y + line.end.y) / 2.0,
    ));
    let apex = scene.screen(m.apex);
    let center = scene.screen(m.center);
    let chord_color = Color::from_rgba(239.0 / 255.0, 68.0 / 255.0, 68.0 / 255.0, 0.62);
    let line_between = |a: Point, b: Point| Path::line(a, b);

    frame.stroke(
        &line_between(start, end),
        Stroke {
            line_dash: dashed(&[8.0, 6.0]),
            ..Stroke::default().with_color(chord_color).with_width(1.25)
        },
    );
    frame.stroke(
        &line_between(mid, apex),
        Stroke {
            line_dash: dashed(&[4.0, 4.0]),
            ..Stroke::default().with_color(CURVE_GUIDE).with_width(1.25)
        },
    );
    let radii = Path::new(|p| {
        p.move_to(center);
        p.line_to(start);
        p.move_to(center);
        p.line_to(end);
    });
    frame.stroke(
        &radii,
        Stroke::default()
            .with_color(Color {
                a: 0.72,
                ..CURVE_GUIDE
            })
            .with_width(1.25),
    );
    let (tick_x, tick_y) = (apex.x - mid.x, apex.y - mid.y);
    let tick_length = tick_x.hypot(tick_y);
    if tick_length > 1.0 {
        let (tx, ty) = (-tick_y / tick_length * 5.0, tick_x / tick_length * 5.0);
        let ticks = Path::new(|p| {
            p.move_to(Point::new(mid.x - tx, mid.y - ty));
            p.line_to(Point::new(mid.x + tx, mid.y + ty));
            p.move_to(Point::new(apex.x - tx, apex.y - ty));
            p.line_to(Point::new(apex.x + tx, apex.y + ty));
        });
        frame.stroke(
            &ticks,
            Stroke::default()
                .with_color(Color {
                    a: 0.72,
                    ..CURVE_GUIDE
                })
                .with_width(1.25),
        );
    }

    let feet = |v: f64| format!("{v:.1}'");
    guide_label(
        frame,
        feet(arc_length),
        Point::new(apex.x, apex.y - 15.0),
        CURVE_GUIDE,
        CONSOLAS_BOLD,
        12.0,
        scene.text_zoom_floor > 0.0,
    );
    guide_label(
        frame,
        feet(chord),
        Point::new(mid.x, mid.y - 11.0),
        preview_color(&scene.drawing.options),
        CONSOLAS_BOLD,
        11.0,
        scene.text_zoom_floor > 0.0,
    );
    let (rise_dx, rise_dy) = (apex.x - mid.x, apex.y - mid.y);
    let rise_len = rise_dx.hypot(rise_dy).max(f32::EPSILON);
    guide_label(
        frame,
        feet(rise),
        Point::new(
            (mid.x + apex.x) / 2.0 + (-rise_dy / rise_len) * 18.0,
            (mid.y + apex.y) / 2.0 + (rise_dx / rise_len) * 18.0,
        ),
        CURVE_GUIDE,
        CONSOLAS,
        11.0,
        scene.text_zoom_floor > 0.0,
    );
    guide_label(
        frame,
        feet(m.radius),
        Point::new((center.x + end.x) / 2.0, (center.y + end.y) / 2.0),
        CURVE_GUIDE,
        CONSOLAS,
        11.0,
        scene.text_zoom_floor > 0.0,
    );
    guide_label(
        frame,
        format!("{}\u{00B0}", js_round(bow.abs()) as i64),
        Point::new(center.x, center.y - 14.0),
        CURVE_GUIDE,
        CONSOLAS_BOLD,
        11.0,
        scene.text_zoom_floor > 0.0,
    );
}

/// `drawBgImage` (canvas-engine.js:3107): the image centered at its world
/// point, rotated, flipped, at the intensity opacity — beneath the drawing.
fn draw_bg_image(frame: &mut Frame, scene: &Scene) {
    let bg = scene.photo;
    let Some(handle) = &bg.bytes else {
        return;
    };
    if !bg.visible {
        return;
    }
    let ppf = sketch_core::geometry::PIXELS_PER_FOOT;
    let (x, y, w, h) = bg.screen_rect(scene.pan, scene.zoom, ppf);
    frame.with_save(|f| {
        f.translate(Vector::new(x + w / 2.0, y + h / 2.0));
        // Rotation (degrees → radians).
        if bg.rotation != 0.0 {
            f.rotate(Radians(bg.rotation as f32 * std::f32::consts::PI / 180.0));
        }
        // Flips.
        let sx: f32 = if bg.flip_h { -1.0 } else { 1.0 };
        let sy: f32 = if bg.flip_v { -1.0 } else { 1.0 };
        if sx != 1.0 || sy != 1.0 {
            f.scale_nonuniform(Vector::new(sx, sy));
        }
        f.draw_image(
            Rectangle::new(Point::new(-w / 2.0, -h / 2.0), Size::new(w, h)),
            iced::advanced::image::Image::new(handle).opacity(bg.opacity as f32),
        );
    });
}

/// `drawScalePoints` (canvas-engine.js:3138): the two clicked points as
/// `#F6AD55` crosshairs with circles + labels, and the line between them.
fn draw_scale_points(frame: &mut Frame, scene: &Scene) {
    let bg = scene.photo;
    if !bg.scale_mode && !bg.orient_mode {
        return;
    }
    let (p1, p2) = if bg.scale_mode {
        (bg.scale_p1, bg.scale_p2)
    } else {
        (bg.orient_p1, bg.orient_p2)
    };
    let marker = |frame: &mut Frame, world: Pt, label: &str| {
        let s = scene.screen(world);
        let color = Color::from_rgb8(0xF6, 0xAD, 0x55);
        let stroke = Stroke::default().with_color(color).with_width(2.0);
        // The crosshair.
        frame.stroke(
            &Path::line(Point::new(s.x - 10.0, s.y), Point::new(s.x + 10.0, s.y)),
            stroke,
        );
        frame.stroke(
            &Path::line(Point::new(s.x, s.y - 10.0), Point::new(s.x, s.y + 10.0)),
            stroke,
        );
        // The circle.
        frame.stroke(&Path::circle(Point::new(s.x, s.y), 6.0), stroke);
        // The label.
        frame.fill_text(iced::widget::canvas::Text {
            content: label.to_string(),
            position: Point::new(s.x + 12.0, s.y - 4.0),
            color,
            size: iced::Pixels(11.0),
            font: ARIAL,
            align_x: iced::widget::text::Alignment::Left,
            align_y: iced::alignment::Vertical::Top,
            ..iced::widget::canvas::Text::default()
        });
    };
    if let Some(p1) = p1 {
        marker(frame, p1, "Point 1");
    }
    if let Some(p2) = p2 {
        marker(frame, p2, "Point 2");
    }
    if let (Some(p1), Some(p2)) = (p1, p2) {
        let s1 = scene.screen(p1);
        let s2 = scene.screen(p2);
        frame.stroke(
            &Path::line(s1, s2),
            Stroke::default()
                .with_color(Color::from_rgb8(0xF6, 0xAD, 0x55))
                .with_width(2.0),
        );
    }
}

/// `drawWitnessLines` — full-view dashed guides through wall endpoints
/// aligned (within 8px) with the pointer, while a chain is live.
fn draw_witness_lines(frame: &mut Frame, scene: &Scene, w: f32, h: f32) {
    let d = scene.drawing;
    // `options.alignmentLines === false` hides the guides.
    if !d.options.alignment_lines {
        return;
    }
    if !d.pen_down || d.points.is_empty() || scene.ctx.kind.is_none() {
        return;
    }
    let threshold = Drawing::tolerance(8.0, scene.zoom);
    let cursor = scene.live_pointer.unwrap_or(d.pointer);
    let stroke = Stroke {
        line_dash: dashed(&[4.0, 6.0]),
        ..Stroke::default().with_color(witness()).with_width(1.0)
    };
    for line in &d.lines {
        for pt in [line.start, line.end] {
            let sp = scene.screen(pt);
            if (pt.x - cursor.x).abs() < threshold {
                frame.stroke(
                    &Path::line(Point::new(sp.x, 0.0), Point::new(sp.x, h)),
                    stroke,
                );
            }
            if (pt.y - cursor.y).abs() < threshold {
                frame.stroke(
                    &Path::line(Point::new(0.0, sp.y), Point::new(w, sp.y)),
                    stroke,
                );
            }
        }
    }
}

/// `drawVertices` — the live chain's points; the first is larger and white.
fn draw_vertices(frame: &mut Frame, scene: &Scene) {
    // `options.alignmentPoints === false` hides the vertex dots.
    if !scene.drawing.options.alignment_points {
        return;
    }
    for (i, pt) in scene.drawing.points.iter().enumerate() {
        let sp = scene.screen(*pt);
        let (r, fill, width) = if i == 0 {
            (6.0, Color::WHITE, 2.5)
        } else {
            (4.0, vertex_color(&scene.drawing.options), 1.5)
        };
        let circle = Path::circle(sp, r);
        frame.fill(&circle, fill);
        frame.stroke(
            &circle,
            Stroke::default()
                .with_color(VERTEX_STROKE)
                .with_width(width),
        );
    }
}

/// `drawRiseRun` — dashed run/rise legs with their lengths.
fn draw_rise_run(frame: &mut Frame, scene: &Scene, anchor_w: Pt, target_w: Pt) {
    let (dxw, dyw) = (target_w.x - anchor_w.x, target_w.y - anchor_w.y);
    if dxw.abs() < 0.1 || dyw.abs() < 0.1 {
        return;
    }
    let (anchor, target) = (scene.screen(anchor_w), scene.screen(target_w));
    let corner = Point::new(target.x, anchor.y);
    let path = Path::new(|p| {
        p.move_to(anchor);
        p.line_to(corner);
        p.line_to(target);
    });
    frame.stroke(
        &path,
        Stroke {
            line_dash: dashed(&[4.0, 4.0]),
            ..Stroke::default().with_color(RISE_RUN).with_width(1.0)
        },
    );
    guide_label(
        frame,
        scene.drawing.options.units.length(dxw.abs()),
        Point::new((anchor.x + corner.x) / 2.0, anchor.y - 11.0),
        preview_color(&scene.drawing.options),
        CONSOLAS,
        11.0,
        scene.text_zoom_floor > 0.0,
    );
    guide_label(
        frame,
        scene.drawing.options.units.length(dyw.abs()),
        Point::new(target.x + 15.0, (corner.y + target.y) / 2.0),
        preview_color(&scene.drawing.options),
        CONSOLAS,
        11.0,
        scene.text_zoom_floor > 0.0,
    );
}

/// `drawLiveAngle` — the arc between the previous wall and the rubber band
/// at the anchor, with the included angle.
fn draw_live_angle(frame: &mut Frame, scene: &Scene, prev_w: Pt, anchor_w: Pt, target_w: Pt) {
    let (prev, anchor, target) = (
        scene.screen(prev_w),
        scene.screen(anchor_w),
        scene.screen(target_w),
    );
    let v1 = (
        prev.x as f64 - anchor.x as f64,
        prev.y as f64 - anchor.y as f64,
    );
    let v2 = (
        target.x as f64 - anchor.x as f64,
        target.y as f64 - anchor.y as f64,
    );
    let (l1, l2) = (v1.0.hypot(v1.1), v2.0.hypot(v2.1));
    if l1 < 4.0 || l2 < 4.0 {
        return;
    }
    let cosine = ((v1.0 * v2.0 + v1.1 * v2.1) / (l1 * l2)).clamp(-1.0, 1.0);
    let degrees = cosine.acos().to_degrees();
    let a1 = v1.1.atan2(v1.0);
    let a2 = v2.1.atan2(v2.0);
    let mut delta = a2 - a1;
    while delta > std::f64::consts::PI {
        delta -= std::f64::consts::TAU;
    }
    while delta < -std::f64::consts::PI {
        delta += std::f64::consts::TAU;
    }
    let radius = 28.0;
    let arc = Path::new(|p| {
        p.arc(Arc {
            center: anchor,
            radius,
            start_angle: Radians(a1 as f32),
            end_angle: Radians((a1 + delta) as f32),
        })
    });
    frame.stroke(
        &arc,
        Stroke::default()
            .with_color(preview_color(&scene.drawing.options))
            .with_width(1.5),
    );
    let mid = a1 + delta / 2.0;
    guide_label(
        frame,
        format!("{}°", js_round(degrees) as i64),
        Point::new(
            anchor.x + (mid.cos() * (radius as f64 + 14.0)) as f32,
            anchor.y + (mid.sin() * (radius as f64 + 14.0)) as f32,
        ),
        preview_color(&scene.drawing.options),
        CONSOLAS_BOLD,
        11.0,
        scene.text_zoom_floor > 0.0,
    );
}

/// `drawReturnGuide` — dashed line from the chain's start to the target.
fn draw_return_guide(frame: &mut Frame, scene: &Scene, start_w: Pt, target_w: Pt) {
    let dist = distance(start_w, target_w);
    if dist < 0.5 {
        return;
    }
    let (start, target) = (scene.screen(start_w), scene.screen(target_w));
    frame.stroke(
        &Path::line(start, target),
        Stroke {
            line_dash: dashed(&[3.0, 5.0]),
            ..Stroke::default().with_color(witness()).with_width(1.0)
        },
    );
    guide_label(
        frame,
        scene.drawing.options.units.length(dist),
        Point::new(
            (start.x + target.x) / 2.0,
            (start.y + target.y) / 2.0 - 10.0,
        ),
        dim_label(),
        CONSOLAS,
        11.0,
        scene.text_zoom_floor > 0.0,
    );
}

/// `drawRubberBand` — the preview wall from the anchor to the snapped
/// pointer, with its dimension and the live guides.
fn draw_rubber_band(frame: &mut Frame, scene: &Scene, target: Pt) {
    let d = scene.drawing;
    let Some(kind) = scene.ctx.kind else { return };
    let Some(&last) = d.points.last() else { return };
    if !d.pen_down || last != d.cursor {
        return;
    }
    let (a, b) = (scene.screen(last), scene.screen(target));
    frame.stroke(
        &Path::line(a, b),
        Stroke {
            line_cap: LineCap::Round,
            ..Stroke::default()
                .with_color(preview_color(&scene.drawing.options))
                .with_width(wall_width(kind))
        },
    );
    if distance(last, target) > 0.3 {
        let mut preview = d.points.clone();
        preview.push(target);
        let side = path_dimension_side(&preview, last, target, scene.placement);
        draw_dimension(
            frame,
            scene,
            (last, target),
            (a, b),
            side,
            None,
            WALL_PREVIEW,
            false,
        );
        draw_rise_run(frame, scene, last, target);
        if d.points.len() >= 2 {
            draw_live_angle(frame, scene, d.points[d.points.len() - 2], last, target);
            draw_return_guide(frame, scene, d.points[0], target);
        }
    }
}

/// Raster reference for pixel tests. Production draws the embedded SVG itself.
#[cfg(test)]
static DRAW_POINTER_PNGS: std::sync::LazyLock<[Vec<u8>; 3]> = std::sync::LazyLock::new(|| {
    let tree = resvg::usvg::Tree::from_data(
        include_bytes!("../../../assets/cursors/floorplan-crosshair.svg"),
        &resvg::usvg::Options::default(),
    )
    .expect("embedded drawing cursor SVG");
    [32, 64, 128].map(|size| {
        let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).unwrap();
        let scale = size as f32 / 32.;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        pixmap.encode_png().expect("encode drawing cursor")
    })
});

static DRAW_POINTER: std::sync::LazyLock<iced::widget::svg::Handle> =
    std::sync::LazyLock::new(|| {
        iced::widget::svg::Handle::from_memory(
            include_bytes!("../../../assets/cursors/floorplan-crosshair.svg").as_slice(),
        )
    });

/// `options.pointerSize`: 32, 64 or 128 (anything else is 128).
pub fn pointer_size(options: &crate::options::Options) -> f32 {
    match options.pointer_size as i64 {
        32 => 32.0,
        64 => 64.0,
        _ => 128.0,
    }
}

/// `drawKeyboardPointer` / the draw-mode mouse pointer, in the size the
/// Drawing Pointer option chose. The SVG's white keyline provides contrast on
/// both light and dark canvases without changing the supplied artwork.
pub fn draw_drawing_pointer(frame: &mut Frame, at: Point, options: &crate::options::Options) {
    let size = pointer_size(options);
    let half = size / 2.0;
    frame.draw_svg(
        iced::Rectangle::new(
            Point::new(at.x - half, at.y - half),
            iced::Size::new(size, size),
        ),
        &*DRAW_POINTER,
    );
}

/// `drawKeyboardPreview` — the arrow keys' dashed preview wall from the last
/// committed point to the keyboard cursor, with the same aids as the mouse
/// rubber band.
fn draw_keyboard_preview(frame: &mut Frame, scene: &Scene) {
    let d = scene.drawing;
    let Some(kind) = scene.ctx.kind else { return };
    let Some(&last) = d.points.last() else { return };
    if !d.pen_down || last == d.cursor {
        return;
    }
    let (a, b) = (scene.screen(last), scene.screen(d.cursor));
    frame.stroke(
        &Path::line(a, b),
        Stroke {
            line_cap: LineCap::Round,
            line_dash: dashed(&[8.0, 5.0]),
            ..Stroke::default()
                .with_color(preview_color(&scene.drawing.options))
                .with_width(wall_width(kind))
        },
    );
    if distance(last, d.cursor) > 0.3 {
        let mut preview = d.points.clone();
        preview.push(d.cursor);
        let side = path_dimension_side(&preview, last, d.cursor, scene.placement);
        draw_dimension(
            frame,
            scene,
            (last, d.cursor),
            (a, b),
            side,
            None,
            WALL_PREVIEW,
            false,
        );
        draw_rise_run(frame, scene, last, d.cursor);
        if d.points.len() >= 2 {
            draw_live_angle(frame, scene, d.points[d.points.len() - 2], last, d.cursor);
            draw_return_guide(frame, scene, d.points[0], d.cursor);
        }
    }
}

/// `drawSnapPreview` — a cross at the drawing target.
fn draw_snap_preview(frame: &mut Frame, scene: &Scene, target: Pt) {
    if !scene.drawing.options.alignment_points {
        return;
    }
    let s = scene.screen(target);
    let cross = Path::new(|p| {
        p.move_to(Point::new(s.x - 6.0, s.y));
        p.line_to(Point::new(s.x + 6.0, s.y));
        p.move_to(Point::new(s.x, s.y - 6.0));
        p.line_to(Point::new(s.x, s.y + 6.0));
    });
    frame.stroke(
        &cross,
        Stroke::default()
            .with_color(snap_color(&scene.drawing.options))
            .with_width(1.5),
    );
}

/// Everything after the grid, in render() order.
/// `drawSymbols` — render placed symbols from the library geometry.
pub fn draw_symbol_placement_preview(frame: &mut Frame, scene: &Scene, symbol: &str, world: Pt) {
    let Some(def) = scene.symbol_lib.and_then(|lib| lib.symbol(symbol)) else {
        return;
    };
    let placement = crate::placed_symbol::resolve_symbol_placement(
        world,
        def.wall_snap,
        def.wall_gap_width,
        &scene.drawing.lines,
        &scene.drawing.polygons,
        None,
        scene.zoom,
    );
    let center = scene.screen(Pt::new(placement.x, placement.y));
    frame.push_transform();
    frame.translate(Vector::new(center.x, center.y));
    frame.rotate(Radians(placement.rotation.to_radians() as f32));
    crate::symlib::render_geometry(
        frame,
        def,
        PIXELS_PER_FOOT * scene.zoom / 12.0,
        0.0,
        0.0,
        crate::theme::hex(0x63B3ED),
        iced::Color::WHITE,
    );
    frame.pop_transform();
}

fn draw_symbols(frame: &mut Frame, scene: &Scene) {
    let lib = match scene.symbol_lib {
        Some(l) => l,
        None => return,
    };
    let pixel_scale = (PIXELS_PER_FOOT * scene.zoom) as f32;
    for sym in scene.symbols {
        // Hidden layers' symbols do not paint (canvas-engine's symbol pass).
        if !scene.gates.shows(sym.id) {
            continue;
        }
        let def = match lib.symbol(&sym.symbol_type) {
            Some(d) => d,
            None => continue,
        };
        let cx = scene.screen(Pt::new(sym.x, sym.y));
        let sc = sym.scale as f32;
        let s = pixel_scale * sc / 12.0;
        if !s.is_finite() || s <= 0.0 {
            continue;
        }
        // Browser drawSymbols order: translate -> rotate -> scale(-1, 1) for flipH.
        frame.push_transform();
        frame.translate(Vector::new(cx.x, cx.y));
        frame.rotate(Radians(
            (sym.rotation as f32) * std::f32::consts::PI / 180.0,
        ));
        if sym.flip_h {
            frame.scale_nonuniform(Vector::new(-1.0, 1.0));
        }
        crate::symlib::render_geometry(
            frame,
            def,
            s as f64,
            0.0,
            0.0,
            crate::theme::hex(0x334155),
            iced::Color::WHITE,
        );
        // Selection handles for selected symbols.
        if scene.selection.symbols.contains(&sym.id) {
            let bw_px =
                ((def.bounds.max_x - def.bounds.min_x) as f32 * sc * pixel_scale / 12.0).max(12.0);
            let bh_px =
                ((def.bounds.max_y - def.bounds.min_y) as f32 * sc * pixel_scale / 12.0).max(12.0);
            let accent = crate::theme::ICON_BLUE;
            let rect = Path::rectangle(
                Point::new(-bw_px / 2.0, -bh_px / 2.0),
                iced::Size::new(bw_px, bh_px),
            );
            frame.stroke(&rect, Stroke::default().with_color(accent).with_width(1.0));
            let hs = 7.0f32;
            for (hx, hy) in [
                (-bw_px / 2.0, -bh_px / 2.0),
                (bw_px / 2.0, -bh_px / 2.0),
                (-bw_px / 2.0, bh_px / 2.0),
                (bw_px / 2.0, bh_px / 2.0),
            ] {
                frame.fill_rectangle(
                    Point::new(hx - hs / 2.0, hy - hs / 2.0),
                    iced::Size::new(hs, hs),
                    accent,
                );
            }
            let rhandle_y = -bh_px / 2.0 - 30.0;
            frame.fill_rectangle(
                Point::new(-hs / 2.0, rhandle_y - hs / 2.0),
                iced::Size::new(hs, hs),
                accent,
            );
            let line = Path::line(
                Point::new(0.0, -bh_px / 2.0),
                Point::new(0.0, rhandle_y + hs / 2.0),
            );
            frame.stroke(&line, Stroke::default().with_color(accent).with_width(1.0));
        }
        frame.pop_transform();
    }
}

/// The ink of a label with no colour of its own (`color: null`, or the old
/// dark-theme `#e2e8f0`): the canvas theme's (`#1f2937` on White).
pub(crate) fn automatic_text_ink() -> Color {
    if crate::canvas_theme::current() == crate::canvas_theme::CanvasTheme::Light {
        crate::theme::hex(0x1f2937)
    } else {
        crate::theme::hex(0xe2e8f0)
    }
}

/// `rgba(r, g, b, a)` / `#rrggbb` -> a colour (the area-code table stores
/// both forms).
pub(crate) fn parse_css_color(s: &str) -> Option<Color> {
    if let Some(c) = parse_hex(s.trim()) {
        return Some(c);
    }
    let inner = s.trim().strip_prefix("rgba(")?.strip_suffix(')')?;
    let p: Vec<f32> = inner
        .split(',')
        .filter_map(|x| x.trim().parse::<f32>().ok())
        .collect();
    (p.len() == 4).then(|| Color::from_rgba(p[0] / 255.0, p[1] / 255.0, p[2] / 255.0, p[3]))
}

/// The ring's segments the hatch lines of one family cross: for each line
/// `a*x + b*y = k` (k on a 10px grid, fixed to the canvas as in the
/// original), the stretches of it inside the polygon (even-odd).
fn hatch_segments(ring: &[Point], a: f32, b: f32, bounds: (f32, f32)) -> Vec<(Point, Point)> {
    let mut out = Vec::new();
    let (w, h) = bounds;
    let kmax = a.abs() * w + b.abs() * h;
    let kmin = (-a.min(0.0).abs() * w).min(0.0) - b.min(0.0).abs() * h;
    let mut k = (kmin / 10.0).floor() * 10.0;
    while k <= kmax {
        // Intersections of the line with every ring edge, parametrised by
        // position along the line (direction (-b, a)).
        let mut hits: Vec<(f32, Point)> = Vec::new();
        for i in 0..ring.len() {
            let (p, q) = (ring[i], ring[(i + 1) % ring.len()]);
            let (fp, fq) = (a * p.x + b * p.y - k, a * q.x + b * q.y - k);
            if (fp < 0.0) != (fq < 0.0) {
                let t = fp / (fp - fq);
                let pt = Point::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t);
                hits.push((-b * pt.x + a * pt.y, pt));
            }
        }
        hits.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        let (pairs, _) = hits.as_chunks::<2>();
        for pair in pairs {
            out.push((pair[0].1, pair[1].1));
        }
        k += 10.0;
    }
    out
}

fn point_in_ring(ring: &[Point], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = ring.len() - 1;
    for i in 0..ring.len() {
        let (pi, pj) = (ring[i], ring[j]);
        if (pi.y > y) != (pj.y > y) && x < (pj.x - pi.x) * (y - pi.y) / (pj.y - pi.y) + pi.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// `drawPolygons`: each area's fill (the polygon's own colour / pattern /
/// opacity, else its area code's, else `fillGLA`), the pattern strokes, the
/// outline, and a selected area's highlight. UND areas stay plain wall
/// linework until picked. Then the auto-post label and size
/// (`getAreaLabelGroups`).
fn draw_areas(frame: &mut Frame, scene: &Scene) {
    if !scene.filters[0] {
        return;
    }
    let d = scene.drawing;
    let (vw, vh) = (frame.width(), frame.height());
    for poly in &d.polygons {
        if poly.points.len() < 3 || !scene.gates.shows(poly.id) {
            continue;
        }
        let selected = scene.selection.is_poly_selected(poly.id);
        let defined = poly.kind != "UND";
        if !selected && !defined {
            continue;
        }
        let def = scene.area_codes.iter().find(|c| c.code == poly.kind);
        let pattern = poly
            .fill_pattern
            .clone()
            .filter(|p| !p.is_empty())
            .or_else(|| def.map(|c| c.fill_style.clone()))
            .unwrap_or_else(|| "solid".into());
        let outline = poly
            .line_color
            .as_deref()
            .and_then(parse_css_color)
            .or_else(|| def.and_then(|c| parse_css_color(&c.color)))
            .unwrap_or(POLY_OUTLINE);
        // The ring on screen, curved edges flattened.
        let ring: Vec<Point> = d
            .engine()
            .flatten_ring(&poly.points, &poly.edges, 0.05)
            .iter()
            .map(|p| scene.screen(*p))
            .collect();
        if ring.len() < 3 {
            continue;
        }
        let path = Path::new(|p| {
            p.move_to(ring[0]);
            for pt in &ring[1..] {
                p.line_to(*pt);
            }
            p.close();
        });
        if defined && pattern != "none" {
            let base = poly
                .fill_color
                .as_deref()
                .filter(|c| !c.is_empty())
                .or_else(|| def.map(|c| c.fill_color.as_str()).filter(|c| !c.is_empty()))
                .and_then(parse_css_color);
            let opacity = poly.fill_opacity.or_else(|| def.map(|c| c.fill_opacity));
            let fill = match (base, opacity) {
                (Some(c), Some(o)) => Color {
                    a: (o / 100.0).clamp(0.0, 1.0) as f32,
                    ..c
                },
                (Some(c), None) => c,
                // `colors.fillGLA`.
                (None, _) => POLY_FILL,
            };
            frame.fill(&path, fill);
            if pattern != "solid" {
                let ink = Color { a: 0.42, ..outline };
                let stroke = Stroke::default().with_color(ink).with_width(1.0);
                let mut families: Vec<(f32, f32)> = Vec::new();
                if pattern == "diagonal" || pattern == "crosshatch" {
                    families.push((1.0, 1.0));
                }
                if pattern == "crosshatch" {
                    families.push((1.0, -1.0));
                }
                for (a, b) in families {
                    for (p, q) in hatch_segments(&ring, a, b, (vw, vh)) {
                        frame.stroke(&Path::line(p, q), stroke);
                    }
                }
                if pattern == "dots" {
                    let (minx, maxx) = ring
                        .iter()
                        .fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p.x), h.max(p.x)));
                    let (miny, maxy) = ring
                        .iter()
                        .fold((f32::MAX, f32::MIN), |(l, h), p| (l.min(p.y), h.max(p.y)));
                    let mut x = 4.0 + ((minx - 4.0) / 10.0).floor() * 10.0;
                    while x <= maxx {
                        let mut y = 4.0 + ((miny - 4.0) / 10.0).floor() * 10.0;
                        while y <= maxy {
                            if point_in_ring(&ring, x, y) {
                                frame.fill(&Path::circle(Point::new(x, y), 1.2), ink);
                            }
                            y += 10.0;
                        }
                        x += 10.0;
                    }
                }
            }
        }
        if selected {
            // `colors.selectPoly` / `selectPolyOutline`.
            frame.fill(
                &path,
                Color::from_rgba(192.0 / 255.0, 90.0 / 255.0, 250.0 / 255.0, 0.15),
            );
        }
        // `lineWidth = isSelected ? max(2, lineThickness || 1) : lineThickness || 1`
        // and the dash from `lineStyle`.
        let thickness = poly.line_thickness.filter(|t| *t != 0.0).unwrap_or(1.0) as f32;
        let dash: &[f32] = match poly.line_style.as_deref() {
            Some("dashed") => &[9.0, 6.0],
            Some("dotted") => &[2.0, 5.0],
            _ => &[],
        };
        frame.stroke(
            &path,
            Stroke {
                line_dash: dashed(dash),
                ..Stroke::default()
                    .with_color(if selected { SELECT_LINE } else { outline })
                    .with_width(if selected {
                        thickness.max(2.0)
                    } else {
                        thickness
                    })
            },
        );
        if defined {
            draw_area_labels(frame, scene, poly);
        }
    }
}

/// `getAreaLabelGroups` + the label / size text objects of `drawPolygons`:
/// the label (code, name) and the size (net, base, factor) are stacked
/// centred lines; the size sits below the label.
fn draw_area_labels(frame: &mut Frame, scene: &Scene, poly: &sketch_core::model::Polygon) {
    let d = scene.drawing;
    let blocks = crate::handles::area_blocks(d, scene.computed, poly);
    let label_lines = blocks
        .iter()
        .find(|b| b.kind == crate::handles::TextKind::AreaLabel)
        .map_or(0, |b| b.lines.len());
    let zoom = f64::from(scene.text_zoom());
    for block in &blocks {
        let at = scene.screen(block.at);
        // A rotated block turns about its centre (`labelRotation` /
        // `areaSizeRotation`); its lines stack in the turned frame.
        let rotated = block.rotation % 360.0 != 0.0;
        let center = if rotated { Point::ORIGIN } else { at };
        if rotated {
            frame.push_transform();
            frame.translate(iced::Vector::new(at.x, at.y));
            frame.rotate(Radians(block.rotation.to_radians() as f32));
        }
        let ink = block
            .color
            .as_deref()
            .and_then(parse_css_color)
            .unwrap_or_else(dim_label);
        let family = crate::handles::block_family(d, block);
        let italic = crate::handles::block_italic(d, block);
        let step = crate::handles::block_line_step(block, zoom);
        let count = block.lines.len();
        for (index, text) in block.lines.iter().enumerate() {
            let (font_size, bold) =
                crate::handles::block_line_font(d, block, label_lines, index, zoom);
            let y = center.y + (index as f32 - (count as f32 - 1.0) / 2.0) * step;
            let font = crate::theme::font_named(family, bold, italic);
            guide_label(
                frame,
                text.clone(),
                Point::new(center.x, y),
                ink,
                font,
                font_size,
                scene.text_zoom_floor > 0.0,
            );
            if block.underline {
                // `moveTo(-w/2, y + fontSize * .43)`: the width is estimated
                // (no text metrics in a canvas draw).
                let width =
                    text.chars().count() as f32 * font_size * if bold { 0.57 } else { 0.52 };
                let uy = y + font_size * 0.43;
                frame.stroke(
                    &Path::line(
                        Point::new(center.x - width / 2.0, uy),
                        Point::new(center.x + width / 2.0, uy),
                    ),
                    Stroke::default()
                        .with_color(ink)
                        .with_width((font_size / 14.0).max(1.0)),
                );
            }
        }
        if rotated {
            frame.pop_transform();
        }
    }
}

/// `drawLabels`: each label centred on its point in its own family / weight /
/// style, over the Text Background colour at the Text Background opacity
/// (`textBackgroundOpacity` 0-100, a 4px pad, radius 3), with an underline
/// when set. A rotated label turns about its centre: under a rotating canvas
/// transform iced draws the glyphs as outline paths (`Text::draw_with`), which
/// is how `ctx.rotate` + `fillText` reads in the original.
fn draw_texts(frame: &mut Frame, scene: &Scene) {
    let opts = &scene.drawing.options;
    let opacity = (opts.text_background_opacity.clamp(0.0, 100.0) / 100.0) as f32;
    for t in scene.texts {
        if !scene.gates.shows(t.id) {
            continue;
        }
        let centre = scene.screen(Pt::new(t.x, t.y));
        let rotated = t.rotation % 360.0 != 0.0;
        let p = if rotated { Point::ORIGIN } else { centre };
        if rotated {
            frame.push_transform();
            frame.translate(iced::Vector::new(centre.x, centre.y));
            frame.rotate(Radians(t.rotation.to_radians() as f32));
        }
        let automatic = t.color.is_empty() || t.color.eq_ignore_ascii_case("#e2e8f0");
        let color = if automatic {
            automatic_text_ink()
        } else {
            crate::render::parse_hex(&t.color).unwrap_or(automatic_text_ink())
        };
        let font = crate::theme::font_named(&t.font_family, t.bold, t.italic);
        // No text metrics inside a canvas draw: ~0.52em a glyph (0.57 bold).
        let (size, width) = crate::handles::label_metrics(t, scene.zoom);
        if opacity > 0.0 {
            let pad = 4.0;
            let bg = parse_hex(&opts.text_background).unwrap_or(Color::from_rgb8(0x11, 0x11, 0x11));
            let path = Path::rounded_rectangle(
                Point::new(p.x - width / 2.0 - pad, p.y - size / 2.0 - pad),
                Size::new(width + pad * 2.0, size + pad * 2.0),
                3.0.into(),
            );
            frame.fill(&path, Color { a: opacity, ..bg });
        }
        let text = Text {
            content: t.text.clone(),
            position: p,
            color,
            size: Pixels(size),
            font,
            align_x: Alignment::Center,
            align_y: Vertical::Center,
            ..Text::default()
        };
        if scene.text_zoom_floor > 0.0 {
            text.draw_with(|path, color| frame.fill(&path, color));
        } else {
            frame.fill_text(text);
        }
        if t.underline {
            let y = p.y + size * 0.43;
            frame.stroke(
                &Path::line(
                    Point::new(p.x - width / 2.0, y),
                    Point::new(p.x + width / 2.0, y),
                ),
                Stroke::default()
                    .with_color(color)
                    .with_width((size / 14.0).max(1.0)),
            );
        }
        if rotated {
            frame.pop_transform();
        }
    }
}

fn draw_measure(frame: &mut Frame, scene: &Scene) {
    let color = crate::theme::hex(0xF6AD55);
    let stroke = Stroke::default().with_color(color).with_width(1.5);
    for run in scene.measure_runs {
        for pair in run.windows(2) {
            let a = scene.screen(pair[0]);
            let b = scene.screen(pair[1]);
            let path = Path::line(a, b);
            frame.stroke(&path, stroke);
        }
        for p in run {
            let s = scene.screen(*p);
            frame.fill_rectangle(
                Point::new(s.x - 3.0, s.y - 3.0),
                iced::Size::new(6.0, 6.0),
                color,
            );
        }
    }
    // current run
    for pair in scene.measure_points.windows(2) {
        let a = scene.screen(pair[0]);
        let b = scene.screen(pair[1]);
        let path = Path::line(a, b);
        frame.stroke(&path, stroke);
    }
    for p in scene.measure_points {
        let s = scene.screen(*p);
        frame.fill_rectangle(
            Point::new(s.x - 3.0, s.y - 3.0),
            iced::Size::new(6.0, 6.0),
            color,
        );
    }
}

/// `drawChains` (canvas-engine.js:1968) — chained dimension strings: a shared
/// dimension line with per-span labels plus an overall tier, select-highlight
/// when the chain is picked, and the offset grab handle while selected.
fn draw_chains(frame: &mut Frame, scene: &Scene) {
    let d = scene.drawing;
    if d.dimension_chains.is_empty() || !scene.filters[3] {
        return;
    }
    for chain in &d.dimension_chains {
        let Some(tiers) = d.chain_tiers(chain) else {
            continue;
        };
        let selected = scene.selection.is_chain_selected(chain.id);
        let color = if selected { SELECT_LINE } else { dim_label() };
        let width = if selected { 1.5 } else { 1.0 };
        let length = tiers.computed.length;
        let refs = &tiers.computed.refs;

        // Screen-space axis of the dimension line, for label rotation.
        let a0 = scene.screen(tiers.point_at(0.0, 0.0));
        let a_end = scene.screen(tiers.point_at(length, 0.0));
        let mut angle = (a_end.y - a0.y).atan2(a_end.x - a0.x);
        let half_pi = std::f32::consts::FRAC_PI_2;
        if angle > half_pi {
            angle -= std::f32::consts::PI;
        }
        if angle < -half_pi {
            angle += std::f32::consts::PI;
        }
        // Screen-space outward normal of the dimension line (points away
        // from the wall run regardless of which side the chain sits on).
        let run0 = scene.screen(tiers.wall_at(0.0));
        let (rux, ruy) = (a_end.x - a0.x, a_end.y - a0.y);
        let axis_len = rux.hypot(ruy);
        if axis_len < 1.0 {
            continue;
        }
        let (raw_nx, raw_ny) = (-ruy / axis_len, rux / axis_len);
        let outward = if raw_nx * (run0.x - a0.x) + raw_ny * (run0.y - a0.y) < 0.0 {
            1.0
        } else {
            -1.0
        };
        let (nx, ny) = (raw_nx * outward, raw_ny * outward);

        let stroke = Stroke::default().with_color(color).with_width(width);
        // Witness stubs from the wall to just past the dimension line.
        for t in refs {
            let wall = scene.screen(tiers.wall_at(*t));
            let dim = scene.screen(tiers.point_at(*t, 0.0));
            let path = Path::new(|p| {
                p.move_to(Point::new(wall.x + nx * 3.0, wall.y + ny * 3.0));
                p.line_to(Point::new(dim.x + nx * 4.0, dim.y + ny * 4.0));
            });
            frame.stroke(&path, stroke);
        }
        // The shared dimension line with ticks at each ref.
        let tier = Path::new(|p| {
            p.move_to(a0);
            p.line_to(a_end);
        });
        frame.stroke(&tier, stroke);
        let tick = |frame: &mut Frame, p: iced::Point| {
            let path = Path::new(|b| {
                b.move_to(Point::new(p.x - 4.0, p.y + 4.0));
                b.line_to(Point::new(p.x + 4.0, p.y - 4.0));
            });
            frame.stroke(&path, stroke);
        };
        for t in refs {
            tick(frame, scene.screen(tiers.point_at(*t, 0.0)));
        }

        // Per-span labels.
        let minimum = if d.options.minimum_dimension_enabled {
            d.options.minimum_dimension
        } else {
            0.0
        };
        let font_size = (d.options.dimension_font_size * scene.text_zoom() as f64).max(9.0) as f32;
        for i in 0..refs.len().saturating_sub(1) {
            let span = refs[i + 1] - refs[i];
            if chain.hidden_spans.contains(&(i as i64)) {
                continue;
            }
            if span < minimum || span < 0.05 {
                continue;
            }
            let mid = scene.screen(tiers.point_at((refs[i] + refs[i + 1]) / 2.0, 0.0));
            let text = d.options.units.length(span);
            let at = Point::new(mid.x + nx * font_size * 0.9, mid.y + ny * font_size * 0.9);
            frame.with_save(|f| {
                f.translate(Vector::new(at.x, at.y));
                f.rotate(angle);
                guide_label(
                    f,
                    text,
                    Point::ORIGIN,
                    color,
                    dimension_font(&d.options),
                    font_size,
                    scene.text_zoom_floor > 0.0,
                );
            });
        }

        // Overall dimension on the second, outer tier.
        if chain.show_overall {
            let extra = tiers.overall_extra();
            let b0 = scene.screen(tiers.point_at(0.0, extra));
            let b_end = scene.screen(tiers.point_at(length, extra));
            for t in [0.0, length] {
                let inner = scene.screen(tiers.point_at(t, 0.0));
                let outer = scene.screen(tiers.point_at(t, extra));
                let path = Path::new(|p| {
                    p.move_to(Point::new(inner.x + nx * 2.0, inner.y + ny * 2.0));
                    p.line_to(Point::new(outer.x + nx * 4.0, outer.y + ny * 4.0));
                });
                frame.stroke(&path, stroke);
            }
            let outline = Path::new(|p| {
                p.move_to(b0);
                p.line_to(b_end);
            });
            frame.stroke(&outline, stroke);
            tick(frame, b0);
            tick(frame, b_end);
            let overall = d.options.units.length(length);
            let at = Point::new(
                (b0.x + b_end.x) / 2.0 + nx * font_size * 0.9,
                (b0.y + b_end.y) / 2.0 + ny * font_size * 0.9,
            );
            frame.with_save(|f| {
                f.translate(Vector::new(at.x, at.y));
                f.rotate(angle);
                guide_label(
                    f,
                    overall,
                    Point::ORIGIN,
                    color,
                    dimension_font(&d.options),
                    font_size,
                    scene.text_zoom_floor > 0.0,
                );
            });
        }

        // Selected: the offset grab handle at the middle of the tier.
        if selected {
            let mid = scene.screen(tiers.point_at(length / 2.0, 0.0));
            frame.fill(&Path::circle(mid, 4.0), SELECT_LINE);
        }
    }
}

/// `drawUnderlay` (canvas-engine.js:1218): the ghost of another page beneath
/// the current one — fixed-alpha gray strokes of its walls and closed area
/// outlines, at 1.5px round caps, before any current-page entities.
fn draw_underlay(frame: &mut Frame, scene: &Scene) {
    let Some(page) = scene.underlay else {
        return;
    };
    let stroke = Stroke::default()
        .with_color(Color::from_rgba8(148, 163, 184, 0.35))
        .with_width(1.5)
        .with_line_cap(iced::widget::canvas::LineCap::Round)
        .with_line_join(iced::widget::canvas::LineJoin::Round);
    for line in &page.lines {
        let a = scene.screen(line.start);
        let b = scene.screen(line.end);
        let path = Path::new(|p| {
            p.move_to(a);
            p.line_to(b);
        });
        frame.stroke(&path, stroke);
    }
    for poly in &page.polygons {
        let points = &poly.points;
        if points.len() < 3 {
            continue;
        }
        let path = Path::new(|p| {
            for (index, point) in points.iter().enumerate() {
                let s = scene.screen(*point);
                if index == 0 {
                    p.move_to(s);
                } else {
                    p.line_to(s);
                }
            }
            p.close();
        });
        frame.stroke(&path, stroke);
    }
}

/// `drawRotationAngle`: the swept wedge between the grab point and the
/// handle with the angle in its middle.
fn draw_rotation_angle(
    frame: &mut Frame,
    center: (f64, f64),
    reference: (f64, f64),
    current: (f64, f64),
    degrees: f64,
) {
    let radius = (reference.0 - center.0)
        .hypot(reference.1 - center.1)
        .clamp(30.0, 72.0);
    let start = (reference.1 - center.1).atan2(reference.0 - center.0);
    let mut end = (current.1 - center.1).atan2(current.0 - center.0);
    // The wedge sweeps the way the rotation went.
    let tau = std::f64::consts::TAU;
    if degrees < 0.0 {
        while end > start {
            end -= tau;
        }
    } else {
        while end < start {
            end += tau;
        }
    }
    let c = Point::new(center.0 as f32, center.1 as f32);
    let wedge = Path::new(|p| {
        p.move_to(c);
        p.arc(Arc {
            center: c,
            radius: radius as f32,
            start_angle: Radians(start as f32),
            end_angle: Radians(end as f32),
        });
        p.close();
    });
    frame.fill(
        &wedge,
        Color::from_rgba(99.0 / 255.0, 102.0 / 255.0, 241.0 / 255.0, 0.16),
    );
    frame.stroke(
        &wedge,
        Stroke::default()
            .with_color(Color::from_rgba(
                99.0 / 255.0,
                102.0 / 255.0,
                241.0 / 255.0,
                0.62,
            ))
            .with_width(1.0),
    );
    guide_label(
        frame,
        format!("{}\u{00B0}", degrees.abs().round() as i64),
        c,
        hex(0x6366F1),
        CONSOLAS_BOLD,
        16.0,
        false,
    );
}

/// The handle circle: an 8px green disc with a light ring.
fn handle_circle(frame: &mut Frame, at: (f64, f64)) {
    let c = Path::circle(Point::new(at.0 as f32, at.1 as f32), 8.0);
    frame.fill(&c, CURVE_GUIDE);
    frame.stroke(
        &c,
        Stroke::default().with_color(hex(0xDCFCE7)).with_width(1.0),
    );
}

/// `drawTransformHandles` for text entities and the selected areas: a dashed
/// green box with corner squares, the stalk and the round rotation handle.
fn draw_transform_handles(frame: &mut Frame, scene: &Scene) {
    if !scene.select_mode {
        return;
    }
    let d = scene.drawing;
    let green = CURVE_GUIDE;
    let light = hex(0xDCFCE7);
    let pt = |p: (f64, f64)| Point::new(p.0 as f32, p.1 as f32);
    if let Some(crate::view_state::Drag::SymbolRotate { id, preview, .. }) = scene.drag {
        // The symbol's own handle sits in its turned frame (drawn with the
        // symbol); while dragging, the wedge shows the turn.
        if let (true, Some(sym)) = (scene.drag_moved, scene.symbols.iter().find(|s| s.id == *id)) {
            if let Some(def) = scene.symbol_lib.and_then(|l| l.symbol(&sym.symbol_type)) {
                let b = crate::handles::symbol_box(
                    Pt::new(sym.x, sym.y),
                    sym.rotation,
                    sym.scale,
                    (
                        def.bounds.min_x,
                        def.bounds.min_y,
                        def.bounds.max_x,
                        def.bounds.max_y,
                    ),
                    scene.pan,
                    scene.zoom,
                );
                let delta = -preview.to_radians();
                let (vx, vy) = (b.handle.0 - b.center.0, b.handle.1 - b.center.1);
                let reference = (
                    b.center.0 + vx * delta.cos() - vy * delta.sin(),
                    b.center.1 + vx * delta.sin() + vy * delta.cos(),
                );
                draw_rotation_angle(frame, b.center, reference, b.handle, *preview);
            }
        }
    }
    for b in crate::handles::selected_text_boxes(
        d,
        scene.selection,
        scene.computed,
        scene.pan,
        scene.zoom,
    ) {
        let outline = Path::new(|p| {
            p.move_to(pt(b.corners[0]));
            for c in &b.corners[1..] {
                p.line_to(pt(*c));
            }
            p.close();
        });
        let dashed_stroke = Stroke {
            line_dash: dashed(&[4.0, 3.0]),
            ..Stroke::default().with_color(green).with_width(1.1)
        };
        frame.stroke(&outline, dashed_stroke);
        frame.stroke(&Path::line(pt(b.top_anchor), pt(b.handle)), dashed_stroke);
        for c in &b.corners {
            let (x, y) = (c.0 as f32 - 4.0, c.1 as f32 - 4.0);
            frame.fill_rectangle(Point::new(x, y), iced::Size::new(8.0, 8.0), green);
            frame.stroke(
                &Path::rectangle(Point::new(x, y), iced::Size::new(8.0, 8.0)),
                Stroke::default().with_color(light).with_width(1.1),
            );
        }
        handle_circle(frame, b.handle);
        if let Some(crate::view_state::Drag::RotateText {
            kind, id, preview, ..
        }) = scene.drag
        {
            if scene.drag_moved && *kind == b.kind && *id == b.id {
                let delta = -preview.to_radians();
                let (vx, vy) = (b.handle.0 - b.center.0, b.handle.1 - b.center.1);
                let reference = (
                    b.center.0 + vx * delta.cos() - vy * delta.sin(),
                    b.center.1 + vx * delta.sin() + vy * delta.cos(),
                );
                draw_rotation_angle(frame, b.center, reference, b.handle, *preview);
            }
        }
    }
    let Some(area) = crate::handles::selected_area_box(d, scene.selection, scene.pan, scene.zoom)
    else {
        return;
    };
    let dashed_stroke = Stroke {
        line_dash: dashed(&[4.0, 4.0]),
        ..Stroke::default().with_color(green).with_width(1.25)
    };
    if let Some(crate::view_state::Drag::RotateArea { preview, .. }) = scene.drag {
        let radians = preview.to_radians();
        let c = area.center_world;
        let rotate_world = |p: Pt| {
            let (dx, dy) = (p.x - c.x, p.y - c.y);
            Pt::new(
                c.x + dx * radians.cos() - dy * radians.sin(),
                c.y + dx * radians.sin() + dy * radians.cos(),
            )
        };
        for poly in d
            .polygons
            .iter()
            .filter(|p| scene.selection.is_poly_selected(p.id))
        {
            if poly.points.is_empty() {
                continue;
            }
            let outline = Path::new(|p| {
                for (i, point) in poly.points.iter().enumerate() {
                    let s = scene.screen(rotate_world(*point));
                    if i == 0 {
                        p.move_to(s);
                    } else {
                        p.line_to(s);
                    }
                }
                p.close();
            });
            frame.stroke(&outline, dashed_stroke);
        }
        let k = PIXELS_PER_FOOT * scene.zoom;
        let handle_world = Pt::new(
            (area.handle.0 - scene.pan.0) / k,
            (scene.pan.1 - area.handle.1) / k,
        );
        let top = scene.screen(rotate_world(Pt::new(c.x, area.max_y)));
        let handle = scene.screen(rotate_world(handle_world));
        frame.stroke(&Path::line(top, handle), dashed_stroke);
        handle_circle(frame, (f64::from(handle.x), f64::from(handle.y)));
        draw_rotation_angle(
            frame,
            area.center,
            area.handle,
            (f64::from(handle.x), f64::from(handle.y)),
            *preview,
        );
    } else {
        frame.stroke(&Path::line(pt(area.top), pt(area.handle)), dashed_stroke);
        handle_circle(frame, area.handle);
    }
}

pub fn draw_scene(frame: &mut Frame, scene: &Scene, w: f32, h: f32) {
    draw_scene_base(frame, scene);
    draw_scene_overlay(frame, scene, w, h);
}

/// The unchanged floorplan can be reused while the drawing pointer moves.
pub fn draw_scene_base(frame: &mut Frame, scene: &Scene) {
    // The underlay ghost is drawn before any current-page entities and is
    // never hit-tested (drawUnderlay, canvas-engine.js:1218).
    draw_underlay(frame, scene);
    // The reference image ghosts beneath the drawing
    // (`drawBgImage`, canvas-engine.js:3107).
    draw_bg_image(frame, scene);
    // `drawPolygons` (fills, outlines, the auto-post labels) is gated on the
    // Area fills filter and sits under the walls.
    draw_areas(frame, scene);
    draw_lines(frame, scene);
}

/// Keep live guides, previews and the pointer outside the floorplan cache.
pub fn draw_scene_overlay(frame: &mut Frame, scene: &Scene, w: f32, h: f32) {
    draw_witness_lines(frame, scene, w, h);
    draw_vertices(frame, scene);
    // `filters.symbols` / `filters.labels` gate their passes.
    if scene.filters[5] {
        draw_symbols(frame, scene);
    }
    if scene.filters[4] {
        draw_texts(frame, scene);
    }
    draw_chains(frame, scene);
    draw_measure(frame, scene);
    draw_transform_handles(frame, scene);
    // The Set Scale / Orient Guide point markers draw on top
    // (`drawScalePoints`, canvas-engine.js:3138).
    draw_scale_points(frame, scene);
    let d = scene.drawing;
    let live = d.pen_down && scene.ctx.kind.is_some() && !d.points.is_empty();
    if live {
        if d.keyboard_pointer {
            // The arrow keys own the cursor: a dashed preview to it, the
            // pointer parked there, and no mouse rubber band.
            draw_keyboard_preview(frame, scene);
            draw_drawing_pointer(frame, scene.screen(d.cursor), &d.options);
        } else {
            let target = d.drawing_target(scene.live_pointer.unwrap_or(d.pointer), scene.ctx);
            draw_rubber_band(frame, scene, target);
            draw_snap_preview(frame, scene, target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimension_side_follows_the_path_winding() {
        // The captured chain (-11,3)->(-1,3)->(-1,-7) is clockwise in world
        // space (negative signed area): its labels go outside, i.e. above
        // the top wall and right of the side wall.
        use crate::view_state::DimensionPlacement::{In, Out};
        let pts = [Pt::new(-11.0, 3.0), Pt::new(-1.0, 3.0), Pt::new(-1.0, -7.0)];
        assert_eq!(path_dimension_side(&pts, pts[0], pts[1], Out), -1.0);
        assert_eq!(path_dimension_side(&pts, pts[1], pts[0], Out), 1.0);
        assert_eq!(path_dimension_side(&pts[..2], pts[0], pts[1], Out), 1.0);
        // 'in' mirrors every side.
        assert_eq!(path_dimension_side(&pts, pts[0], pts[1], In), 1.0);
        assert_eq!(path_dimension_side(&pts[..2], pts[0], pts[1], In), -1.0);
        assert_eq!(parse_hex("#2B6CB0"), Some(WALL_EXT));
        assert_eq!(parse_hex("blue"), None);
    }
}

/// `getCurveMetrics` / `drawCurvedWallSolid` geometry (S5-054's bowed walls).
#[cfg(test)]
mod curve_tests {
    #[test]
    fn supplied_crosshair_svg_has_transparent_edges_and_centered_hotspot() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/drawing-pointer-preview");
        std::fs::create_dir_all(&dir).unwrap();
        for (index, size) in [32, 64, 128].into_iter().enumerate() {
            let png = &super::DRAW_POINTER_PNGS[index];
            let img = image::load_from_memory(png).unwrap().to_rgba8();
            assert_eq!(img.dimensions(), (size, size));
            assert_eq!(img.get_pixel(0, 0).0[3], 0);
            let center = img.get_pixel(size / 2, size / 2).0;
            assert!(center[0] < 60 && center[1] < 70 && center[2] < 80 && center[3] == 255);
            std::fs::write(dir.join(format!("crosshair-{size}.png")), png).unwrap();
        }
    }

    #[test]
    fn the_pointer_size_option_picks_32_64_or_128() {
        let mut o = crate::options::Options::default();
        for (value, shown) in [(32.0, 32.0), (64.0, 64.0), (128.0, 128.0), (48.0, 128.0)] {
            o.pointer_size = value;
            assert_eq!(super::pointer_size(&o), shown, "{value}");
        }
    }

    #[test]
    fn option_fonts_carry_family_weight_and_style() {
        let f = crate::theme::font_named("Georgia", true, true);
        assert_eq!(f.weight, iced::font::Weight::Bold);
        assert_eq!(f.style, iced::font::Style::Italic);
        assert_eq!(f.family, iced::font::Family::Name("Georgia"));
        // A blank family falls back to Arial; a repeat reuses the interned name.
        assert_eq!(
            crate::theme::font_named("  ", false, false).family,
            iced::font::Family::Name("Arial")
        );
        assert_eq!(
            f.family,
            crate::theme::font_named("Georgia", false, false).family
        );
    }

    use super::*;

    fn bowed(bow: f64) -> SketchLine {
        SketchLine {
            id: 1,
            start: Pt::new(-11.0, 3.0),
            end: Pt::new(-1.0, 3.0),
            kind: WallType::Exterior,
            color: None,
            line_thickness: None,
            thickness: None,
            dimension_hidden: false,
            layer_id: None,
            bow_angle: bow,
            arc_height: 0.0,
            line_style: None,
            dimension_position: None,
            dimension_anchor: None,
            extra: Default::default(),
        }
    }

    #[test]
    fn a_thirty_degree_bow_has_the_originals_circle() {
        // 10 ft chord, 30 degrees: radius 5/sin(15deg) = 19.3185, rise
        // 5*tan(7.5deg) = 0.65827 (the original's own arcHeight for that bow).
        let m = curve_metrics(&bowed(30.0)).expect("metrics");
        assert!((m.radius - 19.318516).abs() < 1e-5, "radius {}", m.radius);
        // The wall runs left to right, so the normal is +y: the apex sits
        // above the midpoint and the centre below it.
        assert!((m.apex.x - -6.0).abs() < 1e-9);
        assert!((m.apex.y - 3.658270).abs() < 1e-5, "apex y {}", m.apex.y);
        assert!((m.center.x - -6.0).abs() < 1e-9);
        assert!(
            (m.center.y - -15.660254).abs() < 1e-5,
            "centre y {}",
            m.center.y
        );
    }

    #[test]
    fn a_negative_bow_flips_the_apex() {
        let m = curve_metrics(&bowed(-30.0)).expect("metrics");
        assert!((m.apex.y - 2.341730).abs() < 1e-5, "apex y {}", m.apex.y);
        assert!(
            (m.center.y - 21.660254).abs() < 1e-5,
            "centre y {}",
            m.center.y
        );
    }

    #[test]
    fn a_straight_wall_has_no_curve_metrics() {
        assert!(curve_metrics(&bowed(0.0)).is_none());
    }
}
