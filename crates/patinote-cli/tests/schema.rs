//! JSON Schemas in `docs/schema/` match the Rust output types (A-02).
//! Regenerate with `UPDATE_SCHEMAS=1 cargo test -p patinote-cli --test schema`.

use std::path::Path;

#[test]
fn schemas_in_docs_are_up_to_date() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/schema");
    let update = std::env::var_os("UPDATE_SCHEMAS").is_some();
    let mut stale = Vec::new();
    for (name, schema) in patinote_core::commands::schemas() {
        let text = serde_json::to_string_pretty(&schema).unwrap() + "\n";
        let path = dir.join(name);
        if update {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, &text).unwrap();
        } else if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            stale.push(name);
        }
    }
    assert!(
        stale.is_empty(),
        "out of date: {stale:?}; run UPDATE_SCHEMAS=1 cargo test -p patinote-cli --test schema"
    );
}
