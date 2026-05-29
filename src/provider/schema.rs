use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

pub const TOOL_NAME: &str = "plz_response";

pub const TOOL_DESCRIPTION: &str = "Return either up to 3 candidate shell commands, or a single clarifying \
     question for the user. Set `kind` to \"commands\" or \"clarify\" \
     accordingly. Fill unused string fields with \"\" and unused array fields \
     with [] — array fields must always be JSON arrays, never strings.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub cmd: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Response {
    #[serde(rename = "commands")]
    Commands { commands: Vec<Command> },
    #[serde(rename = "clarify")]
    Clarify {
        question: String,
        choices: Vec<String>,
    },
}

#[derive(Debug, Deserialize)]
struct Raw {
    kind: String,
    #[serde(default, deserialize_with = "lenient_vec")]
    commands: Vec<Command>,
    #[serde(default, deserialize_with = "lenient_string")]
    question: String,
    #[serde(default, deserialize_with = "lenient_vec")]
    choices: Vec<String>,
}

/// Accept array / null / empty-string as a vec. The system prompt tells the
/// model to "fill unused fields with empty arrays / empty strings", which
/// invites it to send `""` instead of `[]` for the unused branch — Anthropic's
/// `input_schema` is a soft hint, so the wrong type isn't rejected upstream.
/// A non-empty string is still an error: that's a real schema violation.
fn lenient_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    use serde::de::Error;
    match Value::deserialize(deserializer)? {
        Value::Array(arr) => arr
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(D::Error::custom))
            .collect(),
        Value::Null => Ok(Vec::new()),
        Value::String(s) if s.is_empty() => Ok(Vec::new()),
        other => Err(D::Error::custom(format!(
            "expected array (or empty placeholder); got {other}"
        ))),
    }
}

/// Accept string / null as a string. Symmetric to `lenient_vec` — the model
/// sometimes returns `null` for the unused branch instead of `""`.
fn lenient_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    match Value::deserialize(deserializer)? {
        Value::String(s) => Ok(s),
        Value::Null => Ok(String::new()),
        other => Err(D::Error::custom(format!("expected string; got {other}"))),
    }
}

impl Response {
    /// When `debug` is true, emit a stderr warning when the model returned
    /// more commands / choices than we'll keep — silent truncation can hide
    /// schema drift or prompt regressions.
    pub fn from_json_with_debug(value: &Value, debug: bool) -> Result<Self, String> {
        let raw: Raw = serde_json::from_value(value.clone())
            .map_err(|e| format!("schema mismatch: {e}; raw: {value}"))?;
        match raw.kind.as_str() {
            "commands" => {
                if raw.commands.is_empty() {
                    return Err("kind=commands but `commands` was empty".into());
                }
                for (i, c) in raw.commands.iter().enumerate() {
                    if c.cmd.contains('\n') || c.cmd.contains('\r') {
                        return Err(format!(
                            "command #{i} contains a line break; refusing to print to stdout \
                             (would be silently truncated by $(…))"
                        ));
                    }
                    if c.cmd.trim().is_empty() {
                        return Err(format!("command #{i} is empty"));
                    }
                }
                let n_total = raw.commands.len();
                let commands: Vec<Command> = raw.commands.into_iter().take(3).collect();
                if debug && n_total > commands.len() {
                    eprintln!(
                        "[plz debug] model returned {n_total} commands; truncating to {}",
                        commands.len()
                    );
                }
                Ok(Response::Commands { commands })
            }
            "clarify" => {
                if raw.question.trim().is_empty() {
                    return Err("kind=clarify but `question` was empty".into());
                }
                let n_total = raw.choices.len();
                let choices: Vec<String> = raw.choices.into_iter().take(4).collect();
                if debug && n_total > choices.len() {
                    eprintln!(
                        "[plz debug] model returned {n_total} clarify choices; truncating to {}",
                        choices.len()
                    );
                }
                Ok(Response::Clarify {
                    question: raw.question,
                    choices,
                })
            }
            other => Err(format!("unknown kind {other:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> Result<Response, String> {
        Response::from_json_with_debug(&v, false)
    }

    #[test]
    fn commands_happy_path() {
        let r = parse(json!({
            "kind": "commands",
            "commands": [
                {"cmd": "ls", "explanation": "list"},
                {"cmd": "pwd", "explanation": "where"}
            ],
            "question": "",
            "choices": []
        }))
        .unwrap();
        match r {
            Response::Commands { commands } => assert_eq!(commands.len(), 2),
            _ => panic!("expected commands variant"),
        }
    }

    #[test]
    fn commands_rejects_empty_list() {
        let err = parse(json!({
            "kind": "commands",
            "commands": [],
            "question": "",
            "choices": []
        }))
        .unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn commands_rejects_newline_in_cmd() {
        let err = parse(json!({
            "kind": "commands",
            "commands": [{"cmd": "echo hi\nrm -rf .", "explanation": "x"}],
            "question": "",
            "choices": []
        }))
        .unwrap_err();
        assert!(err.contains("line break"), "got: {err}");
    }

    #[test]
    fn commands_rejects_carriage_return_in_cmd() {
        let err = parse(json!({
            "kind": "commands",
            "commands": [{"cmd": "echo hi\rrm -rf .", "explanation": "x"}],
            "question": "",
            "choices": []
        }))
        .unwrap_err();
        assert!(err.contains("line break"), "got: {err}");
    }

    #[test]
    fn commands_truncates_to_three() {
        let four = (0..4)
            .map(|i| json!({"cmd": format!("cmd{i}"), "explanation": "x"}))
            .collect::<Vec<_>>();
        let r = parse(json!({
            "kind": "commands",
            "commands": four,
            "question": "",
            "choices": []
        }))
        .unwrap();
        match r {
            Response::Commands { commands } => assert_eq!(commands.len(), 3),
            _ => panic!(),
        }
    }

    #[test]
    fn clarify_happy_path() {
        let r = parse(json!({
            "kind": "clarify",
            "commands": [],
            "question": "which one?",
            "choices": ["a", "b"]
        }))
        .unwrap();
        match r {
            Response::Clarify { question, choices } => {
                assert_eq!(question, "which one?");
                assert_eq!(choices, vec!["a".to_string(), "b".to_string()]);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn clarify_rejects_empty_question() {
        let err = parse(json!({
            "kind": "clarify",
            "commands": [],
            "question": "   ",
            "choices": []
        }))
        .unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn clarify_truncates_choices_to_four() {
        let five: Vec<_> = (0..5).map(|i| json!(format!("c{i}"))).collect();
        let r = parse(json!({
            "kind": "clarify",
            "commands": [],
            "question": "?",
            "choices": five
        }))
        .unwrap();
        match r {
            Response::Clarify { choices, .. } => assert_eq!(choices.len(), 4),
            _ => panic!(),
        }
    }

    #[test]
    fn commands_accepts_empty_string_for_choices() {
        // Regression: Anthropic occasionally returns `"choices": ""` instead of
        // `[]` for the unused branch. The prompt invites this by saying
        // "fill unused fields with empty arrays / empty strings".
        let r = parse(json!({
            "kind": "commands",
            "commands": [{"cmd": "ls", "explanation": "list"}],
            "question": "",
            "choices": ""
        }))
        .unwrap();
        assert!(matches!(r, Response::Commands { .. }));
    }

    #[test]
    fn clarify_accepts_empty_string_for_commands() {
        let r = parse(json!({
            "kind": "clarify",
            "commands": "",
            "question": "which?",
            "choices": ["a"]
        }))
        .unwrap();
        assert!(matches!(r, Response::Clarify { .. }));
    }

    #[test]
    fn accepts_null_for_unused_branch() {
        let r = parse(json!({
            "kind": "commands",
            "commands": [{"cmd": "ls", "explanation": "list"}],
            "question": null,
            "choices": null
        }))
        .unwrap();
        assert!(matches!(r, Response::Commands { .. }));
    }

    #[test]
    fn nonempty_string_for_array_still_rejected() {
        let err = parse(json!({
            "kind": "commands",
            "commands": [{"cmd": "ls", "explanation": "list"}],
            "question": "",
            "choices": "a, b"
        }))
        .unwrap_err();
        assert!(err.contains("expected array"), "got: {err}");
    }

    #[test]
    fn unknown_kind_rejected() {
        let err = parse(json!({
            "kind": "shrug",
            "commands": [],
            "question": "",
            "choices": []
        }))
        .unwrap_err();
        assert!(err.contains("unknown kind"));
    }
}

/// JSON schema describing the structured tool output. The same schema is fed
/// to both providers — Anthropic accepts it as `input_schema`, OpenAI accepts
/// it as the `json_schema` body under strict mode.
///
/// All keys are `required` and `additionalProperties` is false because OpenAI
/// strict mode demands both. The unused branch is filled with empty
/// arrays / empty strings rather than via `oneOf`, which strict mode
/// restricts.
pub fn json_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "commands", "question", "choices"],
        "properties": {
            "kind": {
                "type": "string",
                "enum": ["commands", "clarify"],
                "description": "Discriminator. \"commands\" returns shell commands; \"clarify\" asks the user a question."
            },
            "commands": {
                "type": "array",
                "description": "When kind=\"commands\", 1-3 candidate shell commands. When kind=\"clarify\", an empty array.",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["cmd", "explanation"],
                    "properties": {
                        "cmd": { "type": "string", "description": "The shell command, single line." },
                        "explanation": { "type": "string", "description": "Very short label (~5–12 words): gist if alone, or key difference vs. other candidates — not a restatement of the command." }
                    }
                }
            },
            "question": {
                "type": "string",
                "description": "When kind=\"clarify\", the question to ask the user. When kind=\"commands\", an empty string."
            },
            "choices": {
                "type": "array",
                "description": "When kind=\"clarify\", up to 4 short multiple-choice answers. When kind=\"commands\", an empty array.",
                "items": { "type": "string" }
            }
        }
    })
}
