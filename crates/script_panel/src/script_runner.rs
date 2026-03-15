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

const PYTHON_CANDIDATES: &[&str] = &["python3", "python", "py"];

#[allow(clippy::disallowed_methods)]
pub fn find_python_executable() -> anyhow::Result<PathBuf> {
    for candidate in PYTHON_CANDIDATES {
        let Ok(path) = which::which(candidate) else {
            continue;
        };

        let Ok(output) = new_std_command(&path)
            .args(["-c", "print(1 + 2)"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        else {
            continue;
        };

        if output.stdout.trim_ascii() != b"3" {
            continue;
        }

        return Ok(path);
    }

    anyhow::bail!(
        "Python not found. Tried: {}. Please install Python and ensure it is in your PATH.",
        PYTHON_CANDIDATES.join(", ")
    )
}

impl ScriptRunner {
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
        let python = find_python_executable()?;

        let bspterm_path = self
            .script_path
            .parent()
            .map(|p| p.join("bspterm.py"))
            .unwrap_or_else(|| PathBuf::from("bspterm.py"));

        let mut command = new_std_command(&python);
        command
            .arg(&self.script_path)
            .env("BSPTERM_SOCKET", &self.connection_string)
            .env("PYTHONPATH", bspterm_path.parent().unwrap_or(&PathBuf::from(".")))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(terminal_id) = &self.focused_terminal_id {
            command.env("BSPTERM_CURRENT_TERMINAL", terminal_id);
        }

        for (key, value) in &self.params {
            command.env(key, value);
        }

        let child = command.spawn()?;

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
                    self.status = ScriptStatus::Finished(status.code().unwrap_or(-1));
                    self.process = None;
                }
                Ok(None) => {}
                Err(e) => {
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
                        Err(_) => break,
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
                        Err(_) => break,
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
