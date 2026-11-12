//! The SQLite `.sketchy` revision store: one file holding every revision of a
//! sketch, each as a full `sketchy.json` document.
//!
//! The file is a SQLite database tagged with `application_id` `SKCY` and
//! `user_version` 1. Every write runs in one `IMMEDIATE` transaction, so a
//! failed commit leaves the previous head intact. Final revisions are never
//! rewritten: a new revision is always appended with the old head as parent.

use std::fmt::Display;
use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::{json, Value};

use super::canonical::content_hash;
use super::model::{Document, Revision};
use super::{SketchyError, FORMAT, FORMAT_VERSION};
use crate::skx::iso8601_now;

/// `application_id` of a store file: ASCII `SKCY`.
const APPLICATION_ID: i64 = 0x534B_4359;
/// Schema version of the store file.
const USER_VERSION: i64 = 1;
/// The first 16 bytes of every SQLite database file.
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// Workflow status of a revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionStatus {
    /// Work in progress.
    Draft,
    /// Signed off; never rewritten.
    Final,
}

impl RevisionStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Final => "final",
        }
    }

    fn parse(s: &str) -> Result<Self, SketchyError> {
        match s {
            "draft" => Ok(Self::Draft),
            "final" => Ok(Self::Final),
            other => Err(store_err(format!("unknown revision status {other:?}"))),
        }
    }
}

/// Metadata for a new revision.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    /// Commit-style message.
    pub message: Option<String>,
    /// Author name.
    pub author: Option<String>,
    /// Status of the new revision.
    pub status: RevisionStatus,
    /// Program that wrote the revision (`name`, `version`); stored in `revision.generator`
    /// only, never in the hashed document root.
    pub generator: Option<Value>,
}

/// Result of [`Store::commit`]; carries a revision id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    /// A new revision was written.
    Created(String),
    /// The content matched the draft head, so nothing was written (the head's id).
    Unchanged(String),
}

/// One revision's metadata, without its document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionSummary {
    /// Revision id.
    pub id: String,
    /// Parent revision ids.
    pub parents: Vec<String>,
    /// Position in the chain, starting at 1.
    pub sequence: u64,
    /// RFC 3339 UTC creation time.
    pub created_at: String,
    /// Author name, if one was recorded.
    pub author: Option<String>,
    /// Commit message.
    pub message: Option<String>,
    /// Draft or final.
    pub status: RevisionStatus,
    /// `sha256:` hash of the content.
    pub content_hash: String,
}

/// An open `.sketchy` revision store.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    document_id: String,
}

/// A new UUIDv7 string, used for revision ids.
#[must_use]
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Whether `first_bytes` starts like a SQLite database, which is what a store file is.
#[must_use]
pub fn is_store_bytes(first_bytes: &[u8]) -> bool {
    first_bytes.starts_with(SQLITE_MAGIC)
}

fn store_err(e: impl Display) -> SketchyError {
    SketchyError::Store(e.to_string())
}

const SCHEMA: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE revisions (
  id TEXT PRIMARY KEY, sequence INTEGER NOT NULL, parents TEXT NOT NULL,
  created_at TEXT NOT NULL, author TEXT, message TEXT,
  status TEXT NOT NULL CHECK (status IN ('draft','final')),
  content_hash TEXT NOT NULL, document TEXT NOT NULL
);";

/// The head revision's identity, as read inside a transaction.
struct Head {
    id: String,
    sequence: u64,
    content_hash: String,
}

impl Store {
    /// Creates a new store at `path` holding `doc` as revision 1.
    ///
    /// Fails if `path` already exists. The document's `id` becomes the store's
    /// document id; on failure no file is left behind.
    pub fn create(path: &Path, doc: Document, info: &CommitInfo) -> Result<Store, SketchyError> {
        claim_path(path)?;
        let result = Self::create_inner(path, doc, info);
        if result.is_err() {
            let _ = std::fs::remove_file(path);
        }
        result
    }

    fn create_inner(path: &Path, doc: Document, info: &CommitInfo) -> Result<Store, SketchyError> {
        let mut conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(store_err)?;
        conn.pragma_update(None, "foreign_keys", true).map_err(store_err)?;
        let document_id = doc.id.clone();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(store_err)?;
        tx.pragma_update(None, "application_id", APPLICATION_ID).map_err(store_err)?;
        tx.pragma_update(None, "user_version", USER_VERSION).map_err(store_err)?;
        tx.execute_batch(SCHEMA).map_err(store_err)?;
        let format_version = doc.format_version.clone();
        for (key, value) in [("format", FORMAT), ("format_version", format_version.as_str()), ("document_id", &document_id)] {
            tx.execute("INSERT INTO meta (key, value) VALUES (?1, ?2)", params![key, value]).map_err(store_err)?;
        }
        let id = insert_revision(&tx, doc, &document_id, None, info)?;
        tx.execute("INSERT INTO meta (key, value) VALUES ('head', ?1)", [&id]).map_err(store_err)?;
        tx.commit().map_err(store_err)?;
        Ok(Store { conn, document_id })
    }

    /// Opens an existing store.
    ///
    /// Runs an integrity check and requires the sketchy `application_id`, schema
    /// version and `meta.format`; a SQLite file that is not a store is
    /// [`SketchyError::NotSketchy`], an unreadable or corrupt one is
    /// [`SketchyError::Store`]. Never creates the file.
    pub fn open(path: &Path) -> Result<Store, SketchyError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)
            .map_err(store_err)?;
        conn.pragma_update(None, "foreign_keys", true).map_err(store_err)?;
        conn.pragma_update(None, "trusted_schema", false).map_err(store_err)?;
        let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0)).map_err(store_err)?;
        if check != "ok" {
            return Err(store_err(format!("integrity check failed: {check}")));
        }
        let app_id: i64 = conn.query_row("PRAGMA application_id", [], |r| r.get(0)).map_err(store_err)?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(store_err)?;
        if app_id != APPLICATION_ID || version != USER_VERSION {
            return Err(SketchyError::NotSketchy);
        }
        let active: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_schema WHERE type IN ('trigger', 'view')", [], |r| r.get(0))
            .map_err(store_err)?;
        if active > 0 {
            return Err(SketchyError::NotSketchy);
        }
        let format = meta_value(&conn, "format").map_err(|_| SketchyError::NotSketchy)?;
        if format.as_deref() != Some(FORMAT) {
            return Err(SketchyError::NotSketchy);
        }
        let document_id = meta_value(&conn, "document_id").map_err(store_err)?.ok_or_else(|| store_err("missing document_id"))?;
        let store = Store { conn, document_id };
        store.head()?;
        Ok(store)
    }

    /// The id shared by every revision in this store.
    #[must_use]
    pub fn document_id(&self) -> &str {
        &self.document_id
    }

    /// Whether the file was opened read-only (commits will fail).
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.conn.is_readonly(rusqlite::MAIN_DB).unwrap_or(true)
    }

    /// The head revision's document.
    pub fn head(&self) -> Result<Document, SketchyError> {
        let text: String = self
            .conn
            .query_row(
                "SELECT r.document FROM revisions r JOIN meta m ON m.key = 'head' AND m.value = r.id",
                [],
                |r| r.get(0),
            )
            .map_err(store_err)?;
        let value: Value = serde_json::from_str(&text).map_err(|e| store_err(format!("stored document: {e}")))?;
        Document::from_value(value)
    }

    /// Any stored revision's document (with its `revision` block), by revision id.
    pub fn revision(&self, id: &str) -> Result<Document, SketchyError> {
        let text: String = self
            .conn
            .query_row("SELECT document FROM revisions WHERE id = ?1", [id], |r| r.get(0))
            .map_err(|e| store_err(format!("revision {id}: {e}")))?;
        let value: Value = serde_json::from_str(&text).map_err(|e| store_err(format!("stored document: {e}")))?;
        Document::from_value(value)
    }

    /// Commits `doc` as the next revision.
    ///
    /// Returns [`CommitOutcome::Unchanged`] when a draft is requested and the content
    /// hash equals the head's, whatever the head's status (a draft over an identical
    /// final is a no-op); a final over an identical draft is still written. Otherwise appends a revision whose parent is the
    /// head. The document's `id` is forced to the store's and its `revision` block
    /// is filled in. All of it happens in one transaction.
    pub fn commit(&mut self, doc: Document, info: &CommitInfo) -> Result<CommitOutcome, SketchyError> {
        let document_id = self.document_id.clone();
        let tx = self.conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(store_err)?;
        let head = read_head(&tx)?;
        let mut doc = doc;
        doc.id.clone_from(&document_id);
        if info.status == RevisionStatus::Draft && content_hash(&doc.to_value()) == head.content_hash {
            return Ok(CommitOutcome::Unchanged(head.id));
        }
        let id = insert_revision(&tx, doc, &document_id, Some(&head), info)?;
        tx.execute("UPDATE meta SET value = ?1 WHERE key = 'head'", [&id]).map_err(store_err)?;
        // A revision written by this build is FORMAT_VERSION, so the store now is too.
        tx.execute("UPDATE meta SET value = ?1 WHERE key = 'format_version'", [FORMAT_VERSION]).map_err(store_err)?;
        tx.commit().map_err(store_err)?;
        Ok(CommitOutcome::Created(id))
    }

    /// Every revision, oldest first.
    pub fn history(&self) -> Result<Vec<RevisionSummary>, SketchyError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, parents, sequence, created_at, message, status, content_hash, author \
                 FROM revisions ORDER BY sequence, created_at",
            )
            .map_err(store_err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<String>>(7)?,
                ))
            })
            .map_err(store_err)?;
        let mut out = Vec::new();
        for row in rows {
            let (id, parents, sequence, created_at, message, status, content_hash, author) = row.map_err(store_err)?;
            out.push(RevisionSummary {
                id,
                parents: serde_json::from_str(&parents).map_err(|e| store_err(format!("parents: {e}")))?,
                sequence: u64::try_from(sequence).map_err(|_| store_err("negative sequence"))?,
                created_at,
                author,
                message,
                status: RevisionStatus::parse(&status)?,
                content_hash,
            });
        }
        Ok(out)
    }

    /// Writes a complete copy of the store (all revisions) to `path` with
    /// `VACUUM INTO` and opens it. Fails if `path` already exists.
    pub fn copy_to(&self, path: &Path) -> Result<Store, SketchyError> {
        let target = path.to_str().ok_or_else(|| store_err("path is not valid UTF-8"))?;
        claim_path(path)?;
        let copied = self
            .conn
            .execute("VACUUM INTO ?1", [target])
            .map_err(store_err)
            .and_then(|_| Store::open(path));
        if copied.is_err() {
            let _ = std::fs::remove_file(path);
        }
        copied
    }
}

/// Atomically creates an empty file at `path` so this call owns it; an existing
/// path is an error. SQLite accepts an empty file as a new database.
fn claim_path(path: &Path) -> Result<(), SketchyError> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(drop)
        .map_err(|e| store_err(format!("{}: {e}", path.display())))
}

fn meta_value(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()
}

fn read_head(tx: &Transaction<'_>) -> Result<Head, SketchyError> {
    tx.query_row(
        "SELECT r.id, r.sequence, r.content_hash FROM revisions r \
         JOIN meta m ON m.key = 'head' AND m.value = r.id",
        [],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)),
    )
    .map_err(store_err)
    .and_then(|(id, sequence, content_hash)| {
        Ok(Head { id, sequence: u64::try_from(sequence).map_err(|_| store_err("negative sequence"))?, content_hash })
    })
}

/// Stamps `doc` with a new revision block and inserts it; returns the revision id.
fn insert_revision(
    tx: &Transaction<'_>,
    mut doc: Document,
    document_id: &str,
    parent: Option<&Head>,
    info: &CommitInfo,
) -> Result<String, SketchyError> {
    doc.id = document_id.to_string();
    let hash = content_hash(&doc.to_value());
    let id = new_id();
    let parents: Vec<String> = parent.map(|p| p.id.clone()).into_iter().collect();
    let sequence = parent.map_or(1, |p| p.sequence + 1);
    let created_at = iso8601_now();
    let status = info.status.as_str();
    doc.revision = Some(Revision {
        id: id.clone(),
        parents: parents.clone(),
        sequence: Some(sequence),
        created_at: Some(created_at.clone()),
        author: info.author.as_ref().map(|name| json!({ "name": name })),
        generator: info.generator.clone(),
        message: info.message.clone(),
        status: Some(status.to_string()),
        content_hash: Some(hash.clone()),
        other: serde_json::Map::new(),
    });
    let text = serde_json::to_string(&doc.to_value()).map_err(store_err)?;
    let parents_json = serde_json::to_string(&parents).map_err(store_err)?;
    let sequence_sql = i64::try_from(sequence).map_err(store_err)?;
    tx.execute(
        "INSERT INTO revisions (id, sequence, parents, created_at, author, message, status, content_hash, document) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, sequence_sql, parents_json, created_at, info.author, info.message, status, hash, text],
    )
    .map_err(store_err)?;
    Ok(id)
}
