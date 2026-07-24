use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Manages parallel agent isolation using git worktrees.
pub struct WorktreeManager {
    repo_root: PathBuf,
    worktrees: Arc<Mutex<Vec<WorktreeEntry>>>,
}

struct WorktreeEntry {
    #[allow(dead_code)]
    name: String,
    path: PathBuf,
    branch: String,
}

impl WorktreeManager {
    pub fn new(repo_root: &Path) -> Self {
        Self {
            repo_root: repo_root.to_path_buf(),
            worktrees: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Create a new worktree for an agent.
    pub async fn create_worktree(&self, name: &str) -> Result<PathBuf, String> {
        let worktree_path = self.repo_root.parent()
            .map(|p| p.join(format!("{}-{}", self.repo_root.file_name().unwrap_or_default().to_string_lossy(), name)))
            .unwrap_or_else(|| {
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                cwd.join(format!("worktree-{}", name))
            });

        let branch = format!("agent-{}-{}", name, std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis());

        self.run_git(&[
            "branch", &branch, "HEAD"
        ]).await?;

        self.run_git(&[
            "worktree", "add", &worktree_path.to_string_lossy(), &branch
        ]).await?;

        let mut wts = self.worktrees.lock().await;
        wts.push(WorktreeEntry {
            name: name.to_string(),
            path: worktree_path.clone(),
            branch,
        });

        Ok(worktree_path)
    }

    /// Clean up all created worktrees.
    pub async fn cleanup(&self) -> Result<(), String> {
        let wts = self.worktrees.lock().await;
        for wt in wts.iter() {
            let _ = self.run_git(&["worktree", "remove", &wt.path.to_string_lossy()]).await;
            let _ = self.run_git(&["branch", "-D", &wt.branch]).await;
            let _ = std::fs::remove_dir_all(&wt.path);
        }
        Ok(())
    }

    async fn run_git(&self, args: &[&str]) -> Result<String, String> {
        let output = tokio::process::Command::new("git")
            .args(args)
            .current_dir(&self.repo_root)
            .output()
            .await
            .map_err(|e| format!("git command failed: {}", e))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}

impl Drop for WorktreeManager {
    fn drop(&mut self) {
        // Best-effort cleanup — can't run async in drop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_worktree_not_a_repo() {
        let dir = std::env::temp_dir().join(format!("wt-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let wtm = WorktreeManager::new(&dir);
        let result = wtm.create_worktree("test-agent").await;
        assert!(result.is_err(), "should fail without git repo");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
