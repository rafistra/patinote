//! Editor modes shown in the status line badge (0003 section 6).

/// The mode badge in the status line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    Visual,
    Read,
    Conflict,
}

impl Mode {
    /// The badge text.
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Visual => "VISUAL",
            Self::Read => "READ",
            Self::Conflict => "CONFLICT",
        }
    }
}
