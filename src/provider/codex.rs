//! ChatGPT subscription provider backed by plz-managed OAuth tokens.

use std::time::Duration;

use serde_json::{Value, json};

use super::auth::Credential;
use super::openai::{OpenAi, extract_output_text};
use super::schema::{self, Response};
use super::{Provider, Turn};
use crate::error::{Error, Result};
use crate::oauth::tokens::{self, TokenSet};
use crate::prompt;
use crate::provider::Kind;

const CODEX_ENDPOINT: &str = "https://chatgpt.com/backend-api/codex/responses";
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct Codex {
    cred: Credential,
    model: String,
    debug: bool,
}

impl Codex {
    pub fn new(cred: Credential, model: String, debug: bool) -> Self {
        Self { cred, model, debug }
    }

    fn call(&self, body: &Value) -> Result<Value> {
        match &self.cred {
            Credential::OAuth => {
                let first = valid_context()?;
                match send_once(&first, body) {
                    Err(Error::HttpStatus { status: 401, .. }) => {
                        let refreshed = tokens::force_refresh(Kind::Codex)?;
                        let mut next = tokens::load(Kind::Codex)?
                            .ok_or(Error::NotSignedIn(Kind::Codex.as_str()))?;
                        next.access_token = refreshed;
                        send_once(&next, body)
                    }
                    other => other,
                }
            }
            Credential::ApiKey(_) => Err(Error::Config(
                "ChatGPT provider uses OAuth; use `--provider openai` for API keys".into(),
            )),
        }
    }
}

impl Provider for Codex {
    fn model(&self) -> &str {
        &self.model
    }

    fn complete(&self, turns: &[Turn]) -> Result<Response> {
        let body = json!({
            "model": self.model,
            "instructions": prompt::system_prompt(),
            "input": OpenAi::build_input(turns),
            "text": {
                "format": {
                    "type": "json_schema",
                    "name": "plz_response",
                    "strict": true,
                    "schema": schema::json_schema(),
                }
            },
            // The ChatGPT Codex backend persists conversations by default; `plz`
            // sends complete turns each time and does not reuse server state.
            "store": false,
            "stream": false,
        });

        let v = self.call(&body)?;

        if self.debug {
            eprintln!("[plz debug] chatgpt response: {v}");
        }

        let text = extract_output_text(&v)
            .ok_or_else(|| Error::BadResponse(format!("no output_text in response: {v}")))?;

        let parsed: Value = serde_json::from_str(&text)
            .map_err(|e| Error::BadResponse(format!("response not JSON: {e}; raw: {text}")))?;

        Response::from_json_with_debug(&parsed, self.debug).map_err(Error::BadResponse)
    }
}

pub fn has_auth() -> bool {
    matches!(tokens::load(Kind::Codex), Ok(Some(_)))
}

fn valid_context() -> Result<TokenSet> {
    let access_token = tokens::valid_access_token(Kind::Codex)?;
    let mut token_set =
        tokens::load(Kind::Codex)?.ok_or(Error::NotSignedIn(Kind::Codex.as_str()))?;
    token_set.access_token = access_token;
    if token_set.chatgpt_account_id.is_empty() {
        return Err(Error::Config(
            "ChatGPT OAuth token is missing an account id; run `plz login chatgpt` again".into(),
        ));
    }
    Ok(token_set)
}

fn send_once(tokens: &TokenSet, body: &Value) -> Result<Value> {
    let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();
    let req = agent
        .post(CODEX_ENDPOINT)
        .set("content-type", "application/json")
        .set("Authorization", &format!("Bearer {}", tokens.access_token))
        .set("chatgpt-account-id", &tokens.chatgpt_account_id)
        .set("OpenAI-Beta", "responses=experimental")
        .set("originator", env!("CARGO_PKG_NAME"))
        .set("version", env!("CARGO_PKG_VERSION"));

    match req.send_json(body.clone()) {
        Ok(resp) => resp.into_json().map_err(|e| Error::Network(e.to_string())),
        Err(ureq::Error::Status(status, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            Err(Error::HttpStatus { status, body })
        }
        Err(e) => Err(Error::Network(e.to_string())),
    }
}
