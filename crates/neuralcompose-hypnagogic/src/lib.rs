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
//! Not here, on purpose: `SemanticGraph` and the v2
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
//! ## Deliberate divergences from the Swift
//!
//! This port is otherwise faithful line for line, so every intentional
//! deviation is listed here rather than left for a reader to find scattered
//! across modules and wonder about. **All three are the same defect class** —
//! an absent value given a numeric meaning the field can legitimately hold —
//! and all three are on error paths only, so the happy path (one embedder, one
//! dimension, all roles present) is identical and the conformance fixture is
//! unaffected. The Swift side is tracked in
//! `docs/reviews/absent-as-legitimate-value-2026-08-12.md`.
//!
//! 1. [`embedding::Embedding::cosine_similarity`] returns `Option<f32>`, not a
//!    `0` sentinel. `0` is a legitimate cosine — orthogonal vectors from the
//!    *same* model return it — so a sentinel makes an incomparable pair
//!    indistinguishable from an unaligned one. Propagated through
//!    [`dynamics::energy`], [`dynamics::tension`] and
//!    [`dynamics::synthesis_score`] so **missing** (an early turn, legitimately
//!    neutral at 0.5) stays distinct from **incomparable** (a defect).
//! 2. [`dynamics::centroid`] refuses a mixed set instead of silently averaging
//!    the comparable subset. A subset centroid is still a plausible vector, and
//!    every similarity measured against it afterwards is plausible too.
//! 3. `role_fulfillment` is `Option<f32>`, where the Swift writes
//!    `role?.objective(energy) ?? 0` (`HypnagogicDialecticLoop.swift:241`).
//!    This is the sharpest of the three: `role_fulfillment` exists so a later
//!    stage can notice a role that *failed its own brief*, and `0` is precisely
//!    that signal — so a role-lookup miss would not merely hide, it would
//!    impersonate the diagnostic and report a maximally-failed pole.
//!
//! Checked before adopting (3): the conformance fixture cannot reach the
//! lookup at all. `Sources/DialecticFixture` hardcodes `roleFulfillment: 0`,
//! never emits the field, and exercises `DialecticalDynamics` rather than the
//! loop — so this divergence cannot surface as a conformance failure that looks
//! like a port error.
//!
//! ## Conformance status
//!
//! [`dynamics`] is **verified against the Swift**, not merely self-consistent:
//! `tests/dialectic_conformance.rs` replays 48 cases emitted by
//! `Sources/DialecticFixture` in the NeuralCompose repo and checks every
//! intermediate — per-axis energies, potentials, tension, selection
//! temperature, the full probability vector, margin, decisive flag and selected
//! basin. Selection draws are recorded and injected, never seeded, because the
//! two languages' PRNGs diverge from the same seed.
//!
//! That test has itself been mutation-tested; two comparison operators were
//! only reachable by fixture cases placed exactly on the boundary, and those
//! cases are now asserted present. If you change [`dynamics`], re-check that
//! mutating it still turns the suite red — and confirm the mutation actually
//! applied first, since a patch that matched nothing is indistinguishable from
//! a surviving mutant.
//!
//! [`profile`] and [`role`] are pinned by constant-for-constant unit tests
//! rather than by the fixture. Regenerating the fixture after any change to the
//! Swift dialectic is the mechanism that keeps this honest; the harness is
//! committed for exactly that reason.

pub mod dialectic;
pub mod dynamics;
pub mod embedding;
pub mod loops;
pub mod profile;
pub mod role;
pub mod seams;
pub mod turn_log;
