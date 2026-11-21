//! What the app remembers between sessions (PRD "Gerbang 2: dampak
//! arsitektur", point 3). Never inside the vault:
//!
//! - `$XDG_STATE_HOME/patinote/app.json`: the last vault and window size.
//! - `$XDG_STATE_HOME/patinote/<vault-id>/ui.json`: layout, open buffers and
//!   recent notes of one vault.
//!
//! Files are written whole to a temporary file and renamed, so a crash
//! leaves the old or the new file, never half of one. A missing or unreadable
//! file is the default state.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use patinote_core::config::Layout;
use serde::{Deserialize, Serialize};

/// How many recent notes are kept.
pub const RECENT_KEPT: usize = 50;

/// State shared by every vault.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    /// The vault opened last (F-01).
    pub last_vault: Option<PathBuf>,
    /// Window size in logical pixels (F-28).
    pub window: Option<(f32, f32)>,
    /// Text size, 1.0 being 100 % (F-28: `Ctrl+=`, `Ctrl+-`, `Ctrl+0`).
    pub zoom: Option<f32>,
}

/// A note opened recently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recent {
    /// Path relative to the vault.
    pub path: String,
    /// When it was last opened, in seconds since the Unix epoch.
    pub opened: u64,
}

/// State of one vault.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultState {
    /// The layout used last; `None` until the user changes it, so the
    /// configured default applies to a new install (F-28.12).
    pub layout: Option<Layout>,
    /// Open buffers, by path relative to the vault, in bufferline order.
    pub buffers: Vec<String>,
    /// Index of the active buffer.
    pub active: Option<usize>,
    /// Recently opened notes, newest first.
    pub recent: Vec<Recent>,
    /// The side panel and the explorer were open (F-28.11).
    pub panel_open: bool,
    pub explorer_open: bool,
    /// Widths the panels were dragged to (F-28.7).
    pub explorer_w: Option<f32>,
    pub panel_w: Option<f32>,
    /// The cursor (line, character) of each open buffer, to restore the
    /// session (F-09.6).
    pub cursors: HashMap<String, (usize, usize)>,
}

impl VaultState {
    /// Records that `path` was opened at `now`.
    pub fn touch(&mut self, path: &str, now: u64) {
        self.recent.retain(|r| r.path != path);
        self.recent.insert(
            0,
            Recent {
                path: path.to_owned(),
                opened: now,
            },
        );
        self.recent.truncate(RECENT_KEPT);
    }
}

/// `<state home>/patinote/app.json`.
pub fn app_state_path(state_home: &Path) -> PathBuf {
    state_home.join("patinote").join("app.json")
}

/// `<state home>/patinote/<vault-id>/ui.json`.
pub fn vault_state_path(state_home: &Path, vault_id: &str) -> PathBuf {
    patinote_core::location::vault_state_dir(state_home, vault_id).join("ui.json")
}

/// Reads a state file; missing or damaged files give the default.
pub fn load<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Writes a state file atomically.
pub fn save<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let dir = path.parent().ok_or(std::io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(dir)?;
    let json = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// Seconds since the Unix epoch.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_survives_a_save_and_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = vault_state_path(dir.path(), "abc");
        let mut state = VaultState {
            layout: Some(Layout::Classic),
            buffers: vec!["a.md".into(), "b.md".into()],
            active: Some(1),
            ..VaultState::default()
        };
        state.touch("a.md", 10);
        state.touch("b.md", 20);
        state.touch("a.md", 30);
        save(&path, &state).unwrap();
        let back: VaultState = load(&path);
        assert_eq!(back, state);
        assert_eq!(back.recent[0].path, "a.md");
        assert_eq!(back.recent.len(), 2);
    }

    #[test]
    fn missing_or_damaged_files_give_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = app_state_path(dir.path());
        assert_eq!(load::<AppState>(&path), AppState::default());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(load::<AppState>(&path), AppState::default());
    }
}
