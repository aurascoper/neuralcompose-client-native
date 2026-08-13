//! The `llama-server` chat protocol — request shape and response parsing.
//!
//! **Pure on purpose.** This is the half of `shell/generate.rs` that can be
//! wrong in ways nothing would notice: a dropped field, a wrong path, a
//! response shape read from the wrong key. Only the actual POST lives in the
//! binary. Without this split the entire generate path would be effects, `cargo
//! test` would stay green whatever it did, and there would be nothing for a
//! mutation run to bite on.
//!
//! Derived from `tools/spoken-loop/turn.sh`, which is the working reference on
//! this machine — the same endpoint, the same body fields, the same response
//! key. `tests/turn_sh_parity.rs` pins the agreement.

use crate::seams::{GenerationParams, SeamError, SeamResult};
use serde::Deserialize;

/// `POST` target for a chat completion.
pub fn chat_url(server: &str) -> String {
    format!("{}/v1/chat/completions", server.trim_end_matches('/'))
}

/// Readiness probe. `turn.sh` checks this before doing anything else, and so
/// does the binary — a loop that discovers the server is down on its first
/// generate has already opened the microphone.
pub fn health_url(server: &str) -> String {
    format!("{}/health", server.trim_end_matches('/'))
}

/// The request body, matching `turn.sh`'s `jq -n` payload field for field.
///
/// `enable_thinking: false` is not decoration: without it this model family
/// emits `<think>` blocks that then have to be stripped back out, and a
/// stripped-away reply is an empty utterance. Belt and braces — the stripping
/// stays, because the flag is advisory and the model does not always honour it.
pub fn chat_request_body(system: &str, prompt: &str, params: GenerationParams) -> String {
    let body = serde_json::json!({
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": prompt},
        ],
        "chat_template_kwargs": {"enable_thinking": false},
        "max_tokens": params.max_tokens,
        "temperature": params.temperature,
        "stream": false,
    });
    serde_json::to_string(&body)
        .expect("a JSON object of strings and numbers is always serializable")
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

/// Pulls `.choices[0].message.content` out of a chat response, the same key
/// `turn.sh` reads with `jq`.
///
/// Every failure is distinguishable: malformed JSON, a well-formed response
/// with no choices, and a choice whose content is empty are three different
/// situations, and an empty string is a legitimate value for the third — so
/// none of them collapses into `Ok("")`.
pub fn parse_chat_content(raw: &str) -> SeamResult<String> {
    let parsed: ChatResponse = serde_json::from_str(raw).map_err(|e| {
        // Servers report their own errors as JSON too; quoting a prefix of the
        // body makes a 400 legible instead of "invalid type".
        let head: String = raw.chars().take(200).collect();
        SeamError::Failed(format!(
            "chat response is not the expected shape: {e} — body began: {head}"
        ))
    })?;
    let choice = parsed
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| SeamError::Failed("chat response contained no choices".to_string()))?;
    Ok(choice.message.content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> GenerationParams {
        GenerationParams {
            temperature: 0.45,
            max_tokens: 60,
        }
    }

    #[test]
    fn urls_are_built_from_the_server_base_and_tolerate_a_trailing_slash() {
        assert_eq!(
            chat_url("http://127.0.0.1:8080"),
            "http://127.0.0.1:8080/v1/chat/completions"
        );
        assert_eq!(
            chat_url("http://127.0.0.1:8080/"),
            "http://127.0.0.1:8080/v1/chat/completions"
        );
        assert_eq!(
            health_url("http://127.0.0.1:8080/"),
            "http://127.0.0.1:8080/health"
        );
    }

    /// The body must carry every field `turn.sh` sends. A missing
    /// `enable_thinking` is the one most likely to go unnoticed: the loop still
    /// works, it just starts emitting reasoning that gets stripped to nothing.
    #[test]
    fn the_request_body_matches_turn_sh_field_for_field() {
        let body = chat_request_body("SYS", "HEARD", params());
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["messages"][0]["role"], "system");
        assert_eq!(v["messages"][0]["content"], "SYS");
        assert_eq!(v["messages"][1]["role"], "user");
        assert_eq!(v["messages"][1]["content"], "HEARD");
        assert_eq!(v["chat_template_kwargs"]["enable_thinking"], false);
        assert_eq!(v["max_tokens"], 60);
        assert_eq!(v["stream"], false);
        // turn.sh has no temperature; the dialectic needs one PER ROLE, since a
        // 0.45 coherence pole and a 1.0 displacement pole are the whole point.
        assert_eq!(v["temperature"], 0.45);
    }

    #[test]
    fn per_role_temperature_reaches_the_body() {
        for t in [0.45, 1.0] {
            let body = chat_request_body(
                "s",
                "p",
                GenerationParams {
                    temperature: t,
                    max_tokens: 10,
                },
            );
            let v: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(v["temperature"].as_f64().unwrap(), t);
        }
    }

    #[test]
    fn quotes_and_newlines_in_the_prompt_survive_encoding() {
        let awkward = "she said \"no\"\nand left\\";
        let body = chat_request_body("s", awkward, params());
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["messages"][1]["content"], awkward);
    }

    #[test]
    fn the_content_is_read_from_the_same_key_jq_reads() {
        let raw = r#"{"choices":[{"message":{"role":"assistant","content":"Rest now."}}]}"#;
        assert_eq!(parse_chat_content(raw).unwrap(), "Rest now.");
    }

    /// Three distinct failures, three distinct reports — and an empty reply is
    /// a legitimate value, not a failure, so it must not be reported as one.
    #[test]
    fn the_failure_modes_stay_distinguishable() {
        let malformed = parse_chat_content("not json at all").unwrap_err();
        assert!(format!("{malformed}").contains("not the expected shape"));

        let no_choices = parse_chat_content(r#"{"choices":[]}"#).unwrap_err();
        assert!(format!("{no_choices}").contains("no choices"));

        // An empty content field is a real (if useless) reply. The loop decides
        // what to do with it; parsing must not call it an error.
        let empty = parse_chat_content(r#"{"choices":[{"message":{"content":""}}]}"#);
        assert_eq!(empty.unwrap(), "");
    }

    /// A server error arrives as JSON too. The message must quote enough of it
    /// to be actionable rather than saying only "invalid type".
    #[test]
    fn a_server_error_body_is_quoted_in_the_failure() {
        let err =
            parse_chat_content(r#"{"error":{"message":"context length exceeded"}}"#).unwrap_err();
        assert!(
            format!("{err}").contains("context length exceeded"),
            "the server's own error text was swallowed: {err}"
        );
    }
}
