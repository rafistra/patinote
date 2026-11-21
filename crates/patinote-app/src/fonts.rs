//! Bundled fonts (0003 section 4) and keeping Iced from scanning system fonts
//! (PRD memory budget item 3, docs/design/0002 section 5.1, Q-011).
//!
//! Iced 0.14 builds its font database with `fontdb::Database::load_system_fonts`,
//! which reads the folders listed in `FONTCONFIG_FILE` when that variable is
//! set, and falls back to `/usr/share/fonts` when the file lists none. On start
//! the app writes a fontconfig file that lists one empty folder and runs itself
//! again with `FONTCONFIG_FILE` pointing to it. Setting the variable in-process
//! would need `unsafe` in edition 2024 (`CLAUDE.md` section 9).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use iced::Font;

use crate::theme::tokens::FONT_UI;

/// Every bundled font file, loaded from bytes when the app starts.
pub const FONTS: [&[u8]; 8] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Italic.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-SemiBold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
];

/// The default font of every widget.
pub const DEFAULT: Font = FONT_UI;

/// Set on the restarted process, so it does not restart again.
pub const RESTARTED: &str = "PATINOTE_FONTS_RESTRICTED";
/// The user's own `FONTCONFIG_FILE`, kept for programs the app starts.
pub const ORIGINAL: &str = "PATINOTE_ORIGINAL_FONTCONFIG_FILE";

/// The fontconfig file: one `<dir>`, the empty folder next to it.
pub fn fontconfig_xml(empty_dir: &Path) -> String {
    let dir = empty_dir
        .to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    format!(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"fonts.dtd\">\n\
         <!-- Written by Patinote: no system fonts; the app loads its own. -->\n\
         <fontconfig>\n  <dir>{dir}</dir>\n</fontconfig>\n"
    )
}

/// Writes the fontconfig file under `cache` (`$XDG_CACHE_HOME/patinote`) and
/// returns its path.
pub fn write_fontconfig(cache: &Path) -> std::io::Result<PathBuf> {
    let base = cache.join("fontconfig");
    let empty = base.join("empty");
    std::fs::create_dir_all(&empty)?;
    let file = base.join("fonts.conf");
    let xml = fontconfig_xml(&empty);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(xml.as_str()) {
        std::fs::write(&file, xml)?;
    }
    Ok(file)
}

/// Runs the app again with system fonts hidden, unless this process is that
/// run already. Returns only when no restart happened; the error says why
/// the app goes on with system fonts.
///
/// The restart also sets `ICED_BACKEND` to `backend` (Q-004: tiny-skia by
/// default), unless the user set it: with both renderers built in, Iced
/// picks wgpu first otherwise, and setting the variable in-process would
/// need `unsafe`.
#[cfg(unix)]
pub fn restart_without_system_fonts(backend: &str) -> Result<(), String> {
    use std::os::unix::process::CommandExt;

    if std::env::var_os(RESTARTED).is_some() {
        return Ok(());
    }
    let cache = patinote_core::location::cache_home().ok_or("HOME is not set")?;
    let file =
        write_fontconfig(&cache).map_err(|e| format!("cannot write the fontconfig file: {e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("cannot find the executable: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.args(std::env::args_os().skip(1))
        .env(RESTARTED, "1")
        .env("FONTCONFIG_FILE", file);
    if std::env::var_os(crate::bench::STARTED).is_none() {
        cmd.env(crate::bench::STARTED, crate::bench::now_ns().to_string());
    }
    if std::env::var_os("ICED_BACKEND").is_none() {
        cmd.env("ICED_BACKEND", backend);
    }
    if let Some(original) = std::env::var_os("FONTCONFIG_FILE") {
        cmd.env(ORIGINAL, original);
    }
    // `exec` only returns on failure.
    Err(format!("cannot restart: {}", cmd.exec()))
}

/// Other platforms keep system fonts.
#[cfg(not(unix))]
pub fn restart_without_system_fonts(_backend: &str) -> Result<(), String> {
    Ok(())
}

/// A command for another program (a browser, the default app for a file),
/// with the user's fontconfig back in place.
pub fn external_command(program: impl Into<OsString>) -> Command {
    let mut cmd = Command::new(program.into());
    cmd.env_remove(RESTARTED).env_remove("ICED_BACKEND");
    match std::env::var_os(ORIGINAL) {
        Some(original) => {
            cmd.env("FONTCONFIG_FILE", original).env_remove(ORIGINAL);
        }
        None => {
            cmd.env_remove("FONTCONFIG_FILE");
        }
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fontconfig_file_lists_exactly_one_empty_folder() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_fontconfig(dir.path()).unwrap();
        let xml = std::fs::read_to_string(&file).unwrap();
        assert_eq!(xml.matches("<dir>").count(), 1);
        let empty = dir.path().join("fontconfig/empty");
        assert!(xml.contains(&format!("<dir>{}</dir>", empty.display())));
        assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0);
        // Writing again changes nothing.
        let before = std::fs::metadata(&file).unwrap().modified().unwrap();
        write_fontconfig(dir.path()).unwrap();
        assert_eq!(
            std::fs::metadata(&file).unwrap().modified().unwrap(),
            before
        );
    }

    #[test]
    fn paths_are_escaped_for_xml() {
        assert!(fontconfig_xml(Path::new("/a&b/<c>")).contains("<dir>/a&amp;b/&lt;c&gt;</dir>"));
    }

    #[test]
    fn external_programs_get_the_users_fontconfig_back() {
        let cmd = external_command("xdg-open");
        let envs: Vec<_> = cmd.get_envs().collect();
        assert!(envs.iter().any(|(k, _)| *k == RESTARTED));
        assert!(
            envs.iter()
                .any(|(k, v)| *k == "FONTCONFIG_FILE" && v.is_none())
                || std::env::var_os(ORIGINAL).is_some()
        );
    }
}
