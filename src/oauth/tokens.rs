//! Per-provider OAuth token storage + refresh.
//!
//! Files live under `<config_dir>/oauth/<provider>.json`. Atomic write +
//! 0600 / 0700 permissions are handled by [`crate::secret_file`].

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::oauth::{anthropic, openai};
use crate::provider::Kind;
use crate::secret_file;

/// Stored OAuth tokens for one provider. `expires_at` is unix-seconds; we
/// refresh proactively when it's within 60s of now.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    /// Unix timestamp (seconds) at which `access_token` expires.
    pub expires_at: i64,
    /// Optional OAuth `id_token`; currently used by ChatGPT to persist account
    /// routing metadata and preserved elsewhere if a provider returns one.
    #[serde(default)]
    pub id_token: String,
    /// ChatGPT account id extracted from the id_token. Used as the
    /// `chatgpt-account-id` header for ChatGPT subscription requests.
    #[serde(default)]
    pub chatgpt_account_id: String,
    /// ChatGPT plan label ("plus", "pro", "team", etc.) for status output.
    #[serde(default)]
    pub chatgpt_plan_type: String,
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn oauth_dir() -> Result<PathBuf> {
    let pd = ProjectDirs::from("dev", "sanglee", "plz")
        .ok_or_else(|| Error::Config("could not determine config directory".into()))?;
    Ok(pd.config_dir().join("oauth"))
}

fn path_for(kind: Kind) -> Result<PathBuf> {
    Ok(oauth_dir()?.join(format!("{}.json", kind.as_str())))
}

pub fn load(kind: Kind) -> Result<Option<TokenSet>> {
    let path = path_for(kind)?;
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(Error::Io(e)),
    };
    let t: TokenSet = serde_json::from_str(&text)
        .map_err(|e| Error::OAuth(format!("parsing {}: {e}", path.display())))?;
    Ok(Some(t))
}

pub fn save(kind: Kind, t: &TokenSet) -> Result<()> {
    let text =
        serde_json::to_string_pretty(t).map_err(|e| Error::OAuth(format!("serializing: {e}")))?;
    secret_file::save(&path_for(kind)?, text.as_bytes())
}

pub fn delete(kind: Kind) -> Result<()> {
    secret_file::delete(&path_for(kind)?)
}

/// Return a valid access token, refreshing if the stored one is expired or
/// about to expire. Returns `Err(NotSignedIn)` when no tokens exist.
pub fn valid_access_token(kind: Kind) -> Result<String> {
    let t = load(kind)?.ok_or(Error::NotSignedIn(kind.as_str()))?;
    if t.expires_at - now_unix() > 60 {
        return Ok(t.access_token);
    }
    if t.refresh_token.is_empty() {
        return Err(Error::OAuth(format!(
            "{} access token expired and no refresh_token stored; run `plz login {}`",
            kind.as_str(),
            kind.as_str()
        )));
    }
    handle_refresh(kind, refresh(kind, &t))
}

/// Dispatch to the per-provider refresh. `prev` is passed in (rather than
/// just the refresh_token) so providers can preserve sticky fields that
/// the token endpoint doesn't always re-issue — refresh_token itself when
/// rotation is disabled.
pub fn refresh(kind: Kind, prev: &TokenSet) -> Result<TokenSet> {
    match kind {
        Kind::Anthropic => anthropic::refresh(prev),
        Kind::Codex => openai::refresh(prev),
        Kind::OpenAi => Err(Error::Config(format!(
            "{} does not use plz-managed OAuth tokens",
            kind.as_str()
        ))),
    }
}

/// Force-refresh and persist. Called from the provider HTTP layer on 401.
/// Returns the new access token, or — only when the IdP itself rejects the
/// refresh_token (4xx) — clears tokens and returns `NotSignedIn`. Network
/// failures and 5xx propagate untouched so a transient blip doesn't wipe a
/// still-valid refresh_token.
pub fn force_refresh(kind: Kind) -> Result<String> {
    let stored = load(kind)?.ok_or(Error::NotSignedIn(kind.as_str()))?;
    if stored.refresh_token.is_empty() {
        let _ = delete(kind);
        return Err(Error::NotSignedIn(kind.as_str()));
    }
    handle_refresh(kind, refresh(kind, &stored))
}

/// Shared post-refresh handler:
///   Ok(t)                       → persist, return access_token
///   Err(OAuthInvalidGrant(_))   → delete tokens, return NotSignedIn
///   Err(_)                      → propagate; tokens left intact
fn handle_refresh(kind: Kind, result: Result<TokenSet>) -> Result<String> {
    match result {
        Ok(new) => {
            save(kind, &new)?;
            Ok(new.access_token)
        }
        Err(Error::OAuthInvalidGrant(msg)) => {
            eprintln!(
                "plz: {} refresh rejected by provider; clearing stored tokens ({msg})",
                kind.as_str()
            );
            let _ = delete(kind);
            Err(Error::NotSignedIn(kind.as_str()))
        }
        Err(e) => Err(e),
    }
}
