//! Command-line arguments of `patinote-app`. Kept small on purpose: the CLI
//! (`patinote`) is where users type commands; this binary is started by the
//! desktop launcher and by `patinote open`.

use std::ffi::OsString;
use std::path::PathBuf;

/// What the app was asked to do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    /// `--vault <path>`: the vault to open, ahead of every other choice.
    pub vault: Option<PathBuf>,
    /// `--open <path>`: a note to open, relative to the vault.
    pub open: Option<String>,
    /// `--bench`: replay scripted keys on the open note, print measurements
    /// as JSON and exit (step 2.10).
    pub bench: bool,
}

/// Parses the arguments after the program name. Unknown arguments are
/// returned as an error message.
pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.into_iter();
    while let Some(arg) = it.next() {
        match arg.to_str() {
            Some("--vault") => {
                out.vault = Some(it.next().ok_or("--vault needs a path")?.into());
            }
            Some("--open") => {
                let note = it.next().ok_or("--open needs a note path")?;
                out.open = Some(note.to_string_lossy().into_owned());
            }
            Some("--bench") => out.bench = true,
            _ => return Err(format!("unknown argument: {}", arg.to_string_lossy())),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_vault_and_note() {
        let a = parse(["--vault", "/v", "--open", "a b.md"].map(OsString::from)).unwrap();
        assert_eq!(a.vault, Some(PathBuf::from("/v")));
        assert_eq!(a.open.as_deref(), Some("a b.md"));
        assert!(parse(["--vault"].map(OsString::from)).is_err());
        assert!(parse(["--nope"].map(OsString::from)).is_err());
    }
}
