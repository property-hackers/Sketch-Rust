//! The open `sketchy.json` document format (spec: `sketchy/SPEC.md`).

pub mod canonical;
pub mod convert;
pub mod model;
pub mod store;

use std::fmt;

use serde_json::Value;

pub use convert::{parent_revision, unshown_summary};
pub use model::Document;

/// Value of the `format` field in every sketchy document.
pub const FORMAT: &str = "sketchy";
/// The `format_version` this crate writes (major.minor): draft 0.2.
pub const FORMAT_VERSION: &str = "0.2";
/// Every `format_version` this crate reads. Draft minors may break each other (SPEC 0.2
/// section 14), so any other `0.x` is rejected with [`SketchyError::UnsupportedVersion`].
/// A `0.1` document is upgraded to `0.2` when it is converted for the app.
pub const SUPPORTED_VERSIONS: [&str; 2] = ["0.1", "0.2"];
/// The JSON Schema URL of [`FORMAT_VERSION`].
pub const SCHEMA_URL: &str = "https://property-hackers.github.io/sketchy/schema/0.2/sketchy.schema.json";
/// The JSON Schema URL of draft 0.1, rewritten to [`SCHEMA_URL`] when a 0.1 file is upgraded.
pub const SCHEMA_URL_0_1: &str = "https://property-hackers.github.io/sketchy/schema/0.1/sketchy.schema.json";

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
    /// The `format_version` is not one this crate reads (see [`SUPPORTED_VERSIONS`]).
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
