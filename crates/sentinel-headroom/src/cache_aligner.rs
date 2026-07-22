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

#[derive(Debug, Clone, PartialEq)]
pub struct DynamicContext {
    pub dates: Vec<String>,
    pub times: Vec<String>,
    pub file_paths: Vec<String>,
    pub uuids: Vec<String>,
    pub versions: Vec<String>,
    pub user_context: Vec<String>,
    pub temp_dirs: Vec<String>,
}

impl DynamicContext {
    pub fn is_empty(&self) -> bool {
        self.dates.is_empty() && self.times.is_empty() && self.file_paths.is_empty()
            && self.uuids.is_empty() && self.versions.is_empty()
            && self.user_context.is_empty() && self.temp_dirs.is_empty()
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

        for (k, _) in &changed {
            let pv = match k.as_str() {
                "dates" => Some(format!("{:?}", previous.dates)),
                "times" => Some(format!("{:?}", previous.times)),
                "file_paths" => Some(format!("{:?}", previous.file_paths)),
                "uuids" => Some(format!("{:?}", previous.uuids)),
                "versions" => Some(format!("{:?}", previous.versions)),
                "user_context" => Some(format!("{:?}", previous.user_context)),
                "temp_dirs" => Some(format!("{:?}", previous.temp_dirs)),
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
}

impl CacheAligner {
    pub fn new(config: CacheAlignmentConfig) -> Self {
        Self { config, previous_context: None }
    }

    pub fn with_previous(previous: DynamicContext) -> Self {
        Self {
            config: CacheAlignmentConfig::default(),
            previous_context: Some(previous),
        }
    }

    pub fn align(&mut self, content: &str) -> CacheAlignedPrompt {
        let mut ctx = DynamicContext {
            dates: Vec::new(),
            times: Vec::new(),
            file_paths: Vec::new(),
            uuids: Vec::new(),
            versions: Vec::new(),
            user_context: Vec::new(),
            temp_dirs: Vec::new(),
        };

        let mut result = content.to_string();
        let mut placeholders: Vec<(String, String)> = Vec::new();

        if self.config.extract_dates {
            for cap in date_re().find_iter(content) {
                let val = cap.as_str().to_string();
                if !ctx.dates.contains(&val) {
                    ctx.dates.push(val.clone());
                    let ph = format!("<DATE_{}>", ctx.dates.len());
                    placeholders.push((val, ph));
                }
            }
        }

        let content_for_time = result.clone();
        if self.config.extract_dates {
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
            for cap in filepath_re().find_iter(content) {
                let val = cap.as_str().to_string();
                if !ctx.file_paths.contains(&val) {
                    ctx.file_paths.push(val.clone());
                    let ph = format!("<PATH_{}>", ctx.file_paths.len());
                    placeholders.push((val, ph));
                }
            }
        }

        if self.config.extract_uuids {
            for cap in uuid_re().find_iter(content) {
                let val = cap.as_str().to_string();
                if !ctx.uuids.contains(&val) {
                    ctx.uuids.push(val.clone());
                    let ph = format!("<UUID_{}>", ctx.uuids.len());
                    placeholders.push((val, ph));
                }
            }
        }

        if self.config.extract_versions {
            for cap in version_re().find_iter(content) {
                let val = cap.as_str().to_string();
                if !ctx.versions.contains(&val) {
                    ctx.versions.push(val.clone());
                    let ph = format!("<VER_{}>", ctx.versions.len());
                    placeholders.push((val, ph));
                }
            }
        }

        if self.config.extract_user_context {
            for cap in user_context_re().find_iter(content) {
                let val = cap.as_str().to_string();
                if !ctx.user_context.contains(&val) {
                    ctx.user_context.push(val.clone());
                }
            }
        }

        if self.config.extract_file_paths {
            for cap in temp_dir_re().find_iter(content) {
                let val = cap.as_str().to_string();
                if !ctx.temp_dirs.contains(&val) {
                    ctx.temp_dirs.push(val.clone());
                    let ph = format!("<TEMP_{}>", ctx.temp_dirs.len());
                    placeholders.push((val, ph));
                }
            }
        }

        for (original, placeholder) in &placeholders {
            result = result.replace(original, placeholder);
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
                return "<CONTEXT: no change>".into();
            }
            let mut out = "<CONTEXT_CHANGED:\n".to_string();
            for (key, val) in &delta.changed {
                out.push_str(&format!("  {} → {}\n", key, val));
            }
            out.push('>');
            return out;
        }
    }

    if ctx.is_empty() {
        return String::new();
    }

    let mut out = "<DYNAMIC_CONTEXT:\n".to_string();
    if !ctx.dates.is_empty() {
        out.push_str(&format!("  dates: {}\n", ctx.dates.join(", ")));
    }
    if !ctx.times.is_empty() {
        out.push_str(&format!("  times: {}\n", ctx.times.join(", ")));
    }
    if !ctx.file_paths.is_empty() {
        out.push_str(&format!("  paths: {}\n", ctx.file_paths.join(", ")));
    }
    if !ctx.uuids.is_empty() {
        out.push_str(&format!("  uuids: {}\n", ctx.uuids.join(", ")));
    }
    if !ctx.versions.is_empty() {
        out.push_str(&format!("  versions: {}\n", ctx.versions.join(", ")));
    }
    if !ctx.user_context.is_empty() {
        out.push_str(&format!("  user: {}\n", ctx.user_context.join(", ")));
    }
    if !ctx.temp_dirs.is_empty() {
        out.push_str(&format!("  temps: {}\n", ctx.temp_dirs.join(", ")));
    }
    out.push('>');
    out
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
        assert!(result.static_prefix.contains("<DATE_1>"), "should replace with placeholder: {:?}", result.static_prefix);
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
        assert!(r2.dynamic_suffix.contains("no change"), "should detect no change: {:?}", r2.dynamic_suffix);
    }

    #[test]
    fn test_delta_tracking_with_change() {
        let mut aligner = CacheAligner::new(CacheAlignmentConfig::default());
        let _r1 = aligner.align("Today is 2026-07-22");
        let r2 = aligner.align("Today is 2026-07-23");
        assert!(r2.dynamic_suffix.contains("CHANGED"), "should detect change");
    }

    #[test]
    fn test_cache_aligner_roundtrip() {
        let original = "On 2026-07-22, user alice pushed version 2.4.1 to /repo/main.rs";
        let mut aligner = CacheAligner::new(CacheAlignmentConfig::default());
        let result = aligner.align(original);
        assert!(result.static_prefix.contains("<DATE_1>"), "should replace date, got: {:?}", result.static_prefix);
        assert!(result.static_prefix.contains("<VER_1>"), "should replace version, got: {:?}", result.static_prefix);
        assert!(!result.dynamic_suffix.is_empty(), "should have dynamic suffix");
    }

    #[test]
    fn test_dynamic_context_is_empty_on_plain_text() {
        let mut aligner = CacheAligner::new(CacheAlignmentConfig::default());
        let result = aligner.align("Hello, how are you?");
        assert!(result.context.is_empty(), "no dynamic content should be empty");
    }
}
