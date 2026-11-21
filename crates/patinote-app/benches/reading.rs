//! Switching a 1,000-line note to reading mode (F-27.2: under 100 ms):
//! rendering its Markdown into blocks, then building the view.
//!
//! Run with `cargo bench -p patinote-app --bench reading`.

use std::collections::{HashMap, HashSet};
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use patinote_app::reading::render;
use patinote_app::screens::reading::View;

fn note() -> String {
    let base = include_str!("../../../fixtures/vault-mockup/Proyek/Desain indeks Patinote.md");
    let body = base.split_once("\n---\n").map_or(base, |(_, b)| b);
    let mut out = base.to_owned();
    while out.lines().count() < 1_000 {
        out.push_str(body);
    }
    out
}

fn bench(c: &mut Criterion) {
    let text = note();
    c.bench_function("reading_1000_lines", |b| {
        b.iter(|| {
            let doc = render(black_box(&text));
            let missing = HashSet::new();
            let pictures = HashMap::new();
            let view: iced::Element<'_, ()> = View {
                doc: &doc,
                missing: &missing,
                focused: None,
                pictures: &pictures,
                properties_open: true,
                on_link: |_| (),
                on_toggle: |_| (),
                on_properties: (),
                on_scroll: |_| (),
            }
            .view();
            black_box(view);
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
