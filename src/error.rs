use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(
        "no provider credentials found — set ANTHROPIC_API_KEY or OPENAI_API_KEY, run `plz login anthropic`, or run `plz login chatgpt`"
    )]
    NoApiKey,

    #[error("no API key for provider {0} — set {1}")]
    MissingProviderKey(&'static str, &'static str),

    #[error("network error: {0}")]
    Network(String),

    #[error("provider returned HTTP {status}: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("provider returned a response that didn't match the expected schema: {0}")]
    BadResponse(String),

    #[error("model kept asking to clarify; rerun with a more specific query")]
    ClarifyLoop,

    #[error("config error: {0}")]
    Config(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("user cancelled")]
    Cancelled,

    #[error("oauth: {0}")]
    OAuth(String),

    /// Token endpoint returned 4xx (invalid_grant, invalid_token, etc.).
    /// Signals the stored refresh_token itself is dead — callers wipe local
    /// tokens and force re-login. Distinct from [`Error::OAuth`] which covers
    /// transient failures (network blip, 5xx) that must NOT trigger deletion.
    #[error("oauth: refresh rejected by provider: {0}")]
    OAuthInvalidGrant(String),

    #[error("not signed in to {0} — run `plz login {0}`")]
    NotSignedIn(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;
