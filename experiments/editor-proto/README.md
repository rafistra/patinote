# editor-proto

Throwaway prototype for step 0.6. Results are in
`docs/design/0002-editor-dan-renderer.md`. It is its own Cargo workspace so
that Iced stays out of the main `Cargo.lock` until it moves into
`patinote-app`.

```bash
cd experiments/editor-proto
cargo run --release                         # custom editor on the mockup note
cargo run --release -- --builtin            # stock text_editor with the same highlighting
cargo run --release -- path/to/note.md      # any note
cargo test --lib                            # Vim and highlighting logic
ICED_TEST_BACKEND=tiny-skia cargo test --test screenshot   # PNGs in target/ui-snapshots/
```

Keys: `h j k l w b 0 $ gg G`, `i a A o O`, `x dd`, `v` then `d`, `/` then
`n N`, `Esc`.

## Measurements

Generate long notes from the repository root, then run the benchmarks:

```bash
cargo run --release -p vaultgen -- --notes 20 --long-note-lines 5000 --out target/vaults/long5000
cd experiments/editor-proto && cargo build --release
NOTE="../../target/vaults/long5000/Catatan panjang.md"

# Window: replays keys, prints one JSON line, exits. A window opens for ~15 s.
ICED_BACKEND=tiny-skia ./target/release/editor-proto --bench "$NOTE"
ICED_BACKEND=wgpu ./target/release/editor-proto --bench "$NOTE"

# Headless frame cost per key at scales 1.0 and 2.0 (SCALES=1.25 to change).
./target/release/render_bench tiny-skia "$NOTE"
./target/release/render_bench wgpu "$NOTE"
```

To skip the system font scan, point `FONTCONFIG_FILE` at a file that lists
only the bundled fonts (generated, because it holds an absolute path):

```bash
printf '<?xml version="1.0"?>\n<fontconfig><dir>%s</dir></fontconfig>\n' \
  "$(realpath ../../crates/patinote-app/assets/fonts)" > target/fonts-only.conf
FONTCONFIG_FILE=$PWD/target/fonts-only.conf ICED_BACKEND=tiny-skia ./target/release/editor-proto --bench "$NOTE"
```
