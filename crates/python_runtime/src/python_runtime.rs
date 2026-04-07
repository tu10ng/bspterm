use std::path::PathBuf;
#[cfg(windows)]
use std::path::Path;
use std::process::Stdio;
use std::sync::OnceLock;

use util::command::new_std_command;

const PYTHON_CANDIDATES: &[&str] = &["python3", "python", "py"];

static PYTHON_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Returns the path to a Python executable (cached after first successful discovery).
///
/// Searches system PATH for python3, python, or py. Validates each candidate
/// by running a simple arithmetic check. The result is cached in a `OnceLock`
/// so subsequent calls return instantly.
pub fn python_executable() -> anyhow::Result<PathBuf> {
    if let Some(cached) = PYTHON_PATH.get() {
        log::info!("[python-runtime] Using cached Python: {:?}", cached);
        return Ok(cached.clone());
    }

    let path = find_python_executable()?;
    // Another thread may have raced us; that's fine — just return our result.
    let _ = PYTHON_PATH.set(path.clone());
    Ok(path)
}

#[allow(clippy::disallowed_methods)]
fn find_python_executable() -> anyhow::Result<PathBuf> {
    log::info!("[python-runtime] Searching for Python executable...");

    'candidates: for candidate in PYTHON_CANDIDATES {
        let Ok(path) = which::which(candidate) else {
            log::info!("[python-runtime] Candidate '{}' not found in PATH", candidate);
            continue;
        };

        // Skip Microsoft Store App Execution Aliases on Windows.
        // These are stubs that either open the Store or run a sandboxed Python.
        #[cfg(windows)]
        if is_windows_store_python(&path) {
            log::info!(
                "[python-runtime] Skipping Microsoft Store Python: {:?}",
                path
            );
            continue;
        }

        let Ok(mut child) = new_std_command(&path)
            .args(["-c", "print(1 + 2)"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        else {
            log::info!(
                "[python-runtime] Candidate '{}' found at {:?} but failed to spawn",
                candidate,
                path
            );
            continue;
        };

        // Wait with a 5-second timeout to avoid hanging on broken Python installs
        // (e.g. undetected Microsoft Store stubs).
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(5);
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => break,
                Ok(None) if start.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    log::info!(
                        "[python-runtime] Candidate '{}' at {:?} timed out after 5s",
                        candidate,
                        path
                    );
                    continue 'candidates;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                Err(error) => {
                    log::info!(
                        "[python-runtime] Candidate '{}' at {:?} wait error: {}",
                        candidate,
                        path,
                        error
                    );
                    continue 'candidates;
                }
            }
        }

        // Child has exited — safe to read stdout without deadlock.
        let mut stdout_buf = Vec::new();
        if let Some(mut stdout) = child.stdout.take() {
            let _ = std::io::Read::read_to_end(&mut stdout, &mut stdout_buf);
        }

        if stdout_buf.trim_ascii() != b"3" {
            log::info!(
                "[python-runtime] Candidate '{}' at {:?} returned unexpected output",
                candidate,
                path
            );
            continue;
        }

        log::info!("[python-runtime] Using system Python: {:?}", path);
        return Ok(path);
    }

    anyhow::bail!(
        "Python not found. Tried system PATH candidates: {}. \
         Please install Python and ensure it is in your PATH.",
        PYTHON_CANDIDATES.join(", ")
    )
}

/// Returns the path to the user site-packages directory.
///
/// - Linux: `~/.config/bspterm/python/site-packages/`
/// - Windows: `%LOCALAPPDATA%/Bspterm/python/site-packages/`
pub fn user_site_packages() -> PathBuf {
    #[cfg(unix)]
    {
        if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home)
                .join(".config/bspterm/python/site-packages")
        } else {
            PathBuf::from("/tmp/bspterm/python/site-packages")
        }
    }

    #[cfg(windows)]
    {
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            PathBuf::from(local_app_data)
                .join("Bspterm/python/site-packages")
        } else {
            PathBuf::from("C:/Bspterm/python/site-packages")
        }
    }
}

/// Creates the user site-packages directory if it doesn't exist and returns its path.
pub fn ensure_user_site_packages() -> anyhow::Result<PathBuf> {
    let path = user_site_packages();
    if !path.exists() {
        std::fs::create_dir_all(&path)?;
        log::info!(
            "[python-runtime] Created user site-packages directory: {:?}",
            path
        );
    }
    Ok(path)
}

/// Returns true if the path is a Microsoft Store App Execution Alias.
/// Checks both the `WindowsApps` path component and the reparse point + 0-byte
/// signature that App Execution Aliases use.
#[cfg(windows)]
fn is_windows_store_python(path: &Path) -> bool {
    // Check 1: path contains WindowsApps directory
    if path
        .components()
        .any(|component| component.as_os_str().eq_ignore_ascii_case("WindowsApps"))
    {
        return true;
    }

    // Check 2: App Execution Alias — reparse point with 0 bytes
    if let Ok(metadata) = std::fs::metadata(path) {
        use std::os::windows::fs::MetadataExt;
        let attributes = metadata.file_attributes();
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if attributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 && metadata.len() == 0 {
            log::info!(
                "[python-runtime] Detected App Execution Alias (reparse point): {:?}",
                path
            );
            return true;
        }
    }

    false
}
