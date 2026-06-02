//! In-place self-update from GitHub Releases.
//!
//! Queries the latest release of [`REPO`]; if it is newer than the running
//! build, downloads the `.exe` asset and atomically replaces the running
//! executable via [`self_replace`]. A restart then runs the new build.
//!
//! Exposed as a GPUI [`Updater`] entity so the About window can drive it and
//! reflect progress, mirroring the small surface the previous manifest-based
//! updater offered (`status`, `check`, `download_and_install`, `restart`).

use gpui::{App, AppContext as _, Context, Entity, Global};
use openlogi_core::config::AppSettings;
use tracing::{info, warn};

/// `owner/name` of the GitHub repository releases are published to.
const REPO: &str = "ChrisPrefect/OpenLogi";

/// State of the update flow, surfaced in the About window.
#[derive(Clone, Debug, Default)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    /// A newer version is available (the string is its version).
    Available(String),
    /// Download + install in progress.
    Installing,
    /// The new version is staged on disk; a restart will run it.
    Staged(String),
    Errored(String),
}

/// A newer release found on GitHub.
#[derive(Clone)]
struct Release {
    version: String,
    asset_url: String,
}

/// GPUI entity holding the update state + the pending download.
pub struct Updater {
    status: UpdateStatus,
    pending: Option<Release>,
}

/// App-global handle to the shared updater entity.
#[derive(Clone)]
pub struct SharedUpdater(pub Entity<Updater>);
impl Global for SharedUpdater {}

impl Updater {
    #[must_use]
    pub fn status(&self) -> &UpdateStatus {
        &self.status
    }

    /// Query GitHub for the latest release and compare it to the running build.
    pub fn check(&mut self, cx: &mut Context<Self>) {
        if matches!(
            self.status,
            UpdateStatus::Checking | UpdateStatus::Installing
        ) {
            return;
        }
        self.status = UpdateStatus::Checking;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let task = cx.background_executor().spawn(async { fetch_latest() });
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Some(release)) => {
                        info!(version = %release.version, "update available");
                        this.status = UpdateStatus::Available(release.version.clone());
                        this.pending = Some(release);
                    }
                    Ok(None) => this.status = UpdateStatus::UpToDate,
                    Err(e) => {
                        warn!(error = %e, "update check failed");
                        this.status = UpdateStatus::Errored(e);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Download the staged release's `.exe` and replace the running executable.
    pub fn download_and_install(&mut self, cx: &mut Context<Self>) {
        let Some(release) = self.pending.clone() else {
            return;
        };
        self.status = UpdateStatus::Installing;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let version = release.version.clone();
            let task = cx
                .background_executor()
                .spawn(async move { download_and_replace(&release.asset_url) });
            let result = task.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        info!(version = %version, "update staged — restart to apply");
                        this.status = UpdateStatus::Staged(version);
                    }
                    Err(e) => {
                        warn!(error = %e, "update download/install failed");
                        this.status = UpdateStatus::Errored(e);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Relaunch the (now-replaced) executable and quit this instance.
    pub fn restart(&mut self, cx: &mut Context<Self>) {
        relaunch();
        cx.spawn(async move |_this, cx| {
            let _ = cx.update(|cx| cx.quit());
        })
        .detach();
    }
}

/// Headless update: check GitHub, download + replace if newer, print the
/// outcome. Backs the `--update` CLI flag. Returns a process exit code.
#[must_use]
pub fn run_cli_update() -> i32 {
    match fetch_latest() {
        Ok(None) => {
            println!("OpenLogi is up to date (v{}).", env!("CARGO_PKG_VERSION"));
            0
        }
        Ok(Some(release)) => {
            println!("Updating to v{}…", release.version);
            match download_and_replace(&release.asset_url) {
                Ok(()) => {
                    println!("Updated to v{}. Restart OpenLogi to run it.", release.version);
                    0
                }
                Err(e) => {
                    eprintln!("Update failed: {e}");
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("Update check failed: {e}");
            1
        }
    }
}

/// Build a fresh updater entity in the `Idle` state.
pub fn new_entity(cx: &mut App) -> Entity<Updater> {
    cx.new(|_| Updater {
        status: UpdateStatus::Idle,
        pending: None,
    })
}

/// Publish the shared updater global and, when opted in, check once on launch.
pub fn install(cx: &mut App, settings: &AppSettings) {
    let updater = new_entity(cx);
    if settings.check_for_updates {
        updater.update(cx, Updater::check);
    }
    cx.set_global(SharedUpdater(updater));
}

/// The shared updater entity, if [`install`] has run.
#[must_use]
pub fn shared(cx: &App) -> Option<Entity<Updater>> {
    cx.try_global::<SharedUpdater>().map(|g| g.0.clone())
}

/// Parse a `"x.y.z"` version into a comparable tuple. Missing parts are 0;
/// pre-release / build suffixes are ignored.
fn parse_version(v: &str) -> (u64, u64, u64) {
    let mut parts = v
        .trim_start_matches('v')
        .split(['.', '-', '+'])
        .filter_map(|p| p.parse::<u64>().ok());
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// Query GitHub for the latest release; `Some` when it is newer than this build.
fn fetch_latest() -> Result<Option<Release>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let mut resp = ureq::get(&url)
        .header("User-Agent", "OpenLogi-Updater")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("request failed: {e}"))?;
    let bytes = resp
        .body_mut()
        .read_to_vec()
        .map_err(|e| format!("read failed: {e}"))?;
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("parse failed: {e}"))?;

    let tag = json["tag_name"].as_str().ok_or("release has no tag_name")?;
    if parse_version(tag) <= parse_version(env!("CARGO_PKG_VERSION")) {
        return Ok(None);
    }

    let asset_url = json["assets"]
        .as_array()
        .and_then(|assets| {
            assets
                .iter()
                .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(".exe")))
        })
        .and_then(|a| a["browser_download_url"].as_str())
        .ok_or("latest release has no .exe asset")?;

    Ok(Some(Release {
        version: tag.trim_start_matches('v').to_string(),
        asset_url: asset_url.to_string(),
    }))
}

/// Download `url` to a temp file beside the exe and atomically replace it.
fn download_and_replace(url: &str) -> Result<(), String> {
    let mut resp = ureq::get(url)
        .header("User-Agent", "OpenLogi-Updater")
        .call()
        .map_err(|e| format!("download failed: {e}"))?;

    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let tmp = exe.with_extension("new");
    {
        let mut file = std::fs::File::create(&tmp).map_err(|e| format!("create temp: {e}"))?;
        let mut reader = resp.body_mut().as_reader();
        std::io::copy(&mut reader, &mut file).map_err(|e| format!("write temp: {e}"))?;
    }

    self_replace::self_replace(&tmp).map_err(|e| format!("replace: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

/// Relaunch the executable after a short delay, so this instance can exit and
/// release the single-instance lock before the new one starts.
fn relaunch() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let exe = exe.to_string_lossy().to_string();
    // `cmd /C`: wait ~1s (ping), then start a fresh detached instance.
    let _ = std::process::Command::new("cmd")
        .arg("/C")
        .arg(format!("ping 127.0.0.1 -n 2 >nul & start \"\" \"{exe}\""))
        .spawn();
}
