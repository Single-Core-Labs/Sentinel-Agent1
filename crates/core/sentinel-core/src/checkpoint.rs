use crate::snapshot::{SnapshotManager, default_file_reader, restore_snapshot};
use sentinel_tools::CheckpointStore;
use std::sync::Mutex;

/// Thread-safe wrapper around [`SnapshotManager`] that implements
/// [`CheckpointStore`] so the `undo` tool can roll back file changes made by
/// previous tool batches.
#[derive(Debug)]
pub struct CheckpointManager {
    inner: Mutex<SnapshotManager>,
}

impl Default for CheckpointManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CheckpointManager {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(SnapshotManager::new()),
        }
    }
}

impl CheckpointStore for CheckpointManager {
    fn begin_batch(&self, workspace_dir: &str, turn: u32) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.take_snapshot(turn, Some(workspace_dir), default_file_reader);
    }

    fn end_batch(&self, workspace_dir: &str, turn: u32) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.update_after(turn, Some(workspace_dir), default_file_reader);
    }

    fn restore_latest(&self, workspace_dir: &str) -> Result<Vec<String>, String> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let snapshot = inner
            .pop_latest()
            .ok_or_else(|| "nothing to undo".to_string())?;
        restore_snapshot(&snapshot, workspace_dir)
    }

    fn snapshot_count(&self) -> usize {
        self.inner
            .lock()
            .map(|inner| inner.all_snapshots().len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("sentinel_checkpoint_{name}_{unique}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, rel: &str, content: &str) -> String {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn restore_reverts_modified_file() {
        let dir = temp_dir("modify");
        let path = write(&dir, "a.txt", "v1");
        let mgr = CheckpointManager::new();
        let dir_str = dir.to_string_lossy().into_owned();

        mgr.begin_batch(&dir_str, 0);
        std::fs::write(&path, "v2").unwrap();
        mgr.end_batch(&dir_str, 0);

        let touched = mgr.restore_latest(&dir_str).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v1");
        assert!(touched.iter().any(|t| t.contains("a.txt")));
        assert_eq!(mgr.snapshot_count(), 0);
    }

    #[test]
    fn restore_deletes_files_created_in_batch() {
        let dir = temp_dir("created");
        write(&dir, "keep.txt", "keep");
        let mgr = CheckpointManager::new();
        let dir_str = dir.to_string_lossy().into_owned();

        mgr.begin_batch(&dir_str, 0);
        write(&dir, "new.txt", "new content");
        write(&dir, "sub/nested.txt", "nested");
        mgr.end_batch(&dir_str, 0);

        mgr.restore_latest(&dir_str).unwrap();
        assert!(!dir.join("new.txt").exists());
        assert!(!dir.join("sub/nested.txt").exists());
        assert!(dir.join("keep.txt").exists());
    }

    #[test]
    fn restore_rewrites_deleted_file() {
        let dir = temp_dir("deleted");
        let path = write(&dir, "a.txt", "v1");
        let mgr = CheckpointManager::new();
        let dir_str = dir.to_string_lossy().into_owned();

        mgr.begin_batch(&dir_str, 0);
        std::fs::remove_file(&path).unwrap();
        mgr.end_batch(&dir_str, 0);

        mgr.restore_latest(&dir_str).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v1");
    }

    #[test]
    fn restore_lifo_across_batches() {
        let dir = temp_dir("lifo");
        let path = write(&dir, "a.txt", "v1");
        let mgr = CheckpointManager::new();
        let dir_str = dir.to_string_lossy().into_owned();

        mgr.begin_batch(&dir_str, 0);
        std::fs::write(&path, "v2").unwrap();
        mgr.end_batch(&dir_str, 0);

        mgr.begin_batch(&dir_str, 1);
        std::fs::write(&path, "v3").unwrap();
        mgr.end_batch(&dir_str, 1);

        mgr.restore_latest(&dir_str).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v2");
        mgr.restore_latest(&dir_str).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v1");
        // Nothing left to undo
        assert!(mgr.restore_latest(&dir_str).is_err());
    }

    #[test]
    fn snapshot_skips_ignored_directories() {
        let dir = temp_dir("ignore");
        write(&dir, "target/main.rs", "build artifact");
        write(&dir, ".git/HEAD", "ref");
        write(&dir, "src/lib.rs", "real code");
        let mgr = CheckpointManager::new();
        let dir_str = dir.to_string_lossy().into_owned();

        mgr.begin_batch(&dir_str, 0);
        let paths: Vec<String> = {
            let inner = mgr.inner.lock().unwrap();
            inner.all_snapshots()[0]
                .before
                .iter()
                .map(|f| f.path.replace('\\', "/"))
                .collect()
        };
        assert!(
            paths.iter().any(|p| p.ends_with("src/lib.rs")),
            "paths: {:?}",
            paths
        );
        assert!(!paths.iter().any(|p| p.contains("target")));
        assert!(!paths.iter().any(|p| p.contains(".git")));
    }
}
