use std::sync::OnceLock;
use regex::Regex;
use crate::config::CacheAlignmentConfig;

static DATE_RE: OnceLock<Regex> = OnceLock::new();
fn date_re() -> &'static Regex {
    DATE_RE.get_or_init(|| Regex::new(
        r"(?i)\b(\d{4}[-/]\d{1,2}[-/]\d{1,2}|\d{1,2}[-/]\d{1,2}[-/]\d{4}|(?:mon|tue|wed|thu|fri|sat|sun)[a-z]*\s+\d{1,2}(?:st|nd|rd|th)?\s+\d{4}|today|yesterday|tomorrow)\b"
    ).unwrap())
}

static TIME_RE: OnceLock<Regex> = OnceLock::new();
fn time_re() -> &'static Regex {
    TIME_RE.get_or_init(|| Regex::new(
        r"\b(\d{1,2}:\d{2}(?::\d{2})?\s*(?:am|pm)?|\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)\b"
    ).unwrap())
}

static FILEPATH_RE: OnceLock<Regex> = OnceLock::new();
fn filepath_re() -> &'static Regex {
    FILEPATH_RE.get_or_init(|| Regex::new(
        r#"(?i)\b(/\w[\w/.\-]+\.[a-z0-9]+|[a-zA-Z]:\\[\w.\-\\]+\.[a-z0-9]+)"#
    ).unwrap())
}

static UUID_RE: OnceLock<Regex> = OnceLock::new();
fn uuid_re() -> &'static Regex {
    UUID_RE.get_or_init(|| Regex::new(
        r"\b([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\b"
    ).unwrap())
}

static VERSION_RE: OnceLock<Regex> = OnceLock::new();
fn version_re() -> &'static Regex {
    VERSION_RE.get_or_init(|| Regex::new(
        r"\b(\d+\.\d+\.\d+(?:-[a-z0-9]+(?:\.[a-z0-9]+)?)?)\b"
    ).unwrap())
}

static USER_CONTEXT_RE: OnceLock<Regex> = OnceLock::new();
fn user_context_re() -> &'static Regex {
    USER_CONTEXT_RE.get_or_init(|| Regex::new(
        r"(?i)(?:user|name|username|author|login):\s*(\S+)"
    ).unwrap())
}

static TEMP_DIR_RE: OnceLock<Regex> = OnceLock::new();
fn temp_dir_re() -> &'static Regex {
    TEMP_DIR_RE.get_or_init(|| Regex::new(
        r"(?i)\b(/tmp/|[a-z]:\\temp\\|/var/folders/|/private/tmp/|%temp%|%tmp%)"
    ).unwrap())
}

static BLANK_LINE_RE: OnceLock<Regex> = OnceLock::new();
fn blank_line_re() -> &'static Regex {
    BLANK_LINE_RE.get_or_init(|| Regex::new(r"\n\s*\n\s*\n+").unwrap())
}

#[derive(Debug, Clone, PartialEq)]
pub struct DynamicContext {
    pub dates: Vec<String>,
    pub times: Vec<String>,
    pub file_paths: Vec<String>,
    pub uuids: Vec<String>,
    pub versions: Vec<String>,
    pub user_context: Vec<String>,
    pub temp_dirs: Vec<String>,
    pub custom_matches: Vec<String>,
}

impl DynamicContext {
    pub fn is_empty(&self) -> bool {
        self.dates.is_empty() && self.times.is_empty() && self.file_paths.is_empty()
            && self.uuids.is_empty() && self.versions.is_empty()
            && self.user_context.is_empty() && self.temp_dirs.is_empty()
            && self.custom_matches.is_empty()
    }

    pub fn delta(&self, previous: &DynamicContext) -> DynamicDelta {
        let mut changed: Vec<(String, String)> = Vec::new();
        let mut removed: Vec<(String, String)> = Vec::new();

        if self.dates != previous.dates {
            changed.push(("dates".into(), format!("{:?}", self.dates)));
        }
        if self.times != previous.times {
            changed.push(("times".into(), format!("{:?}", self.times)));
        }
        if self.file_paths != previous.file_paths {
            changed.push(("file_paths".into(), format!("{:?}", self.file_paths)));
        }
        if self.uuids != previous.uuids {
            changed.push(("uuids".into(), format!("{:?}", self.uuids)));
        }
        if self.versions != previous.versions {
            changed.push(("versions".into(), format!("{:?}", self.versions)));
        }
        if self.user_context != previous.user_context {
            changed.push(("user_context".into(), format!("{:?}", self.user_context)));
        }
        if self.temp_dirs != previous.temp_dirs {
            changed.push(("temp_dirs".into(), format!("{:?}", self.temp_dirs)));
        }
        if self.custom_matches != previous.custom_matches {
            changed.push(("custom".into(), format!("{:?}", self.custom_matches)));
        }

        for (k, _) in &changed {
            let pv = match k.as_str() {
                "dates" => Some(format!("{:?}", previous.dates)),
                "times" => Some(format!("{:?}", previous.times)),
                "file_paths" => Some(format!("{:?}", previous.file_paths)),
                "uuids" => Some(format!("{:?}", previous.uuids)),
                "versions" => Some(format!("{:?}", previous.versions)),
                "user_context" => Some(format!("{:?}", previous.user_context)),
                "temp_dirs" => Some(format!("{:?}", previous.temp_dirs)),
                "custom" => Some(format!("{:?}", previous.custom_matches)),
                _ => None,
            };
            if let Some(pv) = pv {
                let cv = format!("{:?}", match k.as_str() {
                    "dates" => &self.dates,
                    "times" => &self.times,
                    "file_paths" => &self.file_paths,
                    "uuids" => &self.uuids,
                    "versions" => &self.versions,
                    "user_context" => &self.user_context,
                    "temp_dirs" => &self.temp_dirs,
                    "custom" => &self.custom_matches,
                    _ => unreachable!(),
                });
                if pv != cv {
                    removed.push((k.clone(), pv));
                }
            }
        }

        let has_changes = !changed.is_empty();
        DynamicDelta { changed, removed, has_changes }
    }

    fn flatten(&self) -> Vec<(String, String)> {
        let mut items: Vec<(String, String)> = Vec::new();
        for d in &self.dates { items.push(("date".into(), d.clone())); }
        for t in &self.times { items.push(("time".into(), t.clone())); }
        for p in &self.file_paths { items.push(("path".into(), p.clone())); }
        for u in &self.uuids { items.push(("uuid".into(), u.clone())); }
        for v in &self.versions { items.push(("version".into(), v.clone())); }
        for u in &self.user_context { items.push(("user".into(), u.clone())); }
        for t in &self.temp_dirs { items.push(("temp".into(), t.clone())); }
        for c in &self.custom_matches { items.push(("custom".into(), c.clone())); }
        items
    }
}

pub struct DynamicDelta {
    pub changed: Vec<(String, String)>,
    pub removed: Vec<(String, String)>,
    pub has_changes: bool,
}

#[derive(Debug, Clone)]
pub struct CacheAlignedPrompt {
    pub static_prefix: String,
    pub dynamic_suffix: String,
    pub context: DynamicContext,
}

pub struct CacheAligner {
    config: CacheAlignmentConfig,
    previous_context: Option<DynamicContext>,
    compiled_custom: Vec<Regex>,
    compiled_date_patterns: Vec<Regex>,
}

impl CacheAligner {
    pub fn new(config: CacheAlignmentConfig) -> Self {
        let compiled_custom = config.custom_patterns.iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();
        let compiled_date_patterns = config.date_patterns.iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();
        Self { config, previous_context: None, compiled_custom, compiled_date_patterns }
    }

    pub fn with_previous(previous: DynamicContext) -> Self {
        Self {
            config: CacheAlignmentConfig::default(),
            previous_context: Some(previous),
            compiled_custom: Vec::new(),
            compiled_date_patterns: Vec::new(),
        }
    }

    fn normalize(&self, text: &str) -> String {
        let mut s = text.to_string();
        if self.config.normalize_whitespace {
            s = s.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        if self.config.collapse_blank_lines {
            s = blank_line_re().replace_all(&s, "\n\n").to_string();
        }
        s
    }

    pub fn align(&mut self, content: &str) -> CacheAlignedPrompt {
        let normalized = self.normalize(content);
        let mut ctx = DynamicContext {
            dates: Vec::new(),
            times: Vec::new(),
            file_paths: Vec::new(),
            uuids: Vec::new(),
            versions: Vec::new(),
            user_context: Vec::new(),
            temp_dirs: Vec::new(),
            custom_matches: Vec::new(),
        };

        let mut result = normalized;
        let mut placeholders: Vec<(String, String)> = Vec::new();

        // Built-in patterns: replace with placeholders in prefix (keeps sentence structure)
        if self.config.extract_dates {
            for cap in date_re().find_iter(&result.clone()) {
                let val = cap.as_str().to_string();
                if !ctx.dates.contains(&val) {
                    ctx.dates.push(val.clone());
                    let ph = format!("<DATE_{}>", ctx.dates.len());
                    placeholders.push((val, ph));
                }
            }
        }

        if self.config.extract_dates {
            let content_for_time = result.clone();
            for cap in time_re().find_iter(&content_for_time) {
                let val = cap.as_str().to_string();
                if !ctx.times.contains(&val) {
                    ctx.times.push(val.clone());
                    let ph = format!("<TIME_{}>", ctx.times.len());
                    placeholders.push((val, ph));
                }
            }
        }

        if self.config.extract_file_paths {
            for cap in filepath_re().find_iter(&result.clone()) {
                let val = cap.as_str().to_string();
                if !ctx.file_paths.contains(&val) {
                    ctx.file_paths.push(val.clone());
                    let ph = format!("<PATH_{}>", ctx.file_paths.len());
                    placeholders.push((val, ph));
                }
            }
        }

        if self.config.extract_uuids {
            for cap in uuid_re().find_iter(&result.clone()) {
                let val = cap.as_str().to_string();
                if !ctx.uuids.contains(&val) {
                    ctx.uuids.push(val.clone());
                    let ph = format!("<UUID_{}>", ctx.uuids.len());
                    placeholders.push((val, ph));
                }
            }
        }

        if self.config.extract_versions {
            for cap in version_re().find_iter(&result.clone()) {
                let val = cap.as_str().to_string();
                if !ctx.versions.contains(&val) {
                    ctx.versions.push(val.clone());
                    let ph = format!("<VER_{}>", ctx.versions.len());
                    placeholders.push((val, ph));
                }
            }
        }

        if self.config.extract_user_context {
            for cap in user_context_re().find_iter(&result.clone()) {
                let val = cap.as_str().to_string();
                if !ctx.user_context.contains(&val) {
                    ctx.user_context.push(val.clone());
                }
            }
        }

        if self.config.extract_file_paths {
            for cap in temp_dir_re().find_iter(&result.clone()) {
                let val = cap.as_str().to_string();
                if !ctx.temp_dirs.contains(&val) {
                    ctx.temp_dirs.push(val.clone());
                    let ph = format!("<TEMP_{}>", ctx.temp_dirs.len());
                    placeholders.push((val, ph));
                }
            }
        }

        // Apply built-in placeholders
        for (original, placeholder) in &placeholders {
            result = result.replace(original, placeholder);
        }

        // User-defined patterns: strip entire matched phrase and move to suffix
        if !self.compiled_custom.is_empty() || !self.compiled_date_patterns.is_empty() {
            for re in &self.compiled_custom {
                for cap in re.find_iter(&result.clone()) {
                    let val = cap.as_str().to_string();
                    if !ctx.custom_matches.contains(&val) {
                        ctx.custom_matches.push(val.clone());
                        result = result.replace(&val, "");
                    }
                }
            }
            for re in &self.compiled_date_patterns {
                for cap in re.find_iter(&result.clone()) {
                    let val = cap.as_str().to_string();
                    if !ctx.custom_matches.contains(&val) {
                        ctx.custom_matches.push(val.clone());
                        result = result.replace(&val, "");
                    }
                }
            }
        }

        if self.config.normalize_whitespace {
            result = result.split_whitespace().collect::<Vec<_>>().join(" ");
        }

        let dynamic_suffix = build_dynamic_suffix(&ctx, self.previous_context.as_ref(), self.config.delta_tracking);

        self.previous_context = Some(ctx.clone());

        CacheAlignedPrompt {
            static_prefix: result,
            dynamic_suffix,
            context: ctx,
        }
    }

    pub fn previous_context(&self) -> Option<&DynamicContext> {
        self.previous_context.as_ref()
    }

    pub fn reset(&mut self) {
        self.previous_context = None;
    }
}

fn build_dynamic_suffix(
    ctx: &DynamicContext,
    previous: Option<&DynamicContext>,
    delta_tracking: bool,
) -> String {
    if delta_tracking {
        if let Some(prev) = previous {
            let delta = ctx.delta(prev);
            if !delta.has_changes {
                return "[Context: no change]".into();
            }
            let mut out = "[Context changed: ".to_string();
            let parts: Vec<String> = delta.changed.iter()
                .map(|(key, val)| format!("{} → {}", key, val))
                .collect();
            out.push_str(&parts.join(", "));
            out.push(']');
            return out;
        }
    }

    if ctx.is_empty() {
        return String::new();
    }

    let items = ctx.flatten();
    let parts: Vec<String> = items.iter()
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect();
    format!("[Context: {}]", parts.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extracts_dates() {
        let mut aligner = CacheAligner::new(CacheAlignmentConfig {
            extract_uuids: false, ..Default::default()
        });
        let result = aligner.align("Today is 2026-07-22 and yesterday was 2026-07-21");
        assert!(!result.context.dates.is_empty(), "should find dates");
        assert!(result.context.dates.contains(&"2026-07-22".to_string()));
        assert!(result.static_prefix.contains("<DATE_1>"), "should replace date with placeholder: {:?}", result.static_prefix);
        assert!(result.dynamic_suffix.starts_with("[Context:"), "suffix should use [Context: ...] format: {:?}", result.dynamic_suffix);
    }

    #[test]
    fn test_extracts_uuids() {
        let mut aligner = CacheAligner::new(CacheAlignmentConfig {
            extract_dates: false, extract_file_paths: false, extract_versions: false, extract_user_context: false, ..Default::default()
        });
        let result = aligner.align("id: 550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(result.context.uuids.len(), 1);
        assert!(result.static_prefix.contains("<UUID_1>"));
    }

    #[test]
    fn test_extracts_paths() {
        let mut aligner = CacheAligner::new(CacheAlignmentConfig::default());
        let result = aligner.align("file at /home/user/src/main.rs");
        assert!(!result.context.file_paths.is_empty(), "should find file paths: {:?}", result.context.file_paths);
    }

    #[test]
    fn test_delta_tracking_no_change() {
        let content = "Today is 2026-07-22";
        let mut aligner = CacheAligner::new(CacheAlignmentConfig::default());
        let r1 = aligner.align(content);
        let r2 = aligner.align(content);
        assert_eq!(r1.static_prefix, r2.static_prefix, "static prefix should be identical");
        assert!(r2.dynamic_suffix.contains("[Context: no change]"), "should detect no change: {:?}", r2.dynamic_suffix);
    }

    #[test]
    fn test_delta_tracking_with_change() {
        let mut aligner = CacheAligner::new(CacheAlignmentConfig::default());
        let _r1 = aligner.align("Today is 2026-07-22");
        let r2 = aligner.align("Today is 2026-07-23");
        assert!(r2.dynamic_suffix.contains("Context changed"), "should detect change: {:?}", r2.dynamic_suffix);
    }

    #[test]
    fn test_cache_aligner_roundtrip() {
        let original = "On 2026-07-22, user alice pushed version 2.4.1 to /repo/main.rs";
        let mut aligner = CacheAligner::new(CacheAlignmentConfig::default());
        let result = aligner.align(original);
        assert!(result.static_prefix.contains("<DATE_1>"), "should replace date, got: {:?}", result.static_prefix);
        assert!(result.static_prefix.contains("<VER_1>"), "should replace version, got: {:?}", result.static_prefix);
        assert!(!result.dynamic_suffix.is_empty(), "should have dynamic suffix");
        assert!(result.dynamic_suffix.starts_with("[Context:"), "suffix should use [Context: ...] format: {:?}", result.dynamic_suffix);
    }

    #[test]
    fn test_dynamic_context_is_empty_on_plain_text() {
        let mut aligner = CacheAligner::new(CacheAlignmentConfig::default());
        let result = aligner.align("Hello, how are you?");
        assert!(result.context.is_empty(), "no dynamic content should be empty");
    }

    #[test]
    fn test_normalize_whitespace() {
        let mut aligner = CacheAligner::new(CacheAlignmentConfig { normalize_whitespace: true, collapse_blank_lines: false, ..Default::default() });
        let result = aligner.align("hello    world\n\n\n  test");
        assert!(result.static_prefix.contains("hello world"), "should normalize spaces: {:?}", result.static_prefix);
    }

    #[test]
    fn test_collapse_blank_lines() {
        let mut aligner = CacheAligner::new(CacheAlignmentConfig { normalize_whitespace: false, collapse_blank_lines: true, ..Default::default() });
        let result = aligner.align("a\n\n\n\n\nb");
        assert!(!result.static_prefix.contains("\n\n\n\n"), "should collapse blanks: {:?}", result.static_prefix);
        assert!(result.static_prefix.contains("a\n\nb"), "should keep one blank: {:?}", result.static_prefix);
    }

    #[test]
    fn test_custom_patterns() {
        let cfg = CacheAlignmentConfig {
            custom_patterns: vec![r"\bFOO\d+\b".to_string()],
            extract_dates: false, extract_file_paths: false, extract_uuids: false,
            extract_versions: false, extract_user_context: false,
            normalize_whitespace: false, collapse_blank_lines: false,
            ..Default::default()
        };
        let mut aligner = CacheAligner::new(cfg);
        let result = aligner.align("test FOO123 bar");
        assert!(!result.static_prefix.contains("FOO123"), "should strip custom match from prefix: {:?}", result.static_prefix);
        assert!(result.dynamic_suffix.contains("custom: FOO123"), "suffix should have custom match: {:?}", result.dynamic_suffix);
    }

    #[test]
    fn test_date_patterns_strips_entire_phrase() {
        let cfg = CacheAlignmentConfig {
            date_patterns: vec![r"Current (Date|Time): [^\n]+".to_string()],
            extract_dates: false, extract_file_paths: false, extract_uuids: false,
            extract_versions: false, extract_user_context: false,
            normalize_whitespace: false, collapse_blank_lines: false,
            ..Default::default()
        };
        let mut aligner = CacheAligner::new(cfg);
        let result = aligner.align("You are helpful. Current Date: 2025-04-06");
        assert!(!result.static_prefix.contains("Current Date:"), "should strip entire matched phrase: {:?}", result.static_prefix);
        assert!(!result.static_prefix.contains("2025-04-06"), "should not contain date: {:?}", result.static_prefix);
        assert!(result.dynamic_suffix.contains("custom: Current Date: 2025-04-06"), "suffix should have the full phrase: {:?}", result.dynamic_suffix);
    }
}
