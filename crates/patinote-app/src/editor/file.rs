//! A note's bytes on disk and the text the editor works on (F-05).
//!
//! The editor sees text with `\n` line endings and no BOM. Saving puts back
//! what the file had: a BOM, CRLF endings (the first line ending decides),
//! and the trailing newline (kept by the Vim buffer). In a file that mixes
//! CRLF and LF, every unchanged line keeps its own ending. Notes over 5 MB and
//! files that are not UTF-8 open read-only (F-05.8).

use std::sync::Arc;

use patinote_core::MAX_PARSE_BYTES;
use patinote_core::store::Hash;
use patinote_core::write::content_hash;

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// How the file writes its text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Format {
    pub bom: bool,
    /// The ending of the first line, used for new and changed lines.
    pub crlf: bool,
    /// When the file mixes CRLF and LF: each line as read (without its
    /// ending) and whether it ended in CRLF.
    pub mixed: Option<Arc<Vec<(String, bool)>>>,
}

/// Why a note is read-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOnly {
    TooLarge,
    NotUtf8,
}

impl ReadOnly {
    /// The banner title (mockup 17).
    pub fn title(self) -> &'static str {
        match self {
            Self::TooLarge => "Read-only: this note is larger than 5 MB.",
            Self::NotUtf8 => "Read-only: this file isn't UTF-8 text.",
        }
    }
}

/// A note as read from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The text with `\n` endings and no BOM; lossy for non-UTF-8 files.
    pub text: String,
    pub format: Format,
    /// `Some` when the note cannot be edited.
    pub read_only: Option<ReadOnly>,
    /// Hash of the bytes on disk, to detect changes made outside.
    pub hash: Hash,
}

/// Reads a note's bytes.
pub fn open(bytes: &[u8]) -> Opened {
    let hash = content_hash(bytes);
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    let bom = body.len() != bytes.len();
    let (decoded, read_only) = match std::str::from_utf8(body) {
        Ok(_) if bytes.len() as u64 > MAX_PARSE_BYTES => {
            (String::from_utf8_lossy(body), Some(ReadOnly::TooLarge))
        }
        Ok(s) => (std::borrow::Cow::Borrowed(s), None),
        Err(_) => (String::from_utf8_lossy(body), Some(ReadOnly::NotUtf8)),
    };
    let crlf = decoded
        .find('\n')
        .is_some_and(|i| i > 0 && decoded.as_bytes()[i - 1] == b'\r');
    let crlf_count = decoded.matches("\r\n").count();
    let mixed = (crlf_count > 0 && crlf_count < decoded.matches('\n').count()).then(|| {
        Arc::new(
            decoded
                .split_inclusive('\n')
                .map(|l| match l.strip_suffix("\r\n") {
                    Some(t) => (t.to_owned(), true),
                    None => (l.strip_suffix('\n').unwrap_or(l).to_owned(), false),
                })
                .collect(),
        )
    });
    let text = if decoded.contains('\r') {
        decoded.replace("\r\n", "\n")
    } else {
        decoded.into_owned()
    };
    Opened {
        text,
        format: Format { bom, crlf, mixed },
        read_only,
        hash,
    }
}

/// The bytes to write for `text` in `format`.
pub fn encode(text: &str, format: &Format) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 3);
    if format.bom {
        out.extend_from_slice(BOM);
    }
    if let Some(old) = &format.mixed {
        // Unchanged lines (as a line diff finds them) keep their ending.
        let new: Vec<&str> = text.split_inclusive('\n').collect();
        let bare: Vec<&str> = new
            .iter()
            .map(|l| l.strip_suffix('\n').unwrap_or(l))
            .collect();
        let was: Vec<&str> = old.iter().map(|(t, _)| t.as_str()).collect();
        let mut crlf = vec![format.crlf; new.len()];
        // No deadline: a coarser diff would change the endings of lines
        // that did not change. Saves encode off the UI thread (`SaveJob::run`).
        for op in similar::capture_diff_slices(similar::Algorithm::Myers, &was, &bare) {
            if let similar::DiffOp::Equal {
                old_index,
                new_index,
                len,
            } = op
            {
                for k in 0..len {
                    crlf[new_index + k] = old[old_index + k].1;
                }
            }
        }
        for (line, crlf) in new.iter().zip(crlf) {
            match line.strip_suffix('\n') {
                Some(t) if crlf => {
                    out.extend_from_slice(t.as_bytes());
                    out.extend_from_slice(b"\r\n");
                }
                _ => out.extend_from_slice(line.as_bytes()),
            }
        }
    } else if format.crlf {
        out.extend_from_slice(text.replace('\n', "\r\n").as_bytes());
    } else {
        out.extend_from_slice(text.as_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f05_4_crlf_and_bom_round_trip() {
        for bytes in [
            &b"a\nb\n"[..],
            b"a\r\nb\r\n",
            b"\xEF\xBB\xBFa\r\nb",
            b"\xEF\xBB\xBF# T\n",
            b"",
            b"no newline",
        ] {
            let o = open(bytes);
            assert!(!o.text.contains('\r'));
            assert!(!o.text.starts_with('\u{feff}'));
            assert_eq!(encode(&o.text, &o.format), bytes, "{bytes:?}");
            assert_eq!(o.hash, content_hash(bytes));
        }
    }

    /// Mixed endings: every line keeps its own; a changed or new line takes
    /// the first line's.
    #[test]
    fn f05_4_mixed_line_endings_are_kept_per_line() {
        let bytes = b"a\r\nb\nc\r\nd";
        let o = open(bytes);
        assert_eq!(o.text, "a\nb\nc\nd");
        assert_eq!(encode(&o.text, &o.format), bytes);
        assert_eq!(
            encode("a!\nb\nbaru\nc\nd", &o.format),
            b"a!\r\nb\nbaru\r\nc\r\nd"
        );
        let o = open(b"a\nb\r\n");
        assert_eq!(encode("a\nb\n", &o.format), b"a\nb\r\n");
    }

    #[test]
    fn f05_8_large_and_non_utf8_notes_are_read_only() {
        assert_eq!(open(b"caf\xE9").read_only, Some(ReadOnly::NotUtf8));
        assert_eq!(open(b"caf\xE9").text, "caf\u{fffd}");
        let big = vec![b'a'; MAX_PARSE_BYTES as usize + 1];
        assert_eq!(open(&big).read_only, Some(ReadOnly::TooLarge));
        assert_eq!(open(b"ok").read_only, None);
    }
}
