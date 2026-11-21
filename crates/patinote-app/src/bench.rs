//! `patinote-app --bench`: replays scripted keys in a real window and prints
//! one JSON line with the time to the first drawn note, key-to-draw latency,
//! frame intervals and memory after 2 s idle (0002 section 4.1, step 2.10).
//!
//! Keys go through the same path as typed ones (`Message::Key`), so what is
//! measured is the app with its index, panels and autosave.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use iced::keyboard::{self, Key, Location, Modifiers, key};
use serde_json::{Value, json};

/// Environment variable carrying the time the first process started, so
/// the restart for fonts (0002 section 7) is counted in the startup time.
pub const STARTED: &str = "PATINOTE_STARTED_NS";

/// How long the app idles before memory is read.
pub const IDLE: Duration = Duration::from_secs(2);

/// Nanoseconds since the Unix epoch.
pub fn now_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

/// When the first process started: from [`STARTED`] after a restart, now
/// otherwise.
pub fn started_ns() -> u64 {
    std::env::var(STARTED)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(now_ns)
}

/// Written by the editor widget each time it draws; 0 until the first draw.
#[derive(Debug, Clone, Default)]
pub struct DrawClock(Arc<AtomicU64>);

impl DrawClock {
    /// Records a draw now.
    pub fn mark(&self) {
        self.0.store(now_ns(), Ordering::Relaxed);
    }

    /// The last draw, if any.
    pub fn last(&self) -> Option<u64> {
        Some(self.0.load(Ordering::Relaxed)).filter(|&t| t != 0)
    }
}

/// One key of the script.
#[derive(Debug, Clone)]
enum Step {
    Char(char),
    Named(key::Named),
}

impl Step {
    fn event(&self) -> keyboard::Event {
        let (key, text) = match self {
            Step::Char(c) => (
                Key::Character(c.to_string().into()),
                Some(c.to_string().into()),
            ),
            Step::Named(n) => (Key::Named(*n), None),
        };
        keyboard::Event::KeyPressed {
            modified_key: key.clone(),
            key,
            physical_key: key::Physical::Unidentified(key::NativeCode::Unidentified),
            location: Location::Standard,
            modifiers: Modifiers::default(),
            text,
            repeat: false,
        }
    }
}

/// The script and what was measured while it ran.
#[derive(Debug)]
pub struct Bench {
    started_ns: u64,
    phases: Vec<(&'static str, Vec<Step>)>,
    phase: usize,
    index: usize,
    /// The phase of the key waiting for its draw, and when it was sent.
    pending: Option<(usize, u64)>,
    frames: Vec<(u64, usize)>,
    first_draw: Option<u64>,
    samples: Vec<Vec<f64>>,
    ticks: u64,
    /// All keys were sent; the app idles before the report.
    pub done: bool,
}

impl Bench {
    /// The same script as the prototype: hold `j`, type a line, search and
    /// `n`, select 30 lines.
    pub fn new(started_ns: u64) -> Self {
        let typing = "the quick brown fox jumps over the lazy dog ".repeat(4);
        let mut type_keys = vec![Step::Char('o')];
        type_keys.extend(typing.chars().map(Step::Char));
        type_keys.push(Step::Named(key::Named::Escape));
        let mut search = vec![Step::Char('/')];
        search.extend("indeks".chars().map(Step::Char));
        search.push(Step::Named(key::Named::Enter));
        search.extend(std::iter::repeat_n(Step::Char('n'), 30));
        let mut visual = vec![Step::Char('v')];
        visual.extend(std::iter::repeat_n(Step::Char('j'), 30));
        visual.push(Step::Named(key::Named::Escape));
        let phases = vec![
            (
                "hold_j",
                std::iter::repeat_n(Step::Char('j'), 200).collect(),
            ),
            ("typing", type_keys),
            ("search_n", search),
            ("visual_j", visual),
        ];
        let samples = vec![Vec::new(); phases.len()];
        Self {
            started_ns,
            phases,
            phase: 0,
            index: 0,
            pending: None,
            frames: Vec::new(),
            first_draw: None,
            samples,
            ticks: 0,
            done: false,
        }
    }

    /// A frame went by. Returns the next key to send, if it is time.
    pub fn frame(&mut self, drawn: Option<u64>) -> Option<keyboard::Event> {
        let now = now_ns();
        let Some(drawn) = drawn else {
            // The note is not drawn yet: the index or the file is loading.
            return None;
        };
        if self.first_draw.is_none() {
            self.first_draw = Some(drawn);
        }
        self.ticks += 1;
        self.frames.push((now, self.phase));
        if let Some((phase, sent)) = self.pending {
            if drawn <= sent {
                return None;
            }
            self.samples[phase].push(ms(drawn - sent));
            self.pending = None;
        }
        // Every other frame, as in the prototype, so each key gets its own.
        if self.ticks % 2 == 1 || self.done {
            return None;
        }
        loop {
            let (_, keys) = self.phases.get(self.phase)?;
            if let Some(step) = keys.get(self.index) {
                self.index += 1;
                self.pending = Some((self.phase, now_ns()));
                return Some(step.event());
            }
            self.phase += 1;
            self.index = 0;
            if self.phase == self.phases.len() {
                self.done = true;
                return None;
            }
        }
    }

    /// The JSON report.
    pub fn report(&self, notes: usize) -> Value {
        let mut latency = serde_json::Map::new();
        let mut intervals = serde_json::Map::new();
        for (i, ((name, _), samples)) in self.phases.iter().zip(&self.samples).enumerate() {
            latency.insert((*name).into(), stats(samples));
            let gaps: Vec<f64> = self
                .frames
                .windows(2)
                .filter(|w| w[0].1 == i && w[1].1 == i)
                .map(|w| ms(w[1].0.saturating_sub(w[0].0)))
                .collect();
            intervals.insert((*name).into(), stats(&gaps));
        }
        let (pss, rss) = memory_kb();
        json!({
            "backend": std::env::var("ICED_BACKEND").unwrap_or_default(),
            "notes": notes,
            "startup_to_note_drawn_ms": self.first_draw.map(|d| round(ms(d.saturating_sub(self.started_ns)))),
            "pss_kb": pss,
            "rss_kb": rss,
            "event_to_draw_ms": latency,
            "frame_interval_ms": intervals,
        })
    }
}

fn ms(ns: u64) -> f64 {
    // Precision loss only above 2^52 ns (52 days).
    #[allow(clippy::cast_precision_loss)]
    let v = ns as f64;
    v / 1_000_000.0
}

fn round(v: f64) -> f64 {
    // Two decimals are enough for milliseconds.
    format!("{v:.2}").parse().unwrap_or(v)
}

fn stats(values: &[f64]) -> Value {
    let mut s = values.to_vec();
    s.sort_by(f64::total_cmp);
    // The value at `pct` percent, rounded to the nearest sample.
    let q = |pct: usize| -> Option<f64> {
        let last = s.len().checked_sub(1)?;
        s.get((last * pct + 50) / 100).copied().map(round)
    };
    json!({ "n": s.len(), "p50": q(50), "p95": q(95), "max": q(100) })
}

/// PSS and RSS of this process in kB, from `/proc/self/smaps_rollup`.
pub fn memory_kb() -> (u64, u64) {
    let text = std::fs::read_to_string("/proc/self/smaps_rollup").unwrap_or_default();
    let field = |name: &str| {
        text.lines()
            .find(|l| l.starts_with(name))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    (field("Pss:"), field("Rss:"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_waits_for_each_draw_and_ends() {
        let mut b = Bench::new(now_ns());
        assert!(b.frame(None).is_none(), "nothing before the note is drawn");
        let mut sent = 0;
        for _ in 0..2_000 {
            // Each frame is drawn after the key before it was sent.
            let drawn = now_ns() + 1;
            if b.frame(Some(drawn)).is_some() {
                sent += 1;
            }
            if b.done {
                break;
            }
        }
        assert!(b.done);
        assert_eq!(sent, 200 + 178 + 38 + 32);
        let r = b.report(1);
        assert_eq!(r["event_to_draw_ms"]["typing"]["n"], 178);
    }
}
