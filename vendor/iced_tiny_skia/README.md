# iced_tiny_skia 0.14.1, patched

An unmodified copy of [`iced_tiny_skia` 0.14.1](https://crates.io/crates/iced_tiny_skia/0.14.1)
from crates.io (Iced commit `0ecf606`, see `.cargo_vcs_info.json`), plus the
one change in `patinote.patch`. The root `Cargo.toml` uses it through
`[patch.crates-io]`. MIT, see `LICENSE`.

## The change

`window::compositor::present` groups the damage regions and
`Renderer::draw` then walks every layer once per region, rebuilding a
window-sized clip mask for each. Scrolling text damages one region per
line, so a frame took 20+ full passes: about 58 ms on a 1.25-scaled laptop
screen, versus 24 ms with the patch, which draws the union of the regions
once when there are more than four. Measurements and the cause are in
`docs/design/0002-editor-dan-renderer.md`, section 11.

## Removing it

Reported upstream as [iced-rs/iced#3556](https://github.com/iced-rs/iced/issues/3556).
Iced's `master` replaces `tiny_skia` with `vello_cpu`, so a fix can only come
in a 0.14.x release. When one ships, or when Patinote moves to an Iced
version without `tiny_skia`: delete this folder, the
`[patch.crates-io]` section and the `exclude` line in the root
`Cargo.toml`, then run `scripts/measure.sh 3 tiny-skia` to check holding
`j` stays near one frame (Q-018).

## Updating Iced before then

Copy the new `iced_tiny_skia` from `~/.cargo/registry/src/*/` over this
folder (keep this README and `LICENSE`), then `patch -p1 < patinote.patch`.
