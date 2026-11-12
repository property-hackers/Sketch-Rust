use serde_json::{json, Value};
use sketch_core::geometry::{Edge, GeometryEngine, Line, Pt};
use sketch_core::sketchy::convert::{from_sketchy, parent_revision, to_sketchy, unshown_summary, upgrade};
use sketch_core::sketchy::model::{Document, Shape, Space};

const RICH: &str = include_str!("data/save-models/rich.json");

fn rich() -> Value {
    serde_json::from_str(RICH).expect("rich save model")
}

fn level_of(doc: &Document) -> &sketch_core::sketchy::model::Level {
    &doc.structures[0].levels[0]
}

fn model_with(lines: Value, polygons: Value) -> Value {
    let mut m = rich();
    m["pages"][0]["lines"] = lines;
    m["pages"][0]["polygons"] = polygons;
    m
}

/// `m` as it opens again from the Sketchy document written with id `id`: the same
/// model, with the document id parked for the next save. `savedAt` and
/// `settings.canvasTheme` are not content (a timestamp, a UI preference), so
/// both are dropped on write and absent on reopen.
fn opened(m: &Value, id: &str) -> Value {
    let mut m = m.clone();
    if let Some(root) = m.as_object_mut() {
        root.remove("savedAt");
    }
    if let Some(settings) = m.get_mut("settings").and_then(Value::as_object_mut) {
        settings.remove("canvasTheme");
    }
    m["sketchy_other"] = json!({"document_id": id});
    m
}

#[test]
fn rich_model_round_trips_exactly() {
    let rich = rich();
    let doc = to_sketchy(&rich, Some("doc-1")).expect("to");
    assert_eq!(from_sketchy(&doc).expect("from"), opened(&rich, "doc-1"));
}

#[test]
fn unchanged_save_model_hashes_identically_despite_saved_at() {
    use sketch_core::sketchy::canonical::content_hash;
    let (mut a, mut b) = (rich(), rich());
    a["savedAt"] = json!("2026-01-01T00:00:00.000Z");
    b["savedAt"] = json!("2026-02-02T00:00:00.000Z");
    let ha = content_hash(&to_sketchy(&a, Some("doc-1")).expect("a").to_value());
    let hb = content_hash(&to_sketchy(&b, Some("doc-1")).expect("b").to_value());
    assert_eq!(ha, hb);
}

#[test]
fn walls_map_to_core() {
    let doc = to_sketchy(&rich(), Some("doc-1")).expect("to");
    assert_eq!(doc.title.as_deref(), Some("Rodney"));
    assert_eq!(doc.structures[0].id, "building");
    assert_eq!(level_of(&doc).id, "page-1");
    let wall = level_of(&doc).walls.iter().find(|w| w.id == "w-5").expect("w-5");
    assert_eq!(wall.start, [0.0, 0.0]);
    assert_eq!(wall.end, [10.0, 0.0]);
    assert_eq!(wall.kind.as_deref(), Some("exterior"));
    assert!((wall.thickness.expect("thickness") - 4.0 / 12.0).abs() < 1e-12);
    assert_eq!(wall.bulge, None);
}

/// The app's world is +y up (`view_state.rs` `to_screen`: `screen_y = pan.1 - w.y * ...`),
/// the same as Sketchy, so coordinates pass through unchanged.
#[test]
fn y_axis_is_not_flipped() {
    let doc = to_sketchy(&rich(), Some("doc-1")).expect("to");
    let space = &level_of(&doc).spaces[0];
    assert_eq!(space.id, "a-6");
    let Shape::Polygon { points, .. } = &space.shape else { panic!("polygon") };
    assert_eq!(points, &vec![[0.0, 0.0], [10.0, 0.0], [10.0, 8.0]]);
}

#[test]
fn sketchy_y_up_is_app_y_up() {
    let doc = foreign(json!([level(
        json!([{"id": "s", "shape": {"type": "polygon", "points": [[0, 0], [10, 0], [10, 30], [0, 30]]}}]),
        json!([])
    )]));
    let saved = from_sketchy(&doc).expect("from");
    let ys: Vec<f64> =
        saved["pages"][0]["polygons"][0]["points"].as_array().expect("pts").iter().map(|p| p["y"].as_f64().expect("y")).collect();
    assert_eq!(ys, [0.0, 0.0, 30.0, 30.0]);
}

/// Farthest-from-the-chord y of the arc `flatten_ring` (the app's polygon arc) draws over
/// (0,0)->(10,0) with `bow` degrees.
fn app_arc_apex_y(bow: f64) -> f64 {
    let points = [Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)];
    let edges = [Edge { line_id: None, bow_angle: bow }, Edge::default()];
    let flat = GeometryEngine::new().flatten_ring(&points, &edges, 0.05);
    assert!(flat.len() > 2, "arc was sampled");
    flat.iter().map(|p| p.y).fold(0.0, |m, y| if y.abs() > m.abs() { y } else { m })
}

#[test]
fn bowed_wall_bulge() {
    let line = json!({"id": 9, "start": {"x": 0.0, "y": 0.0}, "end": {"x": 10.0, "y": 0.0},
                      "type": "interior", "bowAngle": 90.0});
    let doc = to_sketchy(&model_with(json!([line]), json!([])), Some("d")).expect("to");
    let wall = &level_of(&doc).walls[0];
    let expected = -(90.0_f64 * std::f64::consts::PI / 720.0).tan();
    assert!((wall.bulge.expect("bulge") - expected).abs() < 1e-12);
    let neg = json!({"id": 9, "start": {"x": 0.0, "y": 0.0}, "end": {"x": 10.0, "y": 0.0}, "bowAngle": -90.0});
    let doc = to_sketchy(&model_with(json!([neg]), json!([])), Some("d")).expect("to");
    assert!(level_of(&doc).walls[0].bulge.expect("bulge") > 0.0);
}

/// Apex y of the arc SPEC 0.2 section 4.2 describes over (0,0)->(10,0): bulge 1 passes
/// through (5,-5) (DXF: positive is counter-clockwise, bowing right of travel).
fn spec_apex_y(bulge: f64) -> f64 {
    -bulge * 10.0 / 2.0
}

#[test]
fn bulge_sign_follows_dxf_in_0_2() {
    assert_eq!(spec_apex_y(1.0), -5.0, "SPEC 0.2 4.2: bulge 1 passes through [5,-5]");
    assert_eq!(spec_apex_y(-1.0), 5.0, "SPEC 0.2 4.2: bulge -1 passes through [5,5]");
    // The app's positive bow bows toward +y (pinned against `flatten_ring`), so its 0.2 bulge is negative.
    let line = json!({"id": 9, "start": {"x": 0.0, "y": 0.0}, "end": {"x": 10.0, "y": 0.0}, "bowAngle": 90.0});
    let doc = to_sketchy(&model_with(json!([line]), json!([])), Some("d")).expect("to");
    let bulge = level_of(&doc).walls[0].bulge.expect("bulge");
    assert!(bulge < 0.0, "positive app bow is a negative bulge");
    assert!(spec_apex_y(bulge) > 1.0 && app_arc_apex_y(90.0) > 1.0, "both put the apex at +y");
    assert!((spec_apex_y(bulge) - app_arc_apex_y(90.0)).abs() < 0.06, "the same arc");
    assert!(spec_apex_y(-bulge) < -1.0 && app_arc_apex_y(-90.0) < -1.0);
    // The same holds for polygon edges.
    let mut m = rich();
    m["pages"][0]["polygons"][0]["edges"] =
        json!([{"lineId": null, "bowAngle": 90.0}, {"lineId": null, "bowAngle": 0.0}, {"lineId": null, "bowAngle": 0.0}]);
    let doc = to_sketchy(&m, Some("d")).expect("to");
    let Shape::Polygon { bulges, .. } = &level_of(&doc).spaces[0].shape else { panic!("polygon") };
    assert!(bulges.as_ref().expect("bulges")[0] < 0.0);
    // And Sketchy 0.2 -> app: bulge -1 bows left (+y) so the bow is +180.
    let foreign_doc = foreign(json!([level(json!([]), json!([{"id": "w", "start": [0, 0], "end": [10, 0], "bulge": -1}]))]));
    let saved = from_sketchy(&foreign_doc).expect("from");
    assert!((saved["pages"][0]["lines"][0]["bowAngle"].as_f64().expect("bow") - 180.0).abs() < 1e-9);
}

#[test]
fn core_wins_over_stale_residual() {
    let mut m = rich();
    m["pages"][0]["lines"][0]["bowAngle"] = json!(30.0);
    m["pages"][0]["lines"][0]["arcHeight"] = json!(1.25);
    let mut doc = to_sketchy(&m, Some("d")).expect("to");
    assert_eq!(from_sketchy(&doc).expect("from"), opened(&m, "d"));
    let level = &mut doc.structures[0].levels[0];
    level.walls.iter_mut().find(|w| w.id == "w-5").expect("w-5").bulge = Some(0.5);
    let Shape::Polygon { points, .. } = &mut level.spaces[0].shape else { panic!("polygon") };
    points[2] = [12.0, 8.0];
    let back = from_sketchy(&doc).expect("from");
    let line = &back["pages"][0]["lines"][0];
    let want = -(0.5_f64).atan() * 720.0 / std::f64::consts::PI;
    assert!((line["bowAngle"].as_f64().expect("bowAngle") - want).abs() < 1e-9);
    // The Height-mode rise of the new arc (chord 10, bulge 0.5 is a rise of 2.5, bowing right of
    // travel in 0.2, so the app's negative bow): consistent, not the stale 1.25.
    assert!((line["arcHeight"].as_f64().expect("arcHeight") + 2.5).abs() < 1e-9);
    assert_eq!(back["pages"][0]["polygons"][0]["points"][2], json!({"x": 12.0, "y": 8.0}));
}

#[test]
fn unknown_sketchy_fields_survive_save() {
    let doc = to_sketchy(&rich(), Some("d")).expect("to");
    let mut value = doc.to_value();
    value["structures"][0]["levels"][0]["spaces"][0]["future_field"] = json!({"a": 1});
    let edited: Document = serde_json::from_value(value).expect("doc");
    let saved = from_sketchy(&edited).expect("from");
    let again = to_sketchy(&saved, Some("d")).expect("to").to_value();
    assert_eq!(again["structures"][0]["levels"][0]["spaces"][0]["future_field"], json!({"a": 1}));
}

#[test]
fn subtract_always_becomes_within() {
    let mut m = rich();
    m["pages"][0]["polygons"][0]["subtractMode"] = json!("always");
    m["pages"][0]["polygons"][0]["subtractTargetId"] = json!(6);
    let doc = to_sketchy(&m, Some("d")).expect("to");
    let space = &level_of(&doc).spaces[0];
    assert_eq!(space.within.as_deref(), Some("a-6"));
    assert_eq!(space.subtract, Some(true));
    assert_eq!(from_sketchy(&doc).expect("from"), opened(&m, "d"));
}

#[test]
fn polygon_fields_map_to_core() {
    let doc = to_sketchy(&rich(), Some("d")).expect("to");
    let space = &level_of(&doc).spaces[0];
    assert_eq!(space.name.as_deref(), Some("Living"));
    assert_eq!(space.area_factor, Some(1.5));
    assert_eq!(space.measured_to.as_deref(), Some("interior_face"));
    assert_eq!(space.classifications[0].code, "LIV");
    assert!(space.classifications[0].scheme.ends_with(".area-codes"));
    assert_eq!(doc.property.expect("property")["address"], json!("123 Main St"));
}

#[test]
fn bowed_polygon_edges_round_trip() {
    let mut m = rich();
    m["pages"][0]["polygons"][0]["edges"] =
        json!([{"lineId": null, "bowAngle": 45.0}, {"lineId": null, "bowAngle": 0.0}, {"lineId": null, "bowAngle": -20.0}]);
    let doc = to_sketchy(&m, Some("d")).expect("to");
    let Shape::Polygon { bulges, .. } = &level_of(&doc).spaces[0].shape else { panic!("polygon") };
    let bulges = bulges.as_ref().expect("bulges");
    assert!((bulges[0] + (45.0_f64 * std::f64::consts::PI / 720.0).tan()).abs() < 1e-12);
    assert_eq!(bulges[1], 0.0);
    assert_eq!(from_sketchy(&doc).expect("from"), opened(&m, "d"));
}

// ---- fix round 1 ----

fn foreign(levels: Value) -> Document {
    foreign_v("0.2", levels)
}

fn foreign_v(version: &str, levels: Value) -> Document {
    serde_json::from_value(json!({
        "format": "sketchy", "format_version": version, "id": "foreign", "length_unit": "ft",
        "structures": [{"id": "main", "levels": levels}],
    }))
    .expect("foreign doc")
}

fn square(id: &str, extra: Value) -> Value {
    let mut s = json!({"id": id, "shape": {"type": "polygon", "points": [[0, 0], [10, 0], [10, 10], [0, 10]]}});
    for (k, v) in extra.as_object().expect("obj") {
        s[k] = v.clone();
    }
    s
}

fn level(spaces: Value, walls: Value) -> Value {
    json!({"id": "L1", "grade": "above", "sequence": 1, "spaces": spaces, "walls": walls})
}

#[test]
fn within_alone_is_not_subtract() {
    let doc = foreign(json!([level(
        json!([square("outline", json!({})), square("room", json!({"within": "outline"}))]),
        json!([])
    )]));
    let saved = from_sketchy(&doc).expect("from");
    for poly in saved["pages"][0]["polygons"].as_array().expect("polygons") {
        assert_ne!(poly["subtractMode"], json!("always"));
    }
    let again = to_sketchy(&saved, None).expect("to").to_value();
    let spaces = &again["structures"][0]["levels"][0]["spaces"];
    let room = spaces.as_array().expect("spaces").iter().find(|s| s["id"] == "room").expect("room");
    assert_eq!(room["within"], json!("outline"));
    assert!(room.get("subtract").is_none());
}

#[test]
fn subtract_false_from_foreign_edit_wins() {
    let mut m = rich();
    m["pages"][0]["polygons"][0]["subtractMode"] = json!("always");
    m["pages"][0]["polygons"][0]["subtractTargetId"] = json!(6);
    let mut doc = to_sketchy(&m, Some("d")).expect("to");
    doc.structures[0].levels[0].spaces[0].subtract = Some(false);
    let saved = from_sketchy(&doc).expect("from");
    assert_ne!(saved["pages"][0]["polygons"][0]["subtractMode"], json!("always"));
    let again = to_sketchy(&saved, Some("d")).expect("to");
    let space = &again.structures[0].levels[0].spaces[0];
    assert_eq!(space.within.as_deref(), Some("a-6"));
    assert_eq!(space.subtract, Some(false));
}

#[test]
fn foreign_content_survives_open_and_save() {
    let mut value = serde_json::to_value(foreign(json!([level(json!([]), json!([]))]))).expect("value");
    value["units"] = json!([{"id": "u1", "kind": "primary_dwelling"}]);
    value["site"] = json!({"x": 1});
    value["notes"] = json!([{"text": "hi"}]);
    value["extras"] = json!({"org.example.x": {"k": 1}});
    value["extensions_used"] = json!(["org.example.x"]);
    value["property"] = json!({"address": "1 A St", "apn": "123"});
    value["future_field"] = json!(true);
    value["structures"][0]["levels"][0]["elevation"] = json!(3);
    value["structures"][0]["levels"][0]["extras"] = json!({"org.example.x": 2});
    value["structures"][0]["levels"][0]["openings"] = json!([{"id": "o1", "kind": "door"}]);
    value["structures"][0]["levels"][0]["future_level"] = json!("keep");
    value["structures"][0]["attachment"] = json!("detached");
    value["structures"].as_array_mut().expect("arr").push(json!({"id": "garage", "levels": []}));
    let doc: Document = serde_json::from_value(value.clone()).expect("doc");
    let again = to_sketchy(&from_sketchy(&doc).expect("from"), None).expect("to").to_value();
    for path in ["/units", "/site", "/notes", "/property", "/future_field", "/structures/1"] {
        assert_eq!(again.pointer(path), value.pointer(path), "{path}");
    }
    assert_eq!(again["extras"]["org.example.x"], json!({"k": 1}));
    assert_eq!(again["extensions_used"], json!(["org.example.x", sketch_core::sketchy::convert::NS]));
    let (a, v) = (&again["structures"][0], &value["structures"][0]);
    assert_eq!(a["attachment"], v["attachment"]);
    assert_eq!(a["id"], json!("main"));
    assert_eq!(a["levels"][0]["extras"]["org.example.x"], json!(2));
    for key in ["elevation", "openings", "future_level", "id"] {
        assert_eq!(a["levels"][0][key], v["levels"][0][key], "{key}");
    }
}

#[test]
fn unsupported_required_extension_is_rejected() {
    let mut doc = foreign(json!([]));
    doc.extensions_required = Some(vec!["org.example.x".into()]);
    assert!(from_sketchy(&doc).is_err());
}

#[test]
fn foreign_ids_round_trip_and_do_not_collide() {
    let wall = |id: &str| json!({"id": id, "start": [0, 0], "end": [5, 0]});
    let doc = foreign(json!([level(
        json!([square("a-5", json!({})), square("0b3f-uuid", json!({}))]),
        json!([wall("w-5"), wall("w-007"), wall("9c1e-uuid")])
    )]));
    let saved = from_sketchy(&doc).expect("from");
    let page = &saved["pages"][0];
    let mut ids: Vec<i64> = ["lines", "polygons"]
        .iter()
        .flat_map(|k| page[*k].as_array().expect("arr").iter().filter(|e| e.get("sketchy_display").is_none() || *k == "polygons"))
        .map(|e| e["id"].as_i64().expect("int id"))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 5, "ids collide: {ids:?}");
    // Display walls draw from the same id space: every id on the page is distinct.
    let all: Vec<i64> =
        ["lines", "polygons"].iter().flat_map(|k| page[*k].as_array().expect("arr").iter().map(|e| e["id"].as_i64().expect("id"))).collect();
    assert_eq!(all.len(), all.iter().collect::<std::collections::HashSet<_>>().len(), "ids collide: {all:?}");
    let again = to_sketchy(&saved, None).expect("to").to_value();
    let lvl = &again["structures"][0]["levels"][0];
    let names = |k: &str| -> Vec<String> {
        let mut v: Vec<String> =
            lvl[k].as_array().expect("arr").iter().map(|e| e["id"].as_str().expect("id").to_string()).collect();
        v.sort();
        v
    };
    assert_eq!(names("walls"), ["9c1e-uuid", "w-007", "w-5"]);
    assert_eq!(names("spaces"), ["0b3f-uuid", "a-5"]);
    // Stable across another open/save.
    let doc2: Document = serde_json::from_value(again).expect("doc");
    assert_eq!(from_sketchy(&doc2).expect("from"), saved);
}

#[test]
fn within_target_follows_fresh_ids() {
    let doc = foreign(json!([level(
        json!([square("outer", json!({})), square("hole", json!({"within": "outer", "subtract": true}))]),
        json!([])
    )]));
    let saved = from_sketchy(&doc).expect("from");
    let polys = saved["pages"][0]["polygons"].as_array().expect("arr");
    let outer = polys.iter().find(|p| p["sketchy_id"] == "outer").expect("outer");
    let hole = polys.iter().find(|p| p["sketchy_id"] == "hole").expect("hole");
    assert_eq!(hole["subtractMode"], json!("always"));
    assert_eq!(hole["subtractTargetId"], outer["id"]);
    let again = to_sketchy(&saved, None).expect("to").to_value();
    let spaces = again["structures"][0]["levels"][0]["spaces"].as_array().expect("arr").clone();
    let hole = spaces.iter().find(|s| s["id"] == "hole").expect("hole");
    assert_eq!(hole["within"], json!("outer"));
    assert_eq!(hole["subtract"], json!(true));
}

#[test]
fn metric_document_opens_in_feet() {
    let mut value = serde_json::to_value(foreign(json!([level(
        json!([]),
        json!([{"id": "w-1", "start": [0, 0], "end": [3, 0], "thickness": 0.3}])
    )])))
    .expect("value");
    value["length_unit"] = json!("m");
    let doc: Document = serde_json::from_value(value).expect("doc");
    let saved = from_sketchy(&doc).expect("from");
    let line = &saved["pages"][0]["lines"][0];
    assert!((line["end"]["x"].as_f64().expect("x") - 9.8425197).abs() < 1e-6);
    assert!((line["thickness"].as_f64().expect("t") - 0.3 * 3.280839895 * 12.0).abs() < 1e-6);
    let again = to_sketchy(&saved, None).expect("to");
    assert_eq!(again.length_unit, "m");
    assert!((again.structures[0].levels[0].walls[0].end[0] - 3.0).abs() < 1e-9);
}

#[test]
fn changed_points_drop_stale_derived_data() {
    let mut m = rich();
    m["pages"][0]["polygons"][0]["edges"] = json!([{"lineId": 5, "arcHeight": 1.0, "bowAngle": 0.0}, {"lineId": 6, "bowAngle": 0.0}, {"lineId": 7, "bowAngle": 0.0}]);
    let mut doc = to_sketchy(&m, Some("d")).expect("to");
    assert_eq!(from_sketchy(&doc).expect("from"), opened(&m, "d"));
    let Shape::Polygon { points, .. } = &mut doc.structures[0].levels[0].spaces[0].shape else { panic!("polygon") };
    points.insert(1, [5.0, 0.0]);
    let saved = from_sketchy(&doc).expect("from");
    let poly = &saved["pages"][0]["polygons"][0];
    assert_eq!(poly["points"].as_array().expect("pts").len(), 4);
    assert!(poly.get("area").is_none() && poly.get("gross").is_none());
    assert!(poly["edges"].as_array().expect("edges").is_empty());
}

#[test]
fn open_shape_spaces_are_kept_verbatim() {
    let poly = json!({"id": "p", "shape": {"type": "polyline", "points": [[0, 0], [5, 0]]}});
    let arc = json!({"id": "r", "shape": {"type": "arc", "center": [0, 0], "radius": 4, "start_angle": 0, "end_angle": 90}});
    let doc = foreign(json!([level(json!([poly.clone(), arc.clone()]), json!([]))]));
    let saved = from_sketchy(&doc).expect("from");
    assert!(saved["pages"][0].get("polygons").is_none() || saved["pages"][0]["polygons"] == json!([]));
    let again = to_sketchy(&saved, None).expect("to").to_value();
    assert_eq!(again["structures"][0]["levels"][0]["spaces"], json!([poly, arc]));
}

#[test]
fn other_wall_kinds_and_unspecified_measure_survive() {
    let wall = json!({"id": "w-1", "start": [0, 0], "end": [5, 0], "kind": "party"});
    let space = square("a-2", json!({"measured_to": "unspecified"}));
    let doc = foreign(json!([level(json!([space]), json!([wall]))]));
    let saved = from_sketchy(&doc).expect("from");
    let again = to_sketchy(&saved, None).expect("to").to_value();
    let lvl = &again["structures"][0]["levels"][0];
    assert_eq!(lvl["walls"][0]["kind"], json!("party"));
    assert_eq!(lvl["spaces"][0]["measured_to"], json!("unspecified"));
    // Changing the app-side type wins over the parked kind.
    let mut edited = saved.clone();
    edited["pages"][0]["lines"][0]["type"] = json!("exterior");
    let again = to_sketchy(&edited, None).expect("to").to_value();
    assert_eq!(again["structures"][0]["levels"][0]["walls"][0]["kind"], json!("exterior"));
}

// ---- task 4: annotations, layers, area codes ----

const NS_SYMBOLS: &str = "io.github.garmonbz.sketchrust.symbols";

fn annotation<'a>(doc: &'a Document, id: &str) -> &'a Value {
    let pres = doc.presentation.as_ref().expect("presentation");
    pres.annotations.iter().find(|a| a["id"] == json!(id)).unwrap_or_else(|| panic!("annotation {id}"))
}

/// Drops `extras` and tells ints from floats no more, so foreign JSON compares with ours.
fn norm(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(m.iter().filter(|(k, _)| *k != "extras").map(|(k, v)| (k.clone(), norm(v))).collect()),
        Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
        Value::Number(n) => json!(n.as_f64()),
        other => other.clone(),
    }
}

#[test]
fn labels_become_text_annotations() {
    let mut m = rich();
    m["pages"][0]["labels"][0]["x"] = json!(3.0);
    m["pages"][0]["labels"][0]["y"] = json!(4.0);
    m["pages"][0]["labels"][0]["rotation"] = json!(15.0);
    let doc = to_sketchy(&m, Some("d")).expect("to");
    let t = annotation(&doc, "t-9");
    assert_eq!(t["kind"], json!("text"));
    assert_eq!(t["level"], json!("page-1"));
    assert_eq!(t["position"], json!([3.0, 4.0]));
    assert_eq!(t["text"], json!("Label"));
    assert_eq!(t["rotation"], json!(-15.0));
    assert_eq!(from_sketchy(&doc).expect("from"), opened(&m, "d"));
    assert!(from_sketchy(&doc).expect("from")["pages"][0].get("sketchy_other").is_none());
}

#[test]
fn symbols_become_symbol_annotations() {
    let doc = to_sketchy(&rich(), Some("d")).expect("to");
    let y = annotation(&doc, "y-8");
    assert_eq!(y["kind"], json!("symbol"));
    assert_eq!(y["level"], json!("page-1"));
    assert_eq!(y["ref"], json!({"scheme": NS_SYMBOLS, "name": "door"}));
    assert_eq!(y["position"], json!([5.0, 0.0]));
    assert_eq!(y["rotation"], json!(-90.0));
    assert_eq!(y["scale"], json!([1.0, 1.0]));
    assert_eq!(y["mirror"], json!("x"));
    assert_eq!(y["host"], json!({"wall": "w-5"}));
    // Editing the Sketchy side wins over the residual.
    let mut value = doc.to_value();
    let annos = value["presentation"]["annotations"].as_array_mut().expect("annos");
    let y = annos.iter_mut().find(|a| a["id"] == json!("y-8")).expect("y-8");
    y["mirror"] = json!("none");
    y["scale"] = json!([2.0, 2.0]);
    y["ref"]["name"] = json!("window");
    y["position"] = json!([6.0, 1.0]);
    let edited: Document = serde_json::from_value(value).expect("doc");
    let back = from_sketchy(&edited).expect("from");
    let sym = &back["pages"][0]["symbols"][0];
    assert_eq!(sym["flipH"], json!(false));
    assert_eq!(sym["scale"], json!(2.0));
    assert_eq!(sym["symbolType"], json!("window"));
    assert_eq!((sym["x"].as_f64(), sym["y"].as_f64()), (Some(6.0), Some(1.0)));
    assert_eq!(sym["wallId"], json!(5));
}

#[test]
fn chains_become_dimensions() {
    let doc = to_sketchy(&rich(), Some("d")).expect("to");
    let d = annotation(&doc, "d-3");
    assert_eq!(d["kind"], json!("dimension"));
    assert_eq!(d["level"], json!("page-1"));
    assert_eq!(d["target"], json!({"wall": "w-5"}));
    // offset 3 with side -1: the app draws the chain at -3 along its left normal.
    assert_eq!(d["offset"], json!(-3.0));
    assert_eq!(d["points"], json!([[0.0, 0.0], [10.0, 0.0]]));
}

#[test]
fn layers_map_and_assign() {
    let doc = to_sketchy(&rich(), Some("d")).expect("to");
    let pres = doc.presentation.as_ref().expect("presentation");
    assert_eq!(pres.other["layers"], json!([{
        "id": "layer:layer-1", "name": "Layer 1", "visible": true, "locked": false,
        "extras": {"io.github.garmonbz.sketchrust": {"collapsed": false, "isGroup": false,
                                                      "locked": false, "name": "Layer 1", "visible": true}}
    }]));
    for id in ["w-5", "a-6", "y-8"] {
        assert_eq!(pres.other["element_layers"][id], json!("layer:layer-1"), "{id}");
    }
    // A foreign layer and assignment open as an app layer id; removing the assignment wins.
    let mut value = doc.to_value();
    value["presentation"]["layers"] = json!([{"id": "lay-dims", "name": "Dims", "visible": false, "locked": true, "parent": "x"}]);
    value["presentation"]["element_layers"] = json!({"w-5": "lay-dims", "a-6": "lay-dims", "y-8": "lay-dims", "o-1": "lay-dims"});
    value["structures"][0]["levels"][0]["openings"] = json!([{"id": "o-1", "kind": "door"}]);
    let foreign_doc: Document = serde_json::from_value(value.clone()).expect("doc");
    let saved = from_sketchy(&foreign_doc).expect("from");
    assert_eq!(saved["layers"][0]["id"], json!("lay-dims"));
    assert_eq!(saved["layers"][0]["visible"], json!(false));
    assert_eq!(saved["layers"][0]["locked"], json!(true));
    assert_eq!(saved["pages"][0]["lines"][0]["layerId"], json!("lay-dims"));
    assert_eq!(saved["pages"][0]["symbols"][0]["layerId"], json!("lay-dims"));
    let again = to_sketchy(&saved, Some("d")).expect("to").to_value();
    assert_eq!(again["presentation"]["layers"][0]["id"], json!("lay-dims"));
    assert_eq!(again["presentation"]["layers"][0]["parent"], json!("x"));
    assert_eq!(again["presentation"]["element_layers"], value["presentation"]["element_layers"]);
    let mut value = doc.to_value();
    value["presentation"]["element_layers"] = json!({"a-6": "layer:layer-1"});
    let unassigned: Document = serde_json::from_value(value).expect("doc");
    let saved = from_sketchy(&unassigned).expect("from");
    assert!(saved["pages"][0]["lines"][0].get("layerId").is_none());
    assert_eq!(saved["pages"][0]["polygons"][0]["layerId"], json!("layer-1"));
    assert!(saved["pages"][0]["symbols"][0].get("layerId").is_none());
}

#[test]
fn area_codes_become_scheme() {
    let doc = to_sketchy(&rich(), Some("d")).expect("to");
    let scheme = doc.classification_schemes.iter().find(|s| s.scheme.ends_with(".area-codes")).expect("scheme");
    assert_eq!(scheme.name.as_deref(), Some("Sketch Rust area codes"));
    assert_eq!(scheme.codes.len(), 32);
    let code = |c: &str| scheme.codes.iter().find(|e| e["code"] == json!(c)).unwrap_or_else(|| panic!("{c}"));
    let gla = code("GLA1");
    assert_eq!((&gla["label"], &gla["use"], &gla["finished"]), (&json!("First Floor"), &json!("living"), &json!(true)));
    assert!(gla.get("area_factor").is_none());
    assert_eq!((&code("GAR")["use"], code("GAR").get("finished")), (&json!("garage"), None));
    assert_eq!((&code("BSMT")["use"], &code("BSMT")["finished"]), (&json!("storage"), &json!(false)));
    assert_eq!(code("PORCH")["use"], json!("porch"));
    assert_eq!(code("POOL")["use"], json!("other"));
    assert_eq!(code("NCA")["area_factor"], json!(0.0));
    // Spaces whose code is in the table copy its use and finished.
    let mut m = rich();
    m["pages"][0]["polygons"][0]["type"] = json!("GLA1");
    let doc = to_sketchy(&m, Some("d")).expect("to");
    let space = &level_of(&doc).spaces[0];
    assert_eq!((space.r#use.as_deref(), space.finished), (Some("living"), Some(true)));
    assert_eq!(from_sketchy(&doc).expect("from"), opened(&m, "d"));
}

#[test]
fn foreign_annotations_layers_and_schemes_survive() {
    let value = json!({
        "format": "sketchy", "format_version": "0.2", "id": "f", "length_unit": "ft",
        "structures": [{"id": "main", "levels": [level(json!([]), json!([{"id": "wall-a", "start": [0, 0], "end": [5, 0]}]))]}],
        "classification_schemes": [
            {"scheme": "org.example.codes", "codes": [{"code": "X"}]},
            {"scheme": "io.github.garmonbz.sketchrust.area-codes", "name": "Mine", "version": 2,
             "codes": [{"code": "GLA1", "label": "First", "use": "living", "finished": true, "grade": "above"}]}
        ],
        "definitions": [{"id": "def-1", "name": "Chair"}],
        "presentation": {
            "styles": {"s": {"stroke": "#000000"}},
            "layers": [{"id": "lay-1", "name": "One", "opacity": 0.5}],
            "annotations": [
                {"id": "note-1", "kind": "text", "level": "L1", "position": [1, 2], "text": "Hello", "align": "center"},
                {"id": "t-3", "kind": "text", "level": "L1", "position": [3, 4], "text": "Three"},
                {"id": "shape-1", "kind": "shape", "level": "L1", "shape": {"type": "circle", "center": [0, 0], "radius": 1}},
                {"id": "north", "kind": "north_arrow", "position": [0, 0]},
                {"id": "sym-x", "kind": "symbol", "level": "L1", "definition": "def-1", "position": [0, 0]},
                {"id": "dim-1", "kind": "dimension", "level": "L1", "target": {"wall": "wall-a"},
                 "points": [[0, 0], [2, 0], [5, 0]], "offset": 1, "show_total": true},
                {"id": "sym-r", "kind": "symbol", "level": "L1", "ref": {"scheme": "org.x.syms", "name": "chair"},
                 "position": [1, 1], "mirror": "y"}
            ]
        }
    });
    let doc: Document = serde_json::from_value(value.clone()).expect("doc");
    let saved = from_sketchy(&doc).expect("from");
    let page = &saved["pages"][0];
    let ids: Vec<i64> = ["lines", "labels", "symbols", "dimensionChains"]
        .iter()
        .flat_map(|k| page[*k].as_array().expect("arr").iter().map(|e| e["id"].as_i64().expect("int id")))
        .collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 5, "ids collide: {ids:?}");
    assert_eq!(page["labels"].as_array().expect("labels").len(), 2);
    assert_eq!(page["labels"][1]["id"], json!(3), "canonical t-3 keeps its number");
    assert_eq!(saved["areaCodes"][0]["code"], json!("GLA1"));
    let again = to_sketchy(&saved, None).expect("to").to_value();
    let pres = &again["presentation"];
    let find = |id: &str| pres["annotations"].as_array().expect("annos").iter().find(|a| a["id"] == json!(id)).cloned();
    for id in ["note-1", "t-3", "shape-1", "north", "sym-x", "dim-1", "sym-r"] {
        let want = value["presentation"]["annotations"].as_array().expect("a").iter().find(|a| a["id"] == json!(id)).cloned();
        assert_eq!(find(id).map(|a| norm(&a)), want.map(|a| norm(&a)), "{id}");
    }
    assert_eq!(norm(&pres["layers"]), norm(&value["presentation"]["layers"]));
    assert_eq!(pres["styles"], value["presentation"]["styles"]);
    assert_eq!(again["definitions"], value["definitions"]);
    let schemes = again["classification_schemes"].as_array().expect("schemes");
    for want in value["classification_schemes"].as_array().expect("schemes") {
        assert!(schemes.iter().any(|s| norm(s) == norm(want)), "scheme {want}");
    }
}

// ---- fix round 1 (task 4) ----

fn foreign_with(annotations: Value, walls: Value) -> Document {
    let mut value = serde_json::to_value(foreign(json!([level(json!([]), walls)]))).expect("value");
    value["presentation"] = json!({ "annotations": annotations });
    serde_json::from_value(value).expect("doc")
}

fn anno_ids(doc: &Value) -> Vec<String> {
    doc["presentation"]["annotations"]
        .as_array()
        .expect("annos")
        .iter()
        .map(|a| a["id"].as_str().expect("id").to_string())
        .collect()
}

const WALL_A: fn() -> Value = || json!([{"id": "wall-a", "start": [0, 0], "end": [5, 0]}]);

#[test]
fn rotation_is_clockwise_in_app_counter_clockwise_in_sketchy() {
    // render.rs (symbols ~1363, text blocks ~1647) calls `frame.rotate(Radians(rotation))` on
    // an iced canvas frame translated to a SCREEN point; that frame has y down, so a positive
    // app rotation turns clockwise as seen. Sketchy rotation is counter-clockwise-positive.
    let mut m = rich();
    m["pages"][0]["labels"][0]["rotation"] = json!(30.0);
    let doc = to_sketchy(&m, Some("d")).expect("to");
    assert_eq!(annotation(&doc, "t-9")["rotation"], json!(-30.0));
    assert_eq!(annotation(&doc, "y-8")["rotation"], json!(-90.0));
    assert_eq!(from_sketchy(&doc).expect("from"), opened(&m, "d"));
    let mut value = doc.to_value();
    value["presentation"]["annotations"][0]["rotation"] = json!(45.0);
    let edited: Document = serde_json::from_value(value).expect("doc");
    assert_eq!(from_sketchy(&edited).expect("from")["pages"][0]["labels"][0]["rotation"], json!(-45.0));
}

#[test]
fn dimension_offset_follows_side() {
    // `compute_dimension_chain`'s normal `(-dir.y, dir.x)` is computed in world space: for a
    // wall running +x it is +y, the left of travel with +y up. `ChainTiers::solve` draws the
    // chain at `offset * side` along it, and Sketchy's positive offset is also left.
    let seed = Line::new(1, Pt::new(0.0, 0.0), Pt::new(10.0, 0.0));
    let chain = GeometryEngine::new().compute_dimension_chain(&[seed], &[], 1, 0.1, false, None).expect("chain");
    assert!(chain.normal.y > 0.99 && chain.normal.x.abs() < 1e-9, "normal is left of travel");
    let mut m = rich();
    m["pages"][0]["dimensionChains"][0]["side"] = json!(1);
    let doc = to_sketchy(&m, Some("d")).expect("to");
    assert_eq!(annotation(&doc, "d-3")["offset"], json!(3.0)); // drawn at +3 * normal = left
    assert_eq!(from_sketchy(&doc).expect("from"), opened(&m, "d"));
    assert_eq!(annotation(&to_sketchy(&rich(), Some("d")).expect("to"), "d-3")["offset"], json!(-3.0));
    let foreign_doc = foreign_with(
        json!([{"id": "dim", "kind": "dimension", "level": "L1", "target": {"wall": "wall-a"}, "offset": -2}]),
        WALL_A(),
    );
    let saved = from_sketchy(&foreign_doc).expect("from");
    let chain = &saved["pages"][0]["dimensionChains"][0];
    assert_eq!((&chain["offset"], &chain["side"]), (&json!(2.0), &json!(-1)));
    let again = to_sketchy(&saved, None).expect("to").to_value();
    assert_eq!(again["presentation"]["annotations"][0]["offset"], json!(-2.0));
}

#[test]
fn parked_annotation_ids_are_reserved() {
    let foreign_doc = foreign_with(
        json!([
            {"id": "t-3", "kind": "text", "position": [0, 0], "text": "site"},
            {"id": "d-2", "kind": "dimension", "level": "L1", "points": [[0, 0], [1, 0]]}
        ]),
        json!([{"id": "a1", "start": [0, 0], "end": [5, 0]}, {"id": "a2", "start": [0, 0], "end": [5, 0]},
               {"id": "a3", "start": [0, 0], "end": [5, 0]}]),
    );
    let saved = from_sketchy(&foreign_doc).expect("from");
    for line in saved["pages"][0]["lines"].as_array().expect("lines") {
        let id = line["id"].as_i64().expect("id");
        assert!(id != 2 && id != 3, "reused parked id {id}");
    }
    assert!(saved["nextId"].as_i64().expect("nextId") > 3);
}

#[test]
fn porch_deck_codes_set_use_and_enclosure() {
    let doc = to_sketchy(&rich(), Some("d")).expect("to");
    let scheme = doc.classification_schemes.iter().find(|s| s.scheme.ends_with(".area-codes")).expect("scheme");
    let row = |c: &str| scheme.codes.iter().find(|e| e["code"] == json!(c)).unwrap_or_else(|| panic!("{c}")).clone();
    for (code, usage, enclosure) in [
        ("PORCH", "porch", "covered"),
        ("OFP", "porch", "covered"),
        ("SFP", "porch", "screened"),
        ("EFP", "porch", "enclosed"),
        ("PAT", "patio", "open"),
        ("DK", "deck", "open"),
        ("BAL", "balcony", "open"),
    ] {
        let r = row(code);
        assert_eq!((&r["use"], &r["enclosure"]), (&json!(usage), &json!(enclosure)), "{code}");
    }
    assert!(row("GLA1").get("enclosure").is_none());
    let mut m = rich();
    m["pages"][0]["polygons"][0]["type"] = json!("SFP");
    let doc = to_sketchy(&m, Some("d")).expect("to");
    let space = &level_of(&doc).spaces[0];
    assert_eq!((space.r#use.as_deref(), space.enclosure.as_deref()), (Some("porch"), Some("screened")));
    assert_eq!(from_sketchy(&doc).expect("from"), opened(&m, "d"));
}

#[test]
fn foreign_annotation_order_survives() {
    let annos = json!([
        {"id": "n1", "kind": "text", "level": "L1", "position": [0, 0], "text": "one"},
        {"id": "shape", "kind": "shape", "level": "L1", "shape": {"type": "circle", "center": [0, 0], "radius": 1}},
        {"id": "dim", "kind": "dimension", "level": "L1", "target": {"wall": "wall-a"}, "offset": 1},
        {"id": "north", "kind": "north_arrow", "position": [0, 0]},
        {"id": "sym", "kind": "symbol", "level": "L1", "ref": {"scheme": "s", "name": "n"}, "position": [0, 0]},
        {"id": "n2", "kind": "text", "level": "L1", "position": [1, 1], "text": "two"}
    ]);
    let doc = foreign_with(annos, WALL_A());
    let saved = from_sketchy(&doc).expect("from");
    let again = to_sketchy(&saved, None).expect("to").to_value();
    assert_eq!(anno_ids(&again), ["n1", "shape", "dim", "north", "sym", "n2"]);
    let twice = to_sketchy(&from_sketchy(&serde_json::from_value(again.clone()).expect("doc")).expect("from"), None).expect("to");
    assert_eq!(anno_ids(&twice.to_value()), ["n1", "shape", "dim", "north", "sym", "n2"]);
}

#[test]
fn annotations_naming_missing_walls_are_parked() {
    let annos = json!([
        {"id": "dim", "kind": "dimension", "level": "L1", "target": {"wall": "nowall"}, "offset": 1},
        {"id": "sym", "kind": "symbol", "level": "L1", "ref": {"scheme": "s", "name": "n"}, "position": [0, 0],
         "host": {"wall": "nowall"}}
    ]);
    let saved = from_sketchy(&foreign_with(annos.clone(), WALL_A())).expect("from");
    let page = &saved["pages"][0];
    assert!(page.get("dimensionChains").is_none_or(|c| c == &json!([])));
    assert!(page.get("symbols").is_none_or(|c| c == &json!([])));
    let again = to_sketchy(&saved, None).expect("to").to_value();
    assert_eq!(anno_ids(&again), ["dim", "sym"]);
    assert_eq!(again["presentation"]["annotations"][0]["target"], json!({"wall": "nowall"}));
}

// ---- task 4c: display walls for outline-only spaces ----

const SINGLE_FAMILY: &str = include_str!("data/sketchy/single-family.sketchy.json");
const CONDO: &str = include_str!("data/sketchy/condo-unit.sketchy.json");

fn open(json: &str) -> Value {
    let doc: Document = serde_json::from_str(json).expect("doc");
    from_sketchy(&doc).expect("from")
}

/// Numbers rounded to 1e-9: bulge to bow to bulge is exact only to float precision.
fn rounded(v: &Value) -> Value {
    match v {
        Value::Array(a) => Value::Array(a.iter().map(rounded).collect()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), rounded(v))).collect()),
        Value::Number(n) => json!((n.as_f64().unwrap_or(0.0) * 1e9).round() / 1e9),
        other => other.clone(),
    }
}

fn point_pair(line: &Value) -> [[f64; 2]; 2] {
    let p = |k: &str| [line[k]["x"].as_f64().expect("x"), line[k]["y"].as_f64().expect("y")];
    [p("start"), p("end")]
}

fn same_segment(a: [[f64; 2]; 2], b: [[f64; 2]; 2]) -> bool {
    let eq = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6;
    (eq(a[0], b[0]) && eq(a[1], b[1])) || (eq(a[0], b[1]) && eq(a[1], b[0]))
}

#[test]
fn outline_only_spaces_get_display_walls() {
    let saved = open(SINGLE_FAMILY);
    let page = &saved["pages"][0];
    let lines = page["lines"].as_array().expect("lines");
    assert!(!lines.is_empty());
    for line in lines {
        assert_eq!(line["sketchy_display"], json!(true));
        assert_eq!(line["type"], json!("exterior"));
    }
    for poly in page["polygons"].as_array().expect("polygons") {
        let n = poly["points"].as_array().expect("points").len();
        let edges = poly["edges"].as_array().expect("edges");
        assert_eq!(edges.len(), n);
        for edge in edges {
            let id = &edge["lineId"];
            assert!(lines.iter().any(|l| &l["id"] == id), "edge line {id} resolves in {}", poly["id"]);
        }
    }
    // The garage's west edge (44,24)-(44,2) is one line; the living room's longer
    // east edge (44,0)-(44,28) is a different segment and is its own line.
    let west = [[44.0, 2.0], [44.0, 24.0]];
    assert_eq!(lines.iter().filter(|l| same_segment(point_pair(l), west)).count(), 1);
    // All ids are distinct across lines and polygons.
    let ids: Vec<i64> =
        lines.iter().chain(page["polygons"].as_array().expect("polys")).map(|e| e["id"].as_i64().expect("id")).collect();
    assert_eq!(ids.len(), ids.iter().collect::<std::collections::HashSet<_>>().len());
    assert!(saved["nextId"].as_i64().expect("nextId") > *ids.iter().max().expect("ids"));
}

#[test]
fn shared_segment_gets_one_display_wall() {
    let left = json!({"id": "l", "shape": {"type": "polygon", "points": [[0, 0], [10, 0], [10, 10], [0, 10]]}});
    let right = json!({"id": "r", "shape": {"type": "polygon", "points": [[10, 10], [10, 0], [20, 0], [20, 10]]}});
    let saved = open_doc(&foreign(json!([level(json!([left, right]), json!([]))])));
    let lines = saved["pages"][0]["lines"].as_array().expect("lines");
    assert_eq!(lines.len(), 7, "4 + 4 edges, one segment shared (either direction)");
    let shared = [[10.0, 0.0], [10.0, 10.0]];
    let hits: Vec<&Value> = lines.iter().filter(|l| same_segment(point_pair(l), shared)).collect();
    assert_eq!(hits.len(), 1);
    let polys = saved["pages"][0]["polygons"].as_array().expect("polys");
    assert!(polys.iter().all(|p| p["edges"].as_array().expect("edges").iter().any(|e| e["lineId"] == hits[0]["id"])));
}

fn open_doc(doc: &Document) -> Value {
    from_sketchy(doc).expect("from")
}

#[test]
fn subdivision_edges_are_interior() {
    let outline = square("outline", json!({}));
    let room = json!({"id": "room", "within": "outline",
                      "shape": {"type": "polygon", "points": [[0, 0], [5, 0], [5, 10], [0, 10]]}});
    let saved = open_doc(&foreign(json!([level(json!([outline, room]), json!([]))])));
    let lines = saved["pages"][0]["lines"].as_array().expect("lines");
    let kind = |seg: [[f64; 2]; 2]| lines.iter().find(|l| same_segment(point_pair(l), seg)).map(|l| l["type"].clone());
    assert_eq!(kind([[10.0, 0.0], [10.0, 10.0]]), Some(json!("exterior")));
    assert_eq!(kind([[0.0, 0.0], [0.0, 10.0]]), Some(json!("exterior")), "shared with the outline: first wins");
    assert_eq!(kind([[5.0, 0.0], [5.0, 10.0]]), Some(json!("interior")));
}

#[test]
fn display_wall_bow_comes_from_the_bulge() {
    let s = json!({"id": "s", "shape": {"type": "polygon", "points": [[0, 0], [10, 0], [10, 10], [0, 10]],
                                        "bulges": [0.5, 0, 0, 0]}});
    let saved = open_doc(&foreign(json!([level(json!([s]), json!([]))])));
    let line = saved["pages"][0]["lines"].as_array().expect("lines").iter()
        .find(|l| same_segment(point_pair(l), [[0.0, 0.0], [10.0, 0.0]])).expect("bottom edge").clone();
    let want = -(0.5_f64).atan() * 720.0 / std::f64::consts::PI;
    assert!((line["bowAngle"].as_f64().expect("bow") - want).abs() < 1e-9);
}

#[test]
fn display_walls_are_not_written_back() {
    let doc: Document = serde_json::from_str(SINGLE_FAMILY).expect("doc");
    let saved = from_sketchy(&doc).expect("from");
    let again = to_sketchy(&saved, None).expect("to");
    for (a, b) in doc.structures[0].levels.iter().zip(&again.structures[0].levels) {
        assert!(b.walls.is_empty(), "level {} gained walls", b.id);
        assert_eq!(a.spaces.len(), b.spaces.len());
        for (sa, sb) in a.spaces.iter().zip(&b.spaces) {
            assert_eq!(sa.id, sb.id);
            let shape = |s: &Space| rounded(&serde_json::to_value(&s.shape).expect("shape"));
            assert_eq!(shape(sa), shape(sb));
        }
    }
    // Opening and saving again keeps the display walls and their ids stable. The written
    // document has no `revision` (the store stamps one), so no parent revision is parked.
    let second = from_sketchy(&Document::from_json(&again.to_value().to_string()).expect("doc")).expect("from");
    let mut expected = saved.clone();
    expected["sketchy_other"].as_object_mut().expect("other").remove("parent_revision");
    assert_eq!(second, expected);
}

#[test]
fn referenced_display_wall_is_kept_as_a_wall() {
    let mut saved = open(SINGLE_FAMILY);
    let wall_id = saved["pages"][0]["lines"][0]["id"].clone();
    saved["pages"][0]["dimensionChains"] = json!([{"id": 9999, "seedLineId": wall_id, "offset": 2.0, "side": 1}]);
    let again = to_sketchy(&saved, None).expect("to");
    assert_eq!(again.structures[0].levels[0].walls.len(), 1);
    let back = from_sketchy(&again).expect("from");
    assert_eq!(back["pages"][0]["dimensionChains"].as_array().expect("chains").len(), 1);
}

#[test]
fn condo_walls_are_not_duplicated() {
    let doc: Document = serde_json::from_str(CONDO).expect("doc");
    let saved = from_sketchy(&doc).expect("from");
    let lines = saved["pages"][0]["lines"].as_array().expect("lines");
    let (real, display): (Vec<&Value>, Vec<&Value>) = lines.iter().partition(|l| l.get("sketchy_display").is_none());
    assert_eq!(real.len(), doc.structures[0].levels[0].walls.len());
    for d in &display {
        assert!(!real.iter().any(|r| same_segment(point_pair(r), point_pair(d))), "duplicate of a wall: {d}");
    }
    for (i, a) in display.iter().enumerate() {
        for b in &display[i + 1..] {
            assert!(!same_segment(point_pair(a), point_pair(b)), "duplicate display wall: {a}");
        }
    }
}

#[test]
fn app_authored_polygons_get_no_display_walls() {
    // rich.json has a polygon with uncovered edges the user chose to leave wall-less.
    let saved = from_sketchy(&to_sketchy(&rich(), Some("d")).expect("to")).expect("from");
    assert_eq!(saved["pages"][0]["lines"].as_array().expect("lines").len(), 1);
}

// ---- review fix wave A ----

const COMMERCIAL: &str = include_str!("data/sketchy/commercial-building.sketchy.json");
const MAX_ID: i64 = 9_007_199_254_740_992;

fn wall_at(id: &str, x: f64) -> Value {
    json!({"id": id, "start": [x, 0], "end": [x, 5]})
}

/// Every integer app id on every page, with `nextId`.
fn app_ids(saved: &Value) -> (Vec<i64>, Option<i64>) {
    let mut ids = Vec::new();
    for page in saved["pages"].as_array().expect("pages") {
        for key in ["lines", "polygons", "labels", "symbols", "dimensionChains"] {
            for item in page.get(key).and_then(Value::as_array).into_iter().flatten() {
                ids.push(item["id"].as_i64().expect("integer id"));
            }
        }
    }
    (ids, saved.get("nextId").and_then(Value::as_i64))
}

fn assert_ids_sound(saved: &Value) {
    let (ids, next) = app_ids(saved);
    let distinct: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(distinct.len(), ids.len(), "ids collide: {ids:?}");
    for id in &ids {
        assert!((1..MAX_ID).contains(id), "id {id} out of range");
    }
    if let Some(n) = next {
        assert!((1..=MAX_ID).contains(&n), "nextId {n} out of range");
        assert!(ids.iter().all(|id| *id < n), "nextId {n} not above {ids:?}");
    }
}

#[test]
fn duplicate_element_ids_are_rejected() {
    let doc = foreign(json!([level(json!([]), json!([wall_at("w-1", 0.0), wall_at("w-1", 5.0)]))]));
    let e = from_sketchy(&doc).expect_err("duplicate id");
    assert!(e.to_string().contains("duplicate id"), "{e}");
    let doc = foreign(json!([level(json!([square("x", json!({}))]), json!([wall_at("x", 0.0)]))]));
    assert!(from_sketchy(&doc).expect_err("dup across kinds").to_string().contains("duplicate id"));
}

#[test]
fn out_of_range_ids_get_fresh_in_range_ids() {
    let doc = foreign(json!([level(
        json!([]),
        json!([wall_at("w-9007199254740993", 0.0), wall_at("w-9223372036854775807", 1.0), wall_at("w-0", 2.0)])
    )]));
    let saved = from_sketchy(&doc).expect("from");
    assert_ids_sound(&saved);
    let again = to_sketchy(&saved, None).expect("to").to_value();
    let ids: Vec<&str> = again["structures"][0]["levels"][0]["walls"].as_array().expect("walls")
        .iter().map(|w| w["id"].as_str().expect("id")).collect();
    assert_eq!(ids, ["w-9007199254740993", "w-9223372036854775807", "w-0"]);
}

#[test]
fn huge_next_id_does_not_overflow() {
    let mut value = serde_json::to_value(foreign(json!([level(json!([]), json!([wall_at("odd", 0.0)]))]))).expect("value");
    value["extras"] = json!({ "io.github.garmonbz.sketchrust": {"nextId": i64::MAX} });
    let doc: Document = serde_json::from_value(value).expect("doc");
    let saved = from_sketchy(&doc).expect("from");
    assert_ids_sound(&saved);
    assert!(saved["nextId"].as_i64().is_some_and(|n| n <= MAX_ID));
}

#[test]
fn display_ids_skip_taken_ids() {
    let residual = json!({"points": [], "sketchy_display": true,
                          "edges": [{"lineId": 4}, {"lineId": null}, {"lineId": null}, {"lineId": null}]});
    let space = square("a-1", json!({"extras": {"io.github.garmonbz.sketchrust": residual}}));
    let saved = from_sketchy(&foreign(json!([level(json!([space]), json!([]))]))).expect("from");
    let lines = saved["pages"][0]["lines"].as_array().expect("lines");
    let mut ids: Vec<i64> = lines.iter().map(|l| l["id"].as_i64().expect("id")).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 4, "display ids collide: {lines:?}");
    assert_ids_sound(&saved);
    assert!(saved["nextId"].as_i64().expect("nextId") > *ids.iter().max().expect("max"));
}

#[test]
fn document_identity_round_trips() {
    for text in [SINGLE_FAMILY, CONDO, COMMERCIAL] {
        let doc = Document::from_json(text).expect("doc");
        let saved = from_sketchy(&doc).expect("from");
        assert_eq!(to_sketchy(&saved, None).expect("to").id, doc.id);
        assert_eq!(parent_revision(&saved), doc.revision.as_ref().map(|r| r.id.clone()));
        assert_eq!(to_sketchy(&saved, Some("other")).expect("to").id, "other");
    }
    let e = to_sketchy(&rich(), None).expect_err("no id");
    assert!(e.to_string().contains("document id required"), "{e}");
    assert_eq!(parent_revision(&rich()), None);
}

/// Drops every `extras[NS]` key (and an `extras` left empty), `NS` in `extensions_used`
/// and the root keys the writer regenerates (`revision`, `history`, `generator`).
fn strip_ns(v: &Value) -> Value {
    fn walk(v: &Value) -> Value {
        match v {
            Value::Object(m) => {
                let mut out = serde_json::Map::new();
                for (k, v) in m {
                    let mut v = walk(v);
                    if k == "extras" {
                        if let Some(e) = v.as_object_mut() {
                            e.remove(sketch_core::sketchy::convert::NS);
                            if e.is_empty() {
                                continue;
                            }
                        }
                    }
                    out.insert(k.clone(), v);
                }
                Value::Object(out)
            }
            Value::Array(a) => Value::Array(a.iter().map(walk).collect()),
            other => other.clone(),
        }
    }
    let mut v = rounded(&walk(v));
    if let Some(root) = v.as_object_mut() {
        for key in ["revision", "history", "generator"] {
            root.remove(key);
        }
        if let Some(Value::Array(used)) = root.get_mut("extensions_used") {
            used.retain(|e| e != sketch_core::sketchy::convert::NS);
            if used.is_empty() {
                root.remove("extensions_used");
            }
        }
    }
    v
}

#[test]
fn fixtures_round_trip_in_order() {
    for text in [SINGLE_FAMILY, CONDO, COMMERCIAL] {
        let input: Value = serde_json::from_str(text).expect("value");
        let doc = Document::from_json(text).expect("doc");
        let once = to_sketchy(&from_sketchy(&doc).expect("from"), None).expect("to").to_value();
        assert_eq!(strip_ns(&once), strip_ns(&input), "{}", input["id"]);
        let doc2 = Document::from_json(&once.to_string()).expect("doc");
        let twice = to_sketchy(&from_sketchy(&doc2).expect("from"), None).expect("to").to_value();
        assert_eq!(twice, once, "second open/save differs for {}", input["id"]);
    }
}

#[test]
fn order_sensitive_arrays_keep_their_order() {
    let mut value = serde_json::to_value(foreign(json!([level(
        json!([
            {"id": "p", "shape": {"type": "polyline", "points": [[0, 0], [5, 0]]}},
            square("a-2", json!({"classifications": [
                {"scheme": "org.x", "code": "A"},
                {"scheme": "io.github.garmonbz.sketchrust.area-codes", "code": "GLA1"},
                {"scheme": "org.y", "code": "B"}
            ]})),
            {"id": "r", "shape": {"type": "arc", "center": [0, 0], "radius": 4, "start_angle": 0, "end_angle": 90}},
            square("a-3", json!({}))
        ]),
        json!([])
    )])))
    .expect("value");
    value["extensions_used"] = json!(["org.a", "io.github.garmonbz.sketchrust", "org.b"]);
    value["classification_schemes"] = json!([
        {"scheme": "org.x", "codes": []},
        {"scheme": "io.github.garmonbz.sketchrust.area-codes", "codes": [{"code": "GLA1", "use": "living"}]},
        {"scheme": "org.y", "codes": []}
    ]);
    let doc: Document = serde_json::from_value(value.clone()).expect("doc");
    let again = to_sketchy(&from_sketchy(&doc).expect("from"), None).expect("to").to_value();
    let spaces = &again["structures"][0]["levels"][0]["spaces"];
    let ids: Vec<&str> = spaces.as_array().expect("spaces").iter().map(|s| s["id"].as_str().expect("id")).collect();
    assert_eq!(ids, ["p", "a-2", "r", "a-3"]);
    let schemes: Vec<&str> = spaces[1]["classifications"].as_array().expect("cls").iter().map(|c| c["scheme"].as_str().expect("s")).collect();
    assert_eq!(schemes, ["org.x", "io.github.garmonbz.sketchrust.area-codes", "org.y"]);
    assert_eq!(again["extensions_used"], value["extensions_used"]);
    let names: Vec<&str> = again["classification_schemes"].as_array().expect("schemes").iter().map(|s| s["scheme"].as_str().expect("s")).collect();
    assert_eq!(names, ["org.x", "io.github.garmonbz.sketchrust.area-codes", "org.y"]);
}

#[test]
fn dangling_references_do_not_alias_other_elements() {
    let doc = foreign_with(
        json!([{"id": "y", "kind": "symbol", "level": "L1", "ref": {"scheme": "s", "name": "n"}, "position": [0, 0]}]),
        json!([wall_at("w-7", 0.0)]),
    );
    let mut value = doc.to_value();
    value["structures"][0]["levels"][0]["spaces"] =
        json!([square("a-1", json!({})), square("a-2", json!({"within": "a-7", "subtract": true}))]);
    let doc: Document = serde_json::from_value(value).expect("doc");
    let saved = from_sketchy(&doc).expect("from");
    let polys = saved["pages"][0]["polygons"].as_array().expect("polys");
    let hole = polys.iter().find(|p| p["id"] == json!(2)).expect("a-2");
    assert_ne!(hole.get("subtractTargetId"), Some(&json!(7)), "dangling within aliased wall 7: {hole}");
    assert_ne!(hole.get("subtractMode"), Some(&json!("always")));
    let again = to_sketchy(&saved, None).expect("to").to_value();
    let space = again["structures"][0]["levels"][0]["spaces"].as_array().expect("spaces").iter()
        .find(|s| s["id"] == "a-2").expect("a-2").clone();
    assert_eq!((&space["within"], &space["subtract"]), (&json!("a-7"), &json!(true)));
}

#[test]
fn non_finite_results_are_errors() {
    let mut value = serde_json::to_value(foreign(json!([level(json!([]), json!([{"id": "w-1", "start": [0, 0], "end": [1e308, 0]}]))]))).expect("value");
    value["length_unit"] = json!("m");
    let doc: Document = serde_json::from_value(value).expect("doc");
    assert!(matches!(from_sketchy(&doc), Err(sketch_core::sketchy::SketchyError::Convert(_))));
    let mut m = rich();
    m["sketchy_other"] = json!({"length_unit": "mm", "document_id": "d"});
    m["pages"][0]["lines"][0]["thickness"] = json!(1e308);
    // thickness / 12 / (ft per mm) overflows to infinity.
    assert!(matches!(to_sketchy(&m, None), Err(sketch_core::sketchy::SketchyError::Convert(_))));
}

#[test]
fn unshown_content_is_summarised() {
    let commercial = Document::from_json(COMMERCIAL).expect("doc");
    assert_eq!(
        unshown_summary(&commercial),
        [("openings".to_string(), 30), ("features".to_string(), 38), ("voids".to_string(), 2), ("annotations".to_string(), 9)]
    );
    let condo = Document::from_json(CONDO).expect("doc");
    assert_eq!(
        unshown_summary(&condo),
        [("openings".to_string(), 13), ("features".to_string(), 13), ("annotations".to_string(), 11)]
    );
    let family = Document::from_json(SINGLE_FAMILY).expect("doc");
    assert_eq!(unshown_summary(&family)[0], ("structure".to_string(), 1));
    let open = foreign(json!([level(json!([{"id": "p", "shape": {"type": "polyline", "points": [[0, 0], [5, 0]]}}]), json!([]))]));
    assert_eq!(unshown_summary(&open), [("non-area space".to_string(), 1)]);
    assert!(unshown_summary(&to_sketchy(&rich(), Some("d")).expect("to")).is_empty());
}

#[test]
fn malformed_input_errors_without_panicking() {
    use sketch_core::sketchy::SketchyError;
    let bad_models = [
        ("string x", json!({"pages": [{"lines": [{"id": 1, "start": {"x": "0", "y": 0}, "end": {"x": 1, "y": 0}}]}]})),
        ("line without id", json!({"pages": [{"lines": [{"start": {"x": 0, "y": 0}, "end": {"x": 1, "y": 0}}]}]})),
        ("pages not an array", json!({"pages": {"a": 1}})),
        ("page lines not an array", json!({"pages": [{"lines": "x"}]})),
        ("model not an object", json!([1, 2])),
    ];
    for (what, model) in bad_models {
        assert!(matches!(to_sketchy(&model, Some("d")), Err(SketchyError::Convert(_))), "{what}");
    }
    // A non-object extras[NS] is ignored, on the root and on an element.
    let mut value = serde_json::to_value(foreign(json!([level(json!([square("a-1", json!({"extras": {"io.github.garmonbz.sketchrust": 5}}))]), json!([]))]))).expect("value");
    value["extras"] = json!({"io.github.garmonbz.sketchrust": "x"});
    let doc: Document = serde_json::from_value(value).expect("doc");
    let saved = from_sketchy(&doc).expect("from");
    assert_eq!(saved["pages"][0]["polygons"][0]["id"], json!(1));
}

/// The `use` the first space gets when its code is `UND` and the code table row is `row`.
fn und_use(row: Value) -> Option<String> {
    let mut m = rich();
    m["areaCodes"] = json!([row]);
    m["pages"][0]["polygons"][0]["type"] = json!("UND");
    let doc = to_sketchy(&m, Some("doc-1")).expect("to");
    level_of(&doc).spaces[0].r#use.clone()
}

#[test]
fn undefined_code_says_nothing_about_use_unless_it_counts_as_living() {
    assert_eq!(und_use(json!({"code": "UND", "name": "Undefined"})), None);
    assert_eq!(
        und_use(json!({"code": "UND", "name": "Undefined", "includeInGLA": true})).as_deref(),
        Some("living")
    );
}

// ---- task 10: Sketchy draft 0.2 ----

const SINGLE_FAMILY_0_1: &str = include_str!("data/sketchy/single-family-0.1.sketchy.json");

fn bow(line: &Value) -> f64 {
    line["bowAngle"].as_f64().unwrap_or(0.0)
}

fn lvl(v: &Value) -> &Value {
    &v["structures"][0]["levels"][0]
}

#[test]
fn writes_format_version_0_2() {
    assert_eq!(sketch_core::sketchy::FORMAT_VERSION, "0.2");
    let doc = to_sketchy(&rich(), Some("d")).expect("to");
    assert_eq!(doc.format_version, "0.2");
    // A 0.1 file saved again is a 0.2 file, with the 0.2 schema URL.
    let saved = from_sketchy(&Document::from_json(SINGLE_FAMILY_0_1).expect("doc")).expect("from");
    let again = to_sketchy(&saved, None).expect("to").to_value();
    assert_eq!(again["format_version"], json!("0.2"));
    assert_eq!(again["$schema"], json!("https://property-hackers.github.io/sketchy/schema/0.2/sketchy.schema.json"));
}

#[test]
fn reads_0_1_and_0_2() {
    for text in [SINGLE_FAMILY_0_1, SINGLE_FAMILY, CONDO, COMMERCIAL] {
        let bytes = text.as_bytes();
        let doc = sketch_core::sketchy::read(bytes).expect("read").expect("sketchy");
        from_sketchy(&doc).expect("converts");
    }
}

#[test]
fn rejects_other_0x_versions() {
    use sketch_core::sketchy::SketchyError;
    for version in ["0.0", "0.3", "0.7", "1.0", "2.1"] {
        let mut v: Value = serde_json::from_str(SINGLE_FAMILY).expect("value");
        v["format_version"] = json!(version);
        let text = v.to_string();
        assert!(matches!(Document::from_json(&text), Err(SketchyError::UnsupportedVersion(_))), "{version}");
        assert!(matches!(sketch_core::sketchy::read(text.as_bytes()), Err(SketchyError::UnsupportedVersion(_))), "{version}");
    }
    for version in ["0.1", "0.2"] {
        let mut v: Value = serde_json::from_str(SINGLE_FAMILY).expect("value");
        v["format_version"] = json!(version);
        assert!(Document::from_json(&v.to_string()).is_ok(), "{version}");
    }
}

#[test]
fn reads_0_1_bulges_with_flipped_sign() {
    // 0.1: a positive bulge bowed LEFT of travel, which is the app's positive bow.
    let shape = json!({"type": "polygon", "points": [[0, 0], [10, 0], [10, 10], [0, 10]], "bulges": [0.5, 0, 0, 0]});
    let doc = foreign_v(
        "0.1",
        json!([level(
            json!([{"id": "s", "shape": shape}]),
            json!([{"id": "w", "start": [0, 0], "end": [10, 0], "bulge": 0.25}])
        )]),
    );
    let saved = from_sketchy(&doc).expect("from");
    let want = |b: f64| b.atan() * 720.0 / std::f64::consts::PI;
    let page = &saved["pages"][0];
    let wall = page["lines"].as_array().expect("lines").iter().find(|l| l["sketchy_display"].is_null()).expect("wall");
    assert!((bow(wall) - want(0.25)).abs() < 1e-9, "wall bow {}", bow(wall));
    let edges = page["polygons"][0]["edges"].as_array().expect("edges");
    assert!((bow(&edges[0]) - want(0.5)).abs() < 1e-9);
    // Written back as 0.2 the same arc has the opposite sign.
    let again = to_sketchy(&saved, None).expect("to").to_value();
    assert!((again["structures"][0]["levels"][0]["walls"][0]["bulge"].as_f64().expect("bulge") + 0.25).abs() < 1e-9);
    let b0 = again["structures"][0]["levels"][0]["spaces"][0]["shape"]["bulges"][0].as_f64().expect("b0");
    assert!((b0 + 0.5).abs() < 1e-9);
    // Hole rings and polylines flip too.
    let hole = json!({"type": "polygon", "points": [[0, 0], [10, 0], [10, 10], [0, 10]],
                      "holes": [{"points": [[2, 2], [4, 2], [4, 4], [2, 4]], "bulges": [0.3, 0, 0, 0]}]});
    let poly = json!({"type": "polyline", "points": [[0, 0], [5, 0]], "bulges": [0.7]});
    let doc = foreign_v("0.1", json!([level(json!([{"id": "h", "shape": hole}, {"id": "p", "shape": poly}]), json!([]))]));
    let again = to_sketchy(&from_sketchy(&doc).expect("from"), None).expect("to").to_value();
    let spaces = &lvl(&again)["spaces"];
    assert_eq!(spaces[0]["shape"]["holes"][0]["bulges"], json!([-0.3, 0, 0, 0]));
    assert_eq!(spaces[1]["shape"]["bulges"], json!([-0.7]));
}

#[test]
fn ceiling_height_0_1_becomes_ceiling_object() {
    let mut value = serde_json::to_value(foreign_v("0.1", json!([level(json!([square("s", json!({"ceiling_height": 9, "ceiling_sloped": true}))]), json!([]))])))
        .expect("value");
    value["structures"][0]["levels"][0]["ceiling_height"] = json!(8);
    value["structures"][0]["levels"][0]["features"] =
        json!([{"id": "st", "kind": "stair", "connects_to": "L2", "counted_on": "L1", "finished": true}]);
    let doc: Document = serde_json::from_value(value).expect("doc");
    let again = to_sketchy(&from_sketchy(&doc).expect("from"), None).expect("to").to_value();
    let level = lvl(&again);
    assert_eq!(level["ceiling"], json!({"height": 8}));
    assert!(level.get("ceiling_height").is_none());
    let space = &level["spaces"][0];
    assert_eq!(space["ceiling"], json!({"height": 9}));
    assert!(space.get("ceiling_height").is_none() && space.get("ceiling_sloped").is_none());
    let stair = &level["features"][0];
    assert!(stair.get("counted_on").is_none(), "counted_on is gone in 0.2");
    assert_eq!((&stair["connects_to"], &stair["finished"]), (&json!("L2"), &json!(true)));
    assert_eq!(stair["extras"]["io.github.garmonbz.sketchrust"]["sketchy_0_1"], json!({"counted_on": "L1"}));
    // The dropped values stay in the residual instead of vanishing.
    assert_eq!(space["extras"]["io.github.garmonbz.sketchrust"]["sketchy_0_1"], json!({"ceiling_sloped": true}));
    // A second open/save is stable.
    let doc2 = Document::from_json(&again.to_string()).expect("doc");
    assert_eq!(to_sketchy(&from_sketchy(&doc2).expect("from"), None).expect("to").to_value(), again);
}

#[test]
fn ceilings_stairs_and_openings_of_0_2_are_preserved() {
    let ceiling = json!({"height": 8, "zones": [{"shape": {"type": "polygon", "points": [[0, 0], [5, 0], [5, 5]]}, "height": 6}]});
    let mut value = serde_json::to_value(foreign(json!([level(json!([square("s", json!({"ceiling": ceiling}))]), json!([]))]))).expect("value");
    let l = &mut value["structures"][0]["levels"][0];
    l["ceiling"] = json!({"height": 8});
    l["features"] = json!([{"id": "st", "kind": "stair", "well": "v1", "stair": {"configuration": "u_shaped", "direction": "up", "risers": 16,
        "landings": [{"shape": {"type": "polygon", "points": [[0, 0], [2, 0], [2, 2]]}}]}},
        {"id": "col", "kind": "column", "position": [3, 3], "shape": {"type": "circle", "center": [0, 0], "radius": 0.5}}]);
    l["voids"] = json!([{"id": "v1", "shape": {"type": "polygon", "points": [[1, 1], [2, 1], [2, 2]]}}]);
    l["openings"] = json!([
        {"id": "o1", "kind": "door", "wall": "w", "swing": "unknown", "swing_into": "s"},
        {"id": "o2", "kind": "door", "space": "s", "edge": 1, "hole": 0, "offset": 1, "width": 3},
        {"id": "o3", "kind": "skylight", "space": "s", "shape": {"type": "polygon", "points": [[1, 1], [2, 1], [2, 2]]}}
    ]);
    value["structures"][0]["levels"][0]["walls"] = json!([{"id": "w", "start": [0, 0], "end": [10, 0], "kind": "railing"}]);
    let doc: Document = serde_json::from_value(value.clone()).expect("doc");
    let again = to_sketchy(&from_sketchy(&doc).expect("from"), None).expect("to").to_value();
    let (a, v) = (lvl(&again), lvl(&value));
    for key in ["ceiling", "features", "voids", "openings"] {
        assert_eq!(a[key], v[key], "{key}");
    }
    assert_eq!(a["spaces"][0]["ceiling"], v["spaces"][0]["ceiling"]);
    assert_eq!(a["walls"][0]["kind"], json!("railing"));
}

fn edged_square() -> Value {
    json!({"id": "s", "shape": {"type": "polygon", "points": [[0, 0], [10, 0], [10, 10], [0, 10]],
        "edges": ["physical", "virtual", "physical", "physical"],
        "edge_measured_to": [null, null, "centerline", null],
        "holes": [{"points": [[2, 2], [2, 4], [4, 4], [4, 2]], "bulges": [0, 0.25, 0, 0],
                   "edges": ["virtual", "physical", "physical", "physical"],
                   "edge_measured_to": ["centerline", null, null, null]}]}})
}

#[test]
fn edge_kinds_survive_round_trip() {
    let doc = foreign(json!([level(json!([edged_square()]), json!([]))]));
    let saved = from_sketchy(&doc).expect("from");
    // Display walls exist for every wall-less edge, virtual ones included, and are not written back.
    assert_eq!(saved["pages"][0]["lines"].as_array().expect("lines").len(), 4);
    let again = to_sketchy(&saved, None).expect("to").to_value();
    let level = lvl(&again);
    assert!(level.get("walls").is_none_or(|w| w == &json!([])), "display walls are never written");
    assert_eq!(level["spaces"][0]["shape"], edged_square()["shape"]);
    // The converter never reverses a ring: the points keep their order and orientation.
    let Shape::Polygon { points, .. } = &Document::from_json(&again.to_string()).expect("doc").structures[0].levels[0].spaces[0].shape.clone()
    else {
        panic!("polygon")
    };
    assert_eq!(points, &vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]);
    // A second open/save of the written document changes nothing.
    let second = Document::from_json(&again.to_string()).expect("doc");
    assert_eq!(to_sketchy(&from_sketchy(&second).expect("from"), None).expect("to").to_value(), again);
}

#[test]
fn per_edge_arrays_dropped_when_points_change() {
    let doc = foreign(json!([level(json!([edged_square()]), json!([]))]));
    let mut saved = from_sketchy(&doc).expect("from");
    // Moving a vertex keeps the arrays aligned.
    saved["pages"][0]["polygons"][0]["points"][2]["x"] = json!(12.0);
    let moved = to_sketchy(&saved, None).expect("to").to_value();
    assert_eq!(lvl(&moved)["spaces"][0]["shape"]["edges"], json!(["physical", "virtual", "physical", "physical"]));
    // Adding a vertex does not: the per-edge arrays are dropped rather than misaligned.
    let mut saved = from_sketchy(&doc).expect("from");
    let pts = saved["pages"][0]["polygons"][0]["points"].as_array_mut().expect("pts");
    pts.insert(1, json!({"x": 5.0, "y": 0.0}));
    let grown = to_sketchy(&saved, None).expect("to").to_value();
    let shape = &lvl(&grown)["spaces"][0]["shape"];
    assert_eq!(shape["points"].as_array().expect("pts").len(), 5);
    assert!(shape.get("edges").is_none() && shape.get("edge_measured_to").is_none(), "{shape}");
    // Hole rings keep their own arrays.
    assert!(shape["holes"][0]["edges"].is_array());
}

fn circle_doc(version: &str) -> Document {
    let circle = json!({"id": "c", "shape": {"type": "circle", "center": [10, 5], "radius": 4}});
    foreign_v(version, json!([level(json!([circle]), json!([]))]))
}

#[test]
fn circle_space_opens_as_four_exact_quarter_arcs() {
    let saved = from_sketchy(&circle_doc("0.2")).expect("from");
    let poly = &saved["pages"][0]["polygons"][0];
    let pts = poly["points"].as_array().expect("pts");
    assert_eq!(pts.len(), 4, "no 48-gon");
    let want = [(14.0, 5.0), (10.0, 9.0), (6.0, 5.0), (10.0, 1.0)];
    for (p, (x, y)) in pts.iter().zip(want) {
        assert_eq!((p["x"].as_f64(), p["y"].as_f64()), (Some(x), Some(y)));
    }
    let edges = poly["edges"].as_array().expect("edges");
    assert_eq!(edges.len(), 4);
    // Counter-clockwise ring: the app's negative bow bows right of travel, away from the centre.
    for e in edges {
        assert!((bow(e) + 90.0).abs() < 1e-9, "each edge is a 90 degree outward arc: {e}");
    }
    // Every arc midpoint, from the app's own arc code, lies on the circle.
    let app_pts: Vec<Pt> = pts.iter().map(|p| Pt::new(p["x"].as_f64().expect("x"), p["y"].as_f64().expect("y"))).collect();
    let app_edges: Vec<Edge> = edges.iter().map(|e| Edge { line_id: None, bow_angle: bow(e) }).collect();
    let engine = GeometryEngine::new();
    let ring = engine.flatten_ring(&app_pts, &app_edges, 0.0005);
    for p in &ring {
        assert!(((p.x - 10.0).hypot(p.y - 5.0) - 4.0).abs() < 1e-3, "{p:?} is on the circle");
    }
    for k in 0..4 {
        let single = engine.flatten_ring(&[app_pts[k], app_pts[(k + 1) % 4]], &[app_edges[k], Edge::default()], 0.0005);
        let mid = single.iter().map(|p| ((p.x - 10.0).hypot(p.y - 5.0) - 4.0).abs()).fold(0.0, f64::max);
        assert!(mid < 1e-3);
    }
    // The bulge is the spec's: a CCW quarter arc bowing outward is positive (SPEC 0.2 4.2).
    let mut moved = saved.clone();
    moved["pages"][0]["polygons"][0]["points"][0]["x"] = json!(15.0);
    let again = to_sketchy(&moved, None).expect("to").to_value();
    assert!((again["structures"][0]["levels"][0]["spaces"][0]["shape"]["bulges"][0].as_f64().expect("b") - (std::f64::consts::PI / 8.0).tan()).abs() < 1e-9);
    // Display walls get the same bows, one per edge.
    let lines = saved["pages"][0]["lines"].as_array().expect("lines");
    assert_eq!(lines.len(), 4);
    for (i, l) in lines.iter().enumerate() {
        assert!((bow(l) + 90.0).abs() < 1e-9, "{l}");
        assert_eq!(edges[i]["lineId"], l["id"]);
    }
    let old = from_sketchy(&circle_doc("0.1")).expect("from");
    assert_eq!(old["pages"][0]["polygons"][0]["points"].as_array().expect("pts").len(), 4);
}

#[test]
fn circle_area_matches_pi_r_squared() {
    let saved = from_sketchy(&circle_doc("0.2")).expect("from");
    let poly = &saved["pages"][0]["polygons"][0];
    let points: Vec<Pt> =
        poly["points"].as_array().expect("pts").iter().map(|p| Pt::new(p["x"].as_f64().expect("x"), p["y"].as_f64().expect("y"))).collect();
    let edges: Vec<Edge> = poly["edges"].as_array().expect("edges").iter().map(|e| Edge { line_id: None, bow_angle: bow(e) }).collect();
    // The app's area path (`area_calc::CalcPoly` calls this for a polygon of 3+ points).
    let area = GeometryEngine::new().calculate_area(&points, &edges);
    let exact = std::f64::consts::PI * 16.0;
    assert!(((area - exact) / exact).abs() < 1e-6, "{area} vs {exact}");
}

#[test]
fn circle_written_back_when_unchanged() {
    let saved = from_sketchy(&circle_doc("0.2")).expect("from");
    let shape = json!({"type": "circle", "center": [10, 5], "radius": 4});
    let again = to_sketchy(&saved, None).expect("to").to_value();
    assert_eq!(lvl(&again)["spaces"][0]["shape"], shape);
    // Moving a point, or changing a bow, writes the polygon with bulges instead.
    let mut moved = saved.clone();
    moved["pages"][0]["polygons"][0]["points"][0]["x"] = json!(15.0);
    let again = to_sketchy(&moved, None).expect("to").to_value();
    let s = &lvl(&again)["spaces"][0]["shape"];
    assert_eq!(s["type"], json!("polygon"));
    assert_eq!(s["bulges"].as_array().expect("bulges").len(), 4);
    let mut bowed = saved.clone();
    bowed["pages"][0]["polygons"][0]["edges"][1]["bowAngle"] = json!(-45.0);
    let again = to_sketchy(&bowed, None).expect("to").to_value();
    let s = &lvl(&again)["spaces"][0]["shape"];
    assert_eq!(s["type"], json!("polygon"));
    let b = s["bulges"].as_array().expect("bulges");
    assert!((b[0].as_f64().expect("b0") - (std::f64::consts::PI / 8.0).tan()).abs() < 1e-9);
    assert!((b[1].as_f64().expect("b1") - (45.0_f64 * std::f64::consts::PI / 720.0).tan()).abs() < 1e-9);
    // An ellipse keeps its 48-gon approximation and comes back unchanged.
    let ellipse = json!({"id": "e", "shape": {"type": "ellipse", "center": [0, 0], "radius_x": 6, "radius_y": 3, "rotation": 30}});
    let saved = from_sketchy(&foreign(json!([level(json!([ellipse.clone()]), json!([]))]))).expect("from");
    assert_eq!(saved["pages"][0]["polygons"][0]["points"].as_array().expect("pts").len(), 48);
    let again = to_sketchy(&saved, None).expect("to").to_value();
    assert_eq!(lvl(&again)["spaces"][0]["shape"], ellipse["shape"]);
}

#[test]
fn oversized_bulge_is_not_silently_clamped() {
    // |bulge| > tan(359 deg / 4) is a sweep beyond the app's limit.
    let limit = (359.0_f64 * std::f64::consts::PI / 720.0).tan();
    let (big, bigger) = (limit * 4.0, -limit * 2.0);
    let shape = json!({"type": "polygon", "points": [[0, 0], [10, 0], [10, 10], [0, 10]], "bulges": [big, 0.2, 0, bigger]});
    let wall = json!({"id": "w", "start": [0, 0], "end": [10, 0], "bulge": big});
    let doc = foreign(json!([level(json!([{"id": "s", "shape": shape.clone()}]), json!([wall]))]));
    let saved = from_sketchy(&doc).expect("from");
    let page = &saved["pages"][0];
    let w = page["lines"].as_array().expect("lines").iter().find(|l| l["sketchy_display"].is_null()).expect("wall");
    assert!((bow(w) + 359.0).abs() < 1e-9, "the app shows the nearest arc it can: {}", bow(w));
    let edges = page["polygons"][0]["edges"].as_array().expect("edges");
    assert!((bow(&edges[0]) + 359.0).abs() < 1e-9 && (bow(&edges[3]) - 359.0).abs() < 1e-9);
    // Unchanged on save: the originals come back exactly.
    let again = to_sketchy(&saved, None).expect("to").to_value();
    assert_eq!(lvl(&again)["walls"][0]["bulge"], json!(big));
    let got = lvl(&again)["spaces"][0]["shape"]["bulges"].as_array().expect("bulges").clone();
    for (g, want) in got.iter().zip([big, 0.2, 0.0, bigger]) {
        assert!((g.as_f64().expect("b") - want).abs() < 1e-9, "{g} vs {want}");
    }
    // Editing the arc in the app wins over the parked original.
    let mut edited = saved.clone();
    for l in edited["pages"][0]["lines"].as_array_mut().expect("lines") {
        if l["sketchy_display"].is_null() {
            l["bowAngle"] = json!(-90.0);
        }
    }
    edited["pages"][0]["polygons"][0]["edges"][0]["bowAngle"] = json!(45.0);
    let again = to_sketchy(&edited, None).expect("to").to_value();
    let w = lvl(&again)["walls"][0]["bulge"].as_f64().expect("bulge");
    assert!((w - (90.0_f64 * std::f64::consts::PI / 720.0).tan()).abs() < 1e-9);
    let b = lvl(&again)["spaces"][0]["shape"]["bulges"].as_array().expect("bulges").clone();
    assert!((b[0].as_f64().expect("b0") + (45.0_f64 * std::f64::consts::PI / 720.0).tan()).abs() < 1e-9);
    // A sweep exactly at the limit is representable and not parked.
    let at = json!({"id": "w", "start": [0, 0], "end": [10, 0], "bulge": limit});
    let saved = from_sketchy(&foreign(json!([level(json!([]), json!([at]))]))).expect("from");
    assert!((bow(&saved["pages"][0]["lines"][0]) + 359.0).abs() < 1e-6);
    assert!(saved["pages"][0]["lines"][0].get("sketchy_other").is_none());
}

#[test]
fn height_mode_arc_height_matches_bulge() {
    // The curve tool's Height mode: arcHeight is the signed rise at the chord midpoint
    // (positive is left of travel); the sweep is 4 * atan2(2h, chord), so |bulge| = 2h / chord.
    let chord = 10.0_f64;
    for height in [1.0_f64, -2.5, 0.3] {
        let bow = 4.0 * (2.0 * height).atan2(chord).to_degrees();
        let line = json!({"id": 9, "start": {"x": 0.0, "y": 0.0}, "end": {"x": chord, "y": 0.0},
                          "bowAngle": bow, "arcHeight": height});
        let doc = to_sketchy(&model_with(json!([line]), json!([])), Some("d")).expect("to");
        let bulge = level_of(&doc).walls[0].bulge.expect("bulge");
        assert!((bulge.abs() * chord / 2.0 - height.abs()).abs() < 1e-9, "|bulge| * chord / 2 is the rise");
        assert!((spec_apex_y(bulge) - height).abs() < 1e-9, "the arc's apex sits at the rise, on the same side");
        // Opening it again gives the same bow, and an arcHeight the app agrees with.
        let back = from_sketchy(&doc).expect("from");
        let l = &back["pages"][0]["lines"][0];
        assert!((bow_of(l) - bow).abs() < 1e-9);
        assert!((l["arcHeight"].as_f64().expect("arcHeight") - height).abs() < 1e-9);
    }
    // A bulge edited in Sketchy arrives with a matching arcHeight, not a stale one.
    let mut m = rich();
    m["pages"][0]["lines"][0]["bowAngle"] = json!(30.0);
    m["pages"][0]["lines"][0]["arcHeight"] = json!(1.25);
    let mut doc = to_sketchy(&m, Some("d")).expect("to");
    doc.structures[0].levels[0].walls.iter_mut().find(|w| w.id == "w-5").expect("w-5").bulge = Some(-0.2);
    let back = from_sketchy(&doc).expect("from");
    assert!((back["pages"][0]["lines"][0]["arcHeight"].as_f64().expect("arcHeight") - 1.0).abs() < 1e-9, "chord 10, bulge 0.2");
}

fn bow_of(line: &Value) -> f64 {
    bow(line)
}

#[test]
fn parked_oversized_bulges_are_not_restored_onto_edited_polygons() {
    let limit = (359.0_f64 * std::f64::consts::PI / 720.0).tan();
    let (big, bigger) = (limit * 4.0, -limit * 2.0);
    let shape = json!({"type": "polygon", "points": [[0, 0], [10, 0], [10, 10], [0, 10]], "bulges": [big, 0.2, 0, bigger]});
    let doc = foreign(json!([level(json!([{"id": "s", "shape": shape}]), json!([]))]));
    let saved = from_sketchy(&doc).expect("from");
    for edit in ["insert", "delete"] {
        let mut m = saved.clone();
        let pts = m["pages"][0]["polygons"][0]["points"].as_array_mut().expect("pts");
        if edit == "insert" {
            pts.insert(2, json!({"x": 10.0, "y": 5.0}));
        } else {
            pts.remove(2);
        }
        let n = pts.len();
        let again = to_sketchy(&m, None).expect("to").to_value();
        let s = &lvl(&again)["spaces"][0]["shape"];
        let b = s["bulges"].as_array().map_or(0, Vec::len);
        assert!(b == 0 || b == n, "{edit}: {b} bulges for {n} points");
        for v in s["bulges"].as_array().into_iter().flatten() {
            let v = v.as_f64().expect("b");
            assert!((v - big).abs() > 1.0 && (v - bigger).abs() > 1.0, "{edit}: oversized original restored: {v}");
        }
    }
}

#[test]
fn non_numeric_ceiling_height_is_stashed_not_discarded() {
    let space = square("s", json!({"ceiling_height": "tall"}));
    let doc = foreign_v("0.1", json!([level(json!([space]), json!([]))]));
    let again = to_sketchy(&from_sketchy(&doc).expect("from"), None).expect("to").to_value();
    let space = &lvl(&again)["spaces"][0];
    assert!(space.get("ceiling_height").is_none() && space.get("ceiling").is_none());
    assert_eq!(space["extras"]["io.github.garmonbz.sketchrust"]["sketchy_0_1"], json!({"ceiling_height": "tall"}));
}

#[test]
fn canvas_theme_is_not_content() {
    use sketch_core::sketchy::canonical::content_hash;
    let mut a = rich();
    a["settings"]["canvasTheme"] = json!("dark");
    let mut b = rich();
    b["settings"]["canvasTheme"] = json!("blue");
    let (da, db) = (to_sketchy(&a, Some("d")).expect("a"), to_sketchy(&b, Some("d")).expect("b"));
    assert_eq!(content_hash(&da.to_value()), content_hash(&db.to_value()));
}

#[test]
fn upgrade_migrates_0_1_and_leaves_0_2_alone() {
    let old = Document::from_json(SINGLE_FAMILY_0_1).expect("0.1");
    let up = upgrade(&old).expect("upgrade");
    assert_eq!(up.format_version, "0.2");
    let same = Document::from_json(SINGLE_FAMILY).expect("0.2");
    assert_eq!(upgrade(&same).expect("noop").to_value(), same.to_value());
}

#[test]
fn migrating_the_most_negative_integer_bulge_does_not_panic() {
    let shape = json!({"type": "polygon", "points": [[0, 0], [10, 0], [10, 10], [0, 10]], "bulges": [0, 0, 0, 0]});
    let mut value = serde_json::to_value(foreign_v(
        "0.1",
        json!([level(json!([{"id": "s", "shape": shape}]), json!([{"id": "w", "start": [0, 0], "end": [10, 0], "bulge": 0.25}]))]),
    ))
    .expect("value");
    // Typed bulges are floats; an integer this size can only sit in free-form JSON (annotations).
    value["presentation"] = json!({"annotations": [{"id": "a", "type": "note", "bulge": -9223372036854775808_i64}]});
    let doc: Document = serde_json::from_value(value).expect("doc");
    let up = upgrade(&doc).expect("upgrade");
    assert_eq!(up.format_version, "0.2");
}

#[test]
fn layers_sharing_a_sketchy_id_do_not_get_a_growing_suffix() {
    let mut value = to_sketchy(&rich(), Some("d")).expect("to").to_value();
    let layers = value["presentation"]["layers"].as_array_mut().expect("layers");
    let again = layers[0].clone();
    layers.push(again);
    let doc: Document = serde_json::from_value(value).expect("doc");
    let mut model = from_sketchy(&doc).expect("from");
    let mut seen = Vec::new();
    for _ in 0..3 {
        let doc = to_sketchy(&model, None).expect("to");
        model = from_sketchy(&doc).expect("from");
        seen.push(model["layers"].clone());
    }
    assert_eq!(seen[0], seen[1], "second round renames nothing");
    assert_eq!(seen[1], seen[2], "and neither does any later one");
    assert!(seen[2].to_string().matches("layer-1_").count() == 0, "{}", seen[2]);
}
