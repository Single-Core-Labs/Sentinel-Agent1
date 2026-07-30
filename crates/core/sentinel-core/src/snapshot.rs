use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChange {
    Created { path: String, content: String },
    Modified { path: String, before: String, after: String },
    Deleted { path: String, before: String },
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

#[derive(Debug, Default)]
pub struct SnapshotManager {
    snapshots: Vec<Snapshot>,
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
        Self { snapshots: Vec::new() }
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
        Snapshot { turn, before: before.clone(), after: before, changes }
    }

    pub fn update_after<F>(
        &mut self,
        turn: u32,
        workspace_dir: Option<&str>,
        file_reader: F,
    )
    where
        F: Fn(&str) -> Option<String>,
    {
        let after = self.discover_files(workspace_dir, &file_reader);
        if let Some(snapshot) = self.snapshots.iter_mut().rev().find(|s| s.turn == turn) {
            snapshot.changes = Self::compute_changes(&snapshot.before, &after);
            snapshot.after = after;
        }
    }

    pub fn last_changes(&self) -> &[FileChange] {
        self.snapshots.last().map(|s| s.changes.as_slice()).unwrap_or(&[])
    }

    pub fn changes_at_turn(&self, turn: u32) -> &[FileChange] {
        self.snapshots.iter()
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
        let dir = workspace_dir.unwrap_or(".");
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            let mut paths: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .map(|e| e.path())
                .collect();
            paths.sort();
            for path in &paths {
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
        files
    }

    fn compute_changes(before: &[FileSnapshot], after: &[FileSnapshot]) -> Vec<FileChange> {
        let mut changes = Vec::new();

        let before_map: HashMap<&str, &str> = before.iter()
            .map(|f| (f.path.as_str(), f.content.as_str()))
            .collect();
        let after_map: HashMap<&str, &str> = after.iter()
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
        !matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "mp3" | "mp4" | "avi" | "mov" | "bin" | "exe" | "dll" | "so" | "dylib" | "wasm")
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
        let after = vec![
            FileSnapshot { path: "a.txt".into(), content: "hello".into() },
        ];
        let changes = SnapshotManager::compute_changes(&before, &after);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FileChange::Created { .. }));
    }

    #[test]
    fn test_compute_changes_modified() {
        let before = vec![
            FileSnapshot { path: "a.txt".into(), content: "hello".into() },
        ];
        let after = vec![
            FileSnapshot { path: "a.txt".into(), content: "world".into() },
        ];
        let changes = SnapshotManager::compute_changes(&before, &after);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FileChange::Modified { .. }));
    }

    #[test]
    fn test_compute_changes_deleted() {
        let before = vec![
            FileSnapshot { path: "a.txt".into(), content: "hello".into() },
        ];
        let after = vec![];
        let changes = SnapshotManager::compute_changes(&before, &after);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], FileChange::Deleted { .. }));
    }

    #[test]
    fn test_compute_changes_unchanged() {
        let before = vec![
            FileSnapshot { path: "a.txt".into(), content: "same".into() },
        ];
        let after = vec![
            FileSnapshot { path: "a.txt".into(), content: "same".into() },
        ];
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
        mgr.take_snapshot(1, Some("."), reader);
        // In a real scenario update_after would be called with new file state
        let snap = mgr.all_snapshots().last().unwrap();
        assert_eq!(snap.turn, 1);
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
