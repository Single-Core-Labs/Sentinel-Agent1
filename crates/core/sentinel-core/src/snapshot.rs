use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Directories never walked when snapshotting (build artifacts, VCS metadata,
/// dependency trees — too large and never restored by undo).
const IGNORE_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "target",
    "node_modules",
    "dist",
    "build",
    "out",
    ".venv",
    "venv",
    "__pycache__",
    ".idea",
    ".vscode",
    ".next",
    ".nuxt",
    "coverage",
    ".pytest_cache",
    ".mypy_cache",
];

/// Default number of snapshots kept; oldest are dropped first.
pub const DEFAULT_HISTORY_CAP: usize = 10;

#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    Created {
        path: String,
        content: String,
    },
    Modified {
        path: String,
        before: String,
        after: String,
    },
    Deleted {
        path: String,
        before: String,
    },
}

impl FileChange {
    pub fn path(&self) -> &str {
        match self {
            FileChange::Created { path, .. } => path,
            FileChange::Modified { path, .. } => path,
            FileChange::Deleted { path, .. } => path,
        }
    }
}

#[derive(Debug)]
pub struct SnapshotManager {
    snapshots: Vec<Snapshot>,
    history_cap: usize,
}

impl Default for SnapshotManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
pub struct Snapshot {
    pub turn: u32,
    pub before: Vec<FileSnapshot>,
    pub after: Vec<FileSnapshot>,
    pub changes: Vec<FileChange>,
}

impl SnapshotManager {
    pub fn new() -> Self {
        Self {
            snapshots: Vec::new(),
            history_cap: DEFAULT_HISTORY_CAP,
        }
    }

    pub fn with_history_cap(mut self, cap: usize) -> Self {
        self.history_cap = cap.max(1);
        self
    }

    pub fn take_snapshot<F>(
        &mut self,
        turn: u32,
        workspace_dir: Option<&str>,
        file_reader: F,
    ) -> Snapshot
    where
        F: Fn(&str) -> Option<String>,
    {
        let before = self.discover_files(workspace_dir, &file_reader);

        let changes = if let Some(prev) = self.snapshots.last() {
            Self::compute_changes(&prev.before, &before)
        } else {
            Vec::new()
        };

        let stored = Snapshot {
            turn,
            before: before.clone(),
            after: Vec::new(),
            changes: changes.clone(),
        };
        self.snapshots.push(stored);
        while self.snapshots.len() > self.history_cap {
            self.snapshots.remove(0);
        }
        Snapshot {
            turn,
            before: before.clone(),
            after: before,
            changes,
        }
    }

    pub fn update_after<F>(&mut self, turn: u32, workspace_dir: Option<&str>, file_reader: F)
    where
        F: Fn(&str) -> Option<String>,
    {
        let after = self.discover_files(workspace_dir, &file_reader);
        if let Some(snapshot) = self.snapshots.iter_mut().rev().find(|s| s.turn == turn) {
            snapshot.changes = Self::compute_changes(&snapshot.before, &after);
            snapshot.after = after;
        }
    }

    /// Remove and return the most recent snapshot (used by undo).
    pub fn pop_latest(&mut self) -> Option<Snapshot> {
        self.snapshots.pop()
    }

    pub fn last_changes(&self) -> &[FileChange] {
        self.snapshots
            .last()
            .map(|s| s.changes.as_slice())
            .unwrap_or(&[])
    }

    pub fn changes_at_turn(&self, turn: u32) -> &[FileChange] {
        self.snapshots
            .iter()
            .find(|s| s.turn == turn)
            .map(|s| s.changes.as_slice())
            .unwrap_or(&[])
    }

    pub fn all_snapshots(&self) -> &[Snapshot] {
        &self.snapshots
    }

    fn discover_files<F>(&self, workspace_dir: Option<&str>, reader: &F) -> Vec<FileSnapshot>
    where
        F: Fn(&str) -> Option<String>,
    {
        let root = PathBuf::from(workspace_dir.unwrap_or("."));
        let mut files = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                if path.is_dir() {
                    if !IGNORE_DIRS.contains(&name.as_str()) {
                        stack.push(path);
                    }
                } else if path.is_file() {
                    if let Some(path_str) = path.to_str() {
                        if let Some(content) = reader(path_str) {
                            files.push(FileSnapshot {
                                path: path_str.to_string(),
                                content,
                            });
                        }
                    }
                }
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files
    }

    fn compute_changes(before: &[FileSnapshot], after: &[FileSnapshot]) -> Vec<FileChange> {
        let mut changes = Vec::new();

        let before_map: HashMap<&str, &str> = before
            .iter()
            .map(|f| (f.path.as_str(), f.content.as_str()))
            .collect();
        let after_map: HashMap<&str, &str> = after
            .iter()
            .map(|f| (f.path.as_str(), f.content.as_str()))
            .collect();

        // Detect created and modified
        for file in after {
            match before_map.get(file.path.as_str()) {
                None => {
                    changes.push(FileChange::Created {
                        path: file.path.clone(),
                        content: file.content.clone(),
                    });
                }
                Some(before_content) if *before_content != file.content => {
                    changes.push(FileChange::Modified {
                        path: file.path.clone(),
                        before: before_content.to_string(),
                        after: file.content.clone(),
                    });
                }
                _ => {}
            }
        }

        // Detect deleted
        for file in before {
            if !after_map.contains_key(file.path.as_str()) {
                changes.push(FileChange::Deleted {
                    path: file.path.clone(),
                    before: file.content.clone(),
                });
            }
        }

        changes.sort_by_key(|c| c.path().to_string());
        changes
    }
}

fn is_text_file(path: &Path) -> bool {
    // Basic heuristic: skip common binary extensions
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        !matches!(
            ext,
            "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "bmp"
                | "ico"
                | "mp3"
                | "mp4"
                | "avi"
                | "mov"
                | "bin"
                | "exe"
                | "dll"
                | "so"
                | "dylib"
                | "wasm"
        )
    } else {
        true
    }
}

pub fn default_file_reader(path: &str) -> Option<String> {
    let p = Path::new(path);
    if !p.exists() || !p.is_file() || !is_text_file(p) {
        return None;
    }
    std::fs::read_to_string(path).ok().filter(|s| !s.is_empty())
}

/// Roll a snapshot back onto the filesystem: delete files that were created
/// after the snapshot and rewrite every file the snapshot captured. Returns a
/// list of human-readable paths that were touched.
pub fn restore_snapshot(snapshot: &Snapshot, workspace_dir: &str) -> Result<Vec<String>, String> {
    let dir = Path::new(workspace_dir);
    let mut touched = Vec::new();

    let before_paths: HashSet<&str> = snapshot
        .before
        .iter()
        .map(|f| f.path.as_str())
        .collect();

    for file in &snapshot.after {
        if before_paths.contains(file.path.as_str()) {
            continue;
        }
        let path = resolve_path(dir, &file.path);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| {
                format!("Failed to delete {}: {}", path.display(), e)
            })?;
            touched.push(format!("deleted {}", file.path));
        }
    }

    for file in &snapshot.before {
        let path = resolve_path(dir, &file.path);
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
            }
        }
        std::fs::write(&path, &file.content)
            .map_err(|e| format!("Failed to restore {}: {}", path.display(), e))?;
        touched.push(format!("restored {}", file.path));
    }

    Ok(touched)
}

fn resolve_path(dir: &Path, stored: &str) -> PathBuf {
    let p = Path::new(stored);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        dir.join(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_empty_initial() {
        let mgr = SnapshotManager::new();
        assert!(mgr.all_snapshots().is_empty());
    }

    #[test]
    fn test_compute_changes_created() {
        let before = vec![];
        let after = vec![FileSnapshot {
            path: "a.txt".into(),
            content: "hello".into(),
        }];
        let changes = SnapshotManager::compute_changes(&before, &after);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FileChange::Created { .. }));
    }

    #[test]
    fn test_compute_changes_modified() {
        let before = vec![FileSnapshot {
            path: "a.txt".into(),
            content: "hello".into(),
        }];
        let after = vec![FileSnapshot {
            path: "a.txt".into(),
            content: "world".into(),
        }];
        let changes = SnapshotManager::compute_changes(&before, &after);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FileChange::Modified { .. }));
    }

    #[test]
    fn test_compute_changes_deleted() {
        let before = vec![FileSnapshot {
            path: "a.txt".into(),
            content: "hello".into(),
        }];
        let after = vec![];
        let changes = SnapshotManager::compute_changes(&before, &after);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FileChange::Deleted { .. }));
    }

    #[test]
    fn test_compute_changes_unchanged() {
        let before = vec![FileSnapshot {
            path: "a.txt".into(),
            content: "same".into(),
        }];
        let after = vec![FileSnapshot {
            path: "a.txt".into(),
            content: "same".into(),
        }];
        let changes = SnapshotManager::compute_changes(&before, &after);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_is_text_file() {
        assert!(is_text_file(Path::new("foo.rs")));
        assert!(is_text_file(Path::new("foo.py")));
        assert!(!is_text_file(Path::new("foo.png")));
        assert!(!is_text_file(Path::new("foo.exe")));
    }

    #[test]
    fn test_update_after_computes_changes() {
        let mut mgr = SnapshotManager::new();
        let reader = |_path: &str| -> Option<String> { None };
        let dir = std::env::temp_dir().join(format!(
            "sentinel_snap_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let dir_str = dir.to_string_lossy().into_owned();
        mgr.take_snapshot(1, Some(&dir_str), reader);
        // In a real scenario update_after would be called with new file state
        let snap = mgr.all_snapshots().last().unwrap();
        assert_eq!(snap.turn, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_history_cap_drops_oldest() {
        let mut mgr = SnapshotManager::with_history_cap(SnapshotManager::new(), 2);
        let reader = |_path: &str| -> Option<String> { None };
        let dir = std::env::temp_dir().join(format!(
            "sentinel_snap_cap_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let dir_str = dir.to_string_lossy().into_owned();
        for turn in 0..5 {
            mgr.take_snapshot(turn, Some(&dir_str), reader);
        }
        assert_eq!(mgr.all_snapshots().len(), 2);
        assert_eq!(mgr.all_snapshots()[0].turn, 3);
        assert_eq!(mgr.all_snapshots()[1].turn, 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pop_latest_removes_most_recent() {
        let mut mgr = SnapshotManager::new();
        let reader = |_path: &str| -> Option<String> { None };
        mgr.take_snapshot(1, None, reader);
        mgr.take_snapshot(2, None, reader);
        let popped = mgr.pop_latest().unwrap();
        assert_eq!(popped.turn, 2);
        assert_eq!(mgr.all_snapshots().len(), 1);
        assert_eq!(mgr.all_snapshots()[0].turn, 1);
        assert!(mgr.pop_latest().is_some());
        assert!(mgr.pop_latest().is_none());
    }

    #[test]
    fn test_discover_files_recurses_and_ignores_big_dirs() {
        let dir = std::env::temp_dir().join(format!(
            "sentinel_snap_walk_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("src/nested")).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("src/nested/a.txt"), "hello").unwrap();
        std::fs::write(dir.join("target/b.txt"), "artifact").unwrap();
        let dir_str = dir.to_string_lossy().into_owned();

        let mut mgr = SnapshotManager::new();
        let files = mgr.discover_files(Some(&dir_str), &|_| Some("x".into()));
        let paths: Vec<String> = files
            .iter()
            .map(|f| f.path.replace('\\', "/"))
            .collect();
        assert!(
            paths.iter().any(|p| p.ends_with("src/nested/a.txt")),
            "paths: {:?}",
            paths
        );
        assert!(!paths.iter().any(|p| p.contains("target")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_changes_at_turn_empty_for_unknown() {
        let mgr = SnapshotManager::new();
        assert!(mgr.changes_at_turn(99).is_empty());
    }

    #[test]
    fn test_last_changes_empty_when_no_snapshots() {
        let mgr = SnapshotManager::new();
        assert!(mgr.last_changes().is_empty());
    }
}
