use inquire::list_option::ListOption;
use inquire::{Confirm, InquireError, Password, PasswordDisplayMode, Select, Text};

use crate::error::{Error, Result};
use crate::provider::Kind;
use crate::provider::schema::Command;

/// What the interactive `plz login` picker resolved to.
#[derive(Debug, Clone, Copy)]
pub enum LoginAction {
    /// OAuth — Anthropic only.
    Oauth(Kind),
    /// Save a pasted API key.
    ApiKey(Kind),
    /// Sign in with ChatGPT via OAuth.
    Codex,
}

const OTHER_LABEL: &str = "Other (type your own answer)…";

fn map_inquire_err(e: InquireError) -> Error {
    match e {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => Error::Cancelled,
        other => Error::Config(format!("tui error: {other}")),
    }
}

/// Render a numbered list of candidate commands, return the chosen `cmd`
/// string. The TUI renders to /dev/tty; stdout stays clean.
pub fn pick_command(commands: &[Command]) -> Result<String> {
    if commands.len() == 1 {
        eprintln!("\x1b[2m{}\x1b[0m", commands[0].explanation);
        return Ok(commands[0].cmd.clone());
    }

    let options: Vec<String> = commands
        .iter()
        .map(|c| format!("{}   \x1b[2m— {}\x1b[0m", c.cmd, c.explanation))
        .collect();

    // The chosen command is pushed into the shell's input buffer (see
    // `shell::prefill_tty`), so re-printing it in the post-selection summary
    // would just duplicate what's already on the next prompt. Show only the
    // explanation as confirmation of what was picked.
    let formatter =
        |opt: ListOption<&String>| format!("\x1b[2m{}\x1b[0m", commands[opt.index].explanation);

    let chosen = Select::new("Pick a command:", options)
        .with_help_message("↑↓ to move, Enter to pick, Esc to cancel")
        .with_formatter(&formatter)
        .raw_prompt()
        .map_err(map_inquire_err)?;

    Ok(commands[chosen.index].cmd.clone())
}

/// Ask a clarifying question with multiple-choice answers + a freeform escape
/// hatch. Returns the user's chosen / typed answer.
pub fn ask_clarify(question: &str, choices: &[String]) -> Result<String> {
    let mut options: Vec<String> = choices.iter().cloned().collect();
    options.push(OTHER_LABEL.to_string());

    let chosen = Select::new(question, options.clone())
        .with_help_message("↑↓ to move, Enter to pick, Esc to cancel")
        .prompt()
        .map_err(map_inquire_err)?;

    if chosen == OTHER_LABEL {
        let typed = Text::new("Your answer:")
            .prompt()
            .map_err(map_inquire_err)?;
        Ok(typed)
    } else {
        Ok(chosen)
    }
}

/// Prompt the user to paste the OAuth code shown in the browser after a
/// successful authorization. Used by the Anthropic login flow. Masked so
/// the code/state pair isn't left visible in the user's terminal scrollback.
pub fn prompt_oauth_code() -> Result<String> {
    let pasted = Password::new("Paste the code from the browser:")
        .with_display_mode(PasswordDisplayMode::Masked)
        .without_confirmation()
        .with_help_message("Looks like `<code>#<state>` — copy the whole string")
        .prompt()
        .map_err(map_inquire_err)?;
    Ok(pasted.trim().to_string())
}

/// Interactive picker for `plz login` with no argument.
pub fn pick_login_action() -> Result<LoginAction> {
    let options = vec![
        "Claude account     \x1b[2m— sign in to Claude Pro / Max via the browser\x1b[0m",
        "Anthropic API key  \x1b[2m— paste an Anthropic API key (sk-ant-…)\x1b[0m",
        "OpenAI API key     \x1b[2m— paste an OpenAI API key (sk-…)\x1b[0m",
        "ChatGPT account    \x1b[2m— Sign in with ChatGPT via OAuth\x1b[0m",
    ];

    let chosen = Select::new("Sign in with:", options)
        .with_help_message("↑↓ to move, Enter to pick, Esc to cancel")
        .raw_prompt()
        .map_err(map_inquire_err)?;

    Ok(match chosen.index {
        0 => LoginAction::Oauth(Kind::Anthropic),
        1 => LoginAction::ApiKey(Kind::Anthropic),
        2 => LoginAction::ApiKey(Kind::OpenAi),
        3 => LoginAction::Codex,
        _ => unreachable!(),
    })
}

/// Pick a model for the given provider from a pre-fetched list (usually
/// the live `/v1/models` response, falling back to the curated list).
/// `current` (the model already in config) is pre-selected so a quick Enter
/// keeps the existing choice. An "Other…" escape hatch lets the user type
/// any model ID, in case the provider ships a model the list filter missed.
pub fn pick_model(kind: Kind, models: &[String], current: &str) -> Result<String> {
    let mut options: Vec<String> = models
        .iter()
        .map(|id| {
            let hint = crate::provider::models::tier_hint(kind, id);
            if hint.is_empty() {
                id.clone()
            } else {
                format!("{id}   \x1b[2m— {hint}\x1b[0m")
            }
        })
        .collect();
    options.push(OTHER_LABEL.to_string());

    let starting_cursor = models.iter().position(|id| id == current).unwrap_or(0);

    let chosen = Select::new(&format!("Pick a model for {}:", kind.as_str()), options)
        .with_help_message("↑↓ to move, Enter to pick, Esc to cancel")
        .with_starting_cursor(starting_cursor)
        .raw_prompt()
        .map_err(map_inquire_err)?;

    if chosen.index == models.len() {
        let typed = Text::new("Model ID:")
            .with_default(current)
            .prompt()
            .map_err(map_inquire_err)?;
        let trimmed = typed.trim().to_string();
        if trimmed.is_empty() {
            return Err(Error::Config("no model ID entered".into()));
        }
        Ok(trimmed)
    } else {
        Ok(models[chosen.index].clone())
    }
}

/// Yes/no prompt with a default. Used by `plz configure` to ask whether to
/// install the shell wrapper.
pub fn confirm(question: &str, default: bool) -> Result<bool> {
    Confirm::new(question)
        .with_default(default)
        .prompt()
        .map_err(map_inquire_err)
}

/// Prompt the user to paste an API key, masked. Returns the trimmed key.
pub fn prompt_api_key(kind: Kind) -> Result<String> {
    let prefix = match kind {
        Kind::Anthropic => "sk-ant-",
        Kind::OpenAi => "sk-",
        Kind::Codex => unreachable!("codex has no paste flow"),
    };
    let key = Password::new(&format!("Paste your {} API key:", kind.as_str()))
        .with_display_mode(PasswordDisplayMode::Masked)
        .without_confirmation()
        .with_help_message(&format!(
            "Starts with `{prefix}` — stored at <config_dir>/keys/{}.txt",
            kind.as_str()
        ))
        .prompt()
        .map_err(map_inquire_err)?;
    let trimmed = key.trim().to_string();
    if trimmed.is_empty() {
        return Err(Error::Config("no API key entered".into()));
    }
    Ok(trimmed)
}
