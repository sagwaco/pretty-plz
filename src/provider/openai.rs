//! OpenAI public Responses API.
//!
//! This provider is intentionally API-key-only. ChatGPT subscription access is
//! modeled as a separate provider that uses plz-managed OAuth tokens.
//!
//! Multi-turn clarify shape:
//!   user        → query + context
//!   assistant   → JSON text of previous Response
//!   user        → clarify answer
//!
use std::time::Duration;

use serde_json::{Value, json};

use super::auth::{self, Credential};
use super::schema::{self, Response};
use super::{Provider, Turn};
use crate::error::{Error, Result};
use crate::prompt;
use crate::provider::Kind;

const API_ENDPOINT: &str = "https://api.openai.com/v1/responses";
const TIMEOUT: Duration = Duration::from_secs(30);

pub struct OpenAi {
    cred: Credential,
    model: String,
    debug: bool,
}

impl OpenAi {
    pub fn new(cred: Credential, model: String, debug: bool) -> Self {
        Self { cred, model, debug }
    }

    pub(crate) fn build_input(turns: &[Turn]) -> Vec<Value> {
        let mut out = Vec::with_capacity(turns.len());
        for turn in turns {
            match turn {
                Turn::User(text) => out.push(json!({
                    "role": "user",
                    "content": [{ "type": "input_text", "text": text }]
                })),
                Turn::Assistant(resp) => {
                    let json_text = serde_json::to_string(resp).unwrap_or_default();
                    out.push(json!({
                        "role": "assistant",
                        "content": [{ "type": "output_text", "text": json_text }]
                    }));
                }
                Turn::ClarifyAnswer(answer) => out.push(json!({
                    "role": "user",
                    "content": [{ "type": "input_text", "text": answer }]
                })),
            }
        }
        out
    }

    fn call(&self, body: &Value) -> Result<Value> {
        let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();
        let request_factory = || {
            agent
                .post(API_ENDPOINT)
                .set("content-type", "application/json")
        };
        let bearer_setter =
            |req: ureq::Request, t: &str| req.set("Authorization", &format!("Bearer {t}"));
        auth::call_with_auth(
            &self.cred,
            Kind::OpenAi,
            body,
            request_factory,
            bearer_setter,
            bearer_setter,
        )
    }
}

impl Provider for OpenAi {
    fn name(&self) -> &'static str {
        "openai"
    }

    fn complete(&self, turns: &[Turn]) -> Result<Response> {
        let body = json!({
            "model": self.model,
            "instructions": prompt::system_prompt(),
            "input": Self::build_input(turns),
            "text": {
                "format": {
                    "type": "json_schema",
                    "name": "plz_response",
                    "strict": true,
                    "schema": schema::json_schema(),
                }
            }
        });
        let v = self.call(&body)?;

        if self.debug {
            eprintln!("[plz debug] openai response: {v}");
        }

        let text = extract_output_text(&v)
            .ok_or_else(|| Error::BadResponse(format!("no output_text in response: {v}")))?;

        let parsed: Value = serde_json::from_str(&text)
            .map_err(|e| Error::BadResponse(format!("response not JSON: {e}; raw: {text}")))?;

        Response::from_json_with_debug(&parsed, self.debug).map_err(Error::BadResponse)
    }
}

/// Pull the first `output_text` from a Responses API result. The result has
/// an `output` array of items; the message item has `content` blocks; we want
/// the first one whose `type` is `output_text`.
pub(crate) fn extract_output_text(v: &Value) -> Option<String> {
    let output = v.get("output")?.as_array()?;
    for item in output {
        let content = match item.get("content").and_then(Value::as_array) {
            Some(c) => c,
            None => continue,
        };
        for block in content {
            if block.get("type").and_then(Value::as_str) == Some("output_text") {
                if let Some(t) = block.get("text").and_then(Value::as_str) {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}
