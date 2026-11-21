//! The explorer's model (F-10): a folder tree built from the vault's file
//! paths, which folders are open, the rows that show, and the rules for
//! note and folder names. Pure logic; the view is `screens/explorer.rs`.

use std::collections::{BTreeMap, BTreeSet, HashSet};

/// What a row is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A folder; its path has no trailing `/`.
    Folder { open: bool },
    /// A note (`.md`).
    Note,
    /// Any other file.
    Attachment,
}

/// One visible row of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Vault-relative path.
    pub path: String,
    /// What shows: a folder's or attachment's name, a note's name without `.md`.
    pub name: String,
    pub depth: usize,
    pub kind: Kind,
}

/// The vault's folders and files.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    /// Folder → its subfolders and files (by name, sorted).
    folders: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>,
    open: HashSet<String>,
}

fn parent(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

impl Tree {
    /// A tree of `files` (vault-relative paths) and `extra_folders` (folders
    /// with no files yet). Remembers which folders are open in `open`.
    pub fn new<'a>(
        files: impl IntoIterator<Item = &'a str>,
        extra_folders: impl IntoIterator<Item = &'a str>,
        open: HashSet<String>,
    ) -> Self {
        let mut t = Self {
            folders: BTreeMap::new(),
            open,
        };
        t.folders.insert(String::new(), Default::default());
        for f in extra_folders {
            t.add_folder(f);
        }
        for f in files {
            let (dir, name) = parent(f);
            t.add_folder(dir);
            if let Some(entry) = t.folders.get_mut(dir) {
                entry.1.insert(name.to_owned());
            }
        }
        t
    }

    fn add_folder(&mut self, dir: &str) {
        let mut path = dir;
        while !path.is_empty() && !self.folders.contains_key(path) {
            self.folders.insert(path.to_owned(), Default::default());
            let (up, name) = parent(path);
            self.folders
                .entry(up.to_owned())
                .or_default()
                .0
                .insert(name.to_owned());
            path = up;
        }
    }

    /// Whether `path` is a folder.
    pub fn is_folder(&self, path: &str) -> bool {
        self.folders.contains_key(path)
    }

    /// Every folder, the root first (as `""`).
    pub fn folders(&self) -> impl Iterator<Item = &str> {
        self.folders.keys().map(String::as_str)
    }

    /// The notes and files under folder `path`, at any depth.
    pub fn files_under(&self, path: &str) -> Vec<String> {
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}/")
        };
        self.folders
            .iter()
            .filter(|(dir, _)| dir.as_str() == path || dir.starts_with(&prefix))
            .flat_map(|(dir, (_, files))| {
                files.iter().map(move |f| {
                    if dir.is_empty() {
                        f.clone()
                    } else {
                        format!("{dir}/{f}")
                    }
                })
            })
            .collect()
    }

    /// Opens or closes a folder.
    pub fn set_open(&mut self, path: &str, open: bool) {
        if open {
            self.open.insert(path.to_owned());
        } else {
            self.open.remove(path);
        }
    }

    /// The open folders, to remember them.
    pub fn open_folders(&self) -> &HashSet<String> {
        &self.open
    }

    /// The rows that show: folders first, then files, each sorted by name
    /// (case-insensitive); the content of open folders under them.
    pub fn rows(&self) -> Vec<Row> {
        let mut out = Vec::new();
        self.push_rows("", 0, &mut out);
        out
    }

    fn push_rows(&self, dir: &str, depth: usize, out: &mut Vec<Row>) {
        let Some((subs, files)) = self.folders.get(dir) else {
            return;
        };
        let join = |name: &str| {
            if dir.is_empty() {
                name.to_owned()
            } else {
                format!("{dir}/{name}")
            }
        };
        let mut subs: Vec<&String> = subs.iter().collect();
        subs.sort_by_key(|s| s.to_lowercase());
        for s in subs {
            let path = join(s);
            let open = self.open.contains(&path);
            out.push(Row {
                name: s.clone(),
                depth,
                kind: Kind::Folder { open },
                path: path.clone(),
            });
            if open {
                self.push_rows(&path, depth + 1, out);
            }
        }
        let mut files: Vec<&String> = files.iter().collect();
        files.sort_by_key(|s| s.to_lowercase());
        for f in files {
            let (name, kind) = match f.strip_suffix(".md") {
                Some(n) => (n.to_owned(), Kind::Note),
                None => (f.clone(), Kind::Attachment),
            };
            out.push(Row {
                path: join(f),
                name,
                depth,
                kind,
            });
        }
    }
}

/// What is wrong with a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameCheck {
    Ok,
    /// Allowed, but this character breaks wikilinks to the note (F-10).
    Warning(char),
    /// Not allowed (F-10.2).
    Rejected,
}

/// Characters a note or folder name may not contain (F-10).
pub const FORBIDDEN: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];
/// Characters that work in a file name but break wikilinks (F-10).
pub const RISKY: [char; 4] = ['#', '^', '[', ']'];

/// The message under an input whose name is rejected (mockup 14).
pub const REJECTED: &str = "Names can't contain \\ / : * ? \" < > |";

/// Checks a note or folder name.
pub fn check_name(name: &str) -> NameCheck {
    if name.contains(FORBIDDEN) || name.trim() == "." || name.trim() == ".." {
        return NameCheck::Rejected;
    }
    match name.chars().find(|c| RISKY.contains(c)) {
        Some(c) => NameCheck::Warning(c),
        None => NameCheck::Ok,
    }
}

/// The warning under an input with a risky name (mockup 14).
pub fn warning(c: char) -> String {
    format!("{c} can break wikilinks to this note. Press Enter again to create it anyway.")
}

/// The name of a new note: `name` trimmed, or "Untitled", "Untitled 1", …
/// when it is empty, avoiding names in `taken` (lowercase, without `.md`).
pub fn new_note_name(name: &str, taken: &HashSet<String>) -> String {
    let name = name.trim();
    if !name.is_empty() {
        return name.strip_suffix(".md").unwrap_or(name).to_owned();
    }
    let mut candidate = "Untitled".to_owned();
    let mut n = 1;
    while taken.contains(&candidate.to_lowercase()) {
        candidate = format!("Untitled {n}");
        n += 1;
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(open: &[&str]) -> Tree {
        Tree::new(
            [
                "Harian/2026-11-19.md",
                "Proyek/Patinote.md",
                "Proyek/Desain indeks Patinote.md",
                "Proyek/Lampiran/a.png",
                "Inbox.md",
            ],
            ["Kosong"],
            open.iter().map(|s| s.to_string()).collect(),
        )
    }

    #[test]
    fn rows_show_folders_first_and_open_folders_inside() {
        let names: Vec<(String, usize)> = tree(&["Proyek"])
            .rows()
            .into_iter()
            .map(|r| (r.name, r.depth))
            .collect();
        assert_eq!(
            names,
            [
                ("Harian".into(), 0),
                ("Kosong".into(), 0),
                ("Proyek".into(), 0),
                ("Lampiran".into(), 1),
                ("Desain indeks Patinote".into(), 1),
                ("Patinote".into(), 1),
                ("Inbox".into(), 0),
            ]
        );
    }

    #[test]
    fn files_under_a_folder_include_subfolders() {
        let mut under = tree(&[]).files_under("Proyek");
        under.sort();
        assert_eq!(
            under,
            [
                "Proyek/Desain indeks Patinote.md",
                "Proyek/Lampiran/a.png",
                "Proyek/Patinote.md"
            ]
        );
        assert!(tree(&[]).is_folder("Kosong"));
    }

    #[test]
    fn f10_2_forbidden_names_are_rejected_and_risky_ones_warned() {
        assert_eq!(check_name("Rapat: evaluasi"), NameCheck::Rejected);
        assert_eq!(check_name("a/b"), NameCheck::Rejected);
        assert_eq!(check_name("Ide #rapat"), NameCheck::Warning('#'));
        assert_eq!(check_name("Rapat IKN"), NameCheck::Ok);
        assert_eq!(
            warning('#'),
            "# can break wikilinks to this note. Press Enter again to create it anyway."
        );
    }

    #[test]
    fn empty_names_become_untitled() {
        let taken: HashSet<String> = ["untitled".to_owned(), "untitled 1".to_owned()].into();
        assert_eq!(new_note_name("  ", &taken), "Untitled 2");
        assert_eq!(new_note_name("Ide.md", &taken), "Ide");
    }
}
