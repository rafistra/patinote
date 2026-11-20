//! Name normalization shared by the index and the link resolver
//! (docs/design/0001, section 4).

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
