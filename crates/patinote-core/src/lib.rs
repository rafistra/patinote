//! Core of Patinote: note model, Markdown parser, link resolver, vault
//! scanner, file watcher, write path and command registry.
//!
//! This crate must not depend on SQLite, Iced or any UI crate. Storage is
//! reached through the `Store` trait, implemented by `patinote-index`.

pub mod model;
pub mod parser;

pub use model::{
    Block, Heading, Link, LinkKind, ParsedNote, Property, PropertyValue, Tag, TagSource, Warning,
    WarningCode,
};
pub use parser::parse;
