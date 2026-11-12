//! Lossless conversion between the app's save model (JSON) and a Sketchy
//! [`Document`]: document root, pages, walls, areas, annotations (labels,
//! symbols, dimension chains), layers and the area-code table.
//!
//! Residual rule: every save-model object we convert keeps what Sketchy has no
//! core field for (and the app-side originals of lossy values) in
//! `extras[NS]`. [`from_sketchy`] starts from that residual and layers the
//! core fields on top; a residual value is reused only while the core value it
//! produced is unchanged, so edits made in Sketchy always win.
//!
//! The reverse direction: Sketchy content the save model cannot show (foreign
//! root fields, extra structures, level fields, open or unknown shapes, lossy
//! leftovers) is parked under `sketchy_other` on the matching save-model
//! object and restored by [`to_sketchy`]. Foreign ids that are not the
//! canonical `w-<int>` / `a-<int>` get a fresh integer app id and keep the
//! original in `sketchy_id`. Documents in other length units are scaled to
//! feet on read and written back in their original unit.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use serde_json::{json, Map, Value};

use super::model::{Document, Level, Shape, Space, Wall};
use super::{SketchyError, FORMAT, FORMAT_VERSION};

/// Extras namespace for Sketch Rust data.
pub const NS: &str = "io.github.garmonbz.sketchrust";
/// Residual key holding Sketchy-side fields this converter does not own.
const SKETCHY_OTHER: &str = "sketchy_other";
/// Residual key holding shape fields beyond `type`, `points` and `bulges`.
const SHAPE_REST: &str = "shape_rest";
/// Residual key holding the original circle/ellipse and the points approximating it.
const SHAPE_ORIG: &str = "shape_orig";
/// Residual key recording what a lossy core value looked like when it was parked.
const LOSSY: &str = "lossy";
/// App-object key recording an annotation's index in the Sketchy drawing order.
const ORDER: &str = "sketchy_index";
/// Root residual key holding unmapped annotations with their drawing index.
const PARKED_ANNOS: &str = "parked_annotations";
/// Residual key recording a non-canonical Sketchy id.
const SKETCHY_ID: &str = "sketchy_id";
/// App line/polygon marker: the line only draws an outline-only space's edge (not a Sketchy
/// wall), and a polygon carrying it had such lines made for it.
const DISPLAY: &str = "sketchy_display";
const BULGE_EPS: f64 = 1e-9;
/// Endpoint tolerance, in feet, when matching a space edge to a wall.
const SEGMENT_EPS: f64 = 1e-6;
const FT_PER_M: f64 = 3.280_839_895_013_123;
const CIRCLE_SEGMENTS: usize = 48;
/// App ids are in `1..MAX_ID` and `nextId` in `1..=MAX_ID` (2^53), so every id is
/// exact as a JavaScript number. Sketchy ids naming a number outside that range are
/// treated as non-canonical and get a fresh app id.
pub const MAX_ID: i64 = 9_007_199_254_740_992;
/// Root residual keys holding the document `id` and the `revision.id` it was opened at.
const DOCUMENT_ID: &str = "document_id";
const PARENT_REVISION: &str = "parent_revision";
/// Residual key recording where the area-code classification sat among an element's
/// classifications, so foreign ones keep their order.
const CLASS_INDEX: &str = "classification_index";
/// Root residual key recording the area-code scheme's index among the schemes.
const SCHEME_INDEX: &str = "scheme_index";
/// Page residual key holding spaces the app cannot show, with their index in the level.
const PARKED_SPACES: &str = "parked_spaces";

/// Whether `n` may be an app element id.
fn id_in_range(n: i64) -> bool {
    (1..MAX_ID).contains(&n)
}

fn exhausted() -> SketchyError {
    err("id space exhausted")
}

fn err(msg: impl Into<String>) -> SketchyError {
    SketchyError::Convert(msg.into())
}

// ---------------------------------------------------------------- helpers

/// Splits `obj` into (keys in `keys`, everything else).
fn split_object(obj: &Map<String, Value>, keys: &[&str]) -> (Map<String, Value>, Map<String, Value>) {
    let (mut owned, mut rest) = (Map::new(), Map::new());
    for (k, v) in obj {
        let side = if keys.contains(&k.as_str()) { &mut owned } else { &mut rest };
        side.insert(k.clone(), v.clone());
    }
    (owned, rest)
}

/// Merges `over` into `base`: objects merge key by key, arrays concatenate,
/// anything else is replaced by `over`.
fn merge_objects(mut base: Map<String, Value>, over: Map<String, Value>) -> Map<String, Value> {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(Value::Object(b)), Value::Object(o)) => *b = merge_objects(std::mem::take(b), o),
            (Some(Value::Array(b)), Value::Array(o)) => b.extend(o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
    base
}

/// Number equality with the round-trip tolerance. `Null` counts as absent.
fn approx_eq(a: Option<&Value>, b: Option<&Value>) -> bool {
    let (a, b) = (a.filter(|v| !v.is_null()), b.filter(|v| !v.is_null()));
    match (a, b) {
        (None, None) => true,
        (Some(Value::Number(x)), Some(Value::Number(y))) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => (x - y).abs() <= BULGE_EPS,
            _ => false,
        },
        (Some(Value::Array(x)), Some(Value::Array(y))) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| approx_eq(Some(p), Some(q)))
        }
        (Some(Value::Object(x)), Some(Value::Object(y))) => {
            x.len() == y.len() && x.iter().all(|(k, v)| approx_eq(Some(v), y.get(k)))
        }
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

thread_local! {
    /// Set when [`num`] met a non-finite float during the current conversion.
    static NON_FINITE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A float as JSON (`-0.0` becomes `0.0`). A non-finite value (for example a
/// unit scaling that overflowed) becomes `null` and fails the conversion in
/// [`finite_guard`].
fn num(v: f64) -> Value {
    if !v.is_finite() {
        NON_FINITE.with(|f| f.set(true));
    }
    Value::from(v + 0.0)
}

/// Runs a conversion and turns any non-finite number it produced into an error.
fn finite_guard<T>(run: impl FnOnce() -> Result<T, SketchyError>) -> Result<T, SketchyError> {
    NON_FINITE.with(|f| f.set(false));
    let out = run();
    if NON_FINITE.with(|f| f.replace(false)) {
        return Err(err("a number became infinite or NaN while converting (coordinates too large?)"));
    }
    out
}

fn to_map<T: serde::Serialize>(v: &T) -> Result<Map<String, Value>, SketchyError> {
    match serde_json::to_value(v) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(err("element did not serialize to an object")),
        Err(e) => Err(err(e.to_string())),
    }
}

fn as_object<'a>(v: &'a Value, what: &str) -> Result<&'a Map<String, Value>, SketchyError> {
    v.as_object().ok_or_else(|| err(format!("{what} must be an object")))
}

/// The app's world is +y up, like Sketchy. `geometry.rs` `flatten_ring` and
/// `render.rs` `curve_metrics` both put the apex of a positive bow on the left
/// normal `(-dy, dx)` of start to end, and Sketchy's positive bulge also bows
/// left, so the two share a sign. `tan(bow / 4)` is the bulge of a `bow` degree arc.
fn bulge_of_bow(bow: f64) -> f64 {
    (bow * PI / 720.0).tan()
}

fn bow_of_bulge(bulge: f64) -> f64 {
    bulge.atan() * 720.0 / PI
}

/// Feet per document length unit.
fn unit_scale(unit: &str) -> Result<f64, SketchyError> {
    match unit {
        "ft" => Ok(1.0),
        "in" => Ok(1.0 / 12.0),
        "m" => Ok(FT_PER_M),
        "cm" => Ok(FT_PER_M / 100.0),
        "mm" => Ok(FT_PER_M / 1000.0),
        other => Err(err(format!("unknown length_unit {other}"))),
    }
}

// ------------------------------------------------------ conversion context

/// Per-conversion state: unit scale and the id maps.
#[derive(Default)]
struct Ctx {
    /// Feet per document unit.
    scale: f64,
    /// App polygon id (JSON text) to its Sketchy id.
    to_sk: HashMap<String, String>,
    /// App wall id (JSON text) to its Sketchy id, when not `w-<id>`.
    wall_sk: HashMap<String, String>,
    /// (element kind, Sketchy id) to app id.
    to_app: HashMap<(char, String), Value>,
    /// App wall id (JSON text) to `[start, end]` in Sketchy coordinates.
    wall_pts: HashMap<String, Value>,
    /// Area code to the `use`/`finished` its table row gives spaces.
    code_defaults: HashMap<String, Map<String, Value>>,
    /// Whether layers are mapped (otherwise `layerId`s stay in residuals).
    layers_mapped: bool,
    /// App layer id to Sketchy layer id.
    layer_sid: HashMap<String, String>,
    /// Sketchy layer id to app layer id.
    layer_app: HashMap<String, String>,
    /// Sketchy element id to the app layer id it is assigned to.
    element_layer: HashMap<String, String>,
    /// Whether mapped annotations record their original drawing index.
    stamp_order: bool,
    /// Integer ids taken by every converted element (after [`Ctx::assign_ids`]).
    taken: HashSet<i64>,
    /// First integer id not taken.
    next_free: i64,
    /// Display-wall app lines per level.
    display_lines: Vec<Vec<Value>>,
    /// (level index, space id) to (edge index, app line id) of its display walls.
    display_edges: HashMap<(usize, String), Vec<(usize, i64)>>,
}

impl Ctx {
    fn new(scale: f64) -> Self {
        Ctx { scale, ..Ctx::default() }
    }

    /// Sketchy id of an app wall referenced from an annotation.
    fn wall_sid(&self, v: &Value) -> Option<String> {
        let plain = plain_id(v)?;
        Some(self.wall_sk.get(&v.to_string()).cloned().unwrap_or_else(|| format!("w-{plain}")))
    }

    fn point_to_sketchy(&self, p: &Value) -> Result<[f64; 2], SketchyError> {
        let x = p.get("x").and_then(Value::as_f64);
        let y = p.get("y").and_then(Value::as_f64);
        match (x, y) {
            (Some(x), Some(y)) => Ok([x / self.scale + 0.0, y / self.scale + 0.0]),
            _ => Err(err("point needs numeric x and y")),
        }
    }

    fn point_to_app(&self, p: [f64; 2]) -> Value {
        json!({"x": num(p[0] * self.scale), "y": num(p[1] * self.scale)})
    }

    /// Sketchy id for an app element: the stored `sketchy_id`, else `prefix-<id>`.
    fn element_id(prefix: &str, obj: &Map<String, Value>) -> Result<String, SketchyError> {
        if let Some(Value::String(s)) = obj.get(SKETCHY_ID) {
            return Ok(s.clone());
        }
        match obj.get("id") {
            Some(Value::Number(n)) => Ok(format!("{prefix}{n}")),
            Some(Value::String(s)) => Ok(format!("{prefix}{s}")),
            _ => Err(err("element needs an id")),
        }
    }

    /// Sketchy id for an app polygon id referenced from another polygon.
    fn space_ref_to_sketchy(&self, v: &Value) -> Option<String> {
        let plain = plain_id(v)?;
        Some(self.to_sk.get(&v.to_string()).cloned().unwrap_or_else(|| format!("a-{plain}")))
    }

    /// App id of a referenced element; `None` when no converted element has the Sketchy
    /// id `sid` (a dangling reference must not alias another element's number).
    fn app_ref(&self, kind: char, sid: &str) -> Option<Value> {
        self.to_app.get(&(kind, sid.to_string())).cloned()
    }

    /// App id of a converted element (every one is in `to_app` after [`Ctx::assign_ids`]).
    fn app_id(&self, kind: char, prefix: &str, sid: &str) -> Value {
        self.to_app.get(&(kind, sid.to_string())).cloned().unwrap_or_else(|| {
            let rest = sid.strip_prefix(prefix).unwrap_or(sid);
            rest.parse::<i64>().map_or_else(|_| Value::String(rest.to_string()), Value::from)
        })
    }

    /// Assigns an app id to every wall, area and mapped annotation. Returns the
    /// next free id when any fresh id was handed out.
    fn assign_ids(
        &mut self,
        levels: &[Level],
        annos: &[Mapped],
        parked: &[Value],
        next_id: Option<i64>,
    ) -> Result<bool, SketchyError> {
        let next_id = next_id.filter(|n| (1..=MAX_ID).contains(n));
        let mut used = used_ids(levels);
        let reserved: Vec<i64> = parked
            .iter()
            .filter_map(|a| {
                let spec = ANNO_SPECS.iter().find(|s| a.get("kind").and_then(Value::as_str) == Some(s.kind))?;
                canonical_id(spec.prefix, a.get("id")?.as_str()?)
            })
            .collect();
        used.extend(&reserved);
        let mut elems: Vec<Elem> = Vec::new();
        for level in levels {
            elems.extend(level.walls.iter().map(|w| ('w', w.id.as_str(), ns_extras(&w.extras))));
            elems.extend(
                level.spaces.iter().filter(|s| is_area(&s.shape)).map(|s| ('a', s.id.as_str(), ns_extras(&s.extras))),
            );
        }
        elems.extend(annos.iter().map(|m| (m.spec.ch, m.id, ns_of(m.obj))));
        let mut pending: Vec<(char, &str)> = Vec::new();
        for (kind, sid, residual) in &elems {
            let kept = residual
                .filter(|r| r.get(SKETCHY_ID).and_then(Value::as_str) == Some(sid))
                .and_then(|r| r.get("id").and_then(Value::as_i64))
                .filter(|n| id_in_range(*n) && used.insert(*n));
            match kept {
                Some(n) => {
                    self.to_app.insert((*kind, sid.to_string()), Value::from(n));
                }
                None => pending.push((*kind, sid)),
            }
        }
        let mut fresh = Vec::new();
        for (kind, sid) in pending {
            match canonical_id(prefix_of(kind), sid).filter(|n| used.insert(*n)) {
                Some(n) => {
                    self.to_app.insert((kind, sid.to_string()), Value::from(n));
                }
                None => fresh.push((kind, sid)),
            }
        }
        let max_used = used.iter().copied().filter(|n| id_in_range(*n)).max().unwrap_or(0);
        self.taken = used;
        self.next_free = (max_used + 1).max(next_id.unwrap_or(1));
        let any_fresh = !fresh.is_empty();
        for (kind, sid) in fresh {
            let n = self.alloc()?;
            self.to_app.insert((kind, sid.to_string()), Value::from(n));
        }
        for ((kind, sid), app) in &self.to_app {
            match kind {
                'a' => self.to_sk.insert(app.to_string(), sid.clone()),
                'w' => self.wall_sk.insert(app.to_string(), sid.clone()),
                _ => None,
            };
        }
        let outgrown = reserved.iter().max().is_some_and(|m| next_id.is_none_or(|n| *m >= n));
        Ok(any_fresh || outgrown)
    }

    /// The first id not taken; fails rather than leave `1..MAX_ID`.
    fn alloc(&mut self) -> Result<i64, SketchyError> {
        while self.taken.contains(&self.next_free) {
            self.next_free = self.next_free.checked_add(1).ok_or_else(exhausted)?;
        }
        let n = self.next_free;
        if !id_in_range(n) {
            return Err(exhausted());
        }
        self.taken.insert(n);
        self.next_free = n + 1;
        Ok(n)
    }
}

/// A straight segment in app coordinates; `id` is set for display walls made so far.
struct Seg {
    a: [f64; 2],
    b: [f64; 2],
    id: Option<i64>,
}

fn same_point(p: [f64; 2], q: [f64; 2]) -> bool {
    (p[0] - q[0]).abs() <= SEGMENT_EPS && (p[1] - q[1]).abs() <= SEGMENT_EPS
}

impl Seg {
    fn joins(&self, a: [f64; 2], b: [f64; 2]) -> bool {
        (same_point(self.a, a) && same_point(self.b, b)) || (same_point(self.a, b) && same_point(self.b, a))
    }
}

/// An outline-only space gets display walls; a space the app wrote itself (its
/// residual holds the app polygon) keeps the wall-less edges its author chose,
/// unless it was opened outline-only before.
fn wants_display(space: &Space) -> bool {
    ns_extras(&space.extras).is_none_or(|r| !r.contains_key("points") || r.get(DISPLAY) == Some(&Value::Bool(true)))
}

impl Ctx {
    fn to_app_xy(&self, p: [f64; 2]) -> [f64; 2] {
        [p[0] * self.scale, p[1] * self.scale]
    }

    /// Id for a new display wall: the one its polygon edge held before, if free, else a fresh one.
    fn display_id(&mut self, space: &Space, edge: usize) -> Result<i64, SketchyError> {
        let kept = ns_extras(&space.extras)
            .and_then(|r| r.get("edges")?.get(edge)?.get("lineId")?.as_i64())
            .filter(|n| id_in_range(*n) && self.taken.insert(*n));
        match kept {
            Some(n) => {
                self.next_free = self.next_free.max(n + 1);
                Ok(n)
            }
            None => self.alloc(),
        }
    }

    /// Plans an app line for every edge of every outline-only space that no wall covers
    /// (top-level spaces first, so their edges are `exterior`). Returns whether the next
    /// free id grew (a fresh id was used or a kept one lies beyond it).
    fn plan_display(&mut self, levels: &[Level]) -> Result<bool, SketchyError> {
        let start = self.next_free;
        self.display_lines = vec![Vec::new(); levels.len()];
        for (li, level) in levels.iter().enumerate() {
            let mut segs: Vec<Seg> = level
                .walls
                .iter()
                .map(|w| Seg { a: self.to_app_xy(w.start), b: self.to_app_xy(w.end), id: None })
                .collect();
            for subdivision in [false, true] {
                for space in level.spaces.iter().filter(|s| s.within.is_some() == subdivision && wants_display(s)) {
                    let Some(outline) = outline_of(&space.shape, &Value::Null) else { continue };
                    let kind = if subdivision { "interior" } else { "exterior" };
                    let mut assigned = Vec::new();
                    let n = outline.points.len();
                    for i in 0..n {
                        let (a, b) = (self.to_app_xy(outline.points[i]), self.to_app_xy(outline.points[(i + 1) % n]));
                        if same_point(a, b) {
                            continue;
                        }
                        match segs.iter().find(|s| s.joins(a, b)) {
                            Some(Seg { id: Some(id), .. }) => assigned.push((i, *id)),
                            Some(_) => {}
                            None => {
                                let id = self.display_id(space, i)?;
                                segs.push(Seg { a, b, id: Some(id) });
                                let bow = outline.bulges.as_ref().and_then(|b| b.get(i)).map_or(0.0, |b| bow_of_bulge(*b));
                                let mut line = json!({"id": id, "start": {"x": num(a[0]), "y": num(a[1])},
                                                      "end": {"x": num(b[0]), "y": num(b[1])},
                                                      "type": kind, DISPLAY: true});
                                if bow != 0.0 {
                                    line["bowAngle"] = num(bow);
                                }
                                self.display_lines[li].push(line);
                                assigned.push((i, id));
                            }
                        }
                    }
                    if !assigned.is_empty() {
                        self.display_edges.insert((li, space.id.clone()), assigned);
                    }
                }
            }
        }
        Ok(self.next_free != start)
    }
}

/// (kind, Sketchy id, residual) of a wall or area being given an app id.
type Elem<'a> = (char, &'a str, Option<&'a Map<String, Value>>);

fn plain_id(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Sketchy id prefix of an element kind (`'w'`, `'a'` or an annotation kind).
fn prefix_of(kind: char) -> &'static str {
    match kind {
        'w' => "w-",
        'a' => "a-",
        c => ANNO_SPECS.iter().find(|s| s.ch == c).map_or("", |s| s.prefix),
    }
}

/// The `extras[NS]` residual of a raw Sketchy object.
fn ns_of(obj: &Map<String, Value>) -> Option<&Map<String, Value>> {
    obj.get("extras").and_then(|e| e.get(NS)).and_then(Value::as_object)
}

fn ns_extras(extras: &Option<Map<String, Value>>) -> Option<&Map<String, Value>> {
    extras.as_ref().and_then(|e| e.get(NS)).and_then(Value::as_object)
}

/// `Some(n)` only for exactly `<prefix><n>` (digits, no leading zeros).
fn canonical_id(prefix: &str, id: &str) -> Option<i64> {
    let rest = id.strip_prefix(prefix)?;
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: i64 = rest.parse().ok()?;
    (n.to_string() == rest && id_in_range(n)).then_some(n)
}

/// Integer ids already taken by objects kept in page residuals (labels, symbols, ...).
fn used_ids(levels: &[Level]) -> HashSet<i64> {
    let mut used = HashSet::new();
    for level in levels {
        let Some(page) = ns_extras(&level.extras) else { continue };
        for item in page.values().filter_map(Value::as_array).flatten() {
            used.extend(item.get("id").and_then(Value::as_i64));
        }
    }
    used
}

fn is_area(shape: &Shape) -> bool {
    matches!(shape, Shape::Polygon { .. } | Shape::Circle { .. } | Shape::Ellipse { .. })
}

// ------------------------------------------------- lossy field groups

/// A group of save-model keys that map onto Sketchy core fields.
///
/// `fwd` projects the save-model object onto the core fields; `back` writes core
/// values back into the object. `fwd` is also how a stale residual is detected.
struct Group {
    core_keys: &'static [&'static str],
    fwd: fn(&Ctx, &Map<String, Value>) -> Map<String, Value>,
    back: fn(&Ctx, &mut Map<String, Value>, &Map<String, Value>),
}

fn single(key: &str, v: Option<Value>) -> Map<String, Value> {
    v.into_iter().map(|v| (key.to_string(), v)).collect()
}

fn f64_of(obj: &Map<String, Value>, key: &str) -> Option<f64> {
    obj.get(key).and_then(Value::as_f64)
}

fn str_of<'a>(obj: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    obj.get(key).and_then(Value::as_str)
}

/// Sets `key` to `v`, or removes it when `v` is `None`.
fn put(obj: &mut Map<String, Value>, key: &str, v: Option<Value>) {
    match v {
        Some(v) => obj.insert(key.to_string(), v),
        None => obj.remove(key),
    };
}

fn bulge_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("bulge", f64_of(obj, "bowAngle").filter(|b| *b != 0.0).map(|b| num(bulge_of_bow(b))))
}

fn bulge_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    let bow = core.get("bulge").and_then(Value::as_f64).map_or(0.0, bow_of_bulge);
    if bow != 0.0 || obj.contains_key("bowAngle") {
        obj.insert("bowAngle".into(), num(bow));
    }
    obj.remove("arcHeight"); // stale; apply_model recomputes it
}

fn thickness_fwd(ctx: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("thickness", f64_of(obj, "thickness").map(|t| num(t / 12.0 / ctx.scale)))
}

fn thickness_back(ctx: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    put(obj, "thickness", core.get("thickness").and_then(Value::as_f64).map(|t| num(t * ctx.scale * 12.0)));
}

fn kind_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    let kind = str_of(obj, "type").map(|t| if t == "exterior" { "exterior" } else { "interior" });
    single("kind", kind.map(Value::from))
}

fn kind_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    let t = core.get("kind").and_then(Value::as_str).map(|k| if k == "exterior" { "exterior" } else { "interior" });
    put(obj, "type", t.map(Value::from));
}

const WALL_GROUPS: &[Group] = &[
    Group { core_keys: &["bulge"], fwd: bulge_fwd, back: bulge_back },
    Group { core_keys: &["thickness"], fwd: thickness_fwd, back: thickness_back },
    Group { core_keys: &["kind"], fwd: kind_fwd, back: kind_back },
];

fn name_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("name", str_of(obj, "name").map(Value::from))
}

fn name_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    put(obj, "name", core.get("name").cloned());
}

fn code_scheme() -> String {
    format!("{NS}.area-codes")
}

fn is_code_class(c: &Value) -> bool {
    c.get("scheme").and_then(Value::as_str) == Some(code_scheme().as_str())
}

fn code_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("classifications", str_of(obj, "type").map(|c| json!([{"scheme": code_scheme(), "code": c}])))
}

fn code_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    let code = core
        .get("classifications")
        .and_then(Value::as_array)
        .and_then(|cs| cs.iter().find(|c| is_code_class(c)))
        .and_then(|c| c.get("code").cloned());
    put(obj, "type", code);
}

fn factor_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("area_factor", f64_of(obj, "factor").filter(|f| *f != 1.0).map(num))
}

fn factor_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    match core.get("area_factor") {
        Some(f) => put(obj, "factor", Some(f.clone())),
        None if obj.contains_key("factor") => put(obj, "factor", Some(num(1.0))),
        None => {}
    }
}

fn measured_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    let m = str_of(obj, "measureTo").map(|m| match m {
        "interior" => "interior_face",
        "exterior" => "exterior_face",
        "center" => "centerline",
        _ => "unspecified",
    });
    single("measured_to", m.map(Value::from))
}

fn measured_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    let m = core.get("measured_to").and_then(Value::as_str).and_then(|m| match m {
        "interior_face" => Some("interior"),
        "exterior_face" => Some("exterior"),
        "centerline" => Some("center"),
        _ => None,
    });
    put(obj, "measureTo", m.map(Value::from));
}

fn subtract_fwd(ctx: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    let within = (str_of(obj, "subtractMode") == Some("always"))
        .then(|| obj.get("subtractTargetId").and_then(|t| ctx.space_ref_to_sketchy(t)))
        .flatten();
    let mut m = single("within", within.map(Value::from));
    if m.contains_key("within") {
        m.insert("subtract".into(), Value::Bool(true));
    }
    m
}

/// Only `subtract: true` deducts (SPEC section 6); a bare `within`, or one naming no
/// converted area, is parked in `sketchy_other` by the leftover rule.
fn subtract_back(ctx: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    let target = core
        .get("within")
        .and_then(Value::as_str)
        .filter(|_| core.get("subtract").and_then(Value::as_bool) == Some(true))
        .and_then(|w| ctx.app_ref('a', w));
    match target {
        Some(id) => {
            obj.insert("subtractMode".into(), "always".into());
            obj.insert("subtractTargetId".into(), id);
        }
        None => {
            if str_of(obj, "subtractMode") == Some("always") {
                obj.insert("subtractMode".into(), "auto".into());
            }
            if obj.contains_key("subtractTargetId") {
                obj.insert("subtractTargetId".into(), Value::Null);
            }
        }
    }
}

fn attributes_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("attributes", obj.get("attributes").filter(|a| a.as_object().is_some_and(|m| !m.is_empty())).cloned())
}

fn attributes_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    match core.get("attributes") {
        Some(a) => put(obj, "attributes", Some(a.clone())),
        None if obj.contains_key("attributes") => put(obj, "attributes", Some(json!({}))),
        None => {}
    }
}

fn bow_of_edge(edge: &Value) -> f64 {
    edge.get("bowAngle").and_then(Value::as_f64).unwrap_or(0.0)
}

fn bulges_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    let edges = obj.get("edges").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    let points = obj.get("points").and_then(Value::as_array).map_or(0, Vec::len);
    if edges.iter().all(|e| bow_of_edge(e) == 0.0) {
        return Map::new();
    }
    let bulges: Vec<Value> = (0..points)
        .map(|i| num(edges.get(i).map_or(0.0, |e| bulge_of_bow(bow_of_edge(e)))))
        .collect();
    single("bulges", Some(Value::Array(bulges)))
}

fn bulges_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    let bulges: Vec<f64> = core
        .get("bulges")
        .and_then(Value::as_array)
        .map_or_else(Vec::new, |b| b.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect());
    let mut edges: Vec<Value> = obj.get("edges").and_then(Value::as_array).cloned().unwrap_or_default();
    while edges.len() < bulges.len() {
        edges.push(json!({"lineId": null, "bowAngle": num(0.0)}));
    }
    for (i, edge) in edges.iter_mut().enumerate() {
        let want = bulges.get(i).copied().map_or(0.0, bow_of_bulge);
        if let Some(e) = edge.as_object_mut() {
            if (bow_of_edge(&Value::Object(e.clone())) - want).abs() > BULGE_EPS {
                e.insert("bowAngle".into(), num(want));
                e.remove("arcHeight");
            }
        }
    }
    if !edges.is_empty() || obj.contains_key("edges") {
        obj.insert("edges".into(), Value::Array(edges));
    }
}

const POLYGON_GROUPS: &[Group] = &[
    Group { core_keys: &["name"], fwd: name_fwd, back: name_back },
    Group { core_keys: &["classifications"], fwd: code_fwd, back: code_back },
    Group { core_keys: &["area_factor"], fwd: factor_fwd, back: factor_back },
    Group { core_keys: &["measured_to"], fwd: measured_fwd, back: measured_back },
    Group { core_keys: &["within", "subtract"], fwd: subtract_fwd, back: subtract_back },
    Group { core_keys: &["attributes"], fwd: attributes_fwd, back: attributes_back },
    Group { core_keys: &["bulges"], fwd: bulges_fwd, back: bulges_back },
    Group { core_keys: &["use", "finished", "enclosure"], fwd: space_use_fwd, back: keep_back },
];

/// Core keys handled elsewhere, so the leftover rule skips them.
const NO_LEFTOVER: &[&str] = &["bulges", "classifications"];

/// Core view of a save-model object: every group's projection merged.
fn project(ctx: &Ctx, obj: &Map<String, Value>, groups: &[Group]) -> Map<String, Value> {
    groups.iter().flat_map(|g| (g.fwd)(ctx, obj)).collect()
}

/// Applies the core view to `obj`, keeping residual values whose projection
/// still equals the core value (core wins otherwise).
fn apply_groups(ctx: &Ctx, obj: &mut Map<String, Value>, view: &Map<String, Value>, groups: &[Group]) {
    for g in groups {
        let stale = (g.fwd)(ctx, obj);
        if g.core_keys.iter().all(|k| approx_eq(stale.get(*k), view.get(*k))) {
            continue;
        }
        let core: Map<String, Value> =
            g.core_keys.iter().filter_map(|k| view.get(*k).map(|v| (k.to_string(), v.clone()))).collect();
        (g.back)(ctx, obj, &core);
    }
}

/// Core values the app object cannot express (wall kind `party`, `within`
/// without `subtract`, `measured_to: unspecified`, ...) go into `rest`, with
/// what the app object projects for them in `lossy`, so [`restore_lossy`]
/// can tell when the app side has since changed.
fn park_leftovers(
    ctx: &Ctx,
    obj: &Map<String, Value>,
    view: &Map<String, Value>,
    groups: &[Group],
    rest: &mut Map<String, Value>,
) {
    let mut lossy = Map::new();
    for g in groups {
        let now = (g.fwd)(ctx, obj);
        for k in g.core_keys.iter().filter(|k| !NO_LEFTOVER.contains(k)) {
            if let Some(v) = view.get(*k).filter(|_| !approx_eq(now.get(*k), view.get(*k))) {
                rest.insert(k.to_string(), v.clone());
                lossy.insert(k.to_string(), now.get(*k).cloned().unwrap_or(Value::Null));
            }
        }
    }
    if !lossy.is_empty() {
        rest.insert(LOSSY.into(), Value::Object(lossy));
    }
}

/// Drops parked leftovers whose app-side projection changed since they were parked.
fn restore_lossy(core: &Map<String, Value>, other: &mut Map<String, Value>) {
    if let Some(Value::Object(lossy)) = other.remove(LOSSY) {
        for (k, base) in lossy {
            if !approx_eq(core.get(&k), Some(&base)) {
                other.remove(&k);
            }
        }
    }
}


/// `use`/`finished` the area-code table gives a space with this code. The app
/// polygon has no field for them, so a differing Sketchy value is parked.
fn space_use_fwd(ctx: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    str_of(obj, "type").and_then(|c| ctx.code_defaults.get(c)).cloned().unwrap_or_default()
}

/// For core fields the app object cannot hold; leftovers are parked instead.
fn keep_back(_: &Ctx, _: &mut Map<String, Value>, _: &Map<String, Value>) {}

// ------------------------------------------------ annotation field groups

fn position_fwd(ctx: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    let p = f64_of(obj, "x").zip(f64_of(obj, "y")).map(|(x, y)| json!([num(x / ctx.scale), num(y / ctx.scale)]));
    single("position", p)
}

fn position_back(ctx: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    let p = core.get("position").and_then(Value::as_array).and_then(|p| Some((p.first()?.as_f64()?, p.get(1)?.as_f64()?)));
    if let Some((x, y)) = p {
        obj.insert("x".into(), num(x * ctx.scale));
        obj.insert("y".into(), num(y * ctx.scale));
    }
}

fn rotation_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    // render.rs rotates labels and symbols with `frame.rotate` inside an iced canvas frame whose
    // y axis points down the screen, so a positive app rotation turns clockwise as seen. Sketchy
    // rotation is counter-clockwise-positive (+y up), hence the negation.
    single("rotation", f64_of(obj, "rotation").map(|r| num(-r)))
}

fn rotation_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    match core.get("rotation").and_then(Value::as_f64) {
        Some(r) => put(obj, "rotation", Some(num(-r))),
        None if obj.contains_key("rotation") => put(obj, "rotation", Some(num(0.0))),
        None => {}
    }
}

fn text_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("text", str_of(obj, "text").map(Value::from))
}

fn text_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    put(obj, "text", core.get("text").cloned());
}

fn symbols_scheme() -> String {
    format!("{NS}.symbols")
}

fn ref_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("ref", str_of(obj, "symbolType").map(|n| json!({"scheme": symbols_scheme(), "name": n})))
}

fn ref_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    let name = core.get("ref").and_then(|r| r.get("name")).filter(|n| n.is_string());
    put(obj, "symbolType", name.cloned());
}

fn scale_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("scale", f64_of(obj, "scale").map(|s| json!([num(s), num(s)])))
}

fn scale_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    let s = core.get("scale").and_then(Value::as_array).and_then(|s| s.first()).filter(|s| s.is_number());
    put(obj, "scale", s.cloned());
}

fn mirror_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    let m = obj.get("flipH").and_then(Value::as_bool).map(|f| if f { "x" } else { "none" });
    single("mirror", m.map(Value::from))
}

fn mirror_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    match core.get("mirror").and_then(Value::as_str) {
        Some(m) => put(obj, "flipH", Some(Value::Bool(m == "x"))),
        None if obj.contains_key("flipH") => put(obj, "flipH", Some(Value::Bool(false))),
        None => {}
    }
}

fn host_fwd(ctx: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("host", obj.get("wallId").and_then(|w| ctx.wall_sid(w)).map(|w| json!({"wall": w})))
}

fn host_back(ctx: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    // A host naming no wall is parked in `sketchy_other` by the leftover rule.
    match core.get("host").and_then(|h| h.get("wall")).and_then(Value::as_str).and_then(|w| ctx.app_ref('w', w)) {
        Some(id) => put(obj, "wallId", Some(id)),
        None if obj.contains_key("wallId") => put(obj, "wallId", Some(Value::Null)),
        None => {}
    }
}

/// The app draws a chain `offset * side` along the normal `(-dir.y, dir.x)` computed in
/// world space (`geometry.rs` `compute_dimension_chain`, `chains.rs` `ChainTiers`), which
/// is the left of start to end with +y up. Sketchy's positive offset is also left.
fn offset_fwd(ctx: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    let side = f64_of(obj, "side").filter(|s| *s != 0.0).unwrap_or(1.0);
    single("offset", f64_of(obj, "offset").map(|o| num(o * side / ctx.scale)))
}

fn offset_back(ctx: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    let Some(o) = core.get("offset").and_then(Value::as_f64).map(|o| o * ctx.scale) else {
        obj.remove("offset");
        return;
    };
    obj.insert("offset".into(), num(o.abs()));
    if o != 0.0 {
        obj.insert("side".into(), Value::from(if o > 0.0 { 1 } else { -1 }));
    }
}

fn target_fwd(ctx: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("target", obj.get("seedLineId").and_then(|w| ctx.wall_sid(w)).map(|w| json!({"wall": w})))
}

fn target_back(ctx: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    let target = core.get("target").and_then(|t| t.get("wall")).and_then(Value::as_str);
    if let Some(id) = target.and_then(|w| ctx.app_ref('w', w)) {
        obj.insert("seedLineId".into(), id);
    }
}

/// A chain measures its seed wall, so its Sketchy `points` are the wall's ends.
fn chain_points_fwd(ctx: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("points", obj.get("seedLineId").and_then(|w| ctx.wall_pts.get(&w.to_string())).cloned())
}

const LABEL_GROUPS: &[Group] = &[
    Group { core_keys: &["position"], fwd: position_fwd, back: position_back },
    Group { core_keys: &["text"], fwd: text_fwd, back: text_back },
    Group { core_keys: &["rotation"], fwd: rotation_fwd, back: rotation_back },
];
const SYMBOL_GROUPS: &[Group] = &[
    Group { core_keys: &["position"], fwd: position_fwd, back: position_back },
    Group { core_keys: &["rotation"], fwd: rotation_fwd, back: rotation_back },
    Group { core_keys: &["ref"], fwd: ref_fwd, back: ref_back },
    Group { core_keys: &["scale"], fwd: scale_fwd, back: scale_back },
    Group { core_keys: &["mirror"], fwd: mirror_fwd, back: mirror_back },
    Group { core_keys: &["host"], fwd: host_fwd, back: host_back },
];
const CHAIN_GROUPS: &[Group] = &[
    Group { core_keys: &["offset"], fwd: offset_fwd, back: offset_back },
    Group { core_keys: &["target"], fwd: target_fwd, back: target_back },
    Group { core_keys: &["points"], fwd: chain_points_fwd, back: keep_back },
];

const LABEL_OWNED: &[&str] = &["id", "kind", "level", "position", "text", "rotation"];
const SYMBOL_OWNED: &[&str] = &["id", "kind", "level", "position", "rotation", "ref", "scale", "mirror", "host"];
const CHAIN_OWNED: &[&str] = &["id", "kind", "level", "offset", "target", "points"];

fn has_position(o: &Map<String, Value>) -> bool {
    o.get("position").and_then(Value::as_array).is_some_and(|p| p.len() == 2 && p.iter().all(Value::is_number))
}

/// The annotation has what the app label needs.
fn text_ok(o: &Map<String, Value>) -> bool {
    has_position(o) && o.get("text").is_some_and(Value::is_string)
}

/// A library symbol (`ref`), not an inline `definition`.
fn symbol_ok(o: &Map<String, Value>) -> bool {
    has_position(o) && o.get("ref").and_then(|r| r.get("name")).is_some_and(Value::is_string)
}

/// A chain that follows a wall.
fn dimension_ok(o: &Map<String, Value>) -> bool {
    o.get("target").and_then(|t| t.get("wall")).is_some_and(Value::is_string)
}

/// An annotation kind the app has: where it lives in a page and how it maps.
struct AnnoSpec {
    /// Sketchy `kind`.
    kind: &'static str,
    /// Element kind character in the id maps.
    ch: char,
    /// Sketchy id prefix.
    prefix: &'static str,
    /// Page key holding the app objects.
    key: &'static str,
    groups: &'static [Group],
    owned: &'static [&'static str],
    /// Whether a Sketchy annotation of this kind can be shown by the app.
    ok: fn(&Map<String, Value>) -> bool,
}

const ANNO_SPECS: &[AnnoSpec] = &[
    AnnoSpec { kind: "text", ch: 't', prefix: "t-", key: "labels", groups: LABEL_GROUPS, owned: LABEL_OWNED, ok: text_ok },
    AnnoSpec { kind: "symbol", ch: 'y', prefix: "y-", key: "symbols", groups: SYMBOL_GROUPS, owned: SYMBOL_OWNED, ok: symbol_ok },
    AnnoSpec {
        kind: "dimension",
        ch: 'd',
        prefix: "d-",
        key: "dimensionChains",
        groups: CHAIN_GROUPS,
        owned: CHAIN_OWNED,
        ok: dimension_ok,
    },
];

/// A Sketchy annotation the app shows as a label, symbol or chain on `page`.
struct Mapped<'a> {
    /// Position in `presentation.annotations`.
    index: usize,
    page: usize,
    spec: &'static AnnoSpec,
    id: &'a str,
    obj: &'a Map<String, Value>,
}

/// Splits annotations into those the app maps and the rest (kept verbatim,
/// with their original index).
fn split_annotations<'a>(levels: &[Level], annotations: &'a [Value]) -> (Vec<Mapped<'a>>, Vec<(usize, Value)>) {
    let walls: HashSet<&str> = levels.iter().flat_map(|l| l.walls.iter().map(|w| w.id.as_str())).collect();
    let (mut mapped, mut parked) = (Vec::new(), Vec::new());
    for (index, a) in annotations.iter().enumerate() {
        match a.as_object().and_then(|o| mapped_of(levels, &walls, index, o)) {
            Some(m) => mapped.push(m),
            None => parked.push((index, a.clone())),
        }
    }
    (mapped, parked)
}

/// Whether the wall a dimension or symbol names exists (no wall named is fine for a symbol).
fn wall_exists(kind: &str, obj: &Map<String, Value>, walls: &HashSet<&str>) -> bool {
    let named = match kind {
        "dimension" => obj.get("target").and_then(|t| t.get("wall")),
        "symbol" => obj.get("host").and_then(|h| h.get("wall")),
        _ => None,
    };
    named.is_none_or(|w| w.as_str().is_some_and(|w| walls.contains(w)))
}

/// Whether mapped annotations come out in the order [`to_sketchy`] writes by default
/// (by page, then text, symbol, dimension).
fn in_default_order(mapped: &[Mapped]) -> bool {
    let key = |m: &Mapped| (m.page, ANNO_SPECS.iter().position(|s| s.ch == m.spec.ch));
    mapped.windows(2).all(|w| key(&w[0]) <= key(&w[1]))
}

fn mapped_of<'a>(levels: &[Level], walls: &HashSet<&str>, index: usize, obj: &'a Map<String, Value>) -> Option<Mapped<'a>> {
    let kind = str_of(obj, "kind")?;
    let spec = ANNO_SPECS.iter().find(|s| s.kind == kind)?;
    let id = str_of(obj, "id")?;
    let level = str_of(obj, "level")?;
    let page = levels.iter().position(|l| l.id == level)?;
    ((spec.ok)(obj) && wall_exists(kind, obj, walls)).then_some(Mapped { index, page, spec, id, obj })
}

// ------------------------------------------------------ layer field groups

fn visible_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("visible", obj.get("visible").filter(|v| v.is_boolean()).cloned())
}

fn visible_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    put(obj, "visible", Some(core.get("visible").cloned().unwrap_or(Value::Bool(true))));
}

fn locked_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("locked", obj.get("locked").filter(|v| v.is_boolean()).cloned())
}

fn locked_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    put(obj, "locked", Some(core.get("locked").cloned().unwrap_or(Value::Bool(false))));
}

const LAYER_GROUPS: &[Group] = &[
    Group { core_keys: &["name"], fwd: name_fwd, back: name_back },
    Group { core_keys: &["visible"], fwd: visible_fwd, back: visible_back },
    Group { core_keys: &["locked"], fwd: locked_fwd, back: locked_back },
];
const LAYER_OWNED: &[&str] = &["id", "name", "visible", "locked"];

// ------------------------------------------------- area-code field groups

/// `use`/`finished` of an area code: `includeInGLA` is living area, otherwise
/// the `group` decides.
fn derive_use(obj: &Map<String, Value>) -> Map<String, Value> {
    let group = str_of(obj, "group").map(|g| g.trim().to_uppercase());
    let code = str_of(obj, "code").map(|c| c.trim().to_uppercase());
    let by_code = match code.as_deref() {
        Some("PORCH" | "OFP") => Some(("porch", "covered")),
        Some("SFP") => Some(("porch", "screened")),
        Some("EFP") => Some(("porch", "enclosed")),
        Some("PAT") => Some(("patio", "open")),
        Some("DK" | "DCK") => Some(("deck", "open")),
        Some("BAL") => Some(("balcony", "open")),
        _ => None,
    };
    let mut m = Map::new();
    if obj.get("includeInGLA").and_then(Value::as_bool) == Some(true) {
        m.insert("use".into(), "living".into());
        m.insert("finished".into(), Value::Bool(true));
    } else if let Some((usage, enclosure)) = by_code {
        m.insert("use".into(), usage.into());
        m.insert("enclosure".into(), enclosure.into());
    } else {
        let (usage, finished) = match group.as_deref() {
            Some("GAR") => ("garage", None),
            Some("CARPORT" | "CP") => ("carport", None),
            Some("POR" | "PORCH") => ("porch", None),
            Some("DECK" | "DCK") => ("deck", None),
            Some("PATIO") => ("patio", None),
            Some("BAL") => ("balcony", None),
            Some("BSMT" | "BAS") => ("storage", Some(false)),
            _ => ("other", None),
        };
        m.insert("use".into(), usage.into());
        if let Some(f) = finished {
            m.insert("finished".into(), Value::Bool(f));
        }
    }
    m
}

fn code_use_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    put(obj, "includeInGLA", core.get("use").and_then(Value::as_str).map(|u| Value::Bool(u == "living")));
}

fn code_label_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("label", str_of(obj, "name").map(Value::from))
}

fn code_label_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    put(obj, "name", core.get("label").cloned());
}

fn multiplier_fwd(_: &Ctx, obj: &Map<String, Value>) -> Map<String, Value> {
    single("area_factor", f64_of(obj, "multiplier").filter(|f| *f != 1.0).map(num))
}

fn multiplier_back(_: &Ctx, obj: &mut Map<String, Value>, core: &Map<String, Value>) {
    match core.get("area_factor") {
        Some(f) => put(obj, "multiplier", Some(f.clone())),
        None if obj.contains_key("multiplier") => put(obj, "multiplier", Some(num(1.0))),
        None => {}
    }
}

const CODE_GROUPS: &[Group] = &[
    Group { core_keys: &["label"], fwd: code_label_fwd, back: code_label_back },
    Group { core_keys: &["area_factor"], fwd: multiplier_fwd, back: multiplier_back },
    Group { core_keys: &["use", "finished", "enclosure"], fwd: |_, o| derive_use(o), back: code_use_back },
];
const CODE_OWNED: &[&str] = &["code", "label", "use", "finished", "enclosure", "area_factor"];
/// Default name of the area-code scheme.
const CODE_SCHEME_NAME: &str = "Sketch Rust area codes";

// ----------------------------------------------- generic view conversion

/// A Sketchy object from an app object: `core` (identity fields) plus the
/// groups' projection, with the app object kept as residual.
fn view_of(ctx: &Ctx, obj: &Map<String, Value>, mut core: Map<String, Value>, groups: &[Group], exact: &[&str]) -> Value {
    core.extend(project(ctx, obj, groups));
    let (residual, other) = take_residual(obj, exact);
    finish_element(core, residual, other)
}

/// An app object from a Sketchy object (without its ids): the residual with the
/// core fields layered on top, and unowned Sketchy fields parked.
fn back_object(ctx: &Ctx, view: &Map<String, Value>, groups: &[Group], owned: &[&str]) -> Map<String, Value> {
    let mut obj = residual_of(view);
    apply_groups(ctx, &mut obj, view, groups);
    let mut rest = sketchy_rest(view, owned);
    park_leftovers(ctx, &obj, view, groups, &mut rest);
    park(&mut obj, rest);
    obj
}

/// Writes the layer the element is assigned to (core wins over a stale `layerId`).
fn apply_layer(ctx: &Ctx, obj: &mut Map<String, Value>, sid: &str) {
    if !ctx.layers_mapped {
        return;
    }
    match ctx.element_layer.get(sid) {
        Some(layer) => {
            obj.insert("layerId".into(), layer.clone().into());
        }
        None => {
            if str_of(obj, "layerId").is_some_and(|l| ctx.layer_sid.contains_key(l)) {
                obj.remove("layerId");
            }
        }
    }
}

fn annotation_from_object(ctx: &Ctx, spec: &AnnoSpec, level_id: &str, v: &Value) -> Result<Value, SketchyError> {
    let obj = as_object(v, "annotation")?;
    let mut core = Map::new();
    core.insert("id".into(), Ctx::element_id(spec.prefix, obj)?.into());
    core.insert("kind".into(), spec.kind.into());
    core.insert("level".into(), level_id.into());
    Ok(view_of(ctx, obj, core, spec.groups, &["id", ORDER]))
}

fn annotation_to_object(ctx: &Ctx, m: &Mapped) -> Value {
    let app_id = ctx.app_id(m.spec.ch, m.spec.prefix, m.id);
    let mut obj = back_object(ctx, m.obj, m.spec.groups, m.spec.owned);
    set_ids(&mut obj, m.spec.prefix, &app_id, m.id);
    apply_layer(ctx, &mut obj, m.id);
    if ctx.stamp_order {
        obj.insert(ORDER.into(), m.index.into());
    }
    Value::Object(obj)
}

/// An app layer is mapped only when every layer has a string id.
fn app_layers(root: &Map<String, Value>) -> Option<&[Value]> {
    let layers = root.get("layers")?.as_array()?;
    layers.iter().all(|l| l.get("id").is_some_and(Value::is_string)).then_some(layers.as_slice())
}

/// The area-code table is mapped only when every row has a string `code`.
fn app_codes(root: &Map<String, Value>) -> Option<&[Value]> {
    let codes = root.get("areaCodes")?.as_array()?;
    codes.iter().all(|c| c.get("code").is_some_and(Value::is_string)).then_some(codes.as_slice())
}

fn layer_to_view(ctx: &Ctx, v: &Value) -> Result<Value, SketchyError> {
    let obj = as_object(v, "layer")?;
    let mut core = Map::new();
    core.insert("id".into(), Ctx::element_id("layer:", obj)?.into());
    Ok(view_of(ctx, obj, core, LAYER_GROUPS, &["id"]))
}

fn code_to_view(ctx: &Ctx, v: &Value) -> Result<Value, SketchyError> {
    let obj = as_object(v, "area code")?;
    let core = single("code", obj.get("code").cloned());
    Ok(view_of(ctx, obj, core, CODE_GROUPS, &[]))
}

fn code_from_view(ctx: &Ctx, view: &Map<String, Value>) -> Value {
    let mut obj = back_object(ctx, view, CODE_GROUPS, CODE_OWNED);
    put(&mut obj, "code", view.get("code").cloned());
    Value::Object(obj)
}

/// Sketchy layers the app maps (every layer has a string id), as objects.
fn sketchy_layers(pres: Option<&Map<String, Value>>) -> Option<Vec<&Map<String, Value>>> {
    let layers = pres?.get("layers")?.as_array()?;
    layers.iter().map(|l| l.as_object().filter(|o| o.get("id").is_some_and(Value::is_string))).collect()
}

/// The area-code scheme the app maps: its position and its rows.
fn sketchy_code_scheme(doc: &Document) -> Option<usize> {
    doc.classification_schemes.iter().position(|s| {
        s.scheme == code_scheme() && s.codes.iter().all(|c| c.get("code").is_some_and(Value::is_string))
    })
}

fn layer_sid_of(obj: &Map<String, Value>) -> String {
    str_of(obj, SKETCHY_ID).map_or_else(|| format!("layer:{}", str_of(obj, "id").unwrap_or_default()), str::to_string)
}

impl Ctx {
    /// To direction: area-code defaults, wall ends and layer ids of the save model.
    fn prepare_to(&mut self, root: &Map<String, Value>) {
        for code in app_codes(root).into_iter().flatten().filter_map(Value::as_object) {
            if let Some(c) = str_of(code, "code") {
                self.code_defaults.insert(c.to_string(), derive_use(code));
            }
        }
        if let Some(layers) = app_layers(root) {
            self.layers_mapped = true;
            for obj in layers.iter().filter_map(Value::as_object) {
                if let Some(id) = str_of(obj, "id") {
                    let sid = Ctx::element_id("layer:", obj).unwrap_or_else(|_| layer_sid_of(obj));
                    self.layer_sid.insert(id.to_string(), sid);
                }
            }
        }
        for line in root.get("pages").and_then(Value::as_array).into_iter().flatten().flat_map(|p| array_of_lenient(p, "lines")) {
            let ends = ["start", "end"].map(|k| line.get(k).and_then(|p| self.point_to_sketchy(p).ok()));
            if let (Some(id), [Some(a), Some(b)]) = (line.get("id"), ends) {
                self.wall_pts.insert(id.to_string(), json!([[num(a[0]), num(a[1])], [num(b[0]), num(b[1])]]));
            }
        }
    }

    /// From direction: area-code defaults, wall ends, layer ids and assignments.
    fn prepare_from(&mut self, doc: &Document, levels: &[Level]) {
        if let Some(i) = sketchy_code_scheme(doc) {
            for row in doc.classification_schemes[i].codes.iter().filter_map(Value::as_object) {
                let defaults: Map<String, Value> =
                    ["use", "finished", "enclosure"].iter().filter_map(|k| row.get(*k).map(|v| (k.to_string(), v.clone()))).collect();
                if let Some(c) = str_of(row, "code") {
                    self.code_defaults.insert(c.to_string(), defaults);
                }
            }
        }
        for w in levels.iter().flat_map(|l| &l.walls) {
            if let Some(app) = self.to_app.get(&('w', w.id.clone())) {
                let pts = json!([[num(w.start[0]), num(w.start[1])], [num(w.end[0]), num(w.end[1])]]);
                self.wall_pts.insert(app.to_string(), pts);
            }
        }
        let pres = doc.presentation.as_ref().map(|p| &p.other);
        let Some(layers) = sketchy_layers(pres) else { return };
        self.layers_mapped = true;
        let mut used: HashSet<String> = HashSet::new();
        for layer in layers {
            let sid = str_of(layer, "id").unwrap_or_default().to_string();
            let stored = ns_of(layer).filter(|r| str_of(r, SKETCHY_ID) == Some(&sid)).and_then(|r| str_of(r, "id"));
            let mut app = stored.map_or_else(|| sid.strip_prefix("layer:").unwrap_or(&sid).to_string(), str::to_string);
            while !used.insert(app.clone()) {
                app.push('_');
            }
            self.layer_sid.insert(app.clone(), sid.clone());
            self.layer_app.insert(sid, app);
        }
        let assigned = pres.and_then(|p| p.get("element_layers")).and_then(Value::as_object);
        for (el, layer) in assigned.into_iter().flatten().filter_map(|(k, v)| Some((k, v.as_str()?))) {
            let app = self.layer_app.get(layer).cloned().unwrap_or_else(|| layer.strip_prefix("layer:").unwrap_or(layer).to_string());
            self.element_layer.insert(el.clone(), app);
        }
    }
}

/// `element_layers` of the save model: each element whose layer is mapped.
fn element_layers_of(ctx: &Ctx, pages: &[Value]) -> Map<String, Value> {
    let kinds = [("lines", "w-"), ("polygons", "a-")]
        .into_iter()
        .chain(ANNO_SPECS.iter().map(|s| (s.key, s.prefix)))
        .collect::<Vec<_>>();
    let mut out = Map::new();
    for page in pages {
        for (key, prefix) in &kinds {
            let hosts = page.as_object().map(referenced_lines).unwrap_or_default();
            for obj in array_of_lenient(page, key).iter().filter_map(Value::as_object) {
                if *key == "lines" && is_unused_display_line(&Value::Object(obj.clone()), &hosts) {
                    continue;
                }
                let layer = str_of(obj, "layerId").and_then(|l| ctx.layer_sid.get(l));
                if let (Some(layer), Ok(id)) = (layer, Ctx::element_id(prefix, obj)) {
                    out.insert(id, layer.clone().into());
                }
            }
        }
    }
    out
}

// ------------------------------------------------------- elements

/// Pulls the residual and Sketchy-side unknowns out of a save-model object.
/// `id` stays in the residual when a `sketchy_id` is stored, so the app id is stable.
fn take_residual(obj: &Map<String, Value>, exact: &[&str]) -> (Map<String, Value>, Option<Map<String, Value>>) {
    let (_, mut residual) = split_object(obj, exact);
    if obj.contains_key(SKETCHY_ID) {
        if let Some(id) = obj.get("id") {
            residual.insert("id".into(), id.clone());
        }
    }
    let other = match residual.remove(SKETCHY_OTHER) {
        Some(Value::Object(m)) => Some(m),
        _ => None,
    };
    (residual, other)
}

/// Adds `extras[NS] = residual` and restores Sketchy-side unknowns onto `core`.
fn finish_element(mut core: Map<String, Value>, residual: Map<String, Value>, other: Option<Map<String, Value>>) -> Value {
    let mut other = other.unwrap_or_default();
    let parked: Vec<String> = other.get(LOSSY).and_then(Value::as_object).map_or_else(Vec::new, |l| l.keys().cloned().collect());
    restore_lossy(&core, &mut other);
    // A parked leftover is a whole core value: it replaces (never merges into) the projection.
    for key in parked {
        if let Some(v) = other.remove(&key) {
            core.insert(key, v);
        }
    }
    restore_classifications(&mut core, &mut other);
    let mut extras = Map::new();
    extras.insert(NS.into(), Value::Object(residual));
    core.insert("extras".into(), Value::Object(extras));
    Value::Object(merge_objects(core, other))
}

/// The Sketchy fields of `view` this converter does not own, if any.
fn sketchy_rest(view: &Map<String, Value>, owned: &[&str]) -> Map<String, Value> {
    let (_, mut rest) = split_object(view, owned);
    strip_ns_extras(&mut rest);
    if let Some(Value::Array(cs)) = rest.get_mut("classifications") {
        let at = cs.iter().position(is_code_class);
        cs.retain(|c| !is_code_class(c));
        if cs.is_empty() {
            rest.remove("classifications");
        } else if let Some(at) = at {
            rest.insert(CLASS_INDEX.into(), at.into());
        }
    }
    rest
}

/// Puts `ours` into `theirs` at `at` (clamped; `None` appends).
fn splice_at(mut theirs: Vec<Value>, at: Option<&Value>, ours: Vec<Value>) -> Vec<Value> {
    let at = at.and_then(Value::as_u64).map_or(theirs.len(), |i| usize::try_from(i).unwrap_or(usize::MAX)).min(theirs.len());
    theirs.splice(at..at, ours);
    theirs
}

/// Foreign classifications parked on an element, with the area-code one put back where it was.
fn restore_classifications(core: &mut Map<String, Value>, other: &mut Map<String, Value>) {
    let at = other.remove(CLASS_INDEX);
    let Some(Value::Array(foreign)) = other.remove("classifications") else { return };
    let ours = match core.remove("classifications") {
        Some(Value::Array(ours)) => ours,
        _ => Vec::new(),
    };
    core.insert("classifications".into(), Value::Array(splice_at(foreign, at.as_ref(), ours)));
}

fn strip_ns_extras(rest: &mut Map<String, Value>) {
    if let Some(Value::Object(extras)) = rest.get_mut("extras") {
        extras.remove(NS);
        if extras.is_empty() {
            rest.remove("extras");
        }
    }
}

fn residual_of(view: &Map<String, Value>) -> Map<String, Value> {
    view.get("extras")
        .and_then(|e| e.get(NS))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Writes `rest` into `obj` as `sketchy_other` when non-empty.
fn park(obj: &mut Map<String, Value>, rest: Map<String, Value>) {
    if !rest.is_empty() {
        obj.insert(SKETCHY_OTHER.into(), Value::Object(rest));
    }
}

const WALL_OWNED: &[&str] = &["id", "start", "end", "bulge", "thickness", "kind"];
const SPACE_OWNED: &[&str] =
    &["id", "shape", "name", "area_factor", "measured_to", "within", "subtract", "attributes", "bulges", "use", "finished", "enclosure"];

fn wall_from_line(ctx: &Ctx, line: &Value) -> Result<Wall, SketchyError> {
    let obj = as_object(line, "line")?;
    let mut core = project(ctx, obj, WALL_GROUPS);
    core.insert("id".into(), Ctx::element_id("w-", obj)?.into());
    for key in ["start", "end"] {
        let p = ctx.point_to_sketchy(obj.get(key).ok_or_else(|| err(format!("line missing {key}")))?)?;
        core.insert(key.into(), json!([num(p[0]), num(p[1])]));
    }
    let (residual, other) = take_residual(obj, &["id", "start", "end"]);
    serde_json::from_value(finish_element(core, residual, other)).map_err(|e| err(e.to_string()))
}

fn line_from_wall(ctx: &Ctx, wall: &Wall) -> Result<Value, SketchyError> {
    let view = to_map(wall)?;
    let mut obj = residual_of(&view);
    let app_id = ctx.app_id('w', "w-", &wall.id);
    set_ids(&mut obj, "w-", &app_id, &wall.id);
    obj.insert("start".into(), ctx.point_to_app(wall.start));
    obj.insert("end".into(), ctx.point_to_app(wall.end));
    apply_groups(ctx, &mut obj, &view, WALL_GROUPS);
    apply_layer(ctx, &mut obj, &wall.id);
    let mut rest = sketchy_rest(&view, WALL_OWNED);
    park_leftovers(ctx, &obj, &view, WALL_GROUPS, &mut rest);
    park(&mut obj, rest);
    Ok(Value::Object(obj))
}

/// Sets the app `id`, and `sketchy_id` when the Sketchy id is not `prefix<id>`.
fn set_ids(obj: &mut Map<String, Value>, prefix: &str, app_id: &Value, sketchy_id: &str) {
    obj.insert("id".into(), app_id.clone());
    obj.remove(SKETCHY_ID);
    let plain = app_id.as_str().map_or_else(|| app_id.to_string(), str::to_string);
    if format!("{prefix}{plain}") != sketchy_id {
        obj.insert(SKETCHY_ID.into(), sketchy_id.into());
    }
}

/// An area outline in Sketchy coordinates.
struct Outline {
    points: Vec<[f64; 2]>,
    bulges: Option<Vec<f64>>,
    /// Circle/ellipse: the original shape, parked for write-back.
    original: Option<Value>,
}

fn ellipse_points(center: [f64; 2], rx: f64, ry: f64, rotation_deg: f64) -> Vec<[f64; 2]> {
    let (sin_r, cos_r) = rotation_deg.to_radians().sin_cos();
    (0..CIRCLE_SEGMENTS)
        .map(|i| {
            let (s, c) = (i as f64 / CIRCLE_SEGMENTS as f64 * 2.0 * PI).sin_cos();
            let (x, y) = (rx * c, ry * s);
            [center[0] + x * cos_r - y * sin_r, center[1] + x * sin_r + y * cos_r]
        })
        .collect()
}

fn outline_of(shape: &Shape, shape_value: &Value) -> Option<Outline> {
    match shape {
        Shape::Polygon { points, bulges, .. } => {
            Some(Outline { points: points.clone(), bulges: bulges.clone(), original: None })
        }
        Shape::Circle { center, radius, .. } => Some(Outline {
            points: ellipse_points(*center, *radius, *radius, 0.0),
            bulges: None,
            original: Some(shape_value.clone()),
        }),
        Shape::Ellipse { center, radius_x, radius_y, rotation, .. } => Some(Outline {
            points: ellipse_points(*center, *radius_x, *radius_y, rotation.unwrap_or(0.0)),
            bulges: None,
            original: Some(shape_value.clone()),
        }),
        _ => None,
    }
}

fn space_from_polygon(ctx: &Ctx, poly: &Value) -> Result<Space, SketchyError> {
    let obj = as_object(poly, "polygon")?;
    let mut core = project(ctx, obj, POLYGON_GROUPS);
    let points = obj
        .get("points")
        .and_then(Value::as_array)
        .ok_or_else(|| err("polygon missing points"))?
        .iter()
        .map(|p| ctx.point_to_sketchy(p).map(|p| json!([num(p[0]), num(p[1])])))
        .collect::<Result<Vec<_>, _>>()?;
    let (residual, mut other) = take_residual(obj, &["id"]);
    let original = other.as_mut().and_then(|o| o.remove(SHAPE_ORIG));
    let mut shape = Map::new();
    if let Some(Value::Object(orig)) = original.as_ref().and_then(|o| o.get("shape")).filter(|_| {
        original.as_ref().and_then(|o| o.get("points")).is_some_and(|p| approx_eq(Some(p), obj.get("points")))
    }) {
        // Unchanged approximation: write the original circle/ellipse back.
        shape = orig.clone();
        core.remove("bulges");
    } else {
        shape.insert("type".into(), "polygon".into());
        shape.insert("points".into(), Value::Array(points));
        if let Some(b) = core.remove("bulges") {
            shape.insert("bulges".into(), b);
        }
        if let Some(Value::Object(rest)) = other.as_mut().and_then(|o| o.remove(SHAPE_REST)) {
            shape = merge_objects(rest, shape);
        }
    }
    core.insert("id".into(), Ctx::element_id("a-", obj)?.into());
    core.insert("shape".into(), Value::Object(shape));
    serde_json::from_value(finish_element(core, residual, other)).map_err(|e| err(e.to_string()))
}

/// Points the polygon's edges at its display walls.
fn link_display_edges(obj: &mut Map<String, Value>, count: usize, links: &[(usize, i64)]) {
    let mut edges: Vec<Value> = obj.get("edges").and_then(Value::as_array).cloned().unwrap_or_default();
    while edges.len() < count {
        edges.push(json!({"lineId": null, "bowAngle": num(0.0)}));
    }
    for (i, id) in links {
        if let Some(edge) = edges.get_mut(*i).and_then(Value::as_object_mut) {
            edge.insert("lineId".into(), Value::from(*id));
        }
    }
    obj.insert("edges".into(), Value::Array(edges));
    obj.insert(DISPLAY.into(), Value::Bool(true));
}

fn polygon_from_space(ctx: &Ctx, level: usize, space: &Space) -> Result<Value, SketchyError> {
    let mut view = to_map(space)?;
    let shape_value = view.remove("shape").unwrap_or(Value::Null);
    let outline = outline_of(&space.shape, &shape_value)
        .ok_or_else(|| err(format!("space {} has no polygon outline", space.id)))?;
    if let Some(b) = &outline.bulges {
        view.insert("bulges".into(), Value::Array(b.iter().map(|b| num(*b)).collect()));
    }
    let mut obj = residual_of(&view);
    let old_points = obj.get("points").cloned();
    let points = Value::Array(outline.points.iter().map(|p| ctx.point_to_app(*p)).collect());
    if old_points.as_ref().is_some_and(|old| !approx_eq(Some(old), Some(&points))) {
        // Geometry changed: derived data and edge links are stale.
        obj.remove("area");
        obj.remove("gross");
        obj.insert("edges".into(), json!([]));
    }
    let app_id = ctx.app_id('a', "a-", &space.id);
    set_ids(&mut obj, "a-", &app_id, &space.id);
    obj.insert("points".into(), points.clone());
    apply_groups(ctx, &mut obj, &view, POLYGON_GROUPS);
    if let Some(links) = ctx.display_edges.get(&(level, space.id.clone())) {
        link_display_edges(&mut obj, outline.points.len(), links);
    }
    apply_layer(ctx, &mut obj, &space.id);
    let mut rest = sketchy_rest(&view, SPACE_OWNED);
    park_leftovers(ctx, &obj, &view, POLYGON_GROUPS, &mut rest);
    match outline.original {
        Some(shape) => {
            rest.insert(SHAPE_ORIG.into(), json!({"shape": shape, "points": points}));
        }
        None => {
            let shape = shape_value.as_object().cloned().unwrap_or_default();
            let (_, shape_rest) = split_object(&shape, &["type", "points", "bulges"]);
            if !shape_rest.is_empty() {
                rest.insert(SHAPE_REST.into(), Value::Object(shape_rest));
            }
        }
    }
    park(&mut obj, rest);
    Ok(Value::Object(obj))
}

// --------------------------------------------------- pages and root

fn array_of<'a>(page: &'a Map<String, Value>, key: &str) -> Result<&'a [Value], SketchyError> {
    match page.get(key) {
        None => Ok(&[]),
        Some(Value::Array(a)) => Ok(a),
        Some(_) => Err(err(format!("{key} must be an array"))),
    }
}

fn level_defaults(i: usize) -> Map<String, Value> {
    let v = json!({"id": format!("page-{}", i + 1), "name": format!("Page {}", i + 1),
                   "grade": "above", "sequence": i + 1});
    v.as_object().cloned().unwrap_or_default()
}

/// Id of the Sketchy level a page becomes (a foreign id is parked in `sketchy_other`).
fn level_id_of_page(i: usize, obj: &Map<String, Value>) -> String {
    let parked = obj.get(SKETCHY_OTHER).and_then(|o| o.get("id")).and_then(Value::as_str);
    parked.map_or_else(|| format!("page-{}", i + 1), str::to_string)
}

/// An annotation with its recorded drawing index, if any.
type Indexed = (Option<u64>, Value);

/// A page's level and the annotations (labels, symbols, chains) it holds, each with
/// its recorded drawing index.
fn level_from_page(ctx: &Ctx, i: usize, page: &Value) -> Result<(Level, Vec<Indexed>), SketchyError> {
    let obj = as_object(page, "page")?;
    let level_id = level_id_of_page(i, obj);
    let mut annotations = Vec::new();
    for spec in ANNO_SPECS {
        for item in array_of(obj, spec.key)? {
            let index = item.get(ORDER).and_then(Value::as_u64);
            annotations.push((index, annotation_from_object(ctx, spec, &level_id, item)?));
        }
    }
    let hosts = referenced_lines(obj);
    let walls = array_of(obj, "lines")?
        .iter()
        .filter(|l| !is_unused_display_line(l, &hosts))
        .map(|l| wall_from_line(ctx, l))
        .collect::<Result<Vec<_>, _>>()?;
    let spaces = array_of(obj, "polygons")?.iter().map(|p| space_from_polygon(ctx, p)).collect::<Result<Vec<_>, _>>()?;
    // Mapped arrays stay in the residual as empty placeholders so absence round-trips.
    let mut residual = obj.clone();
    for key in ["lines", "polygons"].into_iter().chain(ANNO_SPECS.iter().map(|s| s.key)) {
        if residual.contains_key(key) {
            residual.insert(key.into(), json!([]));
        }
    }
    let mut other = match residual.remove(SKETCHY_OTHER) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    };
    let mut spaces: Vec<Value> = spaces.iter().map(|s| to_map(s).map(Value::Object)).collect::<Result<_, _>>()?;
    if let Some(Value::Array(parked)) = other.remove(PARKED_SPACES) {
        for entry in parked {
            if let Some(space) = entry.get("space") {
                spaces = splice_at(spaces, entry.get("index"), vec![space.clone()]);
            }
        }
    }
    let mut level = level_defaults(i);
    level.insert("walls".into(), json!(walls));
    level.insert("spaces".into(), Value::Array(spaces));
    level.insert("extras".into(), json!({ NS: residual }));
    let level = serde_json::from_value(Value::Object(merge_objects(level, other))).map_err(|e| err(e.to_string()))?;
    Ok((level, annotations))
}

/// Ids of the lines a page's dimension chains and symbols are attached to.
fn referenced_lines(page: &Map<String, Value>) -> HashSet<String> {
    let chains = array_of_lenient_obj(page, "dimensionChains").filter_map(|c| c.get("seedLineId"));
    let symbols = array_of_lenient_obj(page, "symbols").filter_map(|y| y.get("wallId"));
    chains.chain(symbols).map(Value::to_string).collect()
}

fn array_of_lenient_obj<'a>(page: &'a Map<String, Value>, key: &str) -> impl Iterator<Item = &'a Value> {
    page.get(key).and_then(Value::as_array).into_iter().flatten()
}

/// A display wall nothing is attached to: the space's shape already carries it, so it
/// is not written as a Sketchy wall. One that a chain or symbol uses stays a wall.
fn is_unused_display_line(line: &Value, hosts: &HashSet<String>) -> bool {
    line.get(DISPLAY) == Some(&Value::Bool(true)) && !line.get("id").is_some_and(|id| hosts.contains(&id.to_string()))
}

fn page_from_level(ctx: &Ctx, i: usize, level: &Level, annos: &[Mapped]) -> Result<Value, SketchyError> {
    let view = to_map(level)?;
    let mut page = residual_of(&view);
    let mut lines = level.walls.iter().map(|w| line_from_wall(ctx, w)).collect::<Result<Vec<_>, _>>()?;
    lines.extend(ctx.display_lines.get(i).into_iter().flatten().cloned());
    let mut polygons = Vec::new();
    let mut verbatim = Vec::new();
    for (index, space) in level.spaces.iter().enumerate() {
        if is_area(&space.shape) {
            polygons.push(polygon_from_space(ctx, i, space)?);
        } else {
            // polyline/arc/unknown: kept as-is, with its place among the level's spaces
            verbatim.push(json!({"index": index, "space": to_map(space)?}));
        }
    }
    for (key, items) in [("lines", lines), ("polygons", polygons)] {
        if !items.is_empty() || page.contains_key(key) {
            page.insert(key.into(), Value::Array(items));
        }
    }
    for spec in ANNO_SPECS {
        let items: Vec<Value> =
            annos.iter().filter(|m| m.page == i && m.spec.ch == spec.ch).map(|m| annotation_to_object(ctx, m)).collect();
        if !items.is_empty() || page.contains_key(spec.key) {
            page.insert(spec.key.into(), Value::Array(items));
        }
    }
    let (_, mut rest) = split_object(&view, &["walls", "spaces"]);
    strip_ns_extras(&mut rest);
    for (k, v) in level_defaults(i) {
        if rest.get(&k) == Some(&v) {
            rest.remove(&k);
        }
    }
    if !verbatim.is_empty() {
        rest.insert(PARKED_SPACES.into(), Value::Array(verbatim));
    }
    park(&mut page, rest);
    Ok(Value::Object(page))
}

/// Root fields the converter regenerates or parks under its own keys; everything else
/// (including `$schema`) is parked as-is. `revision`, `history` and `generator` are not
/// written back: the store stamps a new revision whose parent is [`parent_revision`].
const ROOT_OWNED: &[&str] = &[
    "format", "format_version", "id", "title", "length_unit", "structures", "extensions_used",
    "extensions_required", "revision", "history", "generator",
];

/// Converts a save model (the app's JSON) to a Sketchy document.
///
/// The document root, pages, lines, polygons, labels, symbols, dimension chains,
/// layers and the area-code table are mapped to Sketchy core fields; everything
/// else is kept in `extras[NS]` so [`from_sketchy`] restores it exactly.
///
/// The document `id` is `document_id` when given, else the id the model was opened
/// with (parked by [`from_sketchy`]); with neither this fails with
/// `Convert("document id required")`. `revision`, `history` and `generator` are not
/// written: the store stamps a new revision (see [`parent_revision`]).
pub fn to_sketchy(model: &Value, document_id: Option<&str>) -> Result<Document, SketchyError> {
    finite_guard(|| to_sketchy_inner(model, document_id))
}

/// The `revision.id` of the Sketchy document a save model was opened from, if any:
/// the parent of the next revision a store writes.
#[must_use]
pub fn parent_revision(model: &Value) -> Option<String> {
    model.get(SKETCHY_OTHER)?.get(PARENT_REVISION)?.as_str().map(str::to_string)
}

fn to_sketchy_inner(model: &Value, document_id: Option<&str>) -> Result<Document, SketchyError> {
    let root = as_object(model, "save model")?;
    let file_name = str_of(root, "fileName");
    let (_, mut residual) = split_object(root, &["pages"]);
    if file_name.is_some() {
        residual.remove("fileName");
    }
    let mut other = match residual.remove(SKETCHY_OTHER) {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    };
    let parked_id = other.remove(DOCUMENT_ID);
    other.remove(PARENT_REVISION);
    let document_id = document_id
        .map(str::to_string)
        .or_else(|| parked_id.as_ref().and_then(Value::as_str).map(str::to_string))
        .ok_or_else(|| err("document id required"))?;
    let unit = other.get("length_unit").and_then(Value::as_str).unwrap_or("ft").to_string();
    let mut ctx = Ctx::new(unit_scale(&unit)?);
    let pages = root.get("pages").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    for (key, ids) in [("polygons", &mut ctx.to_sk), ("lines", &mut ctx.wall_sk)] {
        for item in pages.iter().flat_map(|p| array_of_lenient(p, key)) {
            if let (Some(id), Some(Value::String(sid))) = (item.get("id"), item.get(SKETCHY_ID)) {
                ids.insert(id.to_string(), sid.clone());
            }
        }
    }
    ctx.prepare_to(root);
    let (layers, codes) = (app_layers(root), app_codes(root));
    for (key, mapped) in [("layers", layers.is_some()), ("areaCodes", codes.is_some())] {
        if mapped {
            residual.remove(key);
        }
    }
    let converted = array_of(root, "pages")?
        .iter()
        .enumerate()
        .map(|(i, p)| level_from_page(&ctx, i, p))
        .collect::<Result<Vec<_>, _>>()?;
    let (levels, annotations): (Vec<Level>, Vec<Vec<Indexed>>) = converted.into_iter().unzip();
    let mut structure = json!({"id": "building", "kind": "other", "levels": levels}).as_object().cloned().unwrap_or_default();
    if let Some(n) = file_name {
        structure.insert("name".into(), n.into());
    }
    if let Some(Value::Object(s0)) = other.remove("structure0") {
        structure = merge_objects(structure, s0);
    }
    let mut doc = json!({
        "format": FORMAT, "format_version": FORMAT_VERSION, "id": document_id,
        "length_unit": "ft", "structures": [structure],
        "extensions_used": extensions_of(other.remove("extensions_used")), "extras": { NS: residual },
    })
    .as_object()
    .cloned()
    .unwrap_or_default();
    if let Some(n) = file_name {
        doc.insert("title".into(), n.into());
    }
    let address = root.get("subjectInfo").and_then(|s| s.get("propertyAddress")).and_then(Value::as_str);
    if let Some(a) = address {
        doc.insert("property".into(), json!({"address": a}));
    }
    let mut presentation = Map::new();
    let mut ordered: Vec<Indexed> = annotations.into_iter().flatten().collect();
    let parked = match other.remove(PARKED_ANNOS) {
        Some(Value::Array(entries)) => entries,
        _ => Vec::new(),
    };
    for entry in parked {
        if let Some(a) = entry.get("annotation") {
            ordered.push((entry.get("index").and_then(Value::as_u64), a.clone()));
        }
    }
    ordered.sort_by_key(|(i, _)| i.unwrap_or(u64::MAX)); // stable: unindexed keep page order
    let annotations: Vec<Value> = ordered.into_iter().map(|(_, a)| a).collect();
    if !annotations.is_empty() {
        presentation.insert("annotations".into(), Value::Array(annotations));
    }
    if let Some(layers) = layers {
        let views = layers.iter().map(|l| layer_to_view(&ctx, l)).collect::<Result<Vec<_>, _>>()?;
        presentation.insert("layers".into(), Value::Array(views));
        let assigned = element_layers_of(&ctx, pages);
        if !assigned.is_empty() {
            presentation.insert("element_layers".into(), Value::Object(assigned));
        }
    }
    if !presentation.is_empty() {
        doc.insert("presentation".into(), Value::Object(presentation));
    }
    if let Some(codes) = codes {
        let rows = codes.iter().map(|c| code_to_view(&ctx, c)).collect::<Result<Vec<_>, _>>()?;
        let mut scheme = json!({"scheme": code_scheme(), "name": CODE_SCHEME_NAME, "codes": rows})
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(Value::Object(s0)) = other.remove("scheme0") {
            scheme = merge_objects(scheme, s0);
        }
        let at = other.remove(SCHEME_INDEX);
        let others = match other.remove("classification_schemes") {
            Some(Value::Array(o)) => o,
            _ => Vec::new(),
        };
        let at = at.unwrap_or_else(|| json!(0));
        doc.insert("classification_schemes".into(), Value::Array(splice_at(others, Some(&at), vec![Value::Object(scheme)])));
    }
    other.remove(SCHEME_INDEX);
    serde_json::from_value(Value::Object(merge_objects(doc, other))).map_err(|e| err(e.to_string()))
}

/// `extensions_used` as opened (order kept), with `NS` appended when missing.
fn extensions_of(parked: Option<Value>) -> Value {
    let mut used = match parked {
        Some(Value::Array(u)) => u,
        _ => Vec::new(),
    };
    if !used.iter().any(|e| e == NS) {
        used.push(NS.into());
    }
    Value::Array(used)
}

fn array_of_lenient<'a>(page: &'a Value, key: &str) -> &'a [Value] {
    page.get(key).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// Converts a Sketchy document back to a save model.
///
/// Core fields win over the stored residual; see the module docs. Fails on
/// duplicate element ids, unsupported required extensions, id exhaustion and
/// numbers that overflow when scaled to feet.
pub fn from_sketchy(doc: &Document) -> Result<Value, SketchyError> {
    finite_guard(|| from_sketchy_inner(doc))
}

/// Element ids MUST be unique within a document (spec: Identifiers).
fn check_unique_ids(doc: &Document) -> Result<(), SketchyError> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut ids: Vec<&str> = doc.units.iter().map(|u| u.id.as_str()).collect();
    for s in &doc.structures {
        ids.push(&s.id);
        for l in &s.levels {
            ids.push(&l.id);
            ids.extend(l.spaces.iter().map(|e| e.id.as_str()));
            ids.extend(l.walls.iter().map(|e| e.id.as_str()));
            ids.extend(l.openings.iter().map(|e| e.id.as_str()));
            ids.extend(l.features.iter().map(|e| e.id.as_str()));
            ids.extend(l.voids.iter().map(|e| e.id.as_str()));
        }
    }
    let annotations = doc.presentation.iter().flat_map(|p| &p.annotations);
    ids.extend(annotations.filter_map(|a| a.get("id").and_then(Value::as_str)));
    match ids.into_iter().find(|id| !seen.insert(id)) {
        Some(dup) => Err(err(format!("duplicate id {dup}"))),
        None => Ok(()),
    }
}

fn from_sketchy_inner(doc: &Document) -> Result<Value, SketchyError> {
    check_unique_ids(doc)?;
    if let Some(bad) = doc.extensions_required.iter().flatten().find(|e| *e != NS) {
        return Err(err(format!("requires unsupported extension {bad}")));
    }
    let mut ctx = Ctx::new(unit_scale(&doc.length_unit)?);
    let mut root = doc.extras.as_ref().and_then(|e| e.get(NS)).and_then(Value::as_object).cloned().unwrap_or_default();
    let levels: &[Level] = doc.structures.first().map_or(&[], |s| s.levels.as_slice());
    let annotations: &[Value] = doc.presentation.as_ref().map_or(&[], |p| p.annotations.as_slice());
    let (annos, parked_annotations) = split_annotations(levels, annotations);
    let parked_values: Vec<Value> = parked_annotations.iter().map(|(_, a)| a.clone()).collect();
    ctx.stamp_order = !parked_annotations.is_empty() || !in_default_order(&annos);
    let stored_next = root.get("nextId").and_then(Value::as_i64).filter(|n| (1..=MAX_ID).contains(n));
    let assigned = ctx.assign_ids(levels, &annos, &parked_values, stored_next)?;
    ctx.prepare_from(doc, levels);
    let grew = ctx.plan_display(levels)?;
    let pages = levels.iter().enumerate().map(|(i, l)| page_from_level(&ctx, i, l, &annos)).collect::<Result<Vec<_>, _>>()?;
    root.insert("pages".into(), Value::Array(pages));
    if ctx.layers_mapped {
        let layers = doc.presentation.as_ref().and_then(|p| sketchy_layers(Some(&p.other))).unwrap_or_default();
        root.insert("layers".into(), Value::Array(layers.into_iter().map(|l| layer_from_view(&ctx, l)).collect()));
    }
    let scheme = sketchy_code_scheme(doc);
    if let Some(i) = scheme {
        let rows = doc.classification_schemes[i].codes.iter().filter_map(Value::as_object);
        root.insert("areaCodes".into(), Value::Array(rows.map(|c| code_from_view(&ctx, c)).collect()));
    }
    if let Some(title) = &doc.title {
        root.insert("fileName".into(), title.clone().into());
    }
    let invalid_next = root.contains_key("nextId") && stored_next.is_none();
    if invalid_next || ((assigned || grew) && ctx.next_free > stored_next.unwrap_or(0)) {
        root.insert("nextId".into(), ctx.next_free.into());
    }
    let address = doc.property.as_ref().and_then(|p| p.get("address")).and_then(Value::as_str);
    if let Some(a) = address {
        set_property_address(&mut root, a);
    }
    let consumed: HashSet<String> = levels
        .iter()
        .flat_map(|l| {
            let areas = l.spaces.iter().filter(|s| is_area(&s.shape)).map(|s| s.id.clone());
            l.walls.iter().map(|w| w.id.clone()).chain(areas)
        })
        .chain(annos.iter().map(|m| m.id.to_string()))
        .collect();
    let parts = Parked { annotations: parked_annotations, consumed, layers_mapped: ctx.layers_mapped, scheme };
    park(&mut root, root_rest(doc, parts)?);
    Ok(Value::Object(root))
}

/// What the app will keep but not display when it opens `doc`: structures beyond
/// the first, openings, features, voids, spaces without an area shape (polyline,
/// arc, unknown types) and annotations it cannot show. Only non-zero counts are
/// listed, in that order; each label already agrees with its count (`"structure"`
/// for 1, `"openings"` for 12), so a caller can write `"{count} {label}"`.
#[must_use]
pub fn unshown_summary(doc: &Document) -> Vec<(String, usize)> {
    let levels: &[Level] = doc.structures.first().map_or(&[], |s| s.levels.as_slice());
    let annotations: &[Value] = doc.presentation.as_ref().map_or(&[], |p| p.annotations.as_slice());
    let count = |f: fn(&Level) -> usize| levels.iter().map(f).sum::<usize>();
    let rows = [
        ("structure", "structures", doc.structures.len().saturating_sub(1)),
        ("opening", "openings", count(|l| l.openings.len())),
        ("feature", "features", count(|l| l.features.len())),
        ("void", "voids", count(|l| l.voids.len())),
        ("non-area space", "non-area spaces", count(|l| l.spaces.iter().filter(|s| !is_area(&s.shape)).count())),
        ("annotation", "annotations", split_annotations(levels, annotations).1.len()),
    ];
    rows.into_iter()
        .filter(|(_, _, n)| *n > 0)
        .map(|(one, many, n)| ((if n == 1 { one } else { many }).to_string(), n))
        .collect()
}

/// What [`from_sketchy`] consumed, so [`root_rest`] parks only the remainder.
struct Parked {
    /// Annotations the app does not show, with their drawing index.
    annotations: Vec<(usize, Value)>,
    /// Sketchy ids of every element converted to an app object.
    consumed: HashSet<String>,
    layers_mapped: bool,
    /// Index of the area-code scheme that became `areaCodes`.
    scheme: Option<usize>,
}

fn layer_from_view(ctx: &Ctx, view: &Map<String, Value>) -> Value {
    let sid = str_of(view, "id").unwrap_or_default();
    let app = ctx.layer_app.get(sid).cloned().unwrap_or_default();
    let mut obj = back_object(ctx, view, LAYER_GROUPS, LAYER_OWNED);
    set_ids(&mut obj, "layer:", &Value::String(app), sid);
    Value::Object(obj)
}

/// Presentation content outside what the save model holds: unmapped annotations,
/// styles, pages, underlays, foreign layers and assignments of unconverted elements.
fn presentation_rest(doc: &Document, parts: &Parked) -> Result<Option<Value>, SketchyError> {
    let Some(pres) = &doc.presentation else { return Ok(None) };
    let mut rest = to_map(pres)?;
    rest.remove("annotations");
    if parts.layers_mapped {
        rest.remove("layers");
        if let Some(Value::Object(assigned)) = rest.get_mut("element_layers") {
            assigned.retain(|k, _| !parts.consumed.contains(k));
            if assigned.is_empty() {
                rest.remove("element_layers");
            }
        }
    }
    Ok((!rest.is_empty()).then_some(Value::Object(rest)))
}

/// Schemes other than the mapped one, and that one's own fields (`scheme0`).
fn schemes_rest(doc: &Document, mapped: Option<usize>, rest: &mut Map<String, Value>) -> Result<(), SketchyError> {
    let others: Vec<Value> = doc
        .classification_schemes
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != mapped)
        .map(|(_, s)| to_map(s).map(Value::Object))
        .collect::<Result<_, _>>()?;
    if !others.is_empty() {
        rest.insert("classification_schemes".into(), Value::Array(others));
        if let Some(i) = mapped.filter(|i| *i > 0) {
            rest.insert(SCHEME_INDEX.into(), i.into());
        }
    }
    if let Some(scheme) = mapped.map(|i| &doc.classification_schemes[i]) {
        let mut s = to_map(scheme)?;
        for key in ["scheme", "codes"] {
            s.remove(key);
        }
        strip_ns_extras(&mut s);
        if s.get("name") == Some(&json!(CODE_SCHEME_NAME)) {
            s.remove("name");
        }
        if !s.is_empty() {
            rest.insert("scheme0".into(), Value::Object(s));
        }
    }
    Ok(())
}

/// Sketchy content outside what the save model holds: unowned root fields,
/// foreign extras and extensions, extra structures and structure fields.
fn root_rest(doc: &Document, parts: Parked) -> Result<Map<String, Value>, SketchyError> {
    let view = to_map(doc)?;
    let (_, mut rest) = split_object(&view, ROOT_OWNED);
    strip_ns_extras(&mut rest);
    rest.remove("presentation");
    rest.remove("classification_schemes");
    if let Some(p) = presentation_rest(doc, &parts)? {
        rest.insert("presentation".into(), p);
    }
    schemes_rest(doc, parts.scheme, &mut rest)?;
    rest.insert(DOCUMENT_ID.into(), doc.id.clone().into());
    if let Some(r) = &doc.revision {
        rest.insert(PARENT_REVISION.into(), r.id.clone().into());
    }
    if !parts.annotations.is_empty() {
        let entries = parts.annotations.iter().map(|(i, a)| json!({"index": i, "annotation": a}));
        rest.insert(PARKED_ANNOS.into(), Value::Array(entries.collect()));
    }
    if let Some(Value::Object(prop)) = rest.get_mut("property") {
        if prop.get("address").is_some_and(Value::is_string) {
            prop.remove("address");
        }
        if prop.is_empty() {
            rest.remove("property");
        }
    }
    // Kept whole (with `NS` in its place) when it names any other extension, so the order survives.
    if let Some(used) = doc.extensions_used.as_ref().filter(|u| u.iter().any(|e| e != NS)) {
        rest.insert("extensions_used".into(), json!(used));
    }
    if doc.length_unit != "ft" {
        rest.insert("length_unit".into(), doc.length_unit.clone().into());
    }
    if let Some(first) = doc.structures.first() {
        let mut s = to_map(first)?;
        s.remove("levels");
        strip_ns_extras(&mut s);
        for (k, default) in [("id", json!("building")), ("kind", json!("other")), ("name", json!(doc.title))] {
            if s.get(k) == Some(&default) {
                s.remove(k);
            }
        }
        if !s.is_empty() {
            rest.insert("structure0".into(), Value::Object(s));
        }
    }
    let more: Vec<Value> = doc.structures.iter().skip(1).map(|s| to_map(s).map(Value::Object)).collect::<Result<_, _>>()?;
    if !more.is_empty() {
        rest.insert("structures".into(), Value::Array(more));
    }
    Ok(rest)
}

fn set_property_address(root: &mut Map<String, Value>, address: &str) {
    let info = root.entry("subjectInfo").or_insert_with(|| json!({}));
    if let Some(info) = info.as_object_mut() {
        if info.get("propertyAddress").and_then(Value::as_str) != Some(address) {
            info.insert("propertyAddress".into(), address.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A context with one area (`a-6` = 6), one wall (`w-5` = 5, from (0,0) to (10,0)),
    /// area code `GLA1`, and a scale other than 1.
    fn ctx() -> Ctx {
        let mut ctx = Ctx::new(2.0);
        ctx.to_app.insert(('a', "a-6".into()), json!(6));
        ctx.to_app.insert(('w', "w-5".into()), json!(5));
        ctx.to_sk.insert("6".into(), "a-6".into());
        ctx.wall_pts.insert("5".into(), json!([[0, 0], [10, 0]]));
        ctx.code_defaults.insert("GLA1".into(), json!({"use": "living", "finished": true}).as_object().cloned().unwrap_or_default());
        ctx
    }

    /// (group table, its first core key, seed app object, core sample, round-trips exactly).
    /// A `false` row documents a deliberately lossy value: the leftover rule parks it.
    type Row = (&'static str, &'static str, Value, Value, bool);

    fn rows() -> Vec<Row> {
        let area = format!("{NS}.area-codes");
        let syms = format!("{NS}.symbols");
        vec![
            ("wall", "bulge", json!({}), json!({"bulge": 0.5}), true),
            ("wall", "bulge", json!({}), json!({"bulge": -0.25}), true),
            ("wall", "thickness", json!({}), json!({"thickness": 0.5}), true),
            ("wall", "kind", json!({}), json!({"kind": "exterior"}), true),
            ("wall", "kind", json!({}), json!({"kind": "interior"}), true),
            ("wall", "kind", json!({}), json!({"kind": "party"}), false),
            ("polygon", "name", json!({}), json!({"name": "Living"}), true),
            ("polygon", "classifications", json!({}), json!({"classifications": [{"scheme": area, "code": "LIV"}]}), true),
            ("polygon", "classifications", json!({}), json!({"classifications": [{"scheme": "org.x", "code": "A"}]}), false),
            ("polygon", "area_factor", json!({}), json!({"area_factor": 1.5}), true),
            ("polygon", "area_factor", json!({}), json!({"area_factor": 1.0}), false),
            ("polygon", "measured_to", json!({}), json!({"measured_to": "interior_face"}), true),
            ("polygon", "measured_to", json!({}), json!({"measured_to": "exterior_face"}), true),
            ("polygon", "measured_to", json!({}), json!({"measured_to": "centerline"}), true),
            ("polygon", "measured_to", json!({}), json!({"measured_to": "unspecified"}), false),
            ("polygon", "within", json!({}), json!({"within": "a-6", "subtract": true}), true),
            ("polygon", "within", json!({}), json!({"within": "a-6"}), false),
            ("polygon", "within", json!({}), json!({"within": "ghost", "subtract": true}), false),
            ("polygon", "attributes", json!({}), json!({"attributes": {"k": 1}}), true),
            ("polygon", "bulges", json!({"points": [{}, {}, {}]}), json!({"bulges": [0.5, 0, -0.25]}), true),
            ("polygon", "use", json!({}), json!({"use": "garage"}), false),
            ("label", "position", json!({}), json!({"position": [3, 4]}), true),
            ("label", "text", json!({}), json!({"text": "Hi"}), true),
            ("label", "rotation", json!({}), json!({"rotation": 30}), true),
            ("symbol", "position", json!({}), json!({"position": [3, 4]}), true),
            ("symbol", "rotation", json!({}), json!({"rotation": -90}), true),
            ("symbol", "ref", json!({}), json!({"ref": {"scheme": syms, "name": "door"}}), true),
            ("symbol", "ref", json!({}), json!({"ref": {"scheme": "org.x", "name": "chair"}}), false),
            ("symbol", "scale", json!({}), json!({"scale": [2, 2]}), true),
            ("symbol", "scale", json!({}), json!({"scale": [2, 3]}), false),
            ("symbol", "mirror", json!({}), json!({"mirror": "x"}), true),
            ("symbol", "mirror", json!({}), json!({"mirror": "none"}), true),
            ("symbol", "mirror", json!({}), json!({"mirror": "y"}), false),
            ("symbol", "host", json!({}), json!({"host": {"wall": "w-5"}}), true),
            ("symbol", "host", json!({}), json!({"host": {"wall": "ghost"}}), false),
            ("chain", "offset", json!({}), json!({"offset": -3}), true),
            ("chain", "offset", json!({}), json!({"offset": 2}), true),
            ("chain", "target", json!({}), json!({"target": {"wall": "w-5"}}), true),
            ("chain", "points", json!({"seedLineId": 5}), json!({"points": [[0, 0], [2, 0], [5, 0]]}), false),
            ("layer", "name", json!({}), json!({"name": "Dims"}), true),
            ("layer", "visible", json!({}), json!({"visible": false}), true),
            ("layer", "locked", json!({}), json!({"locked": true}), true),
            ("code", "label", json!({}), json!({"label": "First Floor"}), true),
            ("code", "area_factor", json!({}), json!({"area_factor": 0.5}), true),
            ("code", "use", json!({}), json!({"use": "living", "finished": true}), true),
            ("code", "use", json!({}), json!({"use": "garage"}), false),
        ]
    }

    fn tables() -> [(&'static str, &'static [Group]); 7] {
        [
            ("wall", WALL_GROUPS),
            ("polygon", POLYGON_GROUPS),
            ("label", LABEL_GROUPS),
            ("symbol", SYMBOL_GROUPS),
            ("chain", CHAIN_GROUPS),
            ("layer", LAYER_GROUPS),
            ("code", CODE_GROUPS),
        ]
    }

    #[test]
    fn group_fwd_inverts_back_or_is_listed_lossy() {
        let ctx = ctx();
        let rows = rows();
        for (table, groups) in tables() {
            for g in groups {
                let covered = rows.iter().any(|r| r.0 == table && r.1 == g.core_keys[0]);
                assert!(covered, "{table} group {:?} has no sample row", g.core_keys);
            }
        }
        for (table, key, seed, core, lossless) in rows {
            let groups = tables().into_iter().find(|t| t.0 == table).map(|t| t.1).unwrap_or_default();
            let g = groups.iter().find(|g| g.core_keys[0] == key).unwrap_or_else(|| panic!("{table}.{key}"));
            let (mut obj, core) = (seed.as_object().cloned().unwrap_or_default(), core.as_object().cloned().unwrap_or_default());
            (g.back)(&ctx, &mut obj, &core);
            let again = (g.fwd)(&ctx, &obj);
            let same = g.core_keys.iter().all(|k| approx_eq(again.get(*k), core.get(*k)));
            assert_eq!(same, lossless, "{table}.{key}: {core:?} -> {obj:?} -> {again:?}");
        }
    }
}
