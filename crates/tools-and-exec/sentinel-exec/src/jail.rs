use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::executor::{ExecOutput, ExecError};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JailMode {
    Auto,
    Disabled,
    JobObject,
    Bubblewrap,
    Seatbelt,
}

#[derive(Debug, Clone)]
pub struct OSJailSandbox {
    pub mode: JailMode,
    pub workdir: PathBuf,
}

impl OSJailSandbox {
    pub fn new(workdir: impl Into<PathBuf>) -> Self {
        Self {
            mode: JailMode::Auto,
            workdir: workdir.into(),
        }
    }

    pub fn with_mode(mut self, mode: JailMode) -> Self {
        self.mode = mode;
        self
    }

    pub async fn run(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        let os = std::env::consts::OS;
        match self.mode {
            JailMode::Disabled => self.run_raw(command, args, env).await,
            JailMode::Bubblewrap => self.run_bubblewrap(command, args, env).await,
            JailMode::Seatbelt => self.run_seatbelt(command, args, env).await,
            JailMode::JobObject => self.run_job_object(command, args, env).await,
            JailMode::Auto => {
                match os {
                    "linux" => {
                        if which_exists("bwrap") {
                            self.run_bubblewrap(command, args, env).await
                        } else {
                            self.run_raw(command, args, env).await
                        }
                    }
                    "macos" => {
                        if which_exists("sandbox-exec") {
                            self.run_seatbelt(command, args, env).await
                        } else {
                            self.run_raw(command, args, env).await
                        }
                    }
                    "windows" => self.run_job_object(command, args, env).await,
                    _ => self.run_raw(command, args, env).await,
                }
            }
        }
    }

    async fn run_raw(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args);
        cmd.current_dir(&self.workdir);
        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
        }

        let output = cmd.output().await?;
        Ok(ExecOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_bubblewrap(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        let mut bwrap = tokio::process::Command::new("bwrap");
        let workdir_str = self.workdir.to_string_lossy();
        bwrap.arg("--unshare-all")
            .arg("--proc").arg("/proc")
            .arg("--dev").arg("/dev")
            .arg("--tmpfs").arg("/tmp")
            .arg("--ro-bind").arg("/").arg("/")
            .arg("--bind").arg(&*workdir_str).arg(&*workdir_str)
            .arg("--chdir").arg(&*workdir_str)
            .arg("--").arg(command).args(args);

        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                bwrap.env(k, v);
            }
        }

        let output = bwrap.output().await?;
        Ok(ExecOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    async fn run_seatbelt(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        let mut sbox = tokio::process::Command::new("sandbox-exec");
        let profile = format!("(version 1)(allow default)(deny file-write* (regex #\"^(?!{})\"#))", self.workdir.to_string_lossy());
        sbox.arg("-p").arg(profile)
            .arg(command).args(args)
            .current_dir(&self.workdir);

        if let Some(env_vars) = env {
            for (k, v) in env_vars {
                sbox.env(k, v);
            }
        }

        let output = sbox.output().await?;
        Ok(ExecOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    #[cfg(windows)]
    async fn run_job_object(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        if env.is_some() {
            tracing::warn!("jail: custom env is not supported for the Job Object sandbox; falling back to raw exec");
            return self.run_raw(command, args, env).await;
        }
        let cmdline = build_command_line(command, args);
        let workdir = self.workdir.clone();
        tokio::task::spawn_blocking(move || win32::run_job_object_blocking(&cmdline, &workdir))
            .await
            .map_err(|e| ExecError::Io(std::io::Error::other(e)))?
    }

    #[cfg(not(windows))]
    async fn run_job_object(&self, command: &str, args: &[&str], env: Option<Vec<(String, String)>>) -> Result<ExecOutput, ExecError> {
        tracing::warn!("jail: Job Object sandbox is Windows-only; falling back to raw exec");
        self.run_raw(command, args, env).await
    }
}

#[cfg(windows)]
mod win32 {
    use super::*;
    use std::path::Path;
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
    use windows_sys::Win32::Storage::FileSystem::ReadFile;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
        JobObjectExtendedLimitInformation, JOBOBJECT_BASIC_LIMIT_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Pipes::{CreatePipe, PeekNamedPipe};
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, ResumeThread, WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED,
        GetExitCodeProcess, PROCESS_INFORMATION, STARTUPINFOW, STARTF_USESTDHANDLES,
    };

    pub fn run_job_object_blocking(cmdline: &str, workdir: &Path) -> Result<ExecOutput, ExecError> {
        let cmdline_wide: Vec<u16> = cmdline.encode_utf16().chain(std::iter::once(0)).collect();
        let workdir_wide: Vec<u16> = workdir.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();

        unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return Err(ExecError::Io(std::io::Error::last_os_error()));
            }

            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation = std::mem::zeroed::<JOBOBJECT_BASIC_LIMIT_INFORMATION>();
            limits.BasicLimitInformation.LimitFlags =
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
            limits.BasicLimitInformation.ActiveProcessLimit = 8;
            let configured = SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if configured == 0 {
                let err = std::io::Error::last_os_error();
                CloseHandle(job);
                return Err(ExecError::Io(err));
            }

            let mut sa = std::mem::zeroed::<SECURITY_ATTRIBUTES>();
            sa.nLength = std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32;
            sa.bInheritHandle = 1;

            let mut out_read = 0isize as *mut std::ffi::c_void;
            let mut out_write = 0isize as *mut std::ffi::c_void;
            let mut err_read = 0isize as *mut std::ffi::c_void;
            let mut err_write = 0isize as *mut std::ffi::c_void;
            if CreatePipe(&mut out_read, &mut out_write, &sa, 0) == 0
                || CreatePipe(&mut err_read, &mut err_write, &sa, 0) == 0
            {
                let err = std::io::Error::last_os_error();
                CloseHandle(job);
                return Err(ExecError::Io(err));
            }

            let mut si = std::mem::zeroed::<STARTUPINFOW>();
            si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
            si.dwFlags = STARTF_USESTDHANDLES;
            si.hStdOutput = out_write;
            si.hStdError = err_write;
            si.hStdInput = std::ptr::null_mut();

            let mut pi = std::mem::zeroed::<PROCESS_INFORMATION>();
            let spawned = CreateProcessW(
                std::ptr::null(),
                cmdline_wide.as_ptr() as *mut u16,
                std::ptr::null(),
                std::ptr::null(),
                1,
                CREATE_SUSPENDED | CREATE_NO_WINDOW,
                std::ptr::null(),
                workdir_wide.as_ptr(),
                &si,
                &mut pi,
            );
            if spawned == 0 {
                let err = std::io::Error::last_os_error();
                CloseHandle(out_read);
                CloseHandle(out_write);
                CloseHandle(err_read);
                CloseHandle(err_write);
                CloseHandle(job);
                return Err(ExecError::Io(err));
            }

            if AssignProcessToJobObject(job, pi.hProcess) == 0 {
                let err = std::io::Error::last_os_error();
                CloseHandle(pi.hThread);
                CloseHandle(pi.hProcess);
                CloseHandle(out_read);
                CloseHandle(out_write);
                CloseHandle(err_read);
                CloseHandle(err_write);
                CloseHandle(job);
                return Err(ExecError::Io(err));
            }
            ResumeThread(pi.hThread);
            CloseHandle(pi.hThread);

            CloseHandle(out_write);
            CloseHandle(err_write);

            let mut stdout: Vec<u8> = Vec::new();
            let mut stderr: Vec<u8> = Vec::new();
            drain_pipes(pi.hProcess, out_read, err_read, &mut stdout, &mut stderr);

            let mut exit_code: u32 = 0;
            GetExitCodeProcess(pi.hProcess, &mut exit_code);

            CloseHandle(pi.hProcess);
            CloseHandle(out_read);
            CloseHandle(err_read);
            CloseHandle(job);

            Ok(ExecOutput {
                stdout: String::from_utf8_lossy(&stdout).to_string(),
                stderr: String::from_utf8_lossy(&stderr).to_string(),
                exit_code: exit_code as i32,
            })
        }
    }

    unsafe fn drain_pipes(
        process: *mut std::ffi::c_void,
        out_read: *mut std::ffi::c_void,
        err_read: *mut std::ffi::c_void,
        stdout: &mut Vec<u8>,
        stderr: &mut Vec<u8>,
    ) {
        let mut buf = [0u8; 8192];
        loop {
            let mut any = false;
            let mut avail: u32 = 0;
            if PeekNamedPipe(out_read, std::ptr::null_mut(), 0, std::ptr::null_mut(), &mut avail, std::ptr::null_mut()) != 0
                && avail > 0
            {
                any = true;
                let mut n: u32 = 0;
                if ReadFile(out_read, buf.as_mut_ptr() as *mut _, avail, &mut n, std::ptr::null_mut()) != 0 {
                    stdout.extend_from_slice(&buf[..n as usize]);
                }
            }
            let mut avail: u32 = 0;
            if PeekNamedPipe(err_read, std::ptr::null_mut(), 0, std::ptr::null_mut(), &mut avail, std::ptr::null_mut()) != 0
                && avail > 0
            {
                any = true;
                let mut n: u32 = 0;
                if ReadFile(err_read, buf.as_mut_ptr() as *mut _, avail, &mut n, std::ptr::null_mut()) != 0 {
                    stderr.extend_from_slice(&buf[..n as usize]);
                }
            }
            if any {
                continue;
            }
            if WaitForSingleObject(process, 50) == WAIT_TIMEOUT {
                continue;
            }
            break;
        }
        unsafe fn drain_until_empty(h: *mut std::ffi::c_void, out: &mut Vec<u8>, buf: &mut [u8]) {
            let mut avail: u32 = 0;
            while PeekNamedPipe(h, std::ptr::null_mut(), 0, std::ptr::null_mut(), &mut avail, std::ptr::null_mut()) != 0
                && avail > 0
            {
                let mut n: u32 = 0;
                if ReadFile(h, buf.as_mut_ptr() as *mut _, avail, &mut n, std::ptr::null_mut()) != 0 {
                    out.extend_from_slice(&buf[..n as usize]);
                }
            }
        }
        drain_until_empty(out_read, stdout, &mut buf);
        drain_until_empty(err_read, stderr, &mut buf);
    }
}

#[cfg(windows)]
fn build_command_line(command: &str, args: &[&str]) -> String {
    let mut parts = Vec::with_capacity(args.len() + 1);
    parts.push(quote_arg(command));
    parts.extend(args.iter().map(|a| quote_arg(a)));
    parts.join(" ")
}

#[cfg(windows)]
fn quote_arg(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quotes = arg.contains(' ') || arg.contains('\t') || arg.contains('"');
    if !needs_quotes {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

fn which_exists(cmd: &str) -> bool {
    if let Ok(path) = std::env::var("PATH") {
        for p in std::env::split_paths(&path) {
            let full = p.join(cmd);
            if full.is_file() {
                return true;
            }
            if cfg!(target_os = "windows") {
                let exe = p.join(format!("{}.exe", cmd));
                if exe.is_file() {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_jail_auto_exec() {
        let jail = OSJailSandbox::new(std::env::temp_dir());
        let res = jail.run(if cfg!(target_os = "windows") { "cmd" } else { "echo" },
                           if cfg!(target_os = "windows") { &["/C", "echo hello"] } else { &["hello"] },
                           None).await;
        assert!(res.is_ok());
        let out = res.unwrap();
        assert!(out.stdout.contains("hello"));
    }
}
