//! Remove plz credentials, shell integration, and the installed binary.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::config;
use crate::error::{Error, Result};
use crate::shell;
use crate::tui;
use crate::update::{self, InstallMethod};

/// Remove config, credentials, shell hooks, and the plz binary.
pub fn run(yes: bool) -> Result<()> {
    if !yes
        && !tui::confirm(
            "Uninstall plz? This removes stored credentials, config, shell integration, and the binary.",
            false,
        )?
    {
        eprintln!("Cancelled.");
        return Ok(());
    }

    eprintln!("Removing stored credentials and config…");
    clear_config()?;

    eprintln!("Removing shell integration…");
    let modified = shell::remove_integration_from_profiles()?;
    if modified.is_empty() {
        eprintln!("  (no plz lines found in shell rc files)");
    } else {
        for path in &modified {
            eprintln!("  updated {}", path.display());
        }
    }

    let method = update::detect_install_method();
    eprintln!("Uninstalling plz binary ({method})…");
    uninstall_binary(method)?;

    eprintln!();
    eprintln!("plz has been uninstalled.");
    eprintln!(
        "If you exported API keys in your shell profile, remove those lines or run:\n\
         \x1b[2m  unset ANTHROPIC_API_KEY OPENAI_API_KEY PLZ_PROVIDER PLZ_AUTH\x1b[0m"
    );
    print_reload_hint(&modified);
    Ok(())
}

fn print_reload_hint(modified: &[PathBuf]) {
    let paths: Vec<PathBuf> = if !modified.is_empty() {
        modified.to_vec()
    } else if let Ok(p) = shell::active_rc_path() {
        if p.exists() {
            vec![p]
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    if paths.is_empty() {
        eprintln!("Open a new shell for changes to take effect.");
        return;
    }

    eprintln!("Open a new shell, or run:");
    for path in paths {
        eprintln!("  source {}", path.display());
    }
}

fn clear_config() -> Result<()> {
    let dir = config::config_dir()?;
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| {
            Error::Config(format!("failed to remove {}: {e}", dir.display()))
        })?;
    }
    Ok(())
}

fn uninstall_binary(method: InstallMethod) -> Result<()> {
    let status = match method {
        InstallMethod::Homebrew => Command::new("brew").args(["uninstall", "plz"]).status(),
        InstallMethod::Npm => Command::new("npm")
            .args(["uninstall", "-g", "@sagwaco/plz"])
            .status(),
        InstallMethod::Cargo => Command::new("cargo").args(["uninstall", "plz"]).status(),
        InstallMethod::Curl | InstallMethod::Unknown => {
            remove_local_binaries()?;
            return Ok(());
        }
    }
    .map_err(Error::Io)?;

    if status.success() {
        return Ok(());
    }

    eprintln!(
        "Package-manager uninstall failed (exit {}) — removing local binary if present.",
        status.code().unwrap_or(-1)
    );
    remove_local_binaries()
}

fn remove_local_binaries() -> Result<()> {
    let mut removed = false;
    if let Ok(exe) = std::env::current_exe()
        && fs::remove_file(&exe).is_ok()
    {
        eprintln!("  removed {}", exe.display());
        removed = true;
    }
    if let Some(local) = local_bin_path()
        && local.exists()
        && fs::remove_file(&local).is_ok()
    {
        eprintln!("  removed {}", local.display());
        removed = true;
    }
    if !removed {
        eprintln!("  could not find a plz binary to remove — delete it manually if it remains on PATH.");
    }
    Ok(())
}

fn local_bin_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".local/bin/plz"))
}
