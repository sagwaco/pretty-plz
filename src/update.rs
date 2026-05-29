//! Version check and self-update.
//!
//! On most invocations we compare the running binary against the latest GitHub
//! release (cached for 24h) and print a one-line hint to stderr when outdated.
//! `plz update` runs the appropriate upgrade path for how the binary was installed.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const REPO: &str = "sagwaco/pretty-plz";
const INSTALL_SCRIPT: &str =
    "https://raw.githubusercontent.com/sagwaco/pretty-plz/main/install.sh";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallMethod {
    Curl,
    Homebrew,
    Npm,
    Cargo,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateCache {
    checked_at: u64,
    latest: String,
}

/// If a newer release exists, print a one-line hint to stderr — using only the
/// cached result, so this never makes a network call on the hot path. When the
/// cache is missing or stale, a detached background refresh is kicked off whose
/// result is read on the *next* invocation. Failures are silent: an update
/// nudge must never delay (let alone block) the user's query.
pub fn maybe_notify() {
    if std::env::var_os("PLZ_NO_UPDATE_CHECK").is_some() {
        return;
    }
    let cached = read_cache();
    if let Some(cache) = &cached {
        notify_if_newer(&cache.latest);
    }
    // A fresh entry — a successful check OR a recorded failure — means we
    // already checked within the interval, so don't spawn another refresh.
    if cached.as_ref().is_some_and(is_fresh) {
        return;
    }
    spawn_refresh();
}

fn notify_if_newer(latest: &str) {
    let current = env!("CARGO_PKG_VERSION");
    if version_newer(latest, current) {
        eprintln!("plz {latest} is available — you have v{current}. Run `plz update` to upgrade.");
    }
}

/// Read the cached check result, or `None` on any error (missing file,
/// unparseable JSON) so callers treat it as "no cache yet".
fn read_cache() -> Option<UpdateCache> {
    let path = cache_path().ok()?;
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn is_fresh(cache: &UpdateCache) -> bool {
    now_unix().saturating_sub(cache.checked_at) < CHECK_INTERVAL.as_secs()
}

/// Refresh the cache off the critical path. The thread is detached: if the
/// process exits first, the worst case is the cache simply isn't updated this
/// run. On failure we still stamp `checked_at` (with an empty `latest`, which
/// never compares as newer) so a transient outage — offline, rate-limited,
/// GitHub down — doesn't make every subsequent invocation retry the fetch.
fn spawn_refresh() {
    std::thread::spawn(|| {
        let latest = fetch_latest_tag().unwrap_or_default();
        let cache = UpdateCache {
            checked_at: now_unix(),
            latest,
        };
        if let Ok(path) = cache_path()
            && let Ok(json) = serde_json::to_string(&cache)
        {
            let _ = write_cache(&path, &json);
        }
    });
}

fn cache_path() -> Result<PathBuf> {
    Ok(crate::config::config_path()?
        .parent()
        .ok_or_else(|| Error::Config("config dir has no parent".into()))?
        .join("update_check.json"))
}

fn write_cache(path: &PathBuf, json: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(json.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

fn fetch_latest_tag() -> Result<String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let agent = ureq::AgentBuilder::new().timeout(HTTP_TIMEOUT).build();
    let resp = agent
        .get(&url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", &format!("plz/{}", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| Error::Network(format!("fetching latest release: {e}")))?;
    let body: serde_json::Value = resp
        .into_json()
        .map_err(|e| Error::Network(format!("parsing release JSON: {e}")))?;
    body.get("tag_name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::BadResponse("release JSON missing tag_name".into()))
}

/// Best-effort guess at how this binary was installed, so `plz update` can
/// delegate to the right package manager.
pub fn detect_install_method() -> InstallMethod {
    let Ok(exe) = std::env::current_exe() else {
        return InstallMethod::Unknown;
    };
    let path = exe.to_string_lossy();
    if path.contains("Cellar") || path.contains("/opt/homebrew/") {
        return InstallMethod::Homebrew;
    }
    if path.contains("node_modules") {
        return InstallMethod::Npm;
    }
    if path.contains(".cargo/bin") {
        return InstallMethod::Cargo;
    }
    if path.contains(".local/bin") {
        return InstallMethod::Curl;
    }
    InstallMethod::Unknown
}

/// Run the upgrade for the detected install method.
pub fn run() -> Result<()> {
    let current = env!("CARGO_PKG_VERSION");
    let latest = fetch_latest_tag()?;
    if !version_newer(&latest, current) {
        eprintln!("plz v{current} is already up to date.");
        return Ok(());
    }

    eprintln!("Updating plz v{current} → {latest}…");

    let status = match detect_install_method() {
        InstallMethod::Homebrew => Command::new("brew").args(["upgrade", "plz"]).status(),
        InstallMethod::Npm => Command::new("npm")
            .args(["update", "-g", "@sagwaco/plz"])
            .status(),
        InstallMethod::Cargo => Command::new("cargo")
            .args([
                "install",
                "plz",
                "--git",
                &format!("https://github.com/{REPO}.git"),
                "--tag",
                &latest,
                "--force",
            ])
            .status(),
        InstallMethod::Curl | InstallMethod::Unknown => Command::new("sh")
            .args(["-c", &format!("curl -fsSL {INSTALL_SCRIPT} | sh")])
            .status(),
    }
    .map_err(Error::Io)?;

    if status.success() {
        eprintln!("Update complete. Run `plz --version` to verify.");
        Ok(())
    } else {
        Err(Error::Config(format!(
            "update failed (exit {}) — try manually:\n  curl -fsSL {INSTALL_SCRIPT} | sh\n\
             or: brew upgrade plz\n\
             or: npm update -g @sagwaco/plz",
            status.code().unwrap_or(-1)
        )))
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Compare semver-like tags (`v1.2.3` or `1.2.3`). Pre-release suffixes are
/// ignored — good enough for release tags.
pub fn version_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let core = s.split('-').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::{parse_version, version_newer};

    #[test]
    fn parse_strips_v_prefix() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3"), Some((1, 2, 3)));
    }

    #[test]
    fn version_newer_compares_semver() {
        assert!(version_newer("v0.2.0", "0.1.0"));
        assert!(version_newer("0.2.0", "0.1.9"));
        assert!(!version_newer("0.1.0", "0.1.0"));
        assert!(!version_newer("0.1.0", "0.2.0"));
    }

    #[test]
    fn empty_latest_never_newer() {
        // Negative-cache sentinel: a failed background refresh stores
        // latest="", which must never trigger an update nudge.
        assert!(!version_newer("", "0.1.0"));
    }
}
