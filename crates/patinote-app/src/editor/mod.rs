//! The note editor: how a note is read and written, Markdown coloring, and
//! the widget that draws it. The Vim engine is in [`crate::vim`].

pub mod document;
pub mod file;
pub mod highlight;
pub mod view;
