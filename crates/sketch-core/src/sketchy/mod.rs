//! The open `sketchy.json` document format (spec: `sketchy/SPEC.md`).

pub mod canonical;
pub mod convert;
pub mod model;

use std::fmt;

use serde_json::Value;

pub use convert::{parent_revision, unshown_summary};
pub use model::Document;

/// Value of the `format` field in every sketchy document.
pub const FORMAT: &str = "sketchy";
/// The `format_version` this crate writes (major.minor).
pub const FORMAT_VERSION: &str = "0.1";

/// Largest input [`read`] accepts.
pub const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;

/// Errors from reading, converting or storing sketchy documents.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SketchyError {
    /// The text is not valid JSON or does not match the document shape.
    Json(String),
    /// The JSON object's `format` is not `"sketchy"`.
    NotSketchy,
    /// The `format_version` has a major version this crate does not support.
    UnsupportedVersion(String),
    /// A storage-layer failure.
    Store(String),
    /// A conversion failure (for example from the save model).
    Convert(String),
}

impl fmt::Display for SketchyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(m) => write!(f, "invalid sketchy JSON: {m}"),
            Self::NotSketchy => write!(f, "not a sketchy document"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported sketchy format_version {v}"),
            Self::Store(m) => write!(f, "sketchy store error: {m}"),
            Self::Convert(m) => write!(f, "sketchy conversion error: {m}"),
        }
    }
}

impl std::error::Error for SketchyError {}

/// Reads a file's bytes as a sketchy document.
///
/// Rejects input over [`MAX_INPUT_BYTES`], strips a UTF-8 byte-order mark and
/// parses the JSON once. Returns `Ok(None)` when the JSON root is not an object
/// with `"format": "sketchy"` (some other JSON file, such as a save model);
/// otherwise validates it as [`Document::from_value`] does.
pub fn read(bytes: &[u8]) -> Result<Option<Document>, SketchyError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(SketchyError::Json("document is larger than 64 MiB".to_string()));
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let value: Value = serde_json::from_slice(bytes).map_err(|e| SketchyError::Json(e.to_string()))?;
    if value.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Ok(None);
    }
    Document::from_value(value).map(Some)
}
