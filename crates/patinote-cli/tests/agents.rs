//! `patinote agents init` (A-05).

mod common;

use common::{Env, json};

#[test]
fn a05_1_conventions_are_detected_from_the_vault() {
    let env = Env::new();
    let vault = env.fixture();
    let r = env.run(&["--vault", vault.to_str().unwrap(), "agents", "init"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let text = std::fs::read_to_string(vault.join("AGENTS.md")).unwrap();
    for expected in [
        "`Proyek/` (4)",
        "`#proyek/patinote` (2)",
        "Daily notes: `Harian/YYYY-MM-DD.md`",
        "Attachments are kept in `Proyek/Lampiran/`",
        "Links are wikilinks",
        "patinote search",
        "## Personal notes",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
    }
    // AGENTS.md is a normal note afterwards.
    let list = env.run(&["--vault", vault.to_str().unwrap(), "list", "--limit", "0"]);
    assert!(list.stdout.lines().any(|l| l.starts_with("AGENTS.md\t")));
}

#[test]
fn a05_2_update_never_changes_the_personal_part() {
    let env = Env::new();
    let vault = env.fixture();
    let v = vault.to_str().unwrap();
    assert_eq!(env.run(&["--vault", v, "agents", "init"]).code, 0);
    let path = vault.join("AGENTS.md");
    let personal = "\nTulis dalam bahasa Indonesia.\n\n- Jangan pernah menghapus catatan.\n  Indentasi, *penekanan* dan [[link]] tetap.\n";
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str(personal);
    std::fs::write(&path, &text).unwrap();
    let tail = text[text.find("<!-- patinote:end -->").unwrap()..].to_owned();

    // The vault changes, so the generated part changes on update.
    env.write("vault/Inbox/Baru.md", "Catatan baru #topikbaru\n");
    let r = env.run(&["--vault", v, "agents", "init", "--update", "--json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(json(&r.stdout)["written"], serde_json::json!(["AGENTS.md"]));
    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("`#topikbaru` (1)"));
    assert_eq!(
        &updated[updated.find("<!-- patinote:end -->").unwrap()..],
        tail,
        "the personal part is byte for byte the same"
    );
}

#[test]
fn a05_3_for_claude_and_gemini_point_to_agents_md() {
    let env = Env::new();
    let vault = env.fixture();
    let r = env.run(&[
        "--vault",
        vault.to_str().unwrap(),
        "agents",
        "init",
        "--for",
        "claude,gemini",
        "--json",
    ]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        json(&r.stdout)["written"],
        serde_json::json!(["AGENTS.md", "CLAUDE.md", "GEMINI.md"])
    );
    for f in ["CLAUDE.md", "GEMINI.md"] {
        let text = std::fs::read_to_string(vault.join(f)).unwrap();
        assert!(text.contains("@AGENTS.md"), "{f}: {text}");
    }
}

#[test]
fn a05_4_init_never_overwrites_and_exits_4() {
    let env = Env::new();
    let vault = env.fixture();
    let v = vault.to_str().unwrap();
    env.write("vault/AGENTS.md", "# Milik saya\n");
    let r = env.run(&["--vault", v, "agents", "init", "--json"]);
    assert_eq!(r.code, 4);
    assert_eq!(json(&r.stderr)["error"]["code"], "already_exists");
    assert_eq!(
        std::fs::read_to_string(vault.join("AGENTS.md")).unwrap(),
        "# Milik saya\n"
    );
    // Without markers, --update refuses rather than guessing.
    let u = env.run(&["--vault", v, "agents", "init", "--update", "--json"]);
    assert_eq!(
        (u.code, json(&u.stderr)["error"]["code"].as_str()),
        (2, Some("invalid_argument"))
    );
    assert_eq!(
        std::fs::read_to_string(vault.join("AGENTS.md")).unwrap(),
        "# Milik saya\n"
    );
}

#[test]
fn writes_are_journaled_as_cli() {
    let env = Env::new();
    let vault = env.fixture();
    assert_eq!(
        env.run(&["--vault", vault.to_str().unwrap(), "agents", "init"])
            .code,
        0
    );
    let state = env.path("state/patinote");
    let journal = std::fs::read_dir(&state)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("journal");
    let entry = std::fs::read_dir(journal)
        .unwrap()
        .filter_map(Result::ok)
        .find(|e| e.path().is_dir())
        .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(entry.path().join("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(
        (manifest["actor"].as_str(), manifest["status"].as_str()),
        (Some("cli"), Some("committed"))
    );
}
