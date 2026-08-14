//! The `claude -p` protocol — argv shape and JSON envelope parsing.
//!
//! Port of `Sources/BCICloudBridge/ClaudeCLIGenerator.swift`. Same shape: drive
//! a Claude model through the local `claude` CLI in headless mode, **no API key
//! and no HTTP client in this process** — the CLI carries the user's own
//! subscription auth.
//!
//! # ⚠️ This is network egress, and it is the only thing here that is
//!
//! Every other seam in this crate's shell talks to `127.0.0.1` or to a
//! subprocess on this machine. This one hands text to a program that sends it
//! to Anthropic. What leaves: the role's system prompt and the transcript the
//! loop composed from what you said. What does not: audio (whisper runs
//! on-device), EEG (never reaches a prompt at all — see ADR-005), and nothing
//! is persisted off-device by this process.
//!
//! The Swift quarantined this in a separate module, `BCICloudBridge`, so the
//! on-device boundary contract could name what it excluded. The equivalent here
//! is that this module is the only one in the crate whose doc comment says
//! "network", and the binary refuses to construct it without an explicit flag.
//!
//! **Pure on purpose**, the same reason as [`crate::http`] and
//! [`crate::command`]: the argv is the egress boundary, and a boundary that is
//! only exercised by running it is a boundary nothing can test. Only
//! `Command::spawn` lives in the binary.

use crate::seams::{SeamError, SeamResult};
use serde::Deserialize;

/// Recorded in the turn log's method identity, so a session's provenance says
/// which sampler wrote its candidates. See [`crate::turn_log`].
pub const GENERATOR_KIND: &str = "claude-cli";

/// Default model. The Swift's default too — this is the port, not a new choice.
pub const DEFAULT_MODEL: &str = "claude-sonnet-5";

/// What goes in the method identity: the kind and the model together, because
/// "a cloud model wrote this" and "*which* cloud model" are different facts and
/// the second one changes between sessions.
pub fn generator_id(model: &str) -> String {
    format!("{GENERATOR_KIND}:{model}")
}

/// The exact argument vector. Everything after the flags is the transcript.
///
/// Three of these flags are load-bearing:
///
/// - `--system-prompt` **replaces** the CLI's own system prompt rather than
///   appending to it (`--append-system-prompt` is the other one). The role's
///   prompt has to be the whole instruction, or a hypnagogic mirror inherits a
///   coding agent's persona.
/// - `--tools ""` disables every built-in tool. Without it `claude -p` is
///   Claude Code: it can read and write files and run shell commands in
///   whatever directory it was spawned in. A text-generation seam has no
///   business holding a shell, and the Swift port predates the flag existing.
/// - `--output-format json` gets the envelope [`parse_result`] reads. The
///   default `text` format would work until the day the CLI prefixes a warning.
///
/// `GenerationParams` is deliberately absent: `claude -p` exposes neither
/// `temperature` nor `max_tokens`. **That is a real loss and the caller is told
/// so**, because per-role temperature is the mechanism that makes the
/// coherence pole faithful and the displacement pole divergent. Through this
/// generator the two poles differ only by their system prompts.
pub fn argv(model: &str, system: &str, prompt: &str) -> Vec<String> {
    vec![
        "-p".into(),
        "--model".into(),
        model.into(),
        "--system-prompt".into(),
        system.into(),
        "--tools".into(),
        String::new(),
        "--output-format".into(),
        "json".into(),
        prompt.into(),
    ]
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    is_error: bool,
    result: Option<String>,
}

/// Pulls `.result` out of the `--output-format json` envelope.
///
/// Four failures, four distinct reports, and — as in [`crate::http`] — an empty
/// reply is a legitimate value rather than a failure, so it does not collapse
/// into an error. `is_error` is checked **before** `result`, because the CLI
/// puts its own error text in `result` and reporting that as a reply would put
/// an error message into the user's ear in a synthesized voice.
pub fn parse_result(raw: &str) -> SeamResult<String> {
    let env: Envelope = serde_json::from_str(raw).map_err(|e| {
        let head: String = raw.chars().take(200).collect();
        SeamError::Failed(format!(
            "the claude CLI did not return the expected JSON envelope: {e} — output began: {head}"
        ))
    })?;
    if env.is_error {
        let detail = env.result.unwrap_or_else(|| "no detail given".to_string());
        return Err(SeamError::Failed(format!(
            "the claude CLI reported an error: {detail}"
        )));
    }
    env.result
        .map(|r| r.trim().to_string())
        .ok_or_else(|| SeamError::Failed("claude CLI JSON had no 'result' field".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `ClaudeCLIGeneratorEgressTests.swift`. The argv **is** the
    /// egress: asserting it in full is the only way to state what leaves the
    /// machine, and a test that only checked the flags would not notice a
    /// second copy of the transcript or a stray path being appended.
    #[test]
    fn the_argv_carries_exactly_the_system_prompt_and_the_transcript() {
        let args = argv("claude-sonnet-5", "TEST-SYS-PROMPT", "user transcript text");
        assert_eq!(
            args,
            vec![
                "-p",
                "--model",
                "claude-sonnet-5",
                "--system-prompt",
                "TEST-SYS-PROMPT",
                "--tools",
                "",
                "--output-format",
                "json",
                "user transcript text",
            ]
        );
        assert_eq!(
            args.iter().filter(|a| *a == "user transcript text").count(),
            1,
            "the transcript must leave exactly once"
        );
    }

    /// The negative half of the same boundary. Audio and EEG are the two things
    /// this project promises never leave, so their shapes are named here rather
    /// than left to a reader's confidence in the line above.
    #[test]
    fn nothing_audio_or_eeg_shaped_reaches_the_argv() {
        let args = argv("claude-sonnet-5", "sys", "I am drifting");
        for a in &args {
            let low = a.to_lowercase();
            assert!(
                !low.ends_with(".wav")
                    && !low.contains("audio")
                    && !low.contains("eeg")
                    && !low.contains(".jsonl"),
                "only transcript text leaves the device, never {a:?}"
            );
        }
    }

    /// `--tools ""` is the difference between a text generator and a shell
    /// agent pointed at the user's home directory. It is asserted on its own
    /// because a careless argv edit could drop it and every other test here
    /// would still pass.
    #[test]
    fn every_built_in_tool_is_disabled() {
        let args = argv("m", "s", "p");
        let i = args
            .iter()
            .position(|a| a == "--tools")
            .expect("--tools must be passed");
        assert_eq!(
            args[i + 1],
            "",
            "an empty --tools value is what disables the built-in set"
        );
    }

    #[test]
    fn a_multiline_system_prompt_stays_one_argument() {
        let sys = "line one\nline two\n\nCONSTRAINTS:\n1. never ask questions";
        let args = argv("m", sys, "p");
        assert!(args.contains(&sys.to_string()));
        assert_eq!(args.len(), 10, "no argument was split on its own newlines");
    }

    #[test]
    fn the_result_is_read_from_the_envelope_and_trimmed() {
        let raw = r#"{"result":"  Rest now.\n","is_error":false,"total_cost_usd":0.01}"#;
        assert_eq!(parse_result(raw).unwrap(), "Rest now.");
    }

    /// An error envelope also carries `result` — holding the error text. Read in
    /// the wrong order that becomes a reply, and with `--speak` the user hears
    /// their error message read aloud in a hypnagogic voice.
    #[test]
    fn an_error_envelope_is_never_mistaken_for_a_reply() {
        let raw = r#"{"is_error":true,"result":"Credit balance is too low"}"#;
        let err = parse_result(raw).unwrap_err();
        let text = format!("{err}");
        assert!(text.contains("reported an error"), "{text}");
        assert!(
            text.contains("Credit balance is too low"),
            "the CLI's own text must survive: {text}"
        );
    }

    #[test]
    fn the_failure_modes_stay_distinguishable() {
        let not_json = parse_result("command not found: claude").unwrap_err();
        assert!(format!("{not_json}").contains("expected JSON envelope"));
        assert!(
            format!("{not_json}").contains("command not found"),
            "the output must be quoted or a bad invocation is unreadable"
        );

        let no_result = parse_result(r#"{"is_error":false}"#).unwrap_err();
        assert!(format!("{no_result}").contains("no 'result' field"));

        // An empty reply is useless but legitimate. The loop decides what to do
        // with it; parsing must not call it an error.
        assert_eq!(
            parse_result(r#"{"result":"","is_error":false}"#).unwrap(),
            ""
        );
    }

    /// A missing `is_error` must not be read as an error, and must not be read
    /// as absent-becomes-false in the direction that hides a failure: the
    /// envelope always carries it on the paths that matter, and defaulting a
    /// *missing* one to `false` only affects hand-written JSON.
    #[test]
    fn a_missing_is_error_flag_still_yields_the_result() {
        assert_eq!(parse_result(r#"{"result":"ok"}"#).unwrap(), "ok");
    }

    #[test]
    fn the_generator_id_names_the_kind_and_the_model() {
        assert_eq!(
            generator_id("claude-sonnet-5"),
            "claude-cli:claude-sonnet-5"
        );
        assert_eq!(generator_id("claude-opus-5"), "claude-cli:claude-opus-5");
    }
}
