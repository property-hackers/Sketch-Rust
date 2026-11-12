use serde_json::{json, Value};
use sketch_core::sketchy::model::Document;
use sketch_core::sketchy::SketchyError;

const FIXTURES: [&str; 3] = [
    include_str!("data/sketchy/single-family.sketchy.json"),
    include_str!("data/sketchy/condo-unit.sketchy.json"),
    include_str!("data/sketchy/commercial-building.sketchy.json"),
];

#[test]
fn examples_round_trip_unchanged() {
    for text in FIXTURES {
        let doc = Document::from_json(text).expect("parse");
        let expected: Value = serde_json::from_str(text).expect("value");
        assert_eq!(doc.to_value(), expected);
    }
}

#[test]
fn unknown_fields_are_preserved() {
    let mut v: Value = serde_json::from_str(FIXTURES[0]).expect("value");
    v["future_field"] = json!({"a": 1});
    v["structures"][0]["levels"][0]["spaces"][0]["future_field"] = json!({"a": 1});
    let doc = Document::from_json(&v.to_string()).expect("parse");
    assert_eq!(doc.to_value(), v);
}

#[test]
fn rejects_non_sketchy() {
    let mut v: Value = serde_json::from_str(FIXTURES[0]).expect("value");
    v["format"] = json!("other");
    assert_eq!(Document::from_json(&v.to_string()).err(), Some(SketchyError::NotSketchy));
    v["format"] = json!("sketchy");
    v["format_version"] = json!("1.0");
    assert!(matches!(
        Document::from_json(&v.to_string()),
        Err(SketchyError::UnsupportedVersion(_))
    ));
    // Draft minors may break each other: only 0.1 and 0.2 are read.
    v["format_version"] = json!("0.7");
    assert!(matches!(Document::from_json(&v.to_string()), Err(SketchyError::UnsupportedVersion(_))));
    v["format_version"] = json!("0.1");
    assert!(Document::from_json(&v.to_string()).is_ok());
}

#[test]
fn typed_access_to_geometry() {
    let doc = Document::from_json(FIXTURES[0]).expect("parse");
    assert!(!doc.structures[0].levels[0].spaces.is_empty());
}

#[test]
fn unknown_shape_type_is_preserved_in_newer_minor() {
    let mut v: Value = serde_json::from_str(FIXTURES[0]).expect("value");
    v["format_version"] = json!("0.2");
    let shape = json!({"type":"rounded_rect","center":[0,0],"width":4,"height":2,"corner_radius":0.5});
    v["structures"][0]["levels"][0]["spaces"][0]["shape"] = shape.clone();
    let doc = Document::from_json(&v.to_string()).expect("parse");
    assert_eq!(doc.to_value(), v);
    let first = &doc.structures[0].levels[0].spaces[0].shape;
    assert!(matches!(first, sketch_core::sketchy::model::Shape::Unknown(m) if m["type"] == "rounded_rect"));
    // Known shapes in the same document stay typed.
    let known = Document::from_json(FIXTURES[0]).expect("parse");
    assert!(!matches!(
        known.structures[0].levels[0].spaces[0].shape,
        sketch_core::sketchy::model::Shape::Unknown(_)
    ));
}

#[test]
fn unknown_keys_in_shape_variant_and_wall_round_trip() {
    let mut v: Value = serde_json::from_str(FIXTURES[0]).expect("value");
    v["structures"][0]["levels"][0]["spaces"][0]["shape"] =
        json!({"type":"polygon","points":[[0,0],[4,0],[4,3]],"future":1});
    v["structures"][0]["levels"][0]["walls"] = json!([
        {"id":"w1","start":[0,0],"end":[4,0],"future":{"x":[1,2]}}
    ]);
    let doc = Document::from_json(&v.to_string()).expect("parse");
    assert_eq!(doc.to_value(), v);
}

// ---- review fix wave A: hostile input ----

#[test]
fn read_parses_sketchy_and_skips_other_json() {
    let doc = sketch_core::sketchy::read(FIXTURES[0].as_bytes()).expect("read").expect("sketchy");
    assert_eq!(doc, Document::from_json(FIXTURES[0]).expect("parse"));
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(FIXTURES[1].as_bytes());
    assert!(sketch_core::sketchy::read(&bom).expect("bom").is_some());
    assert_eq!(sketch_core::sketchy::read(br#"{"pages": []}"#), Ok(None));
    assert_eq!(sketch_core::sketchy::read(b"[1]"), Ok(None));
    assert!(matches!(sketch_core::sketchy::read(b"{nope"), Err(SketchyError::Json(_))));
    let v: Value = serde_json::from_str(FIXTURES[0]).expect("value");
    assert_eq!(Document::from_value(v).expect("from_value"), doc);
}

#[test]
fn read_rejects_oversized_input() {
    let big = vec![b' '; 64 * 1024 * 1024 + 1];
    assert!(matches!(sketch_core::sketchy::read(&big), Err(SketchyError::Json(m)) if m.contains("64 MiB")));
}

#[test]
fn deeply_nested_documents_are_rejected() {
    let mut v: Value = serde_json::from_str(FIXTURES[0]).expect("value");
    let mut deep = json!(1);
    for _ in 0..70 {
        deep = json!([deep]);
    }
    v["future_field"] = deep;
    let text = v.to_string();
    assert!(matches!(Document::from_json(&text), Err(SketchyError::Json(m)) if m.contains("nested")));
    assert!(matches!(sketch_core::sketchy::read(text.as_bytes()), Err(SketchyError::Json(_))));
    // 60 levels under the root is fine.
    let mut ok = json!(1);
    for _ in 0..60 {
        ok = json!([ok]);
    }
    v["future_field"] = ok;
    assert!(Document::from_json(&v.to_string()).is_ok());
}
