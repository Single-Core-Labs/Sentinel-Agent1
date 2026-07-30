use std::path::{Path, PathBuf};
use std::sync::Arc;
use async_trait::async_trait;

#[async_trait]
pub trait Sandbox: Send + Sync {
    fn name(&self) -> &str;
    fn root(&self) -> &Path;
    async fn exec(&self, command: &str, workdir: &Path) -> Result<String, String>;
    async fn read_file(&self, path: &Path) -> Result<String, String>;
    async fn write_file(&self, path: &Path, content: &str) -> Result<(), String>;
    async fn destroy(&self);
}

pub struct NoSandbox;

#[async_trait]
impl Sandbox for NoSandbox {
    fn name(&self) -> &str { "none" }
    fn root(&self) -> &Path { std::path::Path::new(".") }

    async fn exec(&self, command: &str, workdir: &Path) -> Result<String, String> {
        run_shell_command(command, workdir).await
    }

    async fn read_file(&self, path: &Path) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    }

    async fn write_file(&self, path: &Path, content: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, content).map_err(|e| e.to_string())
    }

    async fn destroy(&self) {}
}

pub struct LocalSandbox {
    root: PathBuf,
    name: String,
}

impl LocalSandbox {
    pub fn new(workspace: &Path) -> std::io::Result<Self> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir_name = format!("sentinel-sandbox-{}", ts);
        let root = std::env::temp_dir().join(&dir_name);
        std::fs::create_dir_all(&root)?;

        // Copy workspace contents (symlink on unix, copy on windows)
        let dest = root.join("work");
        std::fs::create_dir_all(&dest)?;
        copy_dir_recursive(workspace, &dest)?;

        Ok(Self { root, name: dir_name })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn resolve(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            let rel = path.strip_prefix("/").unwrap_or(path);
            self.root.join("work").join(rel)
        } else {
            self.root.join("work").join(path)
        }
    }
}

#[async_trait]
impl Sandbox for LocalSandbox {
    fn name(&self) -> &str { &self.name }
    fn root(&self) -> &Path { &self.root }

    async fn exec(&self, command: &str, _workdir: &Path) -> Result<String, String> {
        let wd = self.root.join("work");
        run_shell_command(command, &wd).await
    }

    async fn read_file(&self, path: &Path) -> Result<String, String> {
        let sp = self.resolve(path);
        std::fs::read_to_string(&sp).map_err(|e| format!("sandbox read {}: {}", sp.display(), e))
    }

    async fn write_file(&self, path: &Path, content: &str) -> Result<(), String> {
        let sp = self.resolve(path);
        if let Some(parent) = sp.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&sp, content).map_err(|e| format!("sandbox write {}: {}", sp.display(), e))
    }

    async fn destroy(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if ty.is_dir() {
                std::fs::create_dir_all(&dst_path)?;
                copy_dir_recursive(&src_path, &dst_path)?;
            } else if ty.is_file() {
                let _ = std::fs::copy(&src_path, &dst_path);
            }
        }
    }
    Ok(())
}

pub type SharedSandbox = Arc<dyn Sandbox>;

async fn run_shell_command(command: &str, workdir: &Path) -> Result<String, String> {
    let output = tokio::process::Command::new(if cfg!(target_os = "windows") { "cmd" } else { "sh" })
        .arg(if cfg!(target_os = "windows") { "/C" } else { "-c" })
        .arg(command)
        .current_dir(workdir)
        .output()
        .await
        .map_err(|e| format!("command failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        let combined = if stderr.is_empty() { stdout } else { format!("{}\n{}", stdout, stderr) };
        Err(combined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_no_sandbox_exec() {
        let sb = NoSandbox;
        let result = sb.exec("echo hello", &std::env::temp_dir()).await;
        assert!(result.is_ok());
        assert!(result.unwrap().contains("hello"));
    }
}
