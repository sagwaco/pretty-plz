//! Shell integration: drop the chosen command onto the next prompt instead of
//! making the user copy it.
//!
//! How it works: the binary prints the chosen command to stdout, and a small
//! shell wrapper (installed once via `plz configure` or by hand with
//! `eval "$(plz init zsh|bash)"`) captures that stdout and pushes the command
//! onto the next prompt's edit buffer — zsh's `print -z` for zsh, a DSR /
//! readline-macro polyfill for bash.
//!
//! The wrapper skips `plz` subcommands (`login`, `status`, `init`, `update`, …) so
//! `eval "$(plz init …)"` and `plz --help` pass straight through.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// zsh uses the built-in editing-buffer stack (`print -z`): the pushed text
/// pops onto the command line the next time the line editor starts.
const ZSH: &str = r#"# plz shell integration (zsh) — add to ~/.zshrc:  eval "$(plz init zsh)"
# After you pick a command, it's pushed onto the next prompt for you to review,
# edit, and run — instead of being printed for you to copy.
plz() {
  case "$1" in
  ''|init|configure|login|logout|status|update|uninstall|-h|--help|-V|--version)
    command plz "$@"
    return $?
    ;;
  esac
  local _plz_cmd
  _plz_cmd="$(command plz "$@")" || return $?
  [[ -n "$_plz_cmd" ]] && print -z -- "$_plz_cmd"
}
"#;

/// bash has no editing-buffer stack, so the wrapper uses the standard
/// `print -z` polyfill: ask the terminal for a status report (`ESC [ 6n`-style
/// `ESC [ 5n`) and bind the reply (`ESC [ 0n`) to a readline macro that types
/// the command. The reply arrives once readline starts reading the next prompt,
/// so the command lands on that line, editable, un-run.
const BASH: &str = r#"# plz shell integration (bash) — add to ~/.bashrc:  eval "$(plz init bash)"
# After you pick a command, it's inserted into the next prompt for you to
# review, edit, and run — instead of being printed for you to copy.
plz() {
  case "$1" in
  ''|init|configure|login|logout|status|update|uninstall|-h|--help|-V|--version)
    command plz "$@"
    return $?
    ;;
  esac
  local _plz_cmd
  _plz_cmd="$(command plz "$@")" || return $?
  [[ -n "$_plz_cmd" ]] || return 0
  # Escape backslashes then double-quotes for the readline macro string.
  local _plz_esc="${_plz_cmd//\\/\\\\}"
  _plz_esc="${_plz_esc//\"/\\\"}"
  bind "\"\e[0n\": \"${_plz_esc}\"" 2>/dev/null
  # Send the status-report request to the terminal itself, never to stdout, so
  # the escape can't leak into `cmd=$(plz …)`. No tty -> nothing to pre-fill.
  printf '\033[5n' > /dev/tty 2>/dev/null
}
"#;

/// Print the shell-integration snippet for `shell` to stdout, for use as
/// `eval "$(plz init zsh)"` in a shell rc file. With no argument, falls back to
/// the basename of `$SHELL`.
pub fn init(shell: Option<&str>) -> Result<()> {
    let script = script_for(&resolve_shell(shell)?)?;
    print!("{script}");
    Ok(())
}

fn script_for(shell: &str) -> Result<&'static str> {
    match shell {
        "zsh" => Ok(ZSH),
        "bash" => Ok(BASH),
        other => Err(Error::Config(format!(
            "unsupported shell {other:?}; supported shells are 'zsh' and 'bash'"
        ))),
    }
}

fn resolve_shell(shell: Option<&str>) -> Result<String> {
    match shell {
        Some(s) => Ok(s.to_string()),
        None => detect_shell(),
    }
}

/// Best-effort shell name from `$SHELL` (e.g. `/bin/zsh` -> `zsh`).
fn detect_shell() -> Result<String> {
    let shell = std::env::var("SHELL").map_err(|_| {
        Error::Config(
            "could not detect a shell from $SHELL; pass one explicitly, e.g. `plz init zsh`".into(),
        )
    })?;
    let base = Path::new(&shell)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if base.is_empty() {
        return Err(Error::Config(format!(
            "could not parse a shell name from $SHELL={shell:?}; pass one explicitly, e.g. `plz init zsh`"
        )));
    }
    Ok(base.to_string())
}

/// Outcome of `install_wrapper` — tells the caller whether anything was written
/// so the user-facing message can be accurate.
pub enum InstallOutcome {
    /// Appended the `eval` line to `path`.
    Wrote(PathBuf),
    /// `path` already mentions `plz init`, so we left it alone.
    AlreadyPresent(PathBuf),
}

/// Append `eval "$(plz init <shell>)"` to the user's rc file. Idempotent: if
/// the file already mentions `plz init`, we leave it alone. Validates the shell
/// up front so we can't end up with a half-installed rc.
pub fn install_wrapper(shell: Option<&str>) -> Result<InstallOutcome> {
    let shell = resolve_shell(shell)?;
    script_for(&shell)?;
    let rc_path = rc_path_for(&shell)?;

    if rc_path.exists() {
        let existing = fs::read_to_string(&rc_path).map_err(|e| {
            Error::Config(format!("failed to read {}: {e}", rc_path.display()))
        })?;
        if existing.contains("plz init") {
            return Ok(InstallOutcome::AlreadyPresent(rc_path));
        }
    }

    // Guard the eval with `command -v` so a missing `plz` on PATH (binary not
    // installed yet, install dir not on PATH, …) is a silent no-op at shell
    // startup instead of a noisy `command not found: plz` every login.
    let snippet = format!(
        "\n# plz shell integration — auto-prefills the next prompt with your chosen command\ncommand -v plz >/dev/null 2>&1 && eval \"$(plz init {shell})\"\n"
    );

    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc_path)
        .map_err(|e| Error::Config(format!("failed to open {}: {e}", rc_path.display())))?;
    f.write_all(snippet.as_bytes())
        .map_err(|e| Error::Config(format!("failed to write {}: {e}", rc_path.display())))?;

    Ok(InstallOutcome::Wrote(rc_path))
}

/// True iff a `plz` executable is reachable on the current `$PATH`. Used by
/// `plz configure` to warn when the wrapper is installed but dormant because
/// the binary itself isn't installed yet.
pub fn plz_on_path() -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("plz");
        if !candidate.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&candidate) {
                if meta.permissions().mode() & 0o111 != 0 {
                    return true;
                }
            }
        }
        #[cfg(not(unix))]
        {
            return true;
        }
    }
    false
}

/// On macOS, Terminal.app spawns bash as a *login* shell, which reads
/// `.bash_profile` and not `.bashrc`. On Linux bash, the convention is
/// reversed: interactive non-login shells read `.bashrc`, and `.bash_profile`
/// (if it exists at all) typically sources it. Target the file each platform
/// reads by default.
const INSTALLER_BEGIN: &str = "# >>> plz installer >>>";
const INSTALLER_END: &str = "# <<< plz installer <<<";

/// Shell rc files that may contain plz integration or installer PATH blocks.
pub fn profiles_to_clean() -> Result<Vec<PathBuf>> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| Error::Config("$HOME is not set; can't locate your rc files".into()))?;
    let home = PathBuf::from(home);
    Ok(vec![
        home.join(".zshrc"),
        home.join(".bashrc"),
        home.join(".bash_profile"),
        home.join(".zprofile"),
        home.join(".profile"),
    ])
}

/// Remove plz shell-integration lines and curl-installer PATH blocks from rc files.
pub fn remove_integration_from_profiles() -> Result<Vec<PathBuf>> {
    let mut modified = Vec::new();
    for path in profiles_to_clean()? {
        if !path.exists() {
            continue;
        }
        let content = fs::read_to_string(&path).map_err(|e| {
            Error::Config(format!("failed to read {}: {e}", path.display()))
        })?;
        let (new_content, changed) = clean_profile_content(&content);
        if !changed {
            continue;
        }
        fs::write(&path, &new_content).map_err(|e| {
            Error::Config(format!("failed to write {}: {e}", path.display()))
        })?;
        modified.push(path);
    }
    Ok(modified)
}

fn clean_profile_content(content: &str) -> (String, bool) {
    let (content, path_changed) = strip_installer_path_block(content);
    let (content, shell_changed) = strip_shell_integration_lines(&content);
    (content, path_changed || shell_changed)
}

fn strip_installer_path_block(content: &str) -> (String, bool) {
    let mut changed = false;
    let mut in_block = false;
    let mut kept: Vec<&str> = Vec::new();
    for line in content.lines() {
        if line == INSTALLER_BEGIN {
            in_block = true;
            changed = true;
            continue;
        }
        if in_block {
            if line == INSTALLER_END {
                in_block = false;
            }
            continue;
        }
        kept.push(line);
    }
    (join_lines(&kept, content.ends_with('\n')), changed)
}

fn strip_shell_integration_lines(content: &str) -> (String, bool) {
    let mut changed = false;
    let kept: Vec<&str> = content
        .lines()
        .filter(|line| {
            if is_plz_shell_line(line) {
                changed = true;
                false
            } else {
                true
            }
        })
        .collect();
    (join_lines(&kept, content.ends_with('\n')), changed)
}

fn is_plz_shell_line(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("# plz shell integration")
        || t.contains("plz init")
        || (t.contains("command -v plz") && t.contains("plz init"))
}

fn join_lines(lines: &[&str], trailing_newline: bool) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut out = lines.join("\n");
    if trailing_newline {
        out.push('\n');
    }
    out
}

/// Path to the rc file for the current `$SHELL` (best-effort).
pub fn active_rc_path() -> Result<PathBuf> {
    rc_path_for(&detect_shell()?)
}

fn rc_path_for(shell: &str) -> Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| Error::Config("$HOME is not set; can't locate your rc file".into()))?;
    let rc_name = match shell {
        "zsh" => ".zshrc",
        "bash" if cfg!(target_os = "macos") => ".bash_profile",
        "bash" => ".bashrc",
        other => {
            return Err(Error::Config(format!(
                "no rc file mapping for shell {other:?}; supported shells are 'zsh' and 'bash'"
            )));
        }
    };
    Ok(PathBuf::from(home).join(rc_name))
}

#[cfg(test)]
mod tests {
    use super::{
        clean_profile_content, strip_installer_path_block, strip_shell_integration_lines,
    };

    #[test]
    fn strip_shell_integration_lines_removes_wrapper() {
        let input = "# my config\n\
# plz shell integration — auto-prefills the next prompt with your chosen command\n\
command -v plz >/dev/null 2>&1 && eval \"$(plz init zsh)\"\n\
alias ll='ls -la'\n";
        let (out, changed) = strip_shell_integration_lines(input);
        assert!(changed);
        assert_eq!(out, "# my config\nalias ll='ls -la'\n");
    }

    #[test]
    fn strip_installer_path_block_removes_marked_block() {
        let input = "export FOO=1\n\
# >>> plz installer >>>\n\
export PATH=\"$HOME/.local/bin:$PATH\"\n\
# <<< plz installer <<<\n\
export BAR=2\n";
        let (out, changed) = strip_installer_path_block(input);
        assert!(changed);
        assert_eq!(out, "export FOO=1\nexport BAR=2\n");
    }

    #[test]
    fn clean_profile_content_strips_both() {
        let input = "# >>> plz installer >>>\n\
export PATH=\"$HOME/.local/bin:$PATH\"\n\
# <<< plz installer <<<\n\
# plz shell integration\n\
eval \"$(plz init bash)\"\n\
true\n";
        let (out, changed) = clean_profile_content(input);
        assert!(changed);
        assert_eq!(out, "true\n");
    }
}
