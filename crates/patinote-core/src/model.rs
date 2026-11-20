//! Parse results. The shape follows the index schema (`0001_init.sql`) and the
//! golden corpus format (docs/design/0004-korpus-uji-parser.md).

use serde::Serialize;
use serde::ser::{SerializeStruct, Serializer};

/// Everything the parser reads from one note.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ParsedNote {
    /// Frontmatter `title`, else the first H1, else `None` (the indexer falls
    /// back to the file name).
    pub title: Option<String>,
    /// From frontmatter `aliases` (or legacy `alias`).
    pub aliases: Vec<String>,
    /// Outgoing links in file order.
    pub links: Vec<Link>,
    /// Frontmatter tags first, then inline tags in file order.
    pub tags: Vec<Tag>,
    pub headings: Vec<Heading>,
    pub blocks: Vec<Block>,
    /// One row per frontmatter value, in key order.
    pub properties: Vec<Property>,
    pub warnings: Vec<Warning>,
}

/// How a link is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    /// `[[Note]]`
    Wiki,
    /// `![[Note]]` or `![alt](image.png)`
    Embed,
    /// `[text](Note.md)`
    Markdown,
    /// `key: "[[Note]]"` in frontmatter
    Frontmatter,
}

/// One outgoing link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Link {
    pub kind: LinkKind,
    /// The exact text in the file, including `!` for embeds.
    pub raw: String,
    /// The path part as written; empty for links within the same note.
    pub target: String,
    /// Heading path (`A#B`) or block reference (`^id`).
    pub subpath: Option<String>,
    /// Text after `|`, or the text of a Markdown link.
    pub display: Option<String>,
    /// 1-based line of `byte_start`.
    pub line: usize,
    /// Byte offsets of `raw` in the file as is (including any BOM); end exclusive.
    pub byte_start: usize,
    pub byte_end: usize,
}

/// Where a tag comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TagSource {
    Inline,
    Frontmatter,
}

/// One tag occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tag {
    /// Lowercase, without `#`.
    pub tag: String,
    /// Original spelling, without `#`.
    pub display: String,
    pub source: TagSource,
    /// 1-based line; `None` for frontmatter tags.
    pub line: Option<usize>,
}

/// One heading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Heading {
    pub level: u8,
    /// Heading text without markers, trimmed.
    pub text: String,
    pub line: usize,
    /// Byte offset of the start of the heading line.
    pub byte_start: usize,
}

/// One block ID (`^abc-123`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Block {
    /// Without `^`.
    pub block_id: String,
    pub line: usize,
}

/// A frontmatter value. Lists are split into one value per item.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Text(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    /// `YYYY-MM-DD`
    Date(String),
    /// `YYYY-MM-DDTHH:MM[:SS…]`
    DateTime(String),
    Null,
}

impl PropertyValue {
    /// The type name stored in the index (`properties.type`).
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Integer(_) | Self::Float(_) => "number",
            Self::Bool(_) => "bool",
            Self::Date(_) => "date",
            Self::DateTime(_) => "datetime",
            Self::Null => "null",
        }
    }
}

/// One frontmatter value.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    /// Key as written.
    pub key: String,
    /// Position in a list; 0 for single values.
    pub idx: usize,
    pub value: PropertyValue,
}

impl Serialize for Property {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("Property", 4)?;
        s.serialize_field("key", &self.key)?;
        s.serialize_field("idx", &self.idx)?;
        s.serialize_field("type", self.value.type_name())?;
        match &self.value {
            PropertyValue::Text(v) | PropertyValue::Date(v) | PropertyValue::DateTime(v) => {
                s.serialize_field("value", v)?
            }
            PropertyValue::Integer(v) => s.serialize_field("value", v)?,
            PropertyValue::Float(v) => s.serialize_field("value", v)?,
            PropertyValue::Bool(v) => s.serialize_field("value", v)?,
            PropertyValue::Null => s.serialize_field("value", &None::<()>)?,
        }
        s.end()
    }
}

/// A problem that did not stop parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Warning {
    pub code: WarningCode,
}

/// Machine-readable warning codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    /// The frontmatter is not valid YAML or not a key-value map.
    FrontmatterInvalid,
}
