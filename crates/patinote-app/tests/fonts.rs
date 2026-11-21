//! Q-011: the app runs itself again with `FONTCONFIG_FILE` pointing at an
//! empty folder, so Iced loads only the bundled fonts and its icon font.

use std::process::Command;

/// Runs the binary in font report mode and returns how many faces it loaded.
fn faces(skip_restart: bool) -> usize {
    let dir = tempfile::tempdir().unwrap();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_patinote-app"));
    cmd.env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", dir.path())
        .env("XDG_CACHE_HOME", dir.path().join("cache"))
        .env("PATINOTE_FONT_REPORT", "1");
    if skip_restart {
        // Pretends the restart already happened, without FONTCONFIG_FILE.
        cmd.env("PATINOTE_FONTS_RESTRICTED", "1");
    }
    let out = cmd.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn only_bundled_fonts_are_loaded() {
    let bundled = patinote_app::fonts::FONTS.len();
    // Iced always adds its icon font.
    assert_eq!(faces(false), bundled + 1);
}

#[test]
fn without_the_restart_system_fonts_would_be_scanned() {
    // On a machine with no system fonts both counts are equal; never fewer.
    assert!(faces(true) >= faces(false));
}
