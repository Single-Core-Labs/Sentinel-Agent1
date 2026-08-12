// Per-test-case module for the `pty_e2e` integration test crate.
#[allow(unused_imports)]
use super::common::*;

/// Single whitespace-containing argv → `$SHELL -i -c` (same hop as OSC 52).
const PRINT_APPEARANCE: &str =
    "printf 'ai=%s lc=%s\\n' \"$AI_APPEARANCE\" \"$LC_AI_APPEARANCE\"";

fn parse_printed_appearance(raw: &str) -> Option<(String, String)> {
    let line = raw.lines().find(|l| l.starts_with("ai="))?;
    let rest = line.strip_prefix("ai=")?;
    let (ai, lc) = rest.split_once(" lc=")?;
    Some((ai.to_owned(), lc.to_owned()))
}

/// Appearance stamp e2e through the interactive shell hop.
///
/// Parent pins AI/LC empty. `COLORFGBG` is a dark hint `detect()` would
/// honor, so a wrap that invented polarity from it would stamp `dark`.
/// Do not call `detect_desktop()` here — two live portal probes can disagree.
#[test]
#[ignore = "PTY e2e; run the owning pty_e2e_* Cargo test with --ignored (see Cargo.toml)"]
#[cfg(unix)]
fn wrap_appearance_env_advertised_through_shell() {
    let (code, raw) = run_wrap(
        &[PRINT_APPEARANCE],
        &[
            ("SHELL", "/bin/sh"),
            ("COLORFGBG", "15;0"),
            ("AI_APPEARANCE", ""),
            ("LC_AI_APPEARANCE", ""),
        ],
    );
    let (ai, lc) = parse_printed_appearance(&raw)
        .unwrap_or_else(|| panic!("missing ai=/lc= line\nraw:\n{raw}"));
    match (ai.as_str(), lc.as_str()) {
        ("", "") => {}
        ("dark", "dark") | ("light", "light") => {}
        _ => panic!(
            "AI and LC must agree and not invent from COLORFGBG; ai={ai:?} lc={lc:?}\nraw:\n{raw}"
        ),
    }
    assert_eq!(
        code,
        Some(0),
        "shell-routed printf must exit 0\nraw:\n{raw}"
    );
}

/// Parent `AI_APPEARANCE=light` with LC pinned empty: desktop Some overrides
/// both names to the same polarity; desktop None inherits AI and must not
/// invent LC. No second live desktop probe.
#[test]
#[ignore = "PTY e2e; run the owning pty_e2e_* Cargo test with --ignored (see Cargo.toml)"]
#[cfg(unix)]
fn wrap_appearance_env_desktop_none_does_not_restamp_parent_ai() {
    let (code, raw) = run_wrap(
        &[PRINT_APPEARANCE],
        &[
            ("SHELL", "/bin/sh"),
            ("AI_APPEARANCE", "light"),
            ("LC_AI_APPEARANCE", ""),
        ],
    );
    let (ai, lc) = parse_printed_appearance(&raw)
        .unwrap_or_else(|| panic!("missing ai=/lc= line\nraw:\n{raw}"));
    match (ai.as_str(), lc.as_str()) {
        ("light", "") => {}
        ("dark", "dark") | ("light", "light") => {}
        _ => panic!(
            "expected inherit ai=light with empty lc, or a matching desktop stamp; ai={ai:?} lc={lc:?}\nraw:\n{raw}"
        ),
    }
    assert_eq!(
        code,
        Some(0),
        "shell-routed printf must exit 0\nraw:\n{raw}"
    );
}
