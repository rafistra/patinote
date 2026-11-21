//! A note's bytes on disk and the text the editor works on (F-05).
//!
//! The editor sees text with `\n` line endings and no BOM. Saving puts back
//! what the file had: a BOM, CRLF endings (the first line ending decides),
//! and the trailing newline (kept by the Vim buffer). Notes over 5 MB and
//! files that are not UTF-8 open read-only (F-05.8).

use patinote_core::MAX_PARSE_BYTES;
use patinote_core::store::Hash;
use patinote_core::write::content_hash;

const BOM: &[u8] = b"\xEF\xBB\xBF";

/// How the file writes its text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Format {
    pub bom: bool,
    pub crlf: bool,
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
    let text = if decoded.contains('\r') {
        decoded.replace("\r\n", "\n")
    } else {
        decoded.into_owned()
    };
    Opened {
        text,
        format: Format { bom, crlf },
        read_only,
        hash,
    }
}

/// The bytes to write for `text` in `format`.
pub fn encode(text: &str, format: Format) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 3);
    if format.bom {
        out.extend_from_slice(BOM);
    }
    if format.crlf {
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
            assert_eq!(encode(&o.text, o.format), bytes, "{bytes:?}");
            assert_eq!(o.hash, content_hash(bytes));
        }
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
