//! macOS `LaunchAgent` reconciliation for launch-at-login.
//!
//! When `Config::app_settings.launch_at_login` is `true`, a plist at
//! `~/Library/LaunchAgents/org.openlogi.openlogi.plist` is kept in sync
//! with the currently running executable so the next user-login session
//! relaunches OpenLogi automatically. Setting the flag to `false`
//! removes the plist on the next startup.
//!
//! On Windows the same API writes (or removes) a value under the per-user
//! `Run` registry key so the next login relaunches OpenLogi minimized to the
//! tray. Linux remains a stub (XDG autostart is future work).

use tracing::debug;

#[cfg(target_os = "macos")]
use std::io;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use tracing::{info, warn};

/// Stable launch-agent identifier — matches the bundle id in
/// `crates/openlogi-gui/Cargo.toml [package.metadata.bundle]`.
#[cfg(target_os = "macos")]
const LABEL: &str = "org.openlogi.openlogi";

/// Reconcile the on-disk `LaunchAgent` plist with `enabled`. Idempotent:
/// no-op when the file already matches the desired state.
///
/// Failures are logged at `warn` instead of bubbling up — startup
/// shouldn't abort because the user's `LaunchAgents` directory is
/// read-only.
pub fn reconcile(enabled: bool) {
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = reconcile_macos(enabled) {
            warn!(error = %e, enabled, "LaunchAgent reconcile failed");
        }
    }
    #[cfg(target_os = "windows")]
    {
        reconcile_windows(enabled);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if enabled {
            debug!("launch_at_login set but no autostart backend on this platform");
        }
        let _ = enabled;
    }
}

/// Reconcile the per-user `Run` registry value with `enabled`. Writes
/// `"<exe>" --minimized` under
/// `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` when enabled, removes
/// it when disabled. Failures are logged, not propagated — startup shouldn't
/// abort over a registry hiccup.
#[cfg(target_os = "windows")]
#[allow(
    unsafe_code,
    reason = "registry writes go through raw Win32 FFI; isolated here"
)]
fn reconcile_windows(enabled: bool) {
    use tracing::{info, warn};
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW,
    };

    /// UTF-16, null-terminated — the form the wide Win32 APIs expect.
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    const SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    /// Value name under the Run key; also what shows in Task Manager → Startup.
    const VALUE: &str = "OpenLogi";
    /// `RegDeleteKeyValueW` returns this when the value was already gone.
    const ERROR_FILE_NOT_FOUND: u32 = 2;

    let subkey = wide(SUBKEY);
    let value = wide(VALUE);

    if enabled {
        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => {
                warn!(error = %e, "could not resolve current exe for autostart");
                return;
            }
        };
        // The `--minimized` arg brings the login-launched instance up in the
        // tray with no window, matching the macOS LaunchAgent behavior.
        let command = format!("\"{}\" --minimized", exe.display());
        let data = wide(&command);
        // SAFETY: all pointers reference live, null-terminated wide buffers;
        // `cbData` is their byte length including the terminator.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                value.as_ptr(),
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * std::mem::size_of::<u16>()) as u32,
            )
        };
        if status == 0 {
            info!(command, "autostart Run-key value installed");
        } else {
            warn!(status, "could not write autostart Run-key value");
        }
    } else {
        // SAFETY: both pointers reference live, null-terminated wide buffers.
        let status =
            unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, subkey.as_ptr(), value.as_ptr()) };
        if status == 0 {
            info!("autostart Run-key value removed");
        } else if status == ERROR_FILE_NOT_FOUND {
            debug!("autostart Run-key value already absent");
        } else {
            warn!(status, "could not remove autostart Run-key value");
        }
    }
}

#[cfg(target_os = "macos")]
fn reconcile_macos(enabled: bool) -> io::Result<()> {
    let path = plist_path()?;
    let exe = std::env::current_exe()?;
    let desired = enabled.then(|| render_plist(&exe.to_string_lossy()));

    let current = std::fs::read_to_string(&path).ok();
    match (desired.as_deref(), current.as_deref()) {
        (Some(want), Some(have)) if want == have => {
            debug!(path = %path.display(), "LaunchAgent already current");
        }
        (Some(want), _) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, want)?;
            info!(path = %path.display(), "LaunchAgent installed");
        }
        (None, Some(_)) => {
            std::fs::remove_file(&path)?;
            info!(path = %path.display(), "LaunchAgent removed");
        }
        (None, None) => {
            debug!("LaunchAgent already absent");
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn plist_path() -> io::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "$HOME not set"))?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

#[cfg(target_os = "macos")]
fn render_plist(exe: &str) -> String {
    // launchd accepts both XML and binary plists; XML is human-readable
    // and small enough that the cost is negligible. The `--minimized` arg makes
    // the login-launched instance come up in the menu-bar tray with no window.
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
        <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
        \"http://www.apple.com/DTD/PropertyList-1.0.dtd\">\n\
        <plist version=\"1.0\">\n\
        <dict>\n  \
        <key>Label</key>\n  \
        <string>{LABEL}</string>\n  \
        <key>ProgramArguments</key>\n  \
        <array>\n    \
        <string>{exe}</string>\n    \
        <string>--minimized</string>\n  \
        </array>\n  \
        <key>RunAtLoad</key>\n  \
        <true/>\n  \
        <key>KeepAlive</key>\n  \
        <false/>\n\
        </dict>\n\
        </plist>\n",
    )
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn rendered_plist_contains_expected_keys() {
        let body = render_plist("/Applications/OpenLogi.app/Contents/MacOS/openlogi-gui");
        assert!(body.contains(LABEL));
        assert!(body.contains("/Applications/OpenLogi.app/Contents/MacOS/openlogi-gui"));
        assert!(body.contains("RunAtLoad"));
        assert!(body.contains("--minimized"));
    }
}
