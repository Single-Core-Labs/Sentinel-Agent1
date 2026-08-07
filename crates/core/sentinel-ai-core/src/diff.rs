//! Structured unified-diff parsing, rendering, and generation.
//!
//! This module is the Rust counterpart of `internal/diff/diff.go` in the
//! reference implementation. It turns a raw unified diff *string* into a
//! structured [`DiffResult`] (file headers + hunks of [`DiffLine`]s), tags
//! each line as an addition, removal, or context line, and — where a change is
//! a one-to-one replacement — splits the changed lines into character-level
//! [`DiffSegment`]s so a viewer can highlight the precise intra-line delta.
//!
//! Rendering: [`render_side_by_side`] lays hunks out in two terminal columns
//! (old | new) with optional ANSI color; intra-line segments are emphasized.
//! [`highlight_code`] applies a lightweight, dependency-free syntax
//! highlighter to source text. (A full engine such as `syntect` can later be
//! dropped in behind the same signature.)
//!
//! Generation: [`generate_unified_diff`] / [`generate_unified_diff_file`]
//! produce standard unified diff text from original/modified content using an
//! LCS line alignment. Applying the *resulting* text back onto files is
//! handled by [`crate::apply_patch`].

// ─── Structured model ────────────────────────────────────────────────────────

/// The kind of change a [`DiffLine`] represents in a hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// An unchanged line provided as hunk context.
    Context,
    /// A line that exists only in the new version (`+`).
    Addition,
    /// A line that exists only in the old version (`-`).
    Removal,
}

/// The kind of a character-level [`DiffSegment`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    /// Text present (identically) in both versions.
    Same,
    /// Text present only in the new version.
    Inserted,
    /// Text present only in the old version.
    Removed,
}

/// A character-level slice of a changed line, used to highlight precisely
/// which characters within the line changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffSegment {
    pub text: String,
    pub kind: SegmentKind,
}

/// A single line of a hunk: change kind, literal content, and intra-line
/// segmentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    /// Content without the leading `-`/`+`/blank diff prefix and without the
    /// trailing line terminator.
    pub content: String,
    /// Character-level segments. Context lines and unmatched lines carry a
    /// single [`SegmentKind::Same`] segment covering the whole line.
    pub segments: Vec<DiffSegment>,
}

/// A `@@ -a,b +c,d @@` hunk of a [`FileDiff`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// 1-based start line in the original file.
    pub old_start: usize,
    /// Number of lines taken from the original file.
    pub old_lines: usize,
    /// 1-based start line in the modified file.
    pub new_start: usize,
    /// Number of lines contributing to the modified file.
    pub new_lines: usize,
    pub lines: Vec<DiffLine>,
}

/// The change block for one file of a [`DiffResult`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    /// Path from the `---` header (`/dev/null` for new files).
    pub old_path: String,
    /// Path from the `+++` header (`/dev/null` for deletions).
    pub new_path: String,
    pub hunks: Vec<Hunk>,
}

/// A fully parsed unified diff: file headers and hunks of [`DiffLine`]s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffResult {
    pub files: Vec<FileDiff>,
}

/// Total number of actual changes (additions + removals) in a [`DiffResult`].
pub fn change_count(result: &DiffResult) -> usize {
    result
        .files
        .iter()
        .flat_map(|f| &f.hunks)
        .flat_map(|h| &h.lines)
        .filter(|l| l.kind != LineKind::Context)
        .count()
}

// ─── Parsing ─────────────────────────────────────────────────────────────────

/// Parse a raw unified diff string into a structured [`DiffResult`].
///
/// Accepts the same format as [`crate::apply_patch`]: optional `diff --git`
/// /`index` metadata, `--- old` / `+++ new` file headers (with `/dev/null` for
/// create/delete), `@@ -a[,b] +c[,d] @@` hunk headers, and `-`/`+`/` ` body
/// lines. `\ No newline at end of file` markers and git rename/mode metadata
/// are ignored. After parsing, one-to-one replaced lines are annotated with
/// intra-line segments.
pub fn parse_unified_diff(
    diff: &str,
) -> Result<DiffResult, crate::apply_patch::PatchError> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut current: Option<FileDiff> = None;

    for raw_line in diff.lines() {
        if let Some(old) = raw_line.strip_prefix("--- ") {
            if let Some(f) = current.take() {
                files.push(f);
            }
            current = Some(FileDiff {
                old_path: strip_filename_prefix(old.trim().to_string()),
                new_path: String::new(),
                hunks: Vec::new(),
            });
        } else if let Some(new) = raw_line.strip_prefix("+++ ") {
            if let Some(f) = current.as_mut() {
                if f.new_path.is_empty() {
                    f.new_path = strip_filename_prefix(new.trim().to_string());
                }
            }
        } else if raw_line.starts_with("@@") {
            match current.as_mut() {
                Some(f) => {
                    let (old_start, old_lines, new_start, new_lines) =
                        parse_hunk_header(raw_line)?;
                    f.hunks.push(Hunk {
                        old_start,
                        old_lines,
                        new_start,
                        new_lines,
                        lines: Vec::new(),
                    });
                }
                None => {
                    return Err(crate::apply_patch::PatchError::MalformedDiff(
                        "hunk appears before any --- / +++ file header".to_string(),
                    ));
                }
            }
        } else if let Some(f) = current.as_mut() {
            if let Some(hunk) = f.hunks.last_mut() {
                if let Some((kind, content)) = classify_line(raw_line) {
                    hunk.lines.push(DiffLine {
                        kind,
                        content,
                        segments: Vec::new(),
                    });
                }
            }
        }
        // Anything else (metadata lines before a file section) is ignored.
    }

    if let Some(f) = current.take() {
        files.push(f);
    }

    if files.is_empty() {
        return Err(crate::apply_patch::PatchError::MalformedDiff(
            "no file sections found in diff".to_string(),
        ));
    }
    for f in &files {
        if f.new_path.is_empty() {
            return Err(crate::apply_patch::PatchError::MalformedDiff(format!(
                "file section '{}' is missing its +++ header",
                f.old_path
            )));
        }
    }

    let mut result = DiffResult { files };
    for file in &mut result.files {
        for hunk in &mut file.hunks {
            split_intraline(hunk);
            // Every line must have segments so renderers never draw blanks:
            // context lines (and unmatched change lines) become one whole-line
            // Same / Removed / Inserted segment.
            for line in &mut hunk.lines {
                if line.segments.is_empty() {
                    line.segments = vec![DiffSegment {
                        text: line.content.clone(),
                        kind: match line.kind {
                            LineKind::Removal => SegmentKind::Removed,
                            LineKind::Addition => SegmentKind::Inserted,
                            LineKind::Context => SegmentKind::Same,
                        },
                    }];
                }
            }
        }
    }
    Ok(result)
}

/// Remove the `a/`/`b/` cosmetic prefix git attaches to diff file names.
fn strip_filename_prefix(path: String) -> String {
    path.strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .map(|s| s.to_string())
        .unwrap_or(path)
}

/// Classify a diff body line into kind + content, or `None` if it is not hunk
/// content (metadata like `diff --git` or `\ No newline at end of file`).
fn classify_line(raw_line: &str) -> Option<(LineKind, String)> {
    if let Some(rest) = raw_line.strip_prefix('-') {
        Some((LineKind::Removal, rest.to_string()))
    } else if let Some(rest) = raw_line.strip_prefix('+') {
        Some((LineKind::Addition, rest.to_string()))
    } else if let Some(rest) = raw_line.strip_prefix(' ') {
        Some((LineKind::Context, rest.to_string()))
    } else if raw_line.is_empty() {
        Some((LineKind::Context, String::new()))
    } else {
        None
    }
}

/// Parse `@@ -a[,b] +c[,d] @@` into `(old_start, old_lines, new_start,
/// new_lines)` (all counts default to 1 when omitted).
fn parse_hunk_header(
    header: &str,
) -> Result<(usize, usize, usize, usize), crate::apply_patch::PatchError> {
    let invalid = || {
        crate::apply_patch::PatchError::MalformedDiff(format!(
            "cannot parse hunk header: {header:?}"
        ))
    };
    let Some(rest) = header.strip_prefix("@@") else {
        return Err(invalid());
    };
    let tokens: Vec<&str> = rest
        .split("@@")
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    if tokens.len() < 2 {
        return Err(invalid());
    }
    let parse_range = |tok: &str| -> Option<(usize, usize)> {
        let after = tok.strip_prefix('-').or_else(|| tok.strip_prefix('+'))?;
        if let Some((start, count)) = after.split_once(',') {
            Some((start.parse().ok()?, count.parse().ok()?))
        } else {
            Some((after.parse().ok()?, 1))
        }
    };
    let (old_start, old_lines) = parse_range(tokens[0]).ok_or_else(invalid)?;
    let (new_start, new_lines) = parse_range(tokens[1]).ok_or_else(invalid)?;
    Ok((old_start, old_lines, new_start, new_lines))
}

// ─── Intra-line segmentation ─────────────────────────────────────────────────

/// Annotate a hunk's lines with character-level segments.
///
/// A run of `n` removed lines followed by a run of `n` added lines is paired
/// one-to-one; for each pair, the longest common prefix and suffix are kept as
/// [`SegmentKind::Same`] and the differing middle is marked Removed/Inserted.
/// Runs with unequal counts stay unsegmented (single whole-line segment).
fn split_intraline(hunk: &mut Hunk) {
    let lines = &mut hunk.lines;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].kind != LineKind::Removal {
            i += 1;
            continue;
        }
        let mut added_start = i;
        while added_start < lines.len() && lines[added_start].kind == LineKind::Removal {
            added_start += 1;
        }
        let mut added_end = added_start;
        while added_end < lines.len() && lines[added_end].kind == LineKind::Addition {
            added_end += 1;
        }
        let removed_count = added_start - i;
        let added_count = added_end - added_start;
        if removed_count == added_count && removed_count > 0 {
            for k in 0..removed_count {
                let (rem_segs, add_segs) = split_pair(
                    &lines[i + k].content,
                    &lines[added_start + k].content,
                );
                lines[i + k].segments = rem_segs;
                lines[added_start + k].segments = add_segs;
            }
        } else {
            // Unequal runs: mark each whole line as a single change segment so
            // renderers still colorize the line even without intra detail.
            for line in lines.iter_mut().take(added_end).skip(i) {
                line.segments = vec![DiffSegment {
                    text: line.content.clone(),
                    kind: match line.kind {
                        LineKind::Removal => SegmentKind::Removed,
                        LineKind::Addition => SegmentKind::Inserted,
                        LineKind::Context => SegmentKind::Same,
                    },
                }];
            }
        }
        i = added_end;
    }
}

/// Build the intra-line segments for one removed/added line pair.
fn split_pair(removed: &str, added: &str) -> (Vec<DiffSegment>, Vec<DiffSegment>) {
    let rem: Vec<char> = removed.chars().collect();
    let add: Vec<char> = added.chars().collect();
    let prefix_len = common_prefix_len(&rem, &add);
    let suffix_len = common_suffix_len(&rem, &add, prefix_len);

    let rem_mid_hi = rem.len().saturating_sub(suffix_len);
    let add_mid_hi = add.len().saturating_sub(suffix_len);

    let mut rem_segs = Vec::new();
    let mut add_segs = Vec::new();

    if prefix_len > 0 {
        rem_segs.push(seg_same(&rem[..prefix_len]));
        add_segs.push(seg_same(&add[..prefix_len]));
    }
    if rem_mid_hi > prefix_len {
        rem_segs.push(DiffSegment {
            text: chars_to_string(&rem[prefix_len..rem_mid_hi]),
            kind: SegmentKind::Removed,
        });
    }
    if add_mid_hi > prefix_len {
        add_segs.push(DiffSegment {
            text: chars_to_string(&add[prefix_len..add_mid_hi]),
            kind: SegmentKind::Inserted,
        });
    }
    if suffix_len > 0 {
        rem_segs.push(seg_same(&rem[rem.len() - suffix_len..]));
        add_segs.push(seg_same(&add[add.len() - suffix_len..]));
    }

    // Nothing differs (identical content): keep a single same segment.
    if rem_segs.is_empty() {
        rem_segs.push(seg_same(&rem[..]));
        add_segs.push(seg_same(&add[..]));
    }
    (rem_segs, add_segs)
}

fn seg_same(chars: &[char]) -> DiffSegment {
    DiffSegment {
        text: chars_to_string(chars),
        kind: SegmentKind::Same,
    }
}

fn common_prefix_len(a: &[char], b: &[char]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

fn common_suffix_len(a: &[char], b: &[char], prefix_len: usize) -> usize {
    let max = a
        .len()
        .saturating_sub(prefix_len)
        .min(b.len().saturating_sub(prefix_len));
    let mut n = 0;
    while n < max && a[a.len() - 1 - n] == b[b.len() - 1 - n] {
        n += 1;
    }
    n
}

fn chars_to_string(chars: &[char]) -> String {
    chars.iter().collect()
}

// ─── ANSI helpers ────────────────────────────────────────────────────────────
//
// Escape codes are emitted directly (instead of via a coloring crate) so the
// output is deterministic and testable regardless of TTY detection.

const RESET: &str = "\x1b[0m";

fn ansi(s: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("\x1b[{code}m{s}{RESET}")
    } else {
        s.to_string()
    }
}

fn red(s: &str, on: bool) -> String {
    ansi(s, "31", on)
}
fn green(s: &str, on: bool) -> String {
    ansi(s, "32", on)
}
fn yellow(s: &str, on: bool) -> String {
    ansi(s, "33", on)
}
fn cyan(s: &str, on: bool) -> String {
    ansi(s, "36", on)
}
fn magenta(s: &str, on: bool) -> String {
    ansi(s, "35", on)
}
fn dim(s: &str, on: bool) -> String {
    ansi(s, "2", on)
}
fn bold(s: &str, on: bool) -> String {
    ansi(s, "1", on)
}
fn red_intra(s: &str, on: bool) -> String {
    ansi(s, "1;31", on)
}
fn green_intra(s: &str, on: bool) -> String {
    ansi(s, "1;32", on)
}

// ─── Side-by-side rendering ──────────────────────────────────────────────────

/// Render a [`DiffResult`] as a side-by-side terminal view (`old | new`).
///
/// `width` is the full terminal content width; widths below 40 fall back to
/// 120. Each hunk is introduced by a dimmed `@@` header, and each row holds
/// the old line left of the separator and the new line right of it. With
/// `color` enabled, removed lines are red, added lines green, and the
/// character-level segments of changed lines are bolded.
pub fn render_side_by_side(diff: &DiffResult, width: usize, color: bool) -> String {
    let width = if width < 40 { 120 } else { width };
    let mut out = String::new();
    for file in &diff.files {
        for hunk in &file.hunks {
            out.push_str(&hunk_header_line(hunk, color));
            out.push('\n');
            for row in hunk_rows(hunk, width, color) {
                out.push_str(&row);
                out.push('\n');
            }
        }
    }
    out
}

/// The `@@ -a,b +c,d @@` header line for a hunk.
fn hunk_header_line(hunk: &Hunk, color: bool) -> String {
    let old_part = if hunk.old_lines == 0 {
        "-0,0".to_string()
    } else {
        format!("-{},{}", hunk.old_start, hunk.old_lines)
    };
    let new_part = if hunk.new_lines == 0 {
        "+0,0".to_string()
    } else {
        format!("+{},{}", hunk.new_start, hunk.new_lines)
    };
    let header = format!("@@ {old_part} {new_part} @@");
    magenta(&header, color)
}

/// Build one terminal row per hunk line, split into two columns.
fn hunk_rows(hunk: &Hunk, width: usize, color: bool) -> Vec<String> {
    let col_width = width.saturating_sub(3) / 2;
    let mut rows = Vec::with_capacity(hunk.lines.len());
    for line in &hunk.lines {
        let marker = match line.kind {
            LineKind::Context => "  ",
            LineKind::Removal => "- ",
            LineKind::Addition => "+ ",
        };
        let marker = match line.kind {
            LineKind::Context => marker.to_string(),
            LineKind::Removal => red(marker, color),
            LineKind::Addition => green(marker, color),
        };
        let content = render_cell(&line.segments, color);
        let cell = format!("{marker}{content}");
        let (left, right) = match line.kind {
            LineKind::Context => (cell.clone(), cell),
            LineKind::Removal => (cell, String::new()),
            LineKind::Addition => (String::new(), cell),
        };
        rows.push(format!(
            "{} | {}",
            pad_cell(left, col_width),
            pad_cell(right, col_width)
        ));
    }
    rows
}

/// Render a cell's segments, applying intra-line emphasis.
fn render_cell(segments: &[DiffSegment], color: bool) -> String {
    let mut out = String::new();
    for seg in segments {
        match seg.kind {
            SegmentKind::Same => out.push_str(&seg.text),
            SegmentKind::Removed => out.push_str(&red_intra(&seg.text, color)),
            SegmentKind::Inserted => out.push_str(&green_intra(&seg.text, color)),
        }
    }
    out
}

/// Count visible characters, ignoring ANSI escape sequences.
fn visible_len(s: &str) -> usize {
    let mut count = 0;
    let mut in_esc = false;
    for c in s.chars() {
        if in_esc {
            if c == 'm' {
                in_esc = false;
            }
            continue;
        }
        if c == '\x1b' {
            in_esc = true;
            continue;
        }
        count += 1;
    }
    count
}

/// Left-pad to `width` visible characters (escape codes excluded).
fn pad_cell(text: String, width: usize) -> String {
    let visible = visible_len(&text);
    if visible >= width {
        return text;
    }
    format!("{text}{}", " ".repeat(width - visible))
}

// ─── Lightweight syntax highlighting ─────────────────────────────────────────

/// Very light, dependency-free syntax highlighting.
///
/// Strings are yellow, numbers cyan, line/block comments dim, and keywords
/// bold; the keyword table is selected by `lang` (common names, unknown
/// languages fall back to a small generic set). When `color` is false the
/// input is returned unchanged, so callers can gate rendering by TTY.
pub fn highlight_code(lang: &str, code: &str, color: bool) -> String {
    let keywords = keyword_set(lang);
    let chars: Vec<char> = code.chars().collect();
    let n = chars.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c == '/' && i + 1 < n && chars[i + 1] == '/' {
            let start = i;
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            out.push_str(&dim(&code[start..i].to_string(), color));
            continue;
        }
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(n);
            out.push_str(&dim(&code[start..i].to_string(), color));
            continue;
        }
        if c == '"' || c == '\'' || c == '`' {
            let start = i;
            i += 1;
            while i < n && chars[i] != c && chars[i] != '\n' {
                if chars[i] == '\\' {
                    i = (i + 1).min(n);
                }
                i += 1;
            }
            i = (i + 1).min(n);
            out.push_str(&yellow(&code[start..i].to_string(), color));
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < n && (chars[i].is_ascii_alphanumeric() || chars[i] == '.') {
                i += 1;
            }
            out.push_str(&cyan(&code[start..i].to_string(), color));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < n && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if keywords.contains(&word) {
                out.push_str(&bold(&word, color));
            } else {
                out.push_str(&word);
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Keyword table per language name (case-insensitive).
fn keyword_set(lang: &str) -> std::collections::HashSet<String> {
    let lang = lang.to_lowercase();
    let list: &[&str] = match lang.as_str() {
        "" | "plain" | "text" | "txt" | "markdown" | "md" => &[],
        "rust" | "rs" => &[
            "fn", "let", "mut", "pub", "use", "mod", "struct", "enum", "impl", "trait", "async",
            "await", "match", "if", "else", "for", "while", "loop", "return", "ref", "move",
            "const", "static", "where", "unsafe", "extern", "type", "dyn", "in", "as", "break",
            "continue", "true", "false", "self", "Self", "crate", "super",
        ],
        "python" | "py" => &[
            "def", "return", "if", "elif", "else", "for", "while", "in", "not", "and", "or",
            "import", "from", "as", "class", "with", "try", "except", "finally", "lambda",
            "yield", "raise", "break", "continue", "pass", "global", "nonlocal", "assert", "del",
            "True", "False", "None",
        ],
        "js" | "javascript" | "typescript" | "ts" | "tsx" | "jsx" => &[
            "function", "const", "let", "var", "return", "if", "else", "for", "while", "import",
            "export", "from", "default", "class", "extends", "new", "async", "await", "try",
            "catch", "finally", "throw", "typeof", "instanceof", "in", "of", "break", "continue",
            "this", "true", "false", "null", "undefined", "interface", "type", "enum", "switch",
            "case", "yield", "static", "private", "public", "readonly",
        ],
        "go" | "golang" => &[
            "func", "package", "import", "return", "if", "else", "for", "switch", "case",
            "default", "range", "go", "defer", "var", "const", "type", "struct", "interface",
            "map", "chan", "break", "continue", "fallthrough", "select", "goto", "true", "false",
            "nil", "make", "new", "len", "cap", "append",
        ],
        _ => &[
            "function", "const", "if", "else", "return", "import", "export", "class", "let",
            "var", "true", "false", "null", "new", "for", "while", "try", "catch",
        ],
    };
    list.iter().map(|s| s.to_string()).collect()
}

// ─── Unified diff generation ─────────────────────────────────────────────────

/// Alignment step produced by the LCS backtrack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Align {
    Same,
    Remove,
    Insert,
}

/// A contiguous region of changed lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ChangeBlock {
    /// `[a0, a1)` in the original file.
    a0: usize,
    a1: usize,
    /// `[b0, b1)` in the modified file.
    b0: usize,
    b1: usize,
}

/// Generate a unified diff (with empty headers) between two strings.
///
/// Returns an empty string when the contents are identical. The produced text
/// parses with [`parse_unified_diff`] and applies with
/// [`crate::apply_patch::apply_patch`].
pub fn generate_unified_diff(original: &str, modified: &str) -> String {
    generate_unified_diff_file("", "", original, modified)
}

/// Generate a unified diff with `--- old_path` / `+++ new_path` headers.
///
/// Empty content on either side yields `/dev/null` on that side, so a created
/// or deleted file round-trips through `apply_patch`.
pub fn generate_unified_diff_file(
    old_path: &str,
    new_path: &str,
    original: &str,
    modified: &str,
) -> String {
    if original == modified {
        return String::new();
    }
    let a: Vec<&str> = original.lines().collect();
    let b: Vec<&str> = modified.lines().collect();
    if a.is_empty() && b.is_empty() {
        return String::new();
    }

    let steps = align_lines(&a, &b);
    let blocks = change_blocks(&steps);
    let hunks = build_hunks(&blocks, a.len(), b.len());

    let old_hdr = if original.is_empty() {
        "/dev/null"
    } else if old_path.is_empty() {
        "a/"
    } else {
        old_path
    };
    let new_hdr = if modified.is_empty() {
        "/dev/null"
    } else if new_path.is_empty() {
        "b/"
    } else {
        new_path
    };

    let mut out = format!("--- {old_hdr}\n+++ {new_hdr}\n");
    for hunk in &hunks {
        out.push_str(&render_hunk_text(&a, &b, &steps, hunk));
    }
    if hunks.is_empty() {
        // Content differs only in line endings or a trailing newline: the
        // whole file is one (context-only) hunk.
        out.push_str(&render_hunk_text(
            &a,
            &b,
            &steps,
            &ChangeBlock {
                a0: 0,
                a1: a.len(),
                b0: 0,
                b1: b.len(),
            },
        ));
    }
    out
}

/// LCS alignment of two line lists (longest common subsequence).
fn align_lines(a: &[&str], b: &[&str]) -> Vec<Align> {
    let n = a.len();
    let m = b.len();
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut steps = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a[i] == b[j] {
            steps.push(Align::Same);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            steps.push(Align::Remove);
            i += 1;
        } else {
            steps.push(Align::Insert);
            j += 1;
        }
    }
    while i < n {
        steps.push(Align::Remove);
        i += 1;
    }
    while j < m {
        steps.push(Align::Insert);
        j += 1;
    }
    steps
}

/// Convert an alignment into the maximal contiguous change blocks.
fn change_blocks(steps: &[Align]) -> Vec<ChangeBlock> {
    let mut blocks: Vec<ChangeBlock> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    let mut cur: Option<ChangeBlock> = None;
    for s in steps {
        match s {
            Align::Same => {
                if let Some(c) = cur.take() {
                    blocks.push(c);
                }
                i += 1;
                j += 1;
            }
            Align::Remove => {
                let c = cur.get_or_insert_with(|| ChangeBlock {
                    a0: i,
                    a1: i,
                    b0: j,
                    b1: j,
                });
                c.a1 = i + 1;
                c.b1 = j;
                i += 1;
            }
            Align::Insert => {
                let c = cur.get_or_insert_with(|| ChangeBlock {
                    a0: i,
                    a1: i,
                    b0: j,
                    b1: j,
                });
                c.a1 = i;
                c.b1 = j + 1;
                j += 1;
            }
        }
    }
    if let Some(c) = cur {
        blocks.push(c);
    }
    blocks
}

/// Expand change blocks with context and merge nearby ones, the way
/// `git diff --unified=3` does.
fn build_hunks(changes: &[ChangeBlock], n_a: usize, n_b: usize) -> Vec<ChangeBlock> {
    const CTX: usize = 3;
    let mut hunks: Vec<ChangeBlock> = Vec::new();
    for c in changes {
        let a0 = c.a0.saturating_sub(CTX);
        let a1 = (c.a1 + CTX).min(n_a);
        let b0 = c.b0.saturating_sub(CTX);
        let b1 = (c.b1 + CTX).min(n_b);
        if let Some(last) = hunks.last() {
            let gap_a = a0.saturating_sub(last.a1);
            let gap_b = b0.saturating_sub(last.b1);
            if gap_a < 2 * CTX && gap_b < 2 * CTX {
                let last = hunks.last_mut().unwrap();
                last.a1 = last.a1.max(a1);
                last.b1 = last.b1.max(b1);
                continue;
            }
        }
        hunks.push(ChangeBlock { a0, a1, b0, b1 });
    }
    hunks
}

/// Render one hunk of the diff text (`@@` header + body lines).
fn render_hunk_text(
    a: &[&str],
    b: &[&str],
    steps: &[Align],
    hunk: &ChangeBlock,
) -> String {
    // Mark which lines are removed/added according to the alignment.
    let mut removed: Vec<bool> = vec![false; a.len()];
    let mut inserted: Vec<bool> = vec![false; b.len()];
    {
        let (mut i, mut j) = (0usize, 0usize);
        for s in steps {
            match s {
                Align::Same => {
                    i += 1;
                    j += 1;
                }
                Align::Remove => {
                    removed[i] = true;
                    i += 1;
                }
                Align::Insert => {
                    inserted[j] = true;
                    j += 1;
                }
            }
        }
    }

    let old_lines = hunk.a1 - hunk.a0;
    let new_lines = hunk.b1 - hunk.b0;
    let old_part = if old_lines == 0 {
        "-0,0".to_string()
    } else {
        format!("-{},{}", hunk.a0 + 1, old_lines)
    };
    let new_part = if new_lines == 0 {
        "+0,0".to_string()
    } else {
        format!("+{},{}", hunk.b0 + 1, new_lines)
    };
    let mut out = format!("@@ {old_part} {new_part} @@\n");

    let (mut i, mut j) = (hunk.a0, hunk.b0);
    while i < hunk.a1 && j < hunk.b1 {
        if !removed[i] && !inserted[j] {
            out.push(' ');
            out.push_str(a[i]);
            out.push('\n');
            i += 1;
            j += 1;
        } else if removed[i] {
            out.push('-');
            out.push_str(a[i]);
            out.push('\n');
            i += 1;
        } else {
            out.push('+');
            out.push_str(b[j]);
            out.push('\n');
            j += 1;
        }
    }
    while i < hunk.a1 {
        out.push('-');
        out.push_str(a[i]);
        out.push('\n');
        i += 1;
    }
    while j < hunk.b1 {
        out.push('+');
        out.push_str(b[j]);
        out.push('\n');
        j += 1;
    }
    out
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply_patch::apply_patch;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "sentinel-diff-test-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_file(root: &Path, name: &str, content: &str) {
        fs::write(root.join(name), content).unwrap();
    }

    fn sample_diff() -> &'static str {
        "diff --git a/old.rs b/old.rs
index 0000000..1111111 100644
--- a/old.rs
+++ b/old.rs
@@ -1,3 +1,3 @@
 fn main() {
-    let x = 1;
+    let x = 2;
     println!();
 }
"
    }

    #[test]
    fn parses_kinds_and_paths() {
        let result = parse_unified_diff(sample_diff()).unwrap();
        assert_eq!(result.files.len(), 1);
        let file = &result.files[0];
        assert_eq!(file.old_path, "old.rs");
        assert_eq!(file.new_path, "old.rs");
        assert_eq!(file.hunks.len(), 1);
        let hunk = &file.hunks[0];
        assert_eq!((hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines), (1, 3, 1, 3));
        let kinds: Vec<LineKind> = hunk.lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            vec![
                LineKind::Context,
                LineKind::Removal,
                LineKind::Addition,
                LineKind::Context,
                LineKind::Context,
            ]
        );
        assert_eq!(hunk.lines[1].content, "    let x = 1;");
        assert_eq!(hunk.lines[2].content, "    let x = 2;");
        assert_eq!(hunk.lines[3].segments.len(), 1);
        assert_eq!(change_count(&result), 2);
    }

    #[test]
    fn parses_new_file_and_deletion() {
        let diff = "\
--- /dev/null
+++ b/new.txt
@@ -0,0 +1,2 @@
+hello
+world
--- a/old.txt
+++ /dev/null
@@ -1,2 +0,0 @@
-gone
-forever
";
        let result = parse_unified_diff(diff).unwrap();
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.files[0].old_path, "/dev/null");
        assert_eq!(result.files[0].new_path, "new.txt");
        assert!(result.files[0].hunks[0]
            .lines
            .iter()
            .all(|l| l.kind == LineKind::Addition));
        assert_eq!(result.files[1].new_path, "/dev/null");
        assert!(result.files[1].hunks[0]
            .lines
            .iter()
            .all(|l| l.kind == LineKind::Removal));
        assert_eq!(change_count(&result), 4);
    }

    #[test]
    fn intra_line_segments_on_replacement() {
        let result = parse_unified_diff(sample_diff()).unwrap();
        let hunk = &result.files[0].hunks[0];
        let removed = &hunk.lines[1];
        let added = &hunk.lines[2];

        assert_eq!(removed.kind, LineKind::Removal);
        assert_eq!(
            removed.segments,
            vec![
                DiffSegment { text: "    let x = ".into(), kind: SegmentKind::Same },
                DiffSegment { text: "1".into(), kind: SegmentKind::Removed },
                DiffSegment { text: ";".into(), kind: SegmentKind::Same },
            ]
        );
        assert_eq!(
            added.segments,
            vec![
                DiffSegment { text: "    let x = ".into(), kind: SegmentKind::Same },
                DiffSegment { text: "2".into(), kind: SegmentKind::Inserted },
                DiffSegment { text: ";".into(), kind: SegmentKind::Same },
            ]
        );
    }

    #[test]
    fn intra_segments_handle_unicode_without_splitting() {
        let (rem, add) = split_pair("héllo wörld", "héllo wörlD");
        let rem_text: String = rem.iter().map(|s| s.text.as_str()).collect();
        let add_text: String = add.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(rem_text, "héllo wörld");
        assert_eq!(add_text, "héllo wörlD");
        // [Same prefix, Removed middle]; no suffix when the tail differs.
        assert_eq!(rem.len(), 2);
        assert_eq!(rem[1].text, "d");
        assert_eq!(add[1].text, "D");
    }

    #[test]
    fn unequal_runs_stay_unsegmented() {
        let diff = "\
--- a/x.txt
+++ b/x.txt
@@ -1,3 +1,2 @@
 one
-two
-three
+TWO
";
        let result = parse_unified_diff(diff).unwrap();
        let lines = &result.files[0].hunks[0].lines;
        for line in lines.iter().filter(|l| l.kind != LineKind::Context) {
            assert_eq!(line.segments.len(), 1, "no intra split for unequal runs");
        }
    }

    #[test]
    fn render_side_by_side_columns() {
        let result = parse_unified_diff(sample_diff()).unwrap();
        let out = render_side_by_side(&result, 120, false);
        assert!(out.contains("@@ -1,3 +1,3 @@"), "header: {out}");
        let lines: Vec<&str> = out.lines().collect();
        assert!(
            lines.iter().any(|l| l.starts_with("  fn main() {") && l.contains('|')),
            "context row must appear on both columns: {out}"
        );
        assert!(
            lines.iter().any(|l| l.starts_with("-     let x = 1;")),
            "removal on the left column: {out}"
        );
        assert!(
            lines.iter().any(|l| l.contains("+     let x = 2;")),
            "addition on the right column: {out}"
        );
        // No ANSI codes when color is off.
        assert!(!out.contains('\x1b'));
    }

    #[test]
    fn render_colored_marks_intra_changes() {
        let result = parse_unified_diff(sample_diff()).unwrap();
        let out = render_side_by_side(&result, 120, true);
        assert!(out.contains("\x1b[1;31m1\x1b[0m"), "removed middle: {out}");
        assert!(out.contains("\x1b[1;32m2\x1b[0m"), "inserted middle: {out}");
        assert!(out.contains("\x1b[31m- "), "removed marker: {out}");
    }

    #[test]
    fn highlight_colors_code() {
        let out = highlight_code("rust", "fn main() { let x = 1; // hi\n}", true);
        assert!(out.contains("\x1b[1mfn\x1b[0m"), "keyword bold: {out}");
        assert!(out.contains("\x1b[36m1\x1b[0m"), "number cyan: {out}");
        assert!(out.contains("\x1b[2m// hi"), "comment dim: {out}");
        assert_eq!(
            highlight_code("rust", "fn main() {", false),
            "fn main() {"
        );
    }

    #[test]
    fn generate_roundtrips_through_apply_patch() {
        let original = "alpha\nbeta\ngamma\ndelta\nepsilon\n";
        let modified = "alpha\nBETA\ngamma\ndelta\nEPSILON\n";
        let diff = generate_unified_diff_file("a.txt", "b.txt", original, modified);
        assert!(diff.starts_with("--- a.txt\n+++ b.txt\n"));

        let parsed = parse_unified_diff(&diff).unwrap();
        assert_eq!(change_count(&parsed), 4);

        let root = tmp_dir();
        write_file(&root, "a.txt", original);
        apply_patch(&root, Path::new("a.txt"), &diff).expect("generated diff must apply");
        let result = fs::read_to_string(root.join("a.txt")).unwrap();
        assert_eq!(result, modified);
    }

    #[test]
    fn generate_new_file_applies() {
        let diff = generate_unified_diff_file("", "b.txt", "", "one\ntwo\n");
        let parsed = parse_unified_diff(&diff).unwrap();
        assert_eq!(parsed.files[0].old_path, "/dev/null");
        assert_eq!(change_count(&parsed), 2);

        let root = tmp_dir();
        apply_patch(&root, Path::new("b.txt"), &diff).expect("new-file diff must apply");
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "one\ntwo\n");
    }

    #[test]
    fn generate_identical_returns_empty() {
        assert!(generate_unified_diff("same\n", "same\n").is_empty());
        assert!(generate_unified_diff("", "").is_empty());
    }

    #[test]
    fn malformed_diff_rejected() {
        let err = parse_unified_diff("@@ -1,1 +1,1 @@\n+hi\n").unwrap_err();
        assert!(matches!(
            err,
            crate::apply_patch::PatchError::MalformedDiff(_)
        ));
    }
}
