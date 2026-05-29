//! Live model enumeration for the `plz configure` picker.
//!
//! Anthropic and OpenAI both expose `GET /v1/models`. We hit it after the
//! user has just signed in, filter to chat-capable IDs, and sort by a
//! speed-tier heuristic so the fastest model lands at the top of the picker
//! (and becomes the highlighted default).
//!
//! Codex (ChatGPT OAuth) is scoped to the Responses API and can't list
//! models, so it short-circuits to the curated fallback. The caller falls
//! back to [`curated`] on any error too, so `configure` keeps working
//! offline or behind a misconfigured network.

use std::time::Duration;

use serde_json::Value;

use super::auth::Credential;
use crate::error::{Error, Result};
use crate::oauth::anthropic::OAUTH_BETA_HEADER;
use crate::oauth::tokens;
use crate::provider::Kind;

const ANTHROPIC_VERSION: &str = "2023-06-01";
// limit=1000 is Anthropic's max page size; they have ~tens of models, so one
// page is enough and we skip pagination glue.
const ANTHROPIC_MODELS_URL: &str = "https://api.anthropic.com/v1/models?limit=1000";
const OPENAI_MODELS_URL: &str = "https://api.openai.com/v1/models";
const TIMEOUT: Duration = Duration::from_secs(15);

/// Fastest-tier-first list of model IDs available to this credential.
pub fn list(kind: Kind, cred: &Credential) -> Result<Vec<String>> {
    let url = match kind {
        Kind::Anthropic => ANTHROPIC_MODELS_URL,
        Kind::OpenAi => OPENAI_MODELS_URL,
        // ChatGPT OAuth's scope is Responses-API only — no /v1/models access.
        Kind::Codex => return Ok(curated(kind)),
    };
    let v = auth_get(kind, cred, url)?;
    let mut ids: Vec<String> = v
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::BadResponse(format!("models endpoint missing `data`: {v}")))?
        .iter()
        .filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
        .filter(|id| looks_chat(kind, id))
        .collect();

    if ids.is_empty() {
        return Ok(curated(kind));
    }
    // (tier asc, id desc) → fastest tier first, newest snapshot first within tier.
    ids.sort_by(|a, b| tier(kind, a).cmp(&tier(kind, b)).then_with(|| b.cmp(a)));
    ids.dedup();
    Ok(ids)
}

/// Built-in fallback used when live listing fails or yields nothing usable.
pub fn curated(kind: Kind) -> Vec<String> {
    kind.available_models()
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// Short tag rendered next to each model ID in the picker. Derived from the
/// ID itself so future snapshots (e.g. `claude-haiku-5-…`) get a label
/// automatically, without a code change.
pub fn tier_hint(kind: Kind, id: &str) -> &'static str {
    let l = id.to_ascii_lowercase();
    match kind {
        Kind::Anthropic => {
            if l.contains("haiku") {
                "haiku · fastest"
            } else if l.contains("sonnet") {
                "sonnet · balanced"
            } else if l.contains("opus") {
                "opus · most capable"
            } else {
                ""
            }
        }
        Kind::OpenAi | Kind::Codex => {
            if l.contains("nano") {
                "nano · fastest"
            } else if l.contains("mini") {
                "mini · fast"
            } else if l.starts_with("gpt-") {
                "balanced"
            } else if is_reasoning(&l) {
                "reasoning, slower"
            } else {
                ""
            }
        }
    }
}

fn auth_get(kind: Kind, cred: &Credential, url: &str) -> Result<Value> {
    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();
    let base = || {
        let mut req = agent.get(url);
        if matches!(kind, Kind::Anthropic) {
            req = req.set("anthropic-version", ANTHROPIC_VERSION);
        }
        req
    };
    let attach_oauth = |req: ureq::Request, token: &str| -> ureq::Request {
        match kind {
            Kind::Anthropic => req
                .set("anthropic-beta", OAUTH_BETA_HEADER)
                .set("Authorization", &format!("Bearer {token}")),
            Kind::OpenAi | Kind::Codex => req.set("Authorization", &format!("Bearer {token}")),
        }
    };
    let send = |req: ureq::Request| -> Result<Value> {
        match req.call() {
            Ok(resp) => resp.into_json().map_err(|e| Error::Network(e.to_string())),
            Err(ureq::Error::Status(status, resp)) => Err(Error::HttpStatus {
                status,
                body: resp.into_string().unwrap_or_default(),
            }),
            Err(e) => Err(Error::Network(e.to_string())),
        }
    };
    match cred {
        Credential::ApiKey(k) => {
            let req = match kind {
                Kind::Anthropic => base().set("x-api-key", k),
                Kind::OpenAi | Kind::Codex => base().set("Authorization", &format!("Bearer {k}")),
            };
            send(req)
        }
        Credential::OAuth => {
            let token = tokens::valid_access_token(kind)?;
            // Mirror the 401-refresh-retry-once policy used by the inference
            // path (see auth::call_with_auth) so a just-expired token doesn't
            // crater the picker.
            match send(attach_oauth(base(), &token)) {
                Err(Error::HttpStatus { status: 401, .. }) => {
                    let refreshed = tokens::force_refresh(kind)?;
                    send(attach_oauth(base(), &refreshed))
                }
                other => other,
            }
        }
    }
}

fn looks_chat(kind: Kind, id: &str) -> bool {
    let l = id.to_ascii_lowercase();
    match kind {
        // Anthropic /v1/models only returns Claude models today, but guard
        // against a future endpoint that mixes in non-chat IDs.
        Kind::Anthropic => l.starts_with("claude"),
        Kind::OpenAi => {
            if !(l.starts_with("gpt-") || is_reasoning(&l)) {
                return false;
            }
            // OpenAI's /v1/models returns embeddings, audio, image, etc. mixed
            // in with chat. Filter the obviously-not-chat ones by substring.
            const BLOCK: &[&str] = &[
                "embed",
                "audio",
                "tts",
                "whisper",
                "dall-e",
                "image",
                "moderation",
                "realtime",
                "search",
                "transcribe",
                "instruct",
                "babbage",
                "davinci",
            ];
            !BLOCK.iter().any(|s| l.contains(s))
        }
        Kind::Codex => true,
    }
}

/// `o1`, `o3-mini`, `o4` … — the reasoning-model naming pattern. Bare
/// `octopus` shouldn't match, hence the digit-after-`o` requirement.
fn is_reasoning(l: &str) -> bool {
    let b = l.as_bytes();
    if b.first().copied() != Some(b'o') {
        return false;
    }
    let mut i = 1;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    i > 1 && (i == b.len() || b[i] == b'-')
}

fn tier(kind: Kind, id: &str) -> u8 {
    let l = id.to_ascii_lowercase();
    match kind {
        Kind::Anthropic => {
            if l.contains("haiku") {
                0
            } else if l.contains("sonnet") {
                1
            } else if l.contains("opus") {
                2
            } else {
                3
            }
        }
        Kind::OpenAi | Kind::Codex => {
            if l.contains("nano") {
                0
            } else if l.contains("mini") {
                1
            } else if l.starts_with("gpt-") {
                2
            } else {
                3
            }
        }
    }
}
