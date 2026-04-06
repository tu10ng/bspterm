use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Stdio};

#[cfg(unix)]
use std::os::unix::io::AsRawFd;

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use util::command::new_std_command;

pub enum ScriptStatus {
    NotStarted,
    Running,
    Finished(i32),
    Failed(String),
}

pub struct ScriptRunner {
    script_path: PathBuf,
    connection_string: String,
    focused_terminal_id: Option<String>,
    params: HashMap<String, String>,
    process: Option<Child>,
    status: ScriptStatus,
}

impl ScriptRunner {
    pub fn script_name(&self) -> String {
        self.script_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "script".to_string())
    }

    pub fn new(
        script_path: PathBuf,
        connection_string: String,
        focused_terminal_id: Option<String>,
    ) -> Self {
        Self::new_with_params(script_path, connection_string, focused_terminal_id, HashMap::new())
    }

    pub fn new_with_params(
        script_path: PathBuf,
        connection_string: String,
        focused_terminal_id: Option<String>,
        params: HashMap<String, String>,
    ) -> Self {
        Self {
            script_path,
            connection_string,
            focused_terminal_id,
            params,
            process: None,
            status: ScriptStatus::NotStarted,
        }
    }

    #[allow(clippy::disallowed_methods)]
    pub fn start(&mut self) -> anyhow::Result<()> {
        let python = python_runtime::python_executable()?;

        log::info!(
            "[script-runner] Starting script: path={:?}, python={:?}, connection={}",
            self.script_path,
            python,
            self.connection_string,
        );

        let bspterm_path = self
            .script_path
            .parent()
            .map(|p| p.join("bspterm.py"))
            .unwrap_or_else(|| PathBuf::from("bspterm.py"));

        let scripts_dir = bspterm_path
            .parent()
            .unwrap_or(&PathBuf::from("."))
            .to_path_buf();
        let user_site = python_runtime::user_site_packages();

        let python_path = std::env::join_paths([&scripts_dir, &user_site])
            .unwrap_or_else(|_| scripts_dir.as_os_str().to_os_string());

        log::info!("[script-runner] PYTHONPATH={:?}, terminal_id={:?}", python_path, self.focused_terminal_id);

        let mut command = new_std_command(&python);
        command
            .arg(&self.script_path)
            .env("BSPTERM_SOCKET", &self.connection_string)
            .env("PYTHONPATH", python_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(terminal_id) = &self.focused_terminal_id {
            command.env("BSPTERM_CURRENT_TERMINAL", terminal_id);
        }

        if !self.params.is_empty() {
            log::info!("[script-runner] Script params: {} env vars set", self.params.len());
        }

        for (key, value) in &self.params {
            command.env(key, value);
        }

        let child = command.spawn()?;

        log::info!(
            "[script-runner] Process spawned with pid={}",
            child.id(),
        );

        #[cfg(unix)]
        {
            if let Some(ref stdout) = child.stdout {
                set_nonblocking(stdout.as_raw_fd());
            }
            if let Some(ref stderr) = child.stderr {
                set_nonblocking(stderr.as_raw_fd());
            }
        }

        self.process = Some(child);
        self.status = ScriptStatus::Running;

        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(child) = &mut self.process {
            if let Err(error) = child.kill() {
                log::warn!("Failed to kill script process: {}", error);
            }
            self.status = ScriptStatus::Finished(-1);
        }
        self.process = None;
    }

    pub fn status(&mut self) -> &ScriptStatus {
        if let Some(child) = &mut self.process {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let exit_code = status.code().unwrap_or(-1);
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::ExitStatusExt;
                        if let Some(signal) = status.signal() {
                            log::warn!(
                                "[script-runner] Process killed by signal {} (script: {:?})",
                                signal,
                                self.script_path,
                            );
                        } else {
                            log::info!(
                                "[script-runner] Process exited with code {} (script: {:?})",
                                exit_code,
                                self.script_path,
                            );
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        log::info!(
                            "[script-runner] Process exited with code {} (script: {:?})",
                            exit_code,
                            self.script_path,
                        );
                    }
                    self.status = ScriptStatus::Finished(exit_code);
                    self.process = None;
                }
                Ok(None) => {}
                Err(e) => {
                    log::error!(
                        "[script-runner] Failed to check process status: {} (script: {:?})",
                        e,
                        self.script_path,
                    );
                    self.status = ScriptStatus::Failed(e.to_string());
                    self.process = None;
                }
            }
        }
        &self.status
    }

    pub fn read_output(&mut self) -> Option<String> {
        let child = self.process.as_mut()?;
        let mut output = String::new();

        #[cfg(unix)]
        {
            if let Some(stdout) = child.stdout.as_mut() {
                let mut buf = [0u8; 1024];
                loop {
                    match stdout.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => output.push_str(&String::from_utf8_lossy(&buf[..n])),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(e) => {
                            log::warn!("[script-runner] stdout read error: {}", e);
                            break;
                        }
                    }
                }
            }

            if let Some(stderr) = child.stderr.as_mut() {
                let mut buf = [0u8; 1024];
                loop {
                    match stderr.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => output.push_str(&String::from_utf8_lossy(&buf[..n])),
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                        Err(e) => {
                            log::warn!("[script-runner] stderr read error: {}", e);
                            break;
                        }
                    }
                }
            }
        }

        #[cfg(windows)]
        {
            if let Some(stdout) = child.stdout.as_mut() {
                let handle = stdout.as_raw_handle();
                let available = peek_available(handle);
                if available > 0 {
                    let mut buf = vec![0u8; available.min(4096)];
                    if let Ok(n) = stdout.read(&mut buf) {
                        output.push_str(&String::from_utf8_lossy(&buf[..n]));
                    }
                }
            }

            if let Some(stderr) = child.stderr.as_mut() {
                let handle = stderr.as_raw_handle();
                let available = peek_available(handle);
                if available > 0 {
                    let mut buf = vec![0u8; available.min(4096)];
                    if let Ok(n) = stderr.read(&mut buf) {
                        output.push_str(&String::from_utf8_lossy(&buf[..n]));
                    }
                }
            }
        }

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }
}

#[cfg(unix)]
fn set_nonblocking(fd: i32) {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags == -1 {
            log::warn!("fcntl F_GETFL failed: {}", std::io::Error::last_os_error());
            return;
        }
        if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == -1 {
            log::warn!("fcntl F_SETFL failed: {}", std::io::Error::last_os_error());
        }
    }
}

#[cfg(windows)]
fn peek_available(handle: std::os::windows::io::RawHandle) -> usize {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Pipes::PeekNamedPipe;

    let mut available: u32 = 0;
    unsafe {
        if let Err(error) = PeekNamedPipe(
            HANDLE(handle),
            None,
            0,
            None,
            Some(&mut available),
            None,
        ) {
            log::warn!("PeekNamedPipe failed: {}", error);
        }
    }
    available as usize
}

impl Drop for ScriptRunner {
    fn drop(&mut self) {
        self.stop();
    }
}
