use std::path::PathBuf;
#[cfg(windows)]
use std::path::Path;
use std::process::Stdio;
use std::sync::OnceLock;

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

    for candidate in PYTHON_CANDIDATES {
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

        let Ok(output) = std::process::Command::new(&path)
            .args(["-c", "print(1 + 2)"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output()
        else {
            log::info!("[python-runtime] Candidate '{}' found at {:?} but failed validation", candidate, path);
            continue;
        };

        if output.stdout.trim_ascii() != b"3" {
            log::info!("[python-runtime] Candidate '{}' at {:?} returned unexpected output", candidate, path);
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
/// These live under `%LOCALAPPDATA%\Microsoft\WindowsApps\`.
#[cfg(windows)]
fn is_windows_store_python(path: &Path) -> bool {
    path.components().any(|component| {
        component.as_os_str().eq_ignore_ascii_case("WindowsApps")
    })
}
