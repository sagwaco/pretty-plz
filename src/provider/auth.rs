//! Credential resolution for API-key providers and plz-managed OAuth.
//!
//! Per-provider precedence (default):
//!   1. `ANTHROPIC_API_KEY` / `OPENAI_API_KEY` (set in shell or .env)
//!   2. `<config_dir>/keys/<provider>.txt` (written by `plz login`)
//!   3. Provider OAuth tokens for Anthropic / ChatGPT.
//!
//! Override the choice per call with `--auth api|oauth` (or `PLZ_AUTH=...`).

use serde_json::Value;

use crate::api_key;
use crate::error::{Error, Result};
use crate::oauth::tokens;
use crate::provider::Kind;

#[derive(Debug, Clone, Copy)]
pub enum AuthPref {
    Auto,
    Api,
    OAuth,
}

impl AuthPref {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "api" | "key" | "apikey" => Some(Self::Api),
            "oauth" | "login" => Some(Self::OAuth),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Credential {
    /// Direct API key — sent verbatim. For Anthropic that means
    /// `x-api-key`; for OpenAI a `Bearer` header.
    ApiKey(String),
    /// Stored plz-managed OAuth tokens. The actual access token is fetched and
    /// refreshed at call time.
    OAuth,
}

pub fn anthropic_key() -> Option<String> {
    std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
}

pub fn openai_key() -> Option<String> {
    std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
}

fn env_key(kind: Kind) -> Option<String> {
    match kind {
        Kind::Anthropic => anthropic_key(),
        Kind::OpenAi => openai_key(),
        Kind::Codex => None,
    }
}

/// `plz login`-pasted API key, if any. ChatGPT doesn't have a paste flow.
fn stored_key(kind: Kind) -> Result<Option<String>> {
    if kind == Kind::Codex {
        return Ok(None);
    }
    api_key::load(kind)
}

/// Default credential lookup (API key first, OAuth second). Kept for
/// auto-detect; runtime callers should use [`credential_with_pref`].
pub fn credential(kind: Kind) -> Option<Credential> {
    credential_with_pref(kind, AuthPref::Auto).ok().flatten()
}

/// Resolve a credential under the user's preference. `Auto` keeps the
/// existing precedence; `Api` and `OAuth` force one mode and return an
/// explicit error if that mode isn't provisioned.
pub fn credential_with_pref(kind: Kind, pref: AuthPref) -> Result<Option<Credential>> {
    match pref {
        AuthPref::Auto => {
            if let Some(k) = env_key(kind) {
                return Ok(Some(Credential::ApiKey(k)));
            }
            if let Some(k) = stored_key(kind)? {
                return Ok(Some(Credential::ApiKey(k)));
            }
            if kind == Kind::Anthropic || kind == Kind::Codex {
                Ok(match tokens::load(kind)? {
                    Some(_) => Some(Credential::OAuth),
                    None => None,
                })
            } else {
                Ok(None)
            }
        }
        AuthPref::Api => {
            if let Some(k) = env_key(kind) {
                return Ok(Some(Credential::ApiKey(k)));
            }
            Ok(stored_key(kind)?.map(Credential::ApiKey))
        }
        AuthPref::OAuth => {
            if kind == Kind::Anthropic || kind == Kind::Codex {
                Ok(tokens::load(kind)?.map(|_| Credential::OAuth))
            } else {
                Ok(None)
            }
        }
    }
}

/// Has *any* credential been provisioned for this provider? Cheap check
/// used by config auto-detect.
pub fn has_any(kind: Kind) -> bool {
    credential(kind).is_some()
}

/// Common pattern shared by both providers: build a request, attach the
/// right auth header(s), send, and on 401 with OAuth force-refresh and
/// retry exactly once. Each provider supplies `set_api_key` and
/// `set_bearer` closures because Anthropic also needs an `anthropic-beta`
/// header on bearer auth.
pub fn call_with_auth<RB, K, B>(
    cred: &Credential,
    kind: Kind,
    body: &Value,
    request_factory: RB,
    set_api_key: K,
    set_bearer: B,
) -> Result<Value>
where
    RB: Fn() -> ureq::Request,
    K: Fn(ureq::Request, &str) -> ureq::Request,
    B: Fn(ureq::Request, &str) -> ureq::Request,
{
    match cred {
        Credential::ApiKey(k) => send_once(set_api_key(request_factory(), k), body),
        Credential::OAuth => {
            let token = tokens::valid_access_token(kind)?;
            match send_once(set_bearer(request_factory(), &token), body) {
                Err(Error::HttpStatus { status: 401, .. }) => {
                    let refreshed = tokens::force_refresh(kind)?;
                    send_once(set_bearer(request_factory(), &refreshed), body)
                }
                other => other,
            }
        }
    }
}

fn send_once(req: ureq::Request, body: &Value) -> Result<Value> {
    match req.send_json(body.clone()) {
        Ok(resp) => resp.into_json().map_err(|e| Error::Network(e.to_string())),
        Err(ureq::Error::Status(status, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            Err(Error::HttpStatus { status, body })
        }
        Err(e) => Err(Error::Network(e.to_string())),
    }
}
