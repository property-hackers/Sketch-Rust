use serde_json::{json, Value};
use sketch_core::sketchy::canonical::{canonical_json, content_hash};

#[test]
fn numbers_match_rfc8785() {
    let v = json!([10.0, -0.0, 0.5, 1e21, 1e-7, 123456789.0, 14.4, 0.0104]);
    assert_eq!(
        canonical_json(&v),
        "[10,0,0.5,1e+21,1e-7,123456789,14.4,0.0104]"
    );
}

#[test]
fn keys_sorted_and_compact() {
    let v = json!({"b":1,"a":{"d":[1,2],"c":"x"}});
    assert_eq!(canonical_json(&v), r#"{"a":{"c":"x","d":[1,2]},"b":1}"#);
}

#[test]
fn string_escapes() {
    let v = json!("a\"b\\c\n\u{1}é");
    assert_eq!(canonical_json(&v), "\"a\\\"b\\\\c\\n\\u0001é\"");
}

#[test]
fn keys_sorted_by_utf16_units() {
    // U+FF5E (BMP) sorts after U+1F600 (surrogate pair D83D...) in UTF-16.
    let v = json!({"\u{ff5e}": 1, "\u{1f600}": 2});
    assert_eq!(canonical_json(&v), "{\"\u{1f600}\":2,\"\u{ff5e}\":1}");
}

#[test]
fn example_hashes_match_recorded() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/sketchy");
    let mut n = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if !path.to_string_lossy().ends_with(".sketchy.json") {
            continue;
        }
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let recorded = v["revision"]["content_hash"].as_str().unwrap();
        assert_eq!(content_hash(&v), recorded, "{}", path.display());
        n += 1;
    }
    assert!(n >= 3);
}

#[test]
fn parsed_floats_round_trip_exactly() {
    let v: Value = serde_json::from_str("[9.249531820428997]").unwrap();
    assert_eq!(canonical_json(&v), "[9.249531820428997]");
}

#[test]
fn hash_ignores_only_root_revision_and_history() {
    let base = json!({"format": "sketchy", "id": "d", "title": "T",
                      "revision": {"id": "r1", "parents": []}, "history": [{"id": "r0"}],
                      "structures": [{"id": "s", "revision": "x", "levels": []}]});
    let h = content_hash(&base);
    let mut v = base.clone();
    v["revision"] = json!({"id": "r2", "parents": ["r1"], "message": "m"});
    v["history"] = json!([]);
    assert_eq!(content_hash(&v), h, "root revision/history are ignored");
    let mut v = base.clone();
    v.as_object_mut().expect("obj").remove("history");
    assert_eq!(content_hash(&v), h);
    let mut v = base.clone();
    v["title"] = json!("U");
    assert_ne!(content_hash(&v), h, "other root fields count");
    let mut v = base.clone();
    v["structures"][0]["revision"] = json!("y");
    assert_ne!(content_hash(&v), h, "a nested revision key is not stripped");
}

#[test]
fn control_and_separator_escapes() {
    let v = json!("\u{8}\u{c}\r\t\u{7f}\u{2028}\u{1f}");
    assert_eq!(canonical_json(&v), "\"\\b\\f\\r\\t\u{7f}\u{2028}\\u001f\"");
}

#[test]
fn empty_containers() {
    assert_eq!(canonical_json(&json!({})), "{}");
    assert_eq!(canonical_json(&json!([])), "[]");
    assert_eq!(canonical_json(&json!({"a": [], "b": {}})), r#"{"a":[],"b":{}}"#);
}
