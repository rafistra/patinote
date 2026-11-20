//! Where Patinote keeps data outside the vault (docs/design/0001, section 2;
//! CLAUDE.md, section 5). Nothing here ever points inside a vault.

use std::path::{Path, PathBuf};
use std::{fs, io};

/// Identifies a vault by its canonical path: the first 16 hex characters of
/// its blake3 hash. A moved or renamed vault gets a new ID, and a new index.
pub fn vault_id(root: &Path) -> io::Result<String> {
    Ok(vault_id_of_canonical(&fs::canonicalize(root)?))
}

/// [`vault_id`] for a path that is already canonical.
pub fn vault_id_of_canonical(path: &Path) -> String {
    let hash = blake3::hash(path.as_os_str().as_encoded_bytes());
    hash.to_hex()[..16].to_owned()
}

/// `$XDG_CACHE_HOME`, or `~/.cache`. Relative values are ignored, as the XDG
/// specification requires.
pub fn cache_home() -> Option<PathBuf> {
    xdg_home("XDG_CACHE_HOME", ".cache")
}

/// The cache folder of one vault: `<cache home>/patinote/<vault-id>/`.
pub fn vault_cache_dir(cache_home: &Path, vault_id: &str) -> PathBuf {
    cache_home.join("patinote").join(vault_id)
}

/// `$XDG_STATE_HOME`, or `~/.local/state`.
pub fn state_home() -> Option<PathBuf> {
    xdg_home("XDG_STATE_HOME", ".local/state")
}

/// The state folder of one vault: `<state home>/patinote/<vault-id>/`
/// (open buffers, cursor positions, the write journal).
pub fn vault_state_dir(state_home: &Path, vault_id: &str) -> PathBuf {
    state_home.join("patinote").join(vault_id)
}

fn xdg_home(var: &str, fallback: &str) -> Option<PathBuf> {
    match std::env::var_os(var).map(PathBuf::from) {
        Some(p) if p.is_absolute() => Some(p),
        _ => std::env::var_os("HOME").map(|h| PathBuf::from(h).join(fallback)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_distinct() {
        let a = vault_id_of_canonical(Path::new("/vaults/a"));
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(a, vault_id_of_canonical(Path::new("/vaults/a")));
        assert_ne!(a, vault_id_of_canonical(Path::new("/vaults/b")));
    }
}
