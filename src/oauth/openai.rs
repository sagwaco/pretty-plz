//! ChatGPT OAuth flow.
//!
//! Uses the same Auth0 PKCE client that the official Codex CLI uses, but keeps
//! the token lifecycle in `plz` so `plz login chatgpt` behaves like
//! `plz login anthropic`: browser sign-in, local token storage, refresh on use.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};

use super::pkce::{Pkce, state_token};
use super::server::{CallbackListener, open_in_browser};
use super::tokens::{self, TokenSet, now_unix};
use crate::error::{Error, Result};
use crate::provider::Kind;

pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
pub const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
pub const AUDIENCE: &str = "https://api.openai.com/v1";
pub const SCOPE: &str = "openid profile email offline_access";
/// The Codex CLI OAuth client is registered with this exact redirect — both
/// the host literal (`localhost`, not `127.0.0.1`), port (1455), and path
/// (`/auth/callback`) are strictly validated. Any deviation makes
/// auth.openai.com fail at the authorize step with `unknown_error`.
pub const CALLBACK_PORT: u16 = 1455;
pub const CALLBACK_PATH: &str = "/auth/callback";

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

pub fn login() -> Result<()> {
    let pkce = Pkce::generate()?;
    let state = state_token()?;
    let listener = CallbackListener::bind(CALLBACK_PORT, CALLBACK_PATH).map_err(|e| match e {
        Error::OAuth(msg) => Error::OAuth(format!(
            "{msg} — is `codex login` or another plz login already running on port {CALLBACK_PORT}?"
        )),
        other => other,
    })?;
    let redirect_uri = listener.redirect_uri.clone();

    // `id_token_add_organizations` + `codex_cli_simplified_flow` + `originator`
    // are required by auth.openai.com for this client — omitting any of them
    // returns a generic `unknown_error` page.
    let authorize_url = format!(
        "{AUTHORIZE_URL}?response_type=code\
         &client_id={cid}\
         &redirect_uri={ru}\
         &scope={sc}\
         &audience={aud}\
         &state={state}\
         &code_challenge={cc}\
         &code_challenge_method=S256\
         &id_token_add_organizations=true\
         &codex_cli_simplified_flow=true\
         &originator=codex_cli",
        cid = urlencoding::encode(CLIENT_ID),
        ru = urlencoding::encode(&redirect_uri),
        sc = urlencoding::encode(SCOPE),
        aud = urlencoding::encode(AUDIENCE),
        state = urlencoding::encode(&state),
        cc = urlencoding::encode(&pkce.challenge),
    );

    eprintln!("Opening browser to sign in with ChatGPT…");
    eprintln!("If it doesn't open, visit this URL manually:");
    eprintln!("  {authorize_url}");
    open_in_browser(&authorize_url);

    let callback = listener.accept(&state)?;
    let mut tokens = exchange_code(&callback.code, &pkce.verifier, &redirect_uri)?;
    populate_chatgpt_claims(&mut tokens);
    tokens::save(Kind::Codex, &tokens)?;

    eprintln!("Signed in with ChatGPT. Tokens saved.");
    Ok(())
}

fn exchange_code(code: &str, code_verifier: &str, redirect_uri: &str) -> Result<TokenSet> {
    let body = json!({
        "grant_type": "authorization_code",
        "code": code,
        "redirect_uri": redirect_uri,
        "client_id": CLIENT_ID,
        "code_verifier": code_verifier,
    });
    post_token(&body)
}

pub fn refresh(prev: &TokenSet) -> Result<TokenSet> {
    let body = json!({
        "grant_type": "refresh_token",
        "refresh_token": prev.refresh_token,
        "client_id": CLIENT_ID,
        "scope": SCOPE,
    });
    let mut new = post_token(&body)?;
    if new.refresh_token.is_empty() {
        new.refresh_token = prev.refresh_token.clone();
    }

    if new.id_token.is_empty() {
        new.id_token = prev.id_token.clone();
        new.chatgpt_account_id = prev.chatgpt_account_id.clone();
        new.chatgpt_plan_type = prev.chatgpt_plan_type.clone();
    } else {
        populate_chatgpt_claims(&mut new);
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
    let id_token = v
        .get("id_token")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let expires_in = v.get("expires_in").and_then(Value::as_i64).unwrap_or(3600);

    Ok(TokenSet {
        access_token,
        refresh_token,
        expires_at: now_unix() + expires_in,
        id_token,
        ..Default::default()
    })
}

fn populate_chatgpt_claims(t: &mut TokenSet) {
    if t.id_token.is_empty() {
        return;
    }
    let Some(claims) = parse_jwt_claims(&t.id_token) else {
        return;
    };
    let (account_id, plan_type) = extract_chatgpt_claims(&claims);
    t.chatgpt_account_id = account_id;
    t.chatgpt_plan_type = plan_type;
}

fn parse_jwt_claims(jwt: &str) -> Option<Value> {
    let mut parts = jwt.split('.');
    let _header = parts.next()?;
    let payload_b64 = parts.next()?;
    parts.next()?;
    let trimmed = payload_b64.trim_end_matches('=');
    let bytes = URL_SAFE_NO_PAD.decode(trimmed.as_bytes()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn extract_chatgpt_claims(claims: &Value) -> (String, String) {
    const NS: &str = "https://api.openai.com/auth";

    if let Some(obj) = claims.get(NS).and_then(Value::as_object) {
        let acct = obj
            .get("chatgpt_account_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let plan = obj
            .get("chatgpt_plan_type")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if !acct.is_empty() || !plan.is_empty() {
            return (acct, plan);
        }
    }

    let acct = claims
        .get("chatgpt_account_id")
        .or_else(|| claims.get(format!("{NS}.chatgpt_account_id")))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let plan = claims
        .get("chatgpt_plan_type")
        .or_else(|| claims.get(format!("{NS}.chatgpt_plan_type")))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    (acct, plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_jwt(payload: &Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#);
        let body = URL_SAFE_NO_PAD.encode(serde_json::to_vec(payload).unwrap());
        let sig = URL_SAFE_NO_PAD.encode(b"sig");
        format!("{header}.{body}.{sig}")
    }

    #[test]
    fn extracts_chatgpt_claims_from_namespaced_object() {
        let claims = json!({
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acct-1",
                "chatgpt_plan_type": "plus"
            }
        });
        let (acct, plan) = extract_chatgpt_claims(&claims);
        assert_eq!(acct, "acct-1");
        assert_eq!(plan, "plus");
    }

    #[test]
    fn extracts_chatgpt_claims_from_raw_jwt() {
        let jwt = make_jwt(&json!({
            "chatgpt_account_id": "acct-2",
            "chatgpt_plan_type": "pro"
        }));
        let claims = parse_jwt_claims(&jwt).unwrap();
        let (acct, plan) = extract_chatgpt_claims(&claims);
        assert_eq!(acct, "acct-2");
        assert_eq!(plan, "pro");
    }
}
