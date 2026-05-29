use std::fmt::Display;

use inquire::list_option::ListOption;
use inquire::ui::{RenderConfig, Styled};
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

const CHOICE_HELP: &str = "↑↓ to move, Enter to pick, Esc to cancel";

/// inquire's frame renderer discards wholly empty lines; a lone NBSP reads as blank.
const LIST_FOOTER_GAP: &str = "\n\u{00A0}";

/// `>` on the highlighted row, `-` on the others — same column, caret replaces the dash.
fn choice_render_config() -> RenderConfig<'static> {
    RenderConfig {
        highlighted_option_prefix: Styled::new(">"),
        unhighlighted_option_prefix: Styled::new("-"),
        ..RenderConfig::default()
    }
}

/// Blank line between the prompt and the first option.
fn choice_prompt(message: &str) -> String {
    format!("{message}\n")
}

/// Shared layout for list pickers: no filter input (hides the stray cursor on the prompt).
fn choice_select<'a, T: Display>(prompt: &'a str, options: Vec<T>) -> Select<'a, T> {
    Select::new(prompt, options)
        .with_render_config(choice_render_config())
        .without_filtering()
        .with_help_message(CHOICE_HELP)
}

fn map_inquire_err(e: InquireError) -> Error {
    match e {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => Error::Cancelled,
        other => Error::Config(format!("tui error: {other}")),
    }
}

/// Render a numbered list of candidate commands, return the chosen `cmd`
/// string. The TUI renders to /dev/tty; stdout stays clean.
///
/// Each option is shown on two lines — a short explanation on top, the
/// command in dim text indented underneath. The row prefix is `>` when
/// highlighted and `-` otherwise. Long text is pre-wrapped with a hanging
/// 2-space indent so soft-wrapped continuations do not bleed back to
/// column 0. The embedded `\x1b[0m` after the explanation cancels any outer
/// "selected option" style (default cyan) so the command always renders dim,
/// regardless of whether its row is highlighted.
pub fn pick_command(commands: &[Command]) -> Result<String> {
    if commands.len() == 1 {
        eprintln!("\x1b[2m{}\x1b[0m", commands[0].explanation);
        return Ok(commands[0].cmd.clone());
    }

    // Prefix column (`>` or `-` plus a separating space) eats 2 columns; the
    // command row is indented 2 spaces under the explanation.
    let cols = terminal_cols();
    let expl_width = cols.saturating_sub(2).max(20);
    let cmd_width = cols.saturating_sub(2).max(20);
    let last = commands.len() - 1;

    let options: Vec<String> = commands
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let expl = wrap_hanging(&c.explanation, expl_width, "  ");
            let cmd = wrap_hanging(&c.cmd, cmd_width, "  ");
            let mut block = format!("{expl}\x1b[0m\n  \x1b[2m{cmd}\x1b[0m");
            if i == last {
                block.push_str(LIST_FOOTER_GAP);
            }
            block
        })
        .collect();

    // The chosen command is pushed into the shell's input buffer (see
    // `shell::prefill_tty`), so re-printing it in the post-selection summary
    // would just duplicate what's already on the next prompt. Show only the
    // explanation as confirmation of what was picked.
    let formatter =
        |opt: ListOption<&String>| format!("\x1b[2m{}\x1b[0m", commands[opt.index].explanation);

    let prompt = choice_prompt("Pick a command:");
    let chosen = choice_select(&prompt, options)
        .with_formatter(&formatter)
        .raw_prompt()
        .map_err(map_inquire_err)?;

    Ok(commands[chosen.index].cmd.clone())
}

/// Wrap `text` to `width` columns, prefixing every wrapped continuation line
/// (i.e. all lines after the first) with `cont_indent`. Prefers breaking on
/// whitespace but will hard-split a token that exceeds the line budget so a
/// single long argument never blows out the layout. Width is measured in
/// chars, not unicode display cells — good enough for the ASCII-heavy
/// commands `plz` returns; double-width glyphs may wrap one cell short.
fn wrap_hanging(text: &str, width: usize, cont_indent: &str) -> String {
    let indent_w = cont_indent.chars().count();
    if width == 0 || width <= indent_w || text.chars().count() <= width {
        return text.to_string();
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut col = 0usize;
    let mut at_line_start = true;

    let hard_split = |word: &str,
                      lines: &mut Vec<String>,
                      current: &mut String,
                      col: &mut usize| {
        let mut iter = word.chars();
        loop {
            let mut remaining = width.saturating_sub(*col);
            if remaining == 0 {
                lines.push(std::mem::take(current));
                current.push_str(cont_indent);
                *col = indent_w;
                remaining = width - indent_w;
            }
            let taken: String = iter.by_ref().take(remaining).collect();
            if taken.is_empty() {
                break;
            }
            *col += taken.chars().count();
            current.push_str(&taken);
        }
    };

    for word in text.split(' ') {
        if word.is_empty() {
            // Collapse consecutive spaces — `plz` commands don't have
            // semantically meaningful runs of spaces, so this keeps the
            // wrap math simple.
            continue;
        }
        let wlen = word.chars().count();

        if at_line_start {
            if wlen <= width.saturating_sub(col) {
                current.push_str(word);
                col += wlen;
            } else {
                hard_split(word, &mut lines, &mut current, &mut col);
            }
            at_line_start = false;
            continue;
        }

        // Subsequent word on the current line — needs a separator space.
        if 1 + wlen <= width.saturating_sub(col) {
            current.push(' ');
            current.push_str(word);
            col += 1 + wlen;
            continue;
        }

        // Doesn't fit. Wrap to a continuation line and place the word there.
        lines.push(std::mem::take(&mut current));
        current.push_str(cont_indent);
        col = indent_w;
        if wlen <= width - indent_w {
            current.push_str(word);
            col += wlen;
        } else {
            hard_split(word, &mut lines, &mut current, &mut col);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines.join("\n")
}

/// Width of the controlling terminal in columns. Falls back to 80 when the
/// query fails (non-TTY, redirected stderr) — that's the conventional
/// default and roughly matches what every CI / pipe scenario will assume.
fn terminal_cols() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}

fn choice_options(mut labels: Vec<String>) -> Vec<String> {
    if let Some(last) = labels.last_mut() {
        last.push_str(LIST_FOOTER_GAP);
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::wrap_hanging;

    #[test]
    fn short_text_is_unchanged() {
        assert_eq!(wrap_hanging("ls -lah", 40, "  "), "ls -lah");
    }

    #[test]
    fn wraps_on_word_boundary_with_indent() {
        let out = wrap_hanging("alpha bravo charlie delta", 12, "  ");
        // First line stays at column 0; subsequent lines get the 2-space
        // indent so they line up under whatever caller prefixed them.
        assert_eq!(out, "alpha bravo\n  charlie\n  delta");
    }

    #[test]
    fn hard_splits_oversized_token() {
        let out = wrap_hanging("aaaaaaaaaaaaaaa", 5, "  ");
        // Token longer than width is split across lines; continuation lines
        // start past the indent.
        assert_eq!(out, "aaaaa\n  aaa\n  aaa\n  aaa\n  a");
    }

    #[test]
    fn wraps_a_realistic_long_command() {
        let cmd =
            "find . -type f -name '*.*' | rev | cut -d. -f1 | rev | sort | uniq -c | sort -rn";
        let out = wrap_hanging(cmd, 30, "  ");
        for (i, line) in out.lines().enumerate() {
            assert!(
                line.chars().count() <= 30,
                "line {i} ({line:?}) wider than 30"
            );
            if i > 0 {
                assert!(line.starts_with("  "), "continuation line missing indent: {line:?}");
            }
        }
    }
}

/// Ask a clarifying question with multiple-choice answers + a freeform escape
/// hatch. Returns the user's chosen / typed answer.
pub fn ask_clarify(question: &str, choices: &[String]) -> Result<String> {
    let mut options: Vec<String> = choices.iter().cloned().collect();
    options.push(OTHER_LABEL.to_string());
    let options = choice_options(options);

    let prompt = choice_prompt(question);
    let chosen = choice_select(&prompt, options)
        .raw_prompt()
        .map_err(map_inquire_err)?;

    if chosen.index == choices.len() {
        let typed = Text::new("Your answer:")
            .prompt()
            .map_err(map_inquire_err)?;
        Ok(typed)
    } else {
        Ok(choices[chosen.index].clone())
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
    let options = choice_options(vec![
        "Claude account     \x1b[2m— sign in to Claude Pro / Max via the browser\x1b[0m".into(),
        "ChatGPT account    \x1b[2m— sign in with ChatGPT via OAuth\x1b[0m".into(),
        "Anthropic API key  \x1b[2m— paste an Anthropic API key (sk-ant-…)\x1b[0m".into(),
        "OpenAI API key     \x1b[2m— paste an OpenAI API key (sk-…)\x1b[0m".into(),
    ]);

    let prompt = choice_prompt("Sign in with:");
    let chosen = choice_select(&prompt, options)
        .raw_prompt()
        .map_err(map_inquire_err)?;

    Ok(match chosen.index {
        0 => LoginAction::Oauth(Kind::Anthropic),
        1 => LoginAction::Codex,
        2 => LoginAction::ApiKey(Kind::Anthropic),
        3 => LoginAction::ApiKey(Kind::OpenAi),
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
    let options = choice_options(options);

    let starting_cursor = models.iter().position(|id| id == current).unwrap_or(0);

    let prompt = choice_prompt(&format!("Pick a model for {}:", kind.as_str()));
    let chosen = choice_select(&prompt, options)
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
