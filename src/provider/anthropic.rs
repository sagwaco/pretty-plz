//! Anthropic Messages API with a single forced `tool_use` for structured
//! output.
//!
//! Multi-turn clarify shape:
//!   user        → query + context
//!   assistant   → tool_use(plz_response, input = previous Response JSON)
//!   user        → tool_result(tool_use_id, content = clarify answer)
//! Anthropic requires a `tool_result` to follow a `tool_use`, so this is the
//! only legal shape.

use std::time::Duration;

use serde_json::{Value, json};

use super::auth::{self, Credential};
use super::schema::{self, Response, TOOL_DESCRIPTION, TOOL_NAME};
use super::{Provider, Turn};
use crate::error::{Error, Result};
use crate::oauth::anthropic::OAUTH_BETA_HEADER;
use crate::prompt;
use crate::provider::Kind;

const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const TIMEOUT: Duration = Duration::from_secs(30);
const TOOL_USE_ID: &str = "plz_tool_use_1";

pub struct Anthropic {
    cred: Credential,
    model: String,
    debug: bool,
}

impl Anthropic {
    pub fn new(cred: Credential, model: String, debug: bool) -> Self {
        Self { cred, model, debug }
    }

    fn build_messages(&self, turns: &[Turn]) -> Vec<Value> {
        let mut out = Vec::with_capacity(turns.len());
        for turn in turns {
            match turn {
                Turn::User(text) => out.push(json!({
                    "role": "user",
                    "content": [{ "type": "text", "text": text }]
                })),
                Turn::Assistant(resp) => {
                    let input = serde_json::to_value(resp).unwrap_or(Value::Null);
                    out.push(json!({
                        "role": "assistant",
                        "content": [{
                            "type": "tool_use",
                            "id": TOOL_USE_ID,
                            "name": TOOL_NAME,
                            "input": input,
                        }]
                    }));
                }
                Turn::ClarifyAnswer(answer) => out.push(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": TOOL_USE_ID,
                        "content": answer,
                    }]
                })),
            }
        }
        out
    }

    fn call(&self, body: &Value) -> Result<Value> {
        let agent = ureq::AgentBuilder::new().timeout(TIMEOUT).build();
        let request_factory = || {
            agent
                .post(ENDPOINT)
                .set("anthropic-version", API_VERSION)
                .set("content-type", "application/json")
        };
        let result = auth::call_with_auth(
            &self.cred,
            Kind::Anthropic,
            body,
            request_factory,
            |req, k| req.set("x-api-key", k),
            |req, t| {
                req.set("Authorization", &format!("Bearer {t}"))
                    .set("anthropic-beta", OAUTH_BETA_HEADER)
            },
        );

        // If a 401 persists past force-refresh on OAuth, the beta header may
        // have rotated. Tell the user where to look so they don't have to
        // bisect the entire OAuth stack.
        if matches!(&self.cred, Credential::OAuth)
            && let Err(Error::HttpStatus { status: 401, body }) = &result
        {
            eprintln!(
                "plz: anthropic returned 401 after refresh. If `plz status` shows valid \
                 tokens, the `anthropic-beta: {OAUTH_BETA_HEADER}` header may have rotated \
                 — check Anthropic's release notes and update src/oauth/anthropic.rs.\n\
                 server body: {body}"
            );
        }

        result
    }
}

impl Provider for Anthropic {
    fn model(&self) -> &str {
        &self.model
    }

    fn complete(&self, turns: &[Turn]) -> Result<Response> {
        let body = json!({
            "model": self.model,
            "max_tokens": 1024,
            "system": prompt::system_prompt(),
            "tools": [{
                "name": TOOL_NAME,
                "description": TOOL_DESCRIPTION,
                "input_schema": schema::json_schema(),
            }],
            "tool_choice": { "type": "tool", "name": TOOL_NAME },
            "messages": self.build_messages(turns),
        });

        let v = self.call(&body)?;

        if self.debug {
            eprintln!("[plz debug] anthropic response: {v}");
        }

        let content = v
            .get("content")
            .and_then(|c| c.as_array())
            .ok_or_else(|| Error::BadResponse(format!("missing content array: {v}")))?;

        let tool_input = content
            .iter()
            .find(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
            .and_then(|b| b.get("input"))
            .ok_or_else(|| Error::BadResponse(format!("no tool_use block: {v}")))?;

        Response::from_json_with_debug(tool_input, self.debug).map_err(Error::BadResponse)
    }
}
