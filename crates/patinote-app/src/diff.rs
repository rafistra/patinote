//! Line diffs for comparing two versions of a note (F-11, mockups 08 and
//! 16): which lines exist only on one side, laid out as two columns.

use similar::{ChangeTag, TextDiff};

/// One line of one side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// 1-based line number on that side.
    pub number: usize,
    pub text: String,
    /// The line is not on the other side.
    pub only_here: bool,
}

/// Both sides of a comparison.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sides {
    pub left: Vec<Line>,
    pub right: Vec<Line>,
}

impl Sides {
    /// Lines only on the left side.
    pub fn left_only(&self) -> usize {
        self.left.iter().filter(|l| l.only_here).count()
    }

    /// Lines only on the right side.
    pub fn right_only(&self) -> usize {
        self.right.iter().filter(|l| l.only_here).count()
    }
}

/// Compares `left` and `right` line by line.
pub fn sides(left: &str, right: &str) -> Sides {
    let diff = TextDiff::from_lines(left, right);
    let mut out = Sides::default();
    for change in diff.iter_all_changes() {
        let text = change.value().trim_end_matches(['\n', '\r']).to_owned();
        match change.tag() {
            ChangeTag::Equal => {
                out.left.push(Line {
                    number: out.left.len() + 1,
                    text: text.clone(),
                    only_here: false,
                });
                out.right.push(Line {
                    number: out.right.len() + 1,
                    text,
                    only_here: false,
                });
            }
            ChangeTag::Delete => out.left.push(Line {
                number: out.left.len() + 1,
                text,
                only_here: true,
            }),
            ChangeTag::Insert => out.right.push(Line {
                number: out.right.len() + 1,
                text,
                only_here: true,
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f11_4_lines_only_on_one_side_are_marked() {
        let disk = "# Rapat 19 Nov\n\n- Indeks\n- FTS5 dulu, tantivy kalau perlu\n- Cek ulang angka benchmark\n- Rename memperbarui link\n";
        let mine = "# Rapat 19 Nov\n\n- Indeks\n- Mulai dengan FTS5; tantivy bila > 50 ms\n- Rename memperbarui link\n- Prototipe editor minggu depan\n";
        let s = sides(disk, mine);
        let left: Vec<(usize, bool)> = s.left.iter().map(|l| (l.number, l.only_here)).collect();
        let right: Vec<(usize, bool)> = s.right.iter().map(|l| (l.number, l.only_here)).collect();
        assert_eq!(
            left,
            [
                (1, false),
                (2, false),
                (3, false),
                (4, true),
                (5, true),
                (6, false)
            ]
        );
        assert_eq!(
            right,
            [
                (1, false),
                (2, false),
                (3, false),
                (4, true),
                (5, false),
                (6, true)
            ]
        );
        assert_eq!((s.left_only(), s.right_only()), (2, 2));
    }
}
