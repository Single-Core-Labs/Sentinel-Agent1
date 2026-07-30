use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineDiff {
    pub line_number: usize,
    pub original: Option<String>,
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiffPreview {
    pub file_path: String,
    pub line_diffs: Vec<LineDiff>,
}

pub struct IdeDiffManager;

impl IdeDiffManager {
    pub fn compute_diff(file_path: &str, original: &str, modified: &str) -> FileDiffPreview {
        let orig_lines: Vec<&str> = original.lines().collect();
        let mod_lines: Vec<&str> = modified.lines().collect();
        let mut line_diffs = Vec::new();

        let max_len = orig_lines.len().max(mod_lines.len());
        for i in 0..max_len {
            let o = orig_lines.get(i).copied().map(String::from);
            let m = mod_lines.get(i).copied().map(String::from);
            if o != m {
                line_diffs.push(LineDiff {
                    line_number: i + 1,
                    original: o,
                    modified: m,
                });
            }
        }

        FileDiffPreview {
            file_path: file_path.to_string(),
            line_diffs,
        }
    }
}
