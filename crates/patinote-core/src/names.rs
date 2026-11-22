//! Name normalization shared by the index and the link resolver
//! (docs/design/0001, section 4).

use std::fs;
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

/// Lowercase NFC form used for case-insensitive matching of paths, names,
/// aliases and tags.
pub fn fold(s: &str) -> String {
    s.nfc().collect::<String>().to_lowercase()
}

/// NFC form, as paths are stored (F-01).
pub fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// The vault path `rel` (NFC, as the index keeps it) as it is named on
/// disk, relative to `root`. A file or folder whose name is stored in
/// another form (NFD, common from macOS) is found by reading its folder;
/// parts that do not exist yet are kept as given. Nothing is read when the
/// path exists as written (Q-019).
pub fn disk_rel(root: &Path, rel: &str) -> String {
    if fs::symlink_metadata(root.join(rel)).is_ok() {
        return rel.to_owned();
    }
    let mut dir = root.to_path_buf();
    let mut out: Vec<String> = Vec::new();
    let mut parts = rel.split('/');
    while let Some(part) = parts.next() {
        let here = dir.join(part);
        let name = if fs::symlink_metadata(&here).is_ok() {
            Some(part.to_owned())
        } else if part.is_ascii() {
            // An ASCII name has no other form to look for (a few signs,
            // like the Kelvin sign, normalize to ASCII; they are not
            // looked up): it does not exist yet.
            None
        } else {
            let want = nfc(part);
            fs::read_dir(&dir).ok().and_then(|entries| {
                entries.filter_map(Result::ok).find_map(|e| {
                    let n = e.file_name().into_string().ok()?;
                    // An ASCII name is its own NFC form, so it can't be
                    // another form of this non-ASCII one.
                    (!n.is_ascii() && n != part && nfc(&n) == want).then_some(n)
                })
            })
        };
        match name {
            Some(n) => {
                dir.push(&n);
                out.push(n);
            }
            None => {
                out.push(part.to_owned());
                out.extend(parts.map(str::to_owned));
                break;
            }
        }
    }
    out.join("/")
}

/// [`disk_rel`] joined to `root`: the file to open for vault path `rel`.
pub fn on_disk(root: &Path, rel: &str) -> PathBuf {
    root.join(disk_rel(root, rel))
}

/// The key a link target is matched with: folded, without a `.md` extension.
pub fn link_key(target: &str) -> String {
    let folded = fold(target);
    match folded.strip_suffix(".md") {
        Some(stem) if !stem.is_empty() => stem.to_owned(),
        _ => folded,
    }
}

/// File name without folder and extension: `Proyek/Rapat.md` → `Rapat`.
pub fn stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

/// Lowercase extension without the dot, or an empty string.
pub fn extension(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_and_keys() {
        assert_eq!(fold("Cafe\u{301}"), "café");
        assert_eq!(link_key("Proyek/Rapat.MD"), "proyek/rapat");
        assert_eq!(link_key(".md"), ".md");
        assert_eq!(stem("Proyek/Rapat 19 Nov.md"), "Rapat 19 Nov");
        assert_eq!(stem(".hidden"), ".hidden");
        assert_eq!(extension("a/b.PNG"), "png");
        assert_eq!(extension("LICENSE"), "");
    }
}
