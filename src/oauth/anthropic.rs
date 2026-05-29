//! Anthropic OAuth flow (Claude Pro / Max accounts).
//!
//! These constants are taken from the official `claude` CLI — Anthropic
//! does NOT publicly document this client as a third-party-usable OAuth
//! application, so the values may change. If login starts returning
//! `invalid_client`, this is the first place to look.
//!
//! Flow: manual code-paste. The Claude Code OAuth client is registered with
//! exactly one redirect_uri (`console.anthropic.com/oauth/code/callback`) and
//! the `code=true` query param tells claude.ai to render the code on a copy-
//! paste page instead of redirecting. Loopback redirects are NOT accepted by
//! this client — they fail at the authorize step with "Invalid request format".

use std::time::Duration;

use serde_json::{Value, json};

use super::pkce::Pkce;
use super::server::{constant_time_eq, open_in_browser};
use super::tokens::{self, TokenSet, now_unix};
use crate::error::{Error, Result};
use crate::provider::Kind;
use crate::tui;

pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub const AUTHORIZE_URL: &str = "https://claude.ai/oauth/authorize";
// The same token endpoint is also reachable at console.anthropic.com, but
// that host sits behind a Cloudflare managed challenge that rejects every
// non-browser request with `Invalid request format`. api.anthropic.com hosts
// an identical endpoint without the challenge.
pub const TOKEN_URL: &str = "https://api.anthropic.com/v1/oauth/token";
pub const REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";
pub const SCOPE: &str = "org:create_api_key user:profile user:inference";
/// Set by Anthropic on every Messages API call made with an OAuth bearer
/// token instead of an API key.
pub const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

pub fn login() -> Result<()> {
    let pkce = Pkce::generate()?;
    // Anthropic's authorize endpoint requires `state` to be the PKCE verifier
    // itself. An independently-generated random state is rejected at the
    // authorize step with "Invalid request format".
    let state = pkce.verifier.clone();

    let authorize_url = format!(
        "{AUTHORIZE_URL}?code=true&response_type=code&client_id={cid}&redirect_uri={ru}&scope={sc}&state={state}&code_challenge={cc}&code_challenge_method=S256",
        cid = urlencoding::encode(CLIENT_ID),
        ru = urlencoding::encode(REDIRECT_URI),
        sc = urlencoding::encode(SCOPE),
        state = urlencoding::encode(&state),
        cc = urlencoding::encode(&pkce.challenge),
    );

    eprintln!("Opening browser to sign in to Anthropic…");
    eprintln!("After authorizing, the page will show a code — copy it and paste it here.");
    eprintln!("If the browser didn't open, visit this URL manually:");
    eprintln!("  {authorize_url}");
    open_in_browser(&authorize_url);

    let pasted = tui::prompt_oauth_code()?;
    let (code, returned_state) = split_code_state(&pasted)?;
    if !constant_time_eq(returned_state.as_bytes(), state.as_bytes()) {
        return Err(Error::OAuth(
            "state mismatch in pasted code — run `plz login anthropic` again".into(),
        ));
    }

    let tokens = exchange_code(&code, &pkce.verifier, REDIRECT_URI, &state)?;
    tokens::save(Kind::Anthropic, &tokens)?;

    eprintln!("Signed in to Anthropic. Tokens saved.");
    Ok(())
}

/// Anthropic's code-paste page presents the code as `<code>#<state>` — split
/// on the `#` so we can verify state and forward just the code to the token
/// endpoint.
fn split_code_state(pasted: &str) -> Result<(String, String)> {
    let pasted = pasted.trim();
    match pasted.split_once('#') {
        Some((code, state)) if !code.is_empty() && !state.is_empty() => {
            Ok((code.to_string(), state.to_string()))
        }
        _ => Err(Error::OAuth(
            "pasted code didn't match `<code>#<state>` format — copy the entire string shown in the browser".into(),
        )),
    }
}

fn exchange_code(
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
    state: &str,
) -> Result<TokenSet> {
    // Anthropic's token endpoint echoes the authorize-step `state` back into
    // the exchange body — omitting it returns "Invalid request format".
    let body = json!({
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": redirect_uri,
        "client_id": CLIENT_ID,
        "code_verifier": code_verifier,
        "state": state,
    });
    post_token(&body)
}

pub fn refresh(prev: &TokenSet) -> Result<TokenSet> {
    let body = json!({
        "grant_type": "refresh_token",
        "refresh_token": prev.refresh_token,
        "client_id": CLIENT_ID,
    });
    let mut new = post_token(&body)?;
    if new.refresh_token.is_empty() {
        new.refresh_token = prev.refresh_token.clone();
    }
    Ok(new)
}

fn post_token(body: &Value) -> Result<TokenSet> {
    let agent = ureq::AgentBuilder::new().timeout(HTTP_TIMEOUT).build();
    let resp = agent
        .post(TOKEN_URL)
        .set("content-type", "application/json")
        .set("accept", "application/json")
        .send_json(body.clone());

    let v: Value = match resp {
        Ok(r) => r
            .into_json()
            .map_err(|e| Error::OAuth(format!("token json: {e}")))?,
        Err(ureq::Error::Status(status, r)) => {
            let body = r.into_string().unwrap_or_default();
            let msg = format!("token endpoint HTTP {status}: {body}");
            // 4xx → refresh_token is dead; signal the caller to clear it.
            // 5xx / other → transient; leave stored tokens alone.
            return Err(if (400..500).contains(&status) {
                Error::OAuthInvalidGrant(msg)
            } else {
                Error::OAuth(msg)
            });
        }
        Err(e) => return Err(Error::OAuth(format!("token endpoint network: {e}"))),
    };

    let access_token = v
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::OAuth(format!("no access_token in: {v}")))?
        .to_string();
    let refresh_token = v
        .get("refresh_token")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let expires_in = v.get("expires_in").and_then(Value::as_i64).unwrap_or(3600);

    Ok(TokenSet {
        access_token,
        refresh_token,
        expires_at: now_unix() + expires_in,
        ..Default::default()
    })
}
