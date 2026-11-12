//! Typed model of a `sketchy.json` document.
//!
//! Every struct carries a flattened `other` map so unknown keys round-trip
//! (spec section 14: editors preserve what they do not own).
//!
//! Numbers: coordinates and lengths are typed `f64`, but JSON like `40` would
//! come back as `40.0` and differ as a `serde_json::Value`. Typed numeric
//! fields therefore serialize through [`num::ser`], which writes whole-valued
//! finite floats as integers. Values inside `other`, `Value`-typed fields and
//! extras are never touched, so they round-trip exactly.
//!
//! Normalisation: an explicit `null` on an optional field reads as absent and
//! is not written back, and an empty array in a field that is skipped when
//! empty (`spaces`, `classifications`, `notes`, ...) is dropped on write. A
//! typed round trip is therefore exact only for documents that omit such
//! fields. Content hashes ([`super::canonical::content_hash`]) must be
//! computed on the raw parsed `serde_json::Value`, never on a re-serialized
//! [`Document`].
//!
//! Hostile input: [`Document::from_value`] rejects values nested deeper than
//! [`MAX_DEPTH`]; [`super::read`] also caps the input size.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{SketchyError, FORMAT, SUPPORTED_VERSIONS};

/// Deepest nesting of arrays and objects a document may have (the root counts as 1).
pub const MAX_DEPTH: usize = 64;

/// Whether any array or object in `v` sits deeper than [`MAX_DEPTH`] (iterative walk).
fn too_deep(v: &Value) -> bool {
    let mut stack: Vec<(&Value, usize)> = vec![(v, 1)];
    while let Some((v, depth)) = stack.pop() {
        let children: Box<dyn Iterator<Item = &Value>> = match v {
            Value::Array(a) => Box::new(a.iter()),
            Value::Object(m) => Box::new(m.values()),
            _ => continue,
        };
        if depth > MAX_DEPTH {
            return true;
        }
        stack.extend(children.map(|c| (c, depth + 1)));
    }
    false
}

/// A `[x, y]` point in the document's length unit.
pub type Point = [f64; 2];
/// A drawing mark that exists only in the presentation layer; kept as raw JSON.
pub type Annotation = Value;

/// Serialization helpers that keep whole-valued floats as JSON integers.
mod num {
    use serde::{Serialize, Serializer};
    use serde_json::{Number, Value};

    /// Types whose floats should be written as integers when whole.
    pub trait IntFriendly {
        fn to_json(&self) -> Value;
    }

    fn f(v: f64) -> Value {
        if v.is_finite() && v.fract() == 0.0 && v.abs() < 9.0e15 {
            Value::Number(Number::from(v as i64))
        } else {
            Number::from_f64(v).map_or(Value::Null, Value::Number)
        }
    }

    impl IntFriendly for f64 {
        fn to_json(&self) -> Value {
            f(*self)
        }
    }
    impl IntFriendly for [f64; 2] {
        fn to_json(&self) -> Value {
            Value::Array(self.iter().map(|v| f(*v)).collect())
        }
    }
    impl<T: IntFriendly> IntFriendly for Vec<T> {
        fn to_json(&self) -> Value {
            Value::Array(self.iter().map(IntFriendly::to_json).collect())
        }
    }
    impl<T: IntFriendly> IntFriendly for Option<T> {
        fn to_json(&self) -> Value {
            self.as_ref().map_or(Value::Null, IntFriendly::to_json)
        }
    }

    /// `serialize_with` entry point.
    pub fn ser<T: IntFriendly, S: Serializer>(v: &T, s: S) -> Result<S::Ok, S::Error> {
        v.to_json().serialize(s)
    }
}

/// Declares an element struct with the common element fields (spec section 10)
/// and the flattened `other` map appended.
macro_rules! element {
    ($(#[$sm:meta])* pub struct $name:ident {
        $($(#[$fm:meta])* pub $f:ident : $t:ty,)*
    }) => {
        $(#[$sm])*
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            $($(#[$fm])* pub $f: $t,)*
            /// Display name.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub name: Option<String>,
            /// Codes from any classification scheme.
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            pub classifications: Vec<Classification>,
            /// User-entered key-value pairs.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub attributes: Option<BTreeMap<String, Value>>,
            /// Comments.
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            pub notes: Vec<Value>,
            /// Where the element came from.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub source: Option<Value>,
            /// Namespaced extension data.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub extras: Option<Map<String, Value>>,
            /// Unknown keys, preserved.
            #[serde(flatten)]
            pub other: Map<String, Value>,
        }
    };
}

/// A code from a classification scheme attached to an element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Classification {
    /// Reverse-domain scheme name.
    pub scheme: String,
    /// Code within the scheme.
    pub code: String,
    /// Unknown keys, preserved.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// A code table a document carries for a classification scheme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassificationScheme {
    /// Reverse-domain scheme name.
    pub scheme: String,
    /// Human-readable scheme name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Code table entries, kept as raw JSON.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub codes: Vec<Value>,
    /// Unknown keys, preserved.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// The `revision` block (spec section 15.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Revision {
    /// Identifies this saved version.
    pub id: String,
    /// Ids of parent revisions.
    pub parents: Vec<String>,
    /// Position in the revision chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
    /// RFC 3339 UTC timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Who made the revision (`name`, `id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<Value>,
    /// Program that wrote it (`name`, `version`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<Value>,
    /// Commit-style message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Workflow status such as `draft`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// `sha256:` hash of the content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    /// Unknown keys, preserved.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// A ring cut out of a polygon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ring {
    /// Vertices.
    #[serde(serialize_with = "num::ser")]
    pub points: Vec<Point>,
    /// One bulge per edge.
    #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
    pub bulges: Option<Vec<f64>>,
    /// Unknown keys, preserved.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// A geometric shape (spec section 4.3).
///
/// Shapes whose `type` this crate does not know (added by newer minor spec
/// versions) are kept verbatim as [`Shape::Unknown`]. The derive is `remote`
/// to `Self` so the public `Serialize`/`Deserialize` impls below can delegate
/// to it for known types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(remote = "Shape", tag = "type", rename_all = "snake_case")]
pub enum Shape {
    /// Closed polygon, optionally with curved edges and holes.
    Polygon {
        /// Vertices, first not repeated.
        #[serde(serialize_with = "num::ser")]
        points: Vec<Point>,
        /// One bulge per edge.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        bulges: Option<Vec<f64>>,
        /// Rings cut out of the polygon.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        holes: Option<Vec<Ring>>,
        /// Unknown keys, preserved.
        #[serde(flatten)]
        other: Map<String, Value>,
    },
    /// Circle.
    Circle {
        /// Centre.
        #[serde(serialize_with = "num::ser")]
        center: Point,
        /// Radius.
        #[serde(serialize_with = "num::ser")]
        radius: f64,
        /// Unknown keys, preserved.
        #[serde(flatten)]
        other: Map<String, Value>,
    },
    /// Ellipse.
    Ellipse {
        /// Centre.
        #[serde(serialize_with = "num::ser")]
        center: Point,
        /// X radius before rotation.
        #[serde(serialize_with = "num::ser")]
        radius_x: f64,
        /// Y radius before rotation.
        #[serde(serialize_with = "num::ser")]
        radius_y: f64,
        /// Rotation of the x radius, degrees.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        rotation: Option<f64>,
        /// Unknown keys, preserved.
        #[serde(flatten)]
        other: Map<String, Value>,
    },
    /// Open path.
    Polyline {
        /// Vertices.
        #[serde(serialize_with = "num::ser")]
        points: Vec<Point>,
        /// One bulge per segment.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        bulges: Option<Vec<f64>>,
        /// Unknown keys, preserved.
        #[serde(flatten)]
        other: Map<String, Value>,
    },
    /// Circular arc, counter-clockwise from start to end angle.
    Arc {
        /// Centre.
        #[serde(serialize_with = "num::ser")]
        center: Point,
        /// Radius.
        #[serde(serialize_with = "num::ser")]
        radius: f64,
        /// Start angle, degrees.
        #[serde(serialize_with = "num::ser")]
        start_angle: f64,
        /// End angle, degrees.
        #[serde(serialize_with = "num::ser")]
        end_angle: f64,
        /// Unknown keys, preserved.
        #[serde(flatten)]
        other: Map<String, Value>,
    },
    /// A shape of an unknown `type`, preserved as the original object.
    #[serde(skip)]
    Unknown(Map<String, Value>),
}

const KNOWN_SHAPES: [&str; 5] = ["polygon", "circle", "ellipse", "polyline", "arc"];

impl Serialize for Shape {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Shape::Unknown(map) => map.serialize(s),
            _ => Shape::serialize(self, s),
        }
    }
}

impl<'de> Deserialize<'de> for Shape {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let Value::Object(map) = Value::deserialize(d)? else {
            return Err(D::Error::custom("shape must be an object"));
        };
        let known = map
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| KNOWN_SHAPES.contains(&t));
        if known {
            Shape::deserialize(Value::Object(map)).map_err(D::Error::custom)
        } else if map.contains_key("type") {
            Ok(Shape::Unknown(map))
        } else {
            Err(D::Error::missing_field("type"))
        }
    }
}

element! {
    /// A region of floor with a shape and a use.
    pub struct Space {
        /// Required id.
        pub id: String,
        /// Outline.
        pub shape: Shape,
        /// Dictionary value; unknown values are kept as-is.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub r#use: Option<String>,
        /// Finished to the building's occupied standard.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub finished: Option<bool>,
        /// `interior`, `enclosed`, `screened`, `covered` or `open`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub enclosure: Option<String>,
        /// Id of the space this one subdivides.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub within: Option<String>,
        /// Deducted from the `within` parent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub subtract: Option<bool>,
        /// Area multiplier.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub area_factor: Option<f64>,
        /// Hand-entered area with reason.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub declared_area: Option<Value>,
        /// Id of the unit of occupancy.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub unit: Option<String>,
        /// Boundary the shape is drawn to.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub measured_to: Option<String>,
        /// Space height where it differs from the ceiling.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub height: Option<f64>,
        /// How the space is reached.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub access: Option<String>,
    }
}

element! {
    /// A wall segment on a level.
    pub struct Wall {
        /// Required id.
        pub id: String,
        /// Start point.
        #[serde(serialize_with = "num::ser")]
        pub start: Point,
        /// End point.
        #[serde(serialize_with = "num::ser")]
        pub end: Point,
        /// Curvature (bulge convention).
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub bulge: Option<f64>,
        /// Thickness.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub thickness: Option<f64>,
        /// Dictionary wall kind.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub kind: Option<String>,
        /// `center`, `left` or `right`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub alignment: Option<String>,
        /// Height where it differs from the ceiling.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub height: Option<f64>,
        /// Space edges this wall forms (`{space, edge}`).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub bounds: Vec<Value>,
    }
}

element! {
    /// A door, window or other gap.
    pub struct Opening {
        /// Required id.
        pub id: String,
        /// Dictionary opening kind.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub kind: Option<String>,
        /// Host wall id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub wall: Option<String>,
        /// Host space id (with `edge`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub space: Option<String>,
        /// Host space edge index.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub edge: Option<u32>,
        /// Distance along the host to the near side.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub offset: Option<f64>,
        /// Width.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub width: Option<f64>,
        /// Height.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub height: Option<f64>,
        /// Sill height.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub sill_height: Option<f64>,
        /// Door swing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub swing: Option<String>,
        /// Open-string material.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub material: Option<String>,
        /// Closed shape projecting past the wall (bay windows).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub projection: Option<Shape>,
    }
}

element! {
    /// A fixed item on a level: stair, elevator, fixture.
    pub struct Feature {
        /// Required id.
        pub id: String,
        /// Dictionary feature kind.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub kind: Option<String>,
        /// Centre position.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub position: Option<Point>,
        /// Rotation, degrees.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub rotation: Option<f64>,
        /// Width along local x.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub width: Option<f64>,
        /// Length along local y.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub length: Option<f64>,
        /// Level a stair leads to.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub connects_to: Option<String>,
        /// Finished flag.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub finished: Option<bool>,
    }
}

element! {
    /// Part of a level with no floor.
    pub struct Void {
        /// Required id.
        pub id: String,
        /// Outline.
        pub shape: Shape,
    }
}

element! {
    /// A separately occupied part of the property.
    pub struct Unit {
        /// Required id.
        pub id: String,
        /// Dictionary unit kind.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub kind: Option<String>,
        /// Subordinate to a primary dwelling.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub accessory: Option<bool>,
    }
}

element! {
    /// One floor of a structure.
    pub struct Level {
        /// Required id.
        pub id: String,
        /// `above`, `below` or `partially_below`.
        pub grade: String,
        /// Counts up from 1 above grade, down from 1 below.
        pub sequence: i64,
        /// Floor height relative to the ground reference.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub elevation: Option<f64>,
        /// Spaces on the level.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub spaces: Vec<Space>,
        /// Walls.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub walls: Vec<Wall>,
        /// Openings.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub openings: Vec<Opening>,
        /// Features.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub features: Vec<Feature>,
        /// Voids.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub voids: Vec<Void>,
    }
}

element! {
    /// A building or other improvement.
    pub struct Structure {
        /// Required id.
        pub id: String,
        /// Dictionary structure kind.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub kind: Option<String>,
        /// `detached`, `attached` or `semi_detached`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub attachment: Option<String>,
        /// Origin of the structure frame in the site frame.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub position: Option<Point>,
        /// Rotation of the structure frame, degrees.
        #[serde(default, skip_serializing_if = "Option::is_none", serialize_with = "num::ser")]
        pub rotation: Option<f64>,
        /// Floors.
        pub levels: Vec<Level>,
    }
}

/// How the sketch is drawn (spec section 11); everything but annotations is raw JSON.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Presentation {
    /// Drawing marks, in drawing order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Annotation>,
    /// Layers, styles, pages, underlays and anything else.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

/// A sketchy document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Always `"sketchy"`.
    pub format: String,
    /// `MAJOR.MINOR`.
    pub format_version: String,
    /// JSON Schema URL.
    #[serde(default, rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    /// Extension namespaces present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions_used: Option<Vec<String>>,
    /// Extension namespaces a reader must understand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions_required: Option<Vec<String>>,
    /// Identity of the sketch across revisions.
    pub id: String,
    /// This version of the sketch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<Revision>,
    /// Summaries of earlier revisions (raw JSON or [`Revision`]-shaped).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<Value>>,
    /// `{document, revision}` this was copied from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derived_from: Option<Value>,
    /// Human-readable name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// `{name, version}` of the writing program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generator: Option<Value>,
    /// `ft`, `in`, `m`, `cm` or `mm`.
    pub length_unit: String,
    /// Default boundary spaces are drawn to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measured_to: Option<String>,
    /// Address and identifiers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property: Option<Value>,
    /// Georeference and site-level spaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<Value>,
    /// Units of occupancy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<Unit>,
    /// Buildings.
    pub structures: Vec<Structure>,
    /// Code tables.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classification_schemes: Vec<ClassificationScheme>,
    /// Reusable drawings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definitions: Option<Value>,
    /// Drawing layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<Presentation>,
    /// Stored totals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub results: Option<Value>,
    /// User-entered key-value pairs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attributes: Option<BTreeMap<String, Value>>,
    /// Comments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<Value>,
    /// Namespaced extension data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extras: Option<Map<String, Value>>,
    /// Unknown keys, preserved.
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl Document {
    /// Parses a document, rejecting non-sketchy files and any `format_version` outside
    /// [`SUPPORTED_VERSIONS`]. Draft `0.x` minors may break each other, so a newer minor is
    /// rejected rather than guessed at.
    pub fn from_json(text: &str) -> Result<Self, SketchyError> {
        let value: Value = serde_json::from_str(text).map_err(|e| SketchyError::Json(e.to_string()))?;
        Self::from_value(value)
    }

    /// Builds a document from parsed JSON: rejects non-sketchy values, unsupported
    /// versions and nesting deeper than [`MAX_DEPTH`].
    pub fn from_value(value: Value) -> Result<Self, SketchyError> {
        if too_deep(&value) {
            return Err(SketchyError::Json(format!("document is nested deeper than {MAX_DEPTH} levels")));
        }
        if value.get("format").and_then(Value::as_str) != Some(FORMAT) {
            return Err(SketchyError::NotSketchy);
        }
        let version = value
            .get("format_version")
            .and_then(Value::as_str)
            .ok_or_else(|| SketchyError::Json("missing format_version".to_string()))?;
        if !SUPPORTED_VERSIONS.contains(&version) {
            return Err(SketchyError::UnsupportedVersion(version.to_string()));
        }
        serde_json::from_value(value).map_err(|e| SketchyError::Json(e.to_string()))
    }

    /// Serializes to a JSON value (whole-valued numbers as integers).
    pub fn to_value(&self) -> Value {
        // Serialization of plain data structs cannot fail; Null is unreachable in practice.
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}
