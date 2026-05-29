//! Sign-in helpers.
//!
//! Three credential paths:
//! - Anthropic OAuth (Claude Pro / Max) — PKCE flow owned by `plz`.
//! - Pasted API key (Anthropic or OpenAI) — saved to `<config_dir>/keys/`.
//! - ChatGPT OAuth — PKCE flow owned by `plz`.
//!
//! Anthropic's endpoint/client ID is derived from the official `claude` CLI
//! and is not publicly documented as a third-party API.

pub mod anthropic;
pub mod openai;
pub mod pkce;
pub mod server;
pub mod tokens;

use crate::api_key;
use crate::error::Result;
use crate::provider::{self, Kind};
use crate::tui;

/// Sign in to a provider via OAuth (the historical entry point — kept for
/// `plz login <provider>`).
pub fn login(kind: Kind) -> Result<()> {
    match kind {
        Kind::Anthropic => anthropic::login(),
        Kind::OpenAi => save_pasted_api_key(Kind::OpenAi),
        Kind::Codex => openai::login(),
    }
}

/// Prompt for an API key and persist it to `<config_dir>/keys/<provider>.txt`.
pub fn save_pasted_api_key(kind: Kind) -> Result<()> {
    let key = tui::prompt_api_key(kind)?;
    api_key::save(kind, &key)?;
    eprintln!(
        "Saved {} API key to {}.",
        kind.as_str(),
        api_key::path_for(kind)?.display()
    );
    Ok(())
}

pub fn logout(kind: Kind) -> Result<()> {
    let mut removed = Vec::new();
    if kind != Kind::Codex && api_key::load(kind)?.is_some() {
        api_key::delete(kind)?;
        removed.push("stored API key");
    }
    if (kind == Kind::Anthropic || kind == Kind::Codex) && tokens::load(kind)?.is_some() {
        tokens::delete(kind)?;
        removed.push("OAuth tokens");
    }

    if removed.is_empty() {
        let env_note = match kind {
            Kind::Anthropic => provider::auth::anthropic_key().map(|_| "ANTHROPIC_API_KEY"),
            Kind::OpenAi => provider::auth::openai_key().map(|_| "OPENAI_API_KEY"),
            Kind::Codex => None,
        };
        match env_note {
            Some(env) => eprintln!(
                "Nothing stored for {} — {env} is still set in your shell environment.",
                kind.as_str()
            ),
            None => eprintln!("Nothing stored for {}.", kind.as_str()),
        }
    } else {
        eprintln!(
            "Forgot {} sign-in state ({}).",
            kind.as_str(),
            removed.join(" + ")
        );
    }
    Ok(())
}

pub fn status() -> Result<()> {
    print_status(Kind::Anthropic)?;
    print_status(Kind::OpenAi)?;
    print_status(Kind::Codex)?;
    Ok(())
}

fn print_status(kind: Kind) -> Result<()> {
    let env_var = match kind {
        Kind::Anthropic => "ANTHROPIC_API_KEY",
        Kind::OpenAi => "OPENAI_API_KEY",
        Kind::Codex => "",
    };

    let mut parts: Vec<String> = Vec::new();

    let env_set = match kind {
        Kind::Anthropic => provider::auth::anthropic_key().is_some(),
        Kind::OpenAi => provider::auth::openai_key().is_some(),
        Kind::Codex => false,
    };
    if env_set {
        parts.push(format!("env var {env_var}"));
    }

    if kind != Kind::Codex && api_key::load(kind)?.is_some() {
        parts.push("stored API key (`plz login`)".into());
    }

    if kind == Kind::Anthropic || kind == Kind::Codex {
        match tokens::load(kind) {
            Ok(Some(t)) => {
                let now = tokens::now_unix();
                let label = if kind == Kind::Codex {
                    if t.chatgpt_plan_type.is_empty() {
                        "ChatGPT OAuth".to_string()
                    } else {
                        format!("ChatGPT OAuth ({})", t.chatgpt_plan_type)
                    }
                } else {
                    "OAuth".to_string()
                };
                if t.expires_at > now {
                    parts.push(format!("{label} valid for {}s", t.expires_at - now));
                } else {
                    parts.push(format!("{label} expired, will refresh on next call"));
                }
            }
            Ok(None) => {}
            Err(e) => parts.push(format!("OAuth error: {e}")),
        }
    }

    if parts.is_empty() {
        eprintln!("{}: not signed in", kind.as_str());
    } else {
        eprintln!("{}: {}", kind.as_str(), parts.join(", "));
    }
    Ok(())
}
