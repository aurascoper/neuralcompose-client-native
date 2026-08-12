//! Hypnagogic loop modes, dialectical competition and turn logging on Linux —
//! ported from the macOS NeuralCompose Swift app.
//!
//! **This crate is pure.** No clock, no sockets, no filesystem, no
//! subprocesses, no model. Everything the loops need from the outside world
//! arrives through the traits in [`seams`], and every effect lives in the
//! `neuralcompose-hypnagogic` binary. That is the same rule
//! `neuralcompose-mobile-core` holds, and it is what lets the whole dialectic
//! be exercised in CI against fixed vectors and recorded draws with no
//! llama.cpp, no whisper and no llama-server present.
//!
//! ## What is here, and what is deliberately not
//!
//! Four modes — `mirror`, `focused`, `reflective`, `contemplative` — where
//! mirror runs a plain reply loop and the other three run a dialectical
//! competition at their own [`profile::ContextProfile`] tuning. Three of the
//! four differences are presets over knobs the engine already has; the fourth,
//! the Reflective Witness, is a third generate call and therefore control flow.
//!
//! Not here, on purpose: `DialecticalMemory`, `SemanticGraph` and the v2
//! weight field. Those are later milestones in the Swift, and `FIELD_V2.md`
//! explicitly defers `fieldEnergy` as a latent state variable.
//!
//! ## Non-claims
//!
//! - **Promotes no support-matrix row.** `attained_support_status()` returns
//!   what it returned before this crate existed. A live microphone is the
//!   opposite of a deterministic fixture, so nothing here is
//!   `RuntimeSmokeValidated` evidence — ADR-002 forbids promotion by
//!   implication.
//! - **The dialectic is genuinely two-sided only in form.** Both poles are the
//!   same model under different instructions, so a turn stages a disagreement
//!   rather than holding one. No independent reasoner is on either side.
//! - **Engineering scaffolding, not a validated intervention.** Any efficacy
//!   claim requires the D8 pre-registration.
//! - **No cognitive state is read.** EEG here feeds channel health and the
//!   turn log. It does not bias the dialectic: `SpectralState` needs a Core ML
//!   classifier with no Linux runtime, so the gloss stays at the neutral value
//!   the Swift itself uses for an absent estimator (see
//!   [`turn_log::NEUTRAL_GLOSS`]).
//!
//! ## Conformance status
//!
//! [`dynamics`] and [`profile`] are hand ports whose constants are pinned by
//! unit tests, but whose *arithmetic* is not yet asserted against the Swift.
//! The fixture that makes "port" a checkable claim is generated on the Mac
//! (`swift` is not installed on the Linux box). Until it lands, agreement is
//! intended, not established.

pub mod dynamics;
pub mod embedding;
pub mod profile;
pub mod seams;
pub mod turn_log;
