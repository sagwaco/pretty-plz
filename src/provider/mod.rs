pub mod anthropic;
pub mod auth;
pub mod codex;
pub mod models;
pub mod openai;
pub mod schema;

use crate::error::{Error, Result};
use auth::AuthPref;
use schema::Response;

/// Default models per provider — bump these when the recommended model
/// changes. Kept at the top of the file so the rotate-the-model PR is a
/// one-line, easy-to-review diff. Defaults pick the fastest tier per
/// provider, since `plz` queries are short and latency-sensitive; users
/// can swap to a larger model via `plz configure` or `--model`.
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-haiku-4-5";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-5-mini";
pub const DEFAULT_CODEX_MODEL: &str = "gpt-5-mini";

/// Fallback model list used when `provider::models::list` can't reach the
/// provider's `/v1/models` endpoint (offline, scope-restricted token, 5xx).
/// Ordered fastest-first so the highlighted default in the picker is also
/// the recommended pick. Live enumeration is the primary path — keep this
/// list small.
const ANTHROPIC_MODELS: &[&str] = &[
    "claude-haiku-4-5",
    "claude-sonnet-4-6",
    "claude-opus-4-7",
];
const OPENAI_MODELS: &[&str] = &["gpt-5-nano", "gpt-5-mini", "gpt-5"];

/// One conversational turn in the chat with the model.
#[derive(Debug, Clone)]
pub enum Turn {
    User(String),
    /// The model's prior structured response (only used during the clarify
    /// loop). Stored as the parsed Response — each provider re-serializes it
    /// into its own wire format (Anthropic `tool_use` block; OpenAI assistant
    /// message with the JSON text).
    Assistant(Response),
    /// The user's answer to the clarifying question.
    ClarifyAnswer(String),
}

pub trait Provider {
    fn name(&self) -> &'static str;
    fn complete(&self, turns: &[Turn]) -> Result<Response>;
}

/// Which provider to use. Lowercase string keys match what we persist in
/// config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Anthropic,
    OpenAi,
    Codex,
}

impl Kind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "anthropic" | "claude" => Some(Kind::Anthropic),
            "openai" | "gpt" => Some(Kind::OpenAi),
            "chatgpt" | "codex" | "openai-codex" => Some(Kind::Codex),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Anthropic => "anthropic",
            Kind::OpenAi => "openai",
            Kind::Codex => "chatgpt",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Kind::Anthropic => DEFAULT_ANTHROPIC_MODEL,
            Kind::OpenAi => DEFAULT_OPENAI_MODEL,
            Kind::Codex => DEFAULT_CODEX_MODEL,
        }
    }

    /// Curated fallback list of model IDs in fastest-first order. See
    /// [`models::list`] for the live enumeration path.
    pub fn available_models(self) -> &'static [&'static str] {
        match self {
            Kind::Anthropic => ANTHROPIC_MODELS,
            Kind::OpenAi | Kind::Codex => OPENAI_MODELS,
        }
    }
}

pub fn build(
    kind: Kind,
    model: String,
    debug: bool,
    auth_pref: AuthPref,
) -> Result<Box<dyn Provider>> {
    if kind == Kind::Codex {
        if matches!(auth_pref, AuthPref::Api) {
            return Err(Error::Config(
                "`--provider chatgpt --auth api` is not supported; use `--provider openai --auth api` with OPENAI_API_KEY".into(),
            ));
        }
        let cred = auth::credential_with_pref(kind, auth_pref)?.ok_or_else(|| {
            Error::Config("--auth oauth requires `plz login chatgpt` first".into())
        })?;
        return Ok(Box::new(codex::Codex::new(cred, model, debug)));
    }

    let cred =
        auth::credential_with_pref(kind, auth_pref)?.ok_or_else(|| match (kind, auth_pref) {
            // Explicit overrides give a sharper error message so the user knows
            // *which* mode is missing rather than getting "no API key".
            (Kind::Anthropic, AuthPref::Api) => Error::Config(
                "--auth api requires ANTHROPIC_API_KEY or `plz login` with an Anthropic API key"
                    .into(),
            ),
            (Kind::OpenAi, AuthPref::Api) => Error::Config(
                "--auth api requires OPENAI_API_KEY or `plz login` with an OpenAI API key".into(),
            ),
            (Kind::Codex, AuthPref::Api) => unreachable!(),
            (Kind::Anthropic, AuthPref::OAuth) => {
                Error::Config("--auth oauth requires `plz login anthropic` first".into())
            }
            (Kind::OpenAi, AuthPref::OAuth) => Error::Config(
                "OpenAI API-key access does not use OAuth; use `--provider chatgpt` after `plz login chatgpt` for ChatGPT OAuth"
                    .into(),
            ),
            (Kind::Codex, AuthPref::OAuth) => unreachable!(),
            (Kind::Anthropic, AuthPref::Auto) => {
                Error::MissingProviderKey("anthropic", "ANTHROPIC_API_KEY")
            }
            (Kind::OpenAi, AuthPref::Auto) => Error::MissingProviderKey("openai", "OPENAI_API_KEY"),
            (Kind::Codex, AuthPref::Auto) => unreachable!(),
        })?;
    match kind {
        Kind::Anthropic => Ok(Box::new(anthropic::Anthropic::new(cred, model, debug))),
        Kind::OpenAi => Ok(Box::new(openai::OpenAi::new(cred, model, debug))),
        Kind::Codex => unreachable!(),
    }
}
