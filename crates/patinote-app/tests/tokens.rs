//! The design tokens follow `docs/design/0003-ui-design-system.md`, and
//! widget code never writes its own colors or sizes (`CLAUDE.md` section 6).

use iced::Color;
use patinote_app::theme::tokens::*;

/// WCAG relative luminance.
fn luminance(c: Color) -> f32 {
    let lin = |v: f32| {
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

fn contrast(a: Color, b: Color) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Every text and background pair 0003 names, with the ratio it states
/// (or 4.5, the WCAG AA minimum for text).
#[test]
fn text_pairs_meet_the_contrast_0003_states() {
    let pairs: &[(&str, Color, Color, f32)] = &[
        ("text.strong / bg.app", TEXT_STRONG, BG_APP, 16.3),
        ("text.primary / bg.app", TEXT_PRIMARY, BG_APP, 13.3),
        ("text.secondary / bg.app", TEXT_SECONDARY, BG_APP, 9.6),
        ("text.muted / bg.app", TEXT_MUTED, BG_APP, 7.1),
        ("text.subtle / bg.app", TEXT_SUBTLE, BG_APP, 6.2),
        ("text.gutter / bg.app", TEXT_GUTTER, BG_APP, 4.9),
        ("text.on-accent / accent", TEXT_ON_ACCENT, ACCENT, 8.6),
        (
            "text.on-accent / mode.visual",
            TEXT_ON_ACCENT,
            MODE_VISUAL,
            7.6,
        ),
        (
            "text.on-accent / mode.insert",
            TEXT_ON_ACCENT,
            MODE_INSERT,
            4.5,
        ),
        ("text.on-accent / mode.read", TEXT_ON_ACCENT, MODE_READ, 4.5),
        (
            "text.on-accent / status.error (banner action)",
            TEXT_ON_ACCENT,
            STATUS_ERROR,
            4.5,
        ),
        (
            "text.secondary / bg.selected",
            TEXT_SECONDARY,
            BG_SELECTED,
            5.9,
        ),
        ("status.error / bg.app", STATUS_ERROR, BG_APP, 7.5),
        ("status.error / bg.overlay", STATUS_ERROR, BG_OVERLAY, 6.9),
        (
            "status.warning / bg.statusline",
            STATUS_WARNING,
            BG_STATUSLINE,
            4.5,
        ),
        ("text.muted / bg.statusline", TEXT_MUTED, BG_STATUSLINE, 4.5),
        (
            "text.primary / bg.statusline.segment",
            TEXT_PRIMARY,
            BG_STATUSLINE_SEGMENT,
            4.5,
        ),
        ("read link missing / bg.app", READ_LINK_MISSING, BG_APP, 5.4),
        (
            "banner.warning.text / bg",
            BANNER_WARNING_TEXT,
            BANNER_WARNING_BG,
            4.5,
        ),
        (
            "banner.warning.muted / bg",
            BANNER_WARNING_MUTED,
            BANNER_WARNING_BG,
            4.5,
        ),
        (
            "banner.error.text / bg",
            BANNER_ERROR_TEXT,
            BANNER_ERROR_BG,
            4.5,
        ),
        (
            "banner.error.muted / bg",
            BANNER_ERROR_MUTED,
            BANNER_ERROR_BG,
            4.5,
        ),
        (
            "banner.info.text / bg",
            BANNER_INFO_TEXT,
            BANNER_INFO_BG,
            4.5,
        ),
        (
            "banner.info.muted / bg",
            BANNER_INFO_MUTED,
            BANNER_INFO_BG,
            4.5,
        ),
        (
            "search current text / bg",
            EDITOR_SEARCH_CURRENT_TEXT,
            EDITOR_SEARCH_CURRENT,
            4.5,
        ),
        (
            "accent.soft tag / bg.selected",
            READ_TAG_TEXT,
            READ_TAG_BG,
            4.5,
        ),
    ];
    let mut failures = Vec::new();
    for (name, fg, bg, stated) in pairs {
        let ratio = contrast(*fg, *bg);
        // 0003 rounds to one decimal.
        if ratio + 0.05 < *stated {
            failures.push(format!("{name}: {ratio:.2} < {stated}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// 0003 section 3.5: every syntax color stays readable on a selection (≥ 5:1).
#[test]
fn syntax_colors_are_readable_on_the_selection() {
    for (name, c) in [
        ("heading", SYNTAX_HEADING),
        ("link", SYNTAX_LINK),
        ("code", SYNTAX_CODE),
        ("quote", SYNTAX_QUOTE),
        ("tag", SYNTAX_TAG),
        ("text.primary", TEXT_PRIMARY),
        (
            "text.secondary (gutter text inside a selection)",
            TEXT_SECONDARY,
        ),
    ] {
        let ratio = contrast(c, EDITOR_SELECTION);
        assert!(ratio >= 4.95, "{name}: {ratio:.2}");
    }
}

#[test]
fn spacing_scale_is_the_one_0003_lists() {
    assert_eq!(
        SPACING_SCALE,
        [
            2.0, 4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0, 24.0, 32.0, 48.0
        ]
    );
}

/// No color literal or bare pixel size outside `theme/tokens.rs` (`CLAUDE.md`
/// section 6). Widgets name tokens instead.
#[test]
fn widget_code_has_no_color_or_size_literals() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![src.clone()];
    let mut offenders = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.ends_with("theme/tokens.rs") || path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (i, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                let bad = [
                    "Color::from_rgb",
                    "Color::new(",
                    "color!(",
                    "rgb8(",
                    "rgba8(",
                ]
                .iter()
                .any(|p| code.contains(p))
                    || has_hex_color(code)
                    || has_bare_float(code);
                if bad {
                    offenders.push(format!(
                        "{}:{}: {}",
                        path.strip_prefix(&src).unwrap().display(),
                        i + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "use theme::tokens instead:\n{}",
        offenders.join("\n")
    );
}

/// `#1A2B3C` or `0x1A2B3C` in code.
fn has_hex_color(code: &str) -> bool {
    let b = code.as_bytes();
    (0..b.len()).any(|i| {
        let start = if b[i] == b'#' {
            i + 1
        } else if b[i..].starts_with(b"0x") {
            i + 2
        } else {
            return false;
        };
        b.len() >= start + 6 && b[start..start + 6].iter().all(u8::is_ascii_hexdigit)
    })
}

/// A float literal like `14.0` (sizes); `0.0` and `1.0` stay allowed for
/// "none" and "full", and strings are skipped. Ranges (`1..=6`) and tuple
/// fields (`a.1`) are not floats.
fn has_bare_float(code: &str) -> bool {
    let mut in_str = false;
    let b = code.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => in_str = !in_str,
            c if !in_str
                && c.is_ascii_digit()
                && (i == 0
                    || !(b[i - 1].is_ascii_alphanumeric()
                        || b[i - 1] == b'_'
                        || b[i - 1] == b'.')) =>
            {
                let mut j = i;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                // Digits, a dot, digits: a float literal.
                if j + 1 < b.len() && b[j] == b'.' && b[j + 1].is_ascii_digit() {
                    let mut k = j + 1;
                    while k < b.len() && b[k].is_ascii_digit() {
                        k += 1;
                    }
                    let lit = &code[i..k];
                    if lit != "0.0" && lit != "1.0" {
                        return true;
                    }
                    j = k;
                }
                i = j;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

#[test]
fn the_literal_checks_catch_what_they_should() {
    assert!(has_hex_color("let c = \"#1A2B3C\";"));
    assert!(has_hex_color("hex(0x121614)"));
    assert!(!has_hex_color("let issue = \"#12\";"));
    assert!(has_bare_float(".size(14.0)"));
    assert!(!has_bare_float("width: 0.0, \"v1.25\""));
    assert!(!has_bare_float(
        "(1..=6).contains(&n) && a.1.min(n) && x[i + 1..]"
    ));
    assert!(has_bare_float("let w = 2.5;"));
    assert!(has_bare_float("(b.x - a) / 2.0"));
}
