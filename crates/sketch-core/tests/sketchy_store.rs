use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use rusqlite::Connection;
use serde_json::{json, Value};
use sketch_core::sketchy::canonical::content_hash;
use sketch_core::sketchy::convert::to_sketchy;
use sketch_core::sketchy::model::Document;
use sketch_core::sketchy::store::{is_store_bytes, new_id, CommitInfo, CommitOutcome, RevisionStatus, Store};
use sketch_core::sketchy::SketchyError;

const RICH: &str = include_str!("data/save-models/rich.json");

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A fresh scratch directory under the system temp dir.
fn scratch() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("sketchy-store-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

fn model() -> Value {
    serde_json::from_str(RICH).expect("rich")
}

fn doc(model: &Value) -> Document {
    to_sketchy(model, Some("doc-1")).expect("to_sketchy")
}

fn draft(message: &str) -> CommitInfo {
    CommitInfo { message: Some(message.to_string()), author: Some("Tester".to_string()), status: RevisionStatus::Draft, generator: None }
}

fn changed() -> Document {
    let mut m = model();
    m["pages"][0]["lines"][0]["start"]["x"] = json!(123.5);
    doc(&m)
}

fn rev_id(d: &Document) -> String {
    d.revision.as_ref().expect("revision").id.clone()
}

#[test]
fn create_then_head_returns_document() {
    let path = scratch().join("a.sketchy");
    let store = Store::create(&path, doc(&model()), &draft("first")).expect("create");
    let head = store.head().expect("head");
    let rev = head.revision.as_ref().expect("revision");
    assert_eq!(rev.sequence, Some(1));
    assert!(rev.parents.is_empty());
    assert_eq!(head.id, store.document_id());
    assert_eq!(rev.message.as_deref(), Some("first"));
    assert_eq!(rev.status.as_deref(), Some("draft"));
    let hash = rev.content_hash.clone().expect("hash");
    assert_eq!(hash, content_hash(&head.to_value()));
    assert!(Store::create(&path, doc(&model()), &draft("again")).is_err(), "must not overwrite");
}

#[test]
fn unchanged_commit_creates_no_revision() {
    let path = scratch().join("a.sketchy");
    let mut store = Store::create(&path, doc(&model()), &draft("first")).expect("create");
    let r1 = rev_id(&store.head().expect("head"));
    let outcome = store.commit(doc(&model()), &draft("again")).expect("commit");
    assert_eq!(outcome, CommitOutcome::Unchanged(r1));
    assert_eq!(store.history().expect("history").len(), 1);
}

#[test]
fn changed_commit_links_parent() {
    let path = scratch().join("a.sketchy");
    let mut store = Store::create(&path, doc(&model()), &draft("first")).expect("create");
    let r1 = rev_id(&store.head().expect("head"));
    let CommitOutcome::Created(r2) = store.commit(changed(), &draft("second")).expect("commit") else {
        panic!("expected Created");
    };
    let head = store.head().expect("head");
    let rev = head.revision.as_ref().expect("revision");
    assert_eq!(rev.id, r2);
    assert_eq!(rev.sequence, Some(2));
    assert_eq!(rev.parents, vec![r1.clone()]);
    let history = store.history().expect("history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].id, r1);
    assert_eq!(history[1].parents, vec![r1]);
    // Reopening sees the new head.
    let reopened = Store::open(&path).expect("open");
    assert_eq!(rev_id(&reopened.head().expect("head")), r2);
}

fn stored_document(path: &std::path::Path, id: &str) -> String {
    let conn = Connection::open(path).expect("sql");
    conn.query_row("SELECT document FROM revisions WHERE id = ?1", [id], |r| r.get(0)).expect("row")
}

#[test]
fn final_revision_is_never_rewritten() {
    let path = scratch().join("a.sketchy");
    let mut store = Store::create(&path, doc(&model()), &draft("first")).expect("create");
    let final_info = CommitInfo { message: Some("ship".into()), author: None, status: RevisionStatus::Final, generator: None };
    let CommitOutcome::Created(fin) = store.commit(changed(), &final_info).expect("final") else {
        panic!("expected Created");
    };
    let before = stored_document(&path, &fin);
    // A draft that changes nothing over a Final is a no-op: no needless revision.
    let outcome = store.commit(changed(), &draft("after final")).expect("draft");
    assert_eq!(outcome, CommitOutcome::Unchanged(fin.clone()));
    assert_eq!(store.history().expect("history").len(), 2);
    // An edited draft is appended after the Final, which is left alone.
    let outcome = store.commit(doc(&model()), &draft("edit after final")).expect("draft");
    let CommitOutcome::Created(next) = outcome else { panic!("expected Created, got {outcome:?}") };
    assert_ne!(next, fin);
    let head = store.head().expect("head");
    assert_eq!(head.revision.as_ref().expect("rev").parents, vec![fin.clone()]);
    assert_eq!(stored_document(&path, &fin), before);
}

#[test]
fn create_records_the_documents_format_version() {
    let path = scratch().join("a.sketchy");
    let old = Document::from_json(include_str!("data/sketchy/single-family-0.1.sketchy.json")).expect("0.1");
    Store::create(&path, old, &draft("first")).expect("create");
    let conn = Connection::open(&path).expect("sql");
    let v: String = conn.query_row("SELECT value FROM meta WHERE key='format_version'", [], |r| r.get(0)).expect("meta");
    assert_eq!(v, "0.1");
}

#[test]
fn failed_commit_leaves_head_intact() {
    let path = scratch().join("a.sketchy");
    let store = Store::create(&path, doc(&model()), &draft("first")).expect("create");
    let r1 = rev_id(&store.head().expect("head"));
    drop(store);
    let mut perms = std::fs::metadata(&path).expect("meta").permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&path, perms).expect("chmod");
    let mut store = Store::open(&path).expect("open read-only file");
    let result = store.commit(changed(), &draft("second"));
    assert!(result.is_err(), "commit into a read-only file must fail");
    drop(store);
    let mut perms = std::fs::metadata(&path).expect("meta").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&path, perms).expect("chmod back");
    let reopened = Store::open(&path).expect("open");
    assert_eq!(rev_id(&reopened.head().expect("head")), r1);
    assert_eq!(reopened.history().expect("history").len(), 1);
}

#[test]
fn open_rejects_foreign_sqlite_and_garbage() {
    let dir = scratch();
    let foreign = dir.join("foreign.db");
    Connection::open(&foreign).expect("sql").execute_batch("CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);").expect("ddl");
    assert!(matches!(Store::open(&foreign), Err(SketchyError::NotSketchy)));

    let garbage = dir.join("garbage.sketchy");
    std::fs::write(&garbage, [7u8, 3, 9, 200, 1, 2, 3, 4, 5, 6]).expect("write");
    assert!(Store::open(&garbage).is_err());

    let missing = dir.join("missing.sketchy");
    assert!(Store::open(&missing).is_err());
    assert!(!missing.exists(), "open must not create the file");

    let real = dir.join("real.sketchy");
    let mut store = Store::create(&real, doc(&model()), &draft("first")).expect("create");
    store.commit(changed(), &draft("second")).expect("commit");
    drop(store);
    let bytes = std::fs::read(&real).expect("read");
    let truncated = dir.join("truncated.sketchy");
    std::fs::write(&truncated, &bytes[..bytes.len() / 2]).expect("write");
    assert!(Store::open(&truncated).is_err());
    // Header intact but the body zeroed: corrupt, not a panic.
    let mut zeroed = bytes.clone();
    for b in zeroed.iter_mut().skip(100) {
        *b = 0;
    }
    let zeroed_path = dir.join("zeroed.sketchy");
    std::fs::write(&zeroed_path, zeroed).expect("write");
    assert!(Store::open(&zeroed_path).is_err());
}

#[test]
fn copy_to_keeps_history() {
    let dir = scratch();
    let path = dir.join("a.sketchy");
    let mut store = Store::create(&path, doc(&model()), &draft("first")).expect("create");
    store.commit(changed(), &draft("second")).expect("commit");
    let copy = store.copy_to(&dir.join("b.sketchy")).expect("copy");
    assert_eq!(copy.document_id(), store.document_id());
    assert_eq!(copy.history().expect("history"), store.history().expect("history"));
    assert_eq!(copy.head().expect("head"), store.head().expect("head"));
    assert!(store.copy_to(&dir.join("b.sketchy")).is_err(), "must not overwrite");
}

#[test]
fn magic_bytes_and_ids() {
    let path = scratch().join("a.sketchy");
    Store::create(&path, doc(&model()), &draft("first")).expect("create");
    let bytes = std::fs::read(&path).expect("read");
    assert!(is_store_bytes(&bytes[..16]));
    assert!(!is_store_bytes(b"{\"format\":\"sketchy\"}"));
    assert!(!is_store_bytes(b"short"));
    let (a, b) = (new_id(), new_id());
    assert_ne!(a, b);
    assert_eq!(a.len(), 36);
}

#[test]
fn finalizing_an_unchanged_draft_creates_a_final_revision() {
    let path = scratch().join("a.sketchy");
    let mut store = Store::create(&path, doc(&model()), &draft("first")).expect("create");
    let r1 = rev_id(&store.head().expect("head"));
    let info = CommitInfo { message: Some("ship".into()), author: None, status: RevisionStatus::Final, generator: None };
    let CommitOutcome::Created(r2) = store.commit(doc(&model()), &info).expect("commit") else {
        panic!("expected Created");
    };
    let history = store.history().expect("history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].id, r2);
    assert_eq!(history[1].parents, vec![r1]);
    assert_eq!(history[1].status, RevisionStatus::Final);
}

#[test]
fn generator_lives_in_revision_only() {
    let path = scratch().join("a.sketchy");
    let gen = |v: &str| Some(json!({"name": "Sketch Rust", "version": v}));
    let mut first = draft("first");
    first.generator = gen("0.1.0");
    let mut store = Store::create(&path, doc(&model()), &first).expect("create");
    let r1 = rev_id(&store.head().expect("head"));
    let mut second = draft("second");
    second.generator = gen("0.2.0");
    assert_eq!(store.commit(doc(&model()), &second).expect("commit"), CommitOutcome::Unchanged(r1));
    let head = store.head().expect("head");
    assert_eq!(head.revision.as_ref().expect("rev").generator, gen("0.1.0"));
    assert!(head.generator.is_none());
}

#[test]
fn create_and_copy_do_not_clobber_or_delete_existing_files() {
    let dir = scratch();
    let taken = dir.join("taken.sketchy");
    std::fs::write(&taken, b"precious").expect("write");
    assert!(Store::create(&taken, doc(&model()), &draft("x")).is_err());
    assert_eq!(std::fs::read(&taken).expect("read"), b"precious");
    let store = Store::create(&dir.join("a.sketchy"), doc(&model()), &draft("x")).expect("create");
    assert!(store.copy_to(&taken).is_err());
    assert_eq!(std::fs::read(&taken).expect("read"), b"precious");
}

#[test]
fn open_rejects_stores_with_triggers_or_views() {
    let dir = scratch();
    for (name, ddl) in [
        ("trigger.sketchy", "CREATE TRIGGER t AFTER INSERT ON revisions BEGIN DELETE FROM meta; END;"),
        ("view.sketchy", "CREATE VIEW v AS SELECT * FROM meta;"),
    ] {
        let path = dir.join(name);
        Store::create(&path, doc(&model()), &draft("x")).expect("create");
        Connection::open(&path).expect("sql").execute_batch(ddl).expect("ddl");
        assert!(matches!(Store::open(&path), Err(SketchyError::NotSketchy)), "{name}");
    }
}

#[test]
fn is_read_only_reflects_file_permissions() {
    let path = scratch().join("a.sketchy");
    let store = Store::create(&path, doc(&model()), &draft("x")).expect("create");
    assert!(!store.is_read_only());
    drop(store);
    let mut perms = std::fs::metadata(&path).expect("meta").permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&path, perms).expect("chmod");
    let store = Store::open(&path).expect("open");
    assert!(store.is_read_only());
    drop(store);
    let mut perms = std::fs::metadata(&path).expect("meta").permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    std::fs::set_permissions(&path, perms).expect("chmod back");
}

#[test]
fn revision_returns_any_stored_snapshot() {
    let path = scratch().join("rev.sketchy");
    let mut store = Store::create(&path, doc(&model()), &draft("first")).expect("create");
    let first = rev_id(&store.head().expect("head"));
    store.commit(changed(), &draft("second")).expect("commit");
    let old = store.revision(&first).expect("old revision");
    assert_eq!(rev_id(&old), first);
    assert_eq!(old.revision.as_ref().and_then(|r| r.sequence), Some(1));
    assert_ne!(old, store.head().expect("head"));
    assert!(store.revision("no-such-revision").is_err());
}

#[test]
fn store_writes_0_2_and_still_reads_older_0_1_revisions() {
    use sketch_core::sketchy::convert::from_sketchy;
    let old_text = include_str!("data/sketchy/single-family-0.1.sketchy.json");
    let old = Document::from_json(old_text).expect("0.1 doc");
    assert_eq!(old.format_version, "0.1");
    let path = scratch().join("old.sketchy");
    let mut store = Store::create(&path, old, &draft("imported")).expect("create");
    let first = rev_id(&store.head().expect("head"));
    // A store created by a 0.1 build says so in its meta.
    Connection::open(&path).expect("raw").execute("UPDATE meta SET value = '0.1' WHERE key = 'format_version'", []).expect("set 0.1");
    let meta = || -> String {
        Connection::open(&path).expect("raw").query_row("SELECT value FROM meta WHERE key = 'format_version'", [], |r| r.get(0)).expect("meta")
    };
    assert_eq!(meta(), "0.1");
    // Opening the 0.1 revision and saving writes a 0.2 revision.
    let saved = from_sketchy(&store.head().expect("head")).expect("from");
    let outcome = store.commit(to_sketchy(&saved, None).expect("to"), &draft("converted")).expect("commit");
    assert!(matches!(outcome, CommitOutcome::Created(_)));
    let head = store.head().expect("head");
    assert_eq!(head.format_version, "0.2");
    assert_eq!(store.revision(&first).expect("old revision").format_version, "0.1");
    assert_eq!(meta(), "0.2", "adding a 0.2 revision moves the store's version");
    let new_store = scratch().join("new.sketchy");
    Store::create(&new_store, doc(&model()), &draft("first")).expect("create");
    let conn = Connection::open(&new_store).expect("open raw");
    let version: String = conn.query_row("SELECT value FROM meta WHERE key = 'format_version'", [], |r| r.get(0)).expect("meta");
    assert_eq!(version, "0.2");
}
