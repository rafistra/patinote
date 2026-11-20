//! Obsidian-style link resolution (docs/design/0001, section 4).
//!
//! For a link target, after folding (lowercase, NFC) and dropping `.md`:
//! 1. An empty target (`[[#Heading]]`) is the source note itself.
//! 2. A target with `/` matches the end of a path, at a folder boundary.
//! 3. Otherwise it matches a note name, or an attachment's file name with
//!    its extension.
//! 4. Otherwise it matches a note alias.
//! 5. Several candidates: the shortest path wins, then alphabetical order; the
//!    link is ambiguous.
//! 6. None: the link points to a note that does not exist yet.
//!
//! Markdown links are paths: they are tried relative to the source note's
//! folder, then from the vault root, then with the rules above.

use std::collections::HashMap;

use crate::model::LinkKind;
use crate::names::{fold, link_key, stem};
use crate::scan::FileKind;

/// A file links can point to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// Relative to the vault root, as stored.
    pub path: String,
    pub kind: FileKind,
    /// Note aliases from frontmatter.
    pub aliases: Vec<String>,
}

/// The outcome of resolving one link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved<'a> {
    /// The chosen file.
    pub path: &'a str,
    /// Every matching file, the chosen one first. More than one means the
    /// link is ambiguous (F-08, mockup 12).
    pub candidates: Vec<&'a str>,
}

impl Resolved<'_> {
    /// Whether more than one file matched.
    pub fn is_ambiguous(&self) -> bool {
        self.candidates.len() > 1
    }
}

/// Lookup tables over every link target in a vault.
#[derive(Debug, Default)]
pub struct Resolver {
    targets: Vec<Target>,
    /// Folded full path → target.
    by_path: HashMap<String, usize>,
    /// Folded note stem, or folded attachment file name → targets.
    by_name: HashMap<String, Vec<usize>>,
    /// Folded alias → notes.
    by_alias: HashMap<String, Vec<usize>>,
}

/// Path without `.md` for notes, as written for other files, folded.
fn match_path(t: &Target) -> String {
    let folded = fold(&t.path);
    match t.kind {
        FileKind::Note => folded
            .strip_suffix(".md")
            .map_or(folded.clone(), str::to_owned),
        _ => folded,
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl Resolver {
    /// Builds the tables.
    pub fn new(targets: impl IntoIterator<Item = Target>) -> Self {
        let mut r = Self {
            targets: targets.into_iter().collect(),
            ..Self::default()
        };
        for (i, t) in r.targets.iter().enumerate() {
            r.by_path.insert(fold(&t.path), i);
            let name = match t.kind {
                FileKind::Note => fold(stem(&t.path)),
                _ => fold(file_name(&t.path)),
            };
            r.by_name.entry(name).or_default().push(i);
            if t.kind == FileKind::Note {
                for alias in &t.aliases {
                    r.by_alias.entry(fold(alias)).or_default().push(i);
                }
            }
        }
        r
    }

    /// Resolves a link written in the note at `source`.
    pub fn resolve<'a>(
        &'a self,
        source: &'a str,
        kind: LinkKind,
        target: &str,
    ) -> Option<Resolved<'a>> {
        if target.is_empty() {
            return Some(Resolved {
                path: source,
                candidates: vec![source],
            });
        }
        if (kind == LinkKind::Markdown || (kind == LinkKind::Embed && looks_like_path(target)))
            && let Some(i) = self.markdown_path(source, target)
        {
            let path = self.targets[i].path.as_str();
            return Some(Resolved {
                path,
                candidates: vec![path],
            });
        }
        let key = link_key(target.trim_start_matches('/'));
        let mut found: Vec<usize> = if key.contains('/') {
            let last = key.rsplit('/').next().unwrap_or(&key);
            self.by_name
                .get(last)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&i| {
                    let p = match_path(&self.targets[i]);
                    p == key || p.ends_with(&format!("/{key}"))
                })
                .collect()
        } else {
            self.by_name.get(&key).cloned().unwrap_or_default()
        };
        if found.is_empty() {
            found = self.by_alias.get(&key).cloned().unwrap_or_default();
        }
        if found.is_empty() {
            return None;
        }
        let mut candidates: Vec<&str> = found
            .iter()
            .map(|&i| self.targets[i].path.as_str())
            .collect();
        candidates.sort_by(|a, b| {
            a.chars()
                .count()
                .cmp(&b.chars().count())
                .then_with(|| a.cmp(b))
        });
        candidates.dedup();
        Some(Resolved {
            path: candidates[0],
            candidates,
        })
    }

    /// A Markdown link destination as a path: relative to the source folder,
    /// then from the vault root, with `.md` added when there is no extension.
    fn markdown_path(&self, source: &str, target: &str) -> Option<usize> {
        let dir = source.rsplit_once('/').map_or("", |(d, _)| d);
        let relative = normalize(&format!("{dir}/{target}"));
        let rooted = normalize(target);
        for candidate in [relative, rooted].into_iter().flatten() {
            let folded = fold(&candidate);
            if let Some(&i) = self.by_path.get(&folded) {
                return Some(i);
            }
            if !file_name(&folded).contains('.')
                && let Some(&i) = self.by_path.get(&format!("{folded}.md"))
            {
                return Some(i);
            }
        }
        None
    }
}

/// `![alt](img/a.png)` is a path; `![[a.png]]` is a name.
fn looks_like_path(target: &str) -> bool {
    target.contains('/')
}

/// Resolves `.` and `..` segments; `None` if the path leaves the vault.
fn normalize(path: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(path: &str, aliases: &[&str]) -> Target {
        let kind = if path.ends_with(".md") {
            FileKind::Note
        } else {
            FileKind::Attachment
        };
        Target {
            path: path.into(),
            kind,
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn vault() -> Resolver {
        Resolver::new([
            t("Proyek/Patinote.md", &["Second Brain"]),
            t("Proyek/Rapat.md", &[]),
            t("Arsip/2025/Rapat.md", &[]),
            t("Harian/2026-11-19.md", &[]),
            t("Proyek/Lampiran/diagram-indeks.png", &[]),
            t("Café Bandung.md", &[]),
            t("Proyek/Patinote/Desain antarmuka induk.md", &[]),
        ])
    }

    fn path<'a>(r: &'a Resolver, src: &'a str, kind: LinkKind, target: &str) -> Option<&'a str> {
        r.resolve(src, kind, target).map(|x| x.path)
    }

    #[test]
    fn names_are_case_and_normalization_insensitive() {
        let r = vault();
        assert_eq!(
            path(&r, "a.md", LinkKind::Wiki, "patinote"),
            Some("Proyek/Patinote.md")
        );
        assert_eq!(
            path(&r, "a.md", LinkKind::Wiki, "PATINOTE.md"),
            Some("Proyek/Patinote.md")
        );
        assert_eq!(
            path(&r, "a.md", LinkKind::Wiki, "Cafe\u{301} bandung"),
            Some("Café Bandung.md")
        );
    }

    #[test]
    fn paths_match_at_folder_boundaries() {
        let r = vault();
        assert_eq!(
            path(&r, "a.md", LinkKind::Wiki, "Arsip/2025/Rapat"),
            Some("Arsip/2025/Rapat.md")
        );
        assert_eq!(
            path(&r, "a.md", LinkKind::Wiki, "2025/Rapat.md"),
            Some("Arsip/2025/Rapat.md")
        );
        assert_eq!(
            path(&r, "a.md", LinkKind::Wiki, "/Proyek/Rapat"),
            Some("Proyek/Rapat.md")
        );
        assert_eq!(
            path(&r, "a.md", LinkKind::Wiki, "yek/Rapat"),
            None,
            "not at a folder boundary"
        );
    }

    #[test]
    fn ambiguous_names_pick_the_shortest_path() {
        let r = vault();
        let x = r
            .resolve("Harian/2026-11-19.md", LinkKind::Wiki, "Rapat")
            .unwrap();
        assert_eq!(x.path, "Proyek/Rapat.md");
        assert_eq!(x.candidates, ["Proyek/Rapat.md", "Arsip/2025/Rapat.md"]);
        assert!(x.is_ambiguous());
        assert!(
            !r.resolve("a.md", LinkKind::Wiki, "Proyek/Rapat")
                .unwrap()
                .is_ambiguous()
        );
    }

    #[test]
    fn aliases_attachments_same_note_and_missing() {
        let r = vault();
        assert_eq!(
            path(&r, "a.md", LinkKind::Wiki, "second brain"),
            Some("Proyek/Patinote.md")
        );
        assert_eq!(
            path(&r, "a.md", LinkKind::Embed, "diagram-indeks.png"),
            Some("Proyek/Lampiran/diagram-indeks.png")
        );
        assert_eq!(
            path(&r, "a.md", LinkKind::Embed, "diagram-indeks"),
            None,
            "attachments need their extension"
        );
        assert_eq!(
            path(&r, "Proyek/Rapat.md", LinkKind::Wiki, ""),
            Some("Proyek/Rapat.md")
        );
        assert_eq!(path(&r, "a.md", LinkKind::Wiki, "tantivy"), None);
        assert_eq!(
            path(&r, "a.md", LinkKind::Frontmatter, "Patinote"),
            Some("Proyek/Patinote.md")
        );
    }

    #[test]
    fn markdown_links_are_paths_first() {
        let r = vault();
        let src = "Proyek/Patinote/Desain antarmuka induk.md";
        // Relative to the source folder.
        assert_eq!(
            path(&r, src, LinkKind::Markdown, "../Rapat.md"),
            Some("Proyek/Rapat.md")
        );
        assert_eq!(
            path(&r, "Proyek/Patinote.md", LinkKind::Markdown, "Rapat.md"),
            Some("Proyek/Rapat.md")
        );
        // From the vault root, with or without the extension.
        assert_eq!(
            path(&r, src, LinkKind::Markdown, "Arsip/2025/Rapat.md"),
            Some("Arsip/2025/Rapat.md")
        );
        assert_eq!(
            path(&r, src, LinkKind::Markdown, "Arsip/2025/Rapat"),
            Some("Arsip/2025/Rapat.md")
        );
        // Images by path.
        assert_eq!(
            path(
                &r,
                "Proyek/Patinote.md",
                LinkKind::Embed,
                "Lampiran/diagram-indeks.png"
            ),
            Some("Proyek/Lampiran/diagram-indeks.png")
        );
        // Falling back to names.
        assert_eq!(
            path(&r, "x.md", LinkKind::Markdown, "Patinote.md"),
            Some("Proyek/Patinote.md")
        );
        // Leaving the vault never resolves by path.
        assert_eq!(normalize("../../etc/passwd"), None);
    }
}
