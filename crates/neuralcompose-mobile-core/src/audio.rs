//! Recording lifecycle state machine (M6). The SHELLS own microphones,
//! permission prompts, codecs, and platform file handles (AVAudioSession /
//! AudioRecord / MediaRecorder); this module owns only the deterministic
//! semantics: which transitions are legal, what gets recorded about them,
//! and the portable recording manifest.
//!
//! Acceptance baseline (from the Expo oracle gates, 2026-07-28):
//! - permission denied  → record unreachable, visible reason, no file/entry
//! - permission granted → record becomes reachable
//! - stop               → manifest appears ONLY with a successful persist
//! - play               → second action stops it
//! - interruption       → explicit state, explicit recovery
//! - restart            → persisted entries reload without phantom recordings

use std::sync::Mutex;

use sha2::{Digest, Sha256};

/// The operator-specified phase set.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RecordingPhase {
    Idle,
    PermissionDenied,
    Ready,
    Recording,
    Persisting,
    Recorded,
    Playing,
    Interrupted,
    Failed { reason: String },
}

/// Portable, platform-neutral recording manifest. `created_at_ms` is
/// shell-supplied display metadata (wall clock allowed); every DECISION in
/// this module uses only event ordering, never clocks.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct RecordingManifest {
    pub id: String,
    pub created_at_ms: u64,
    pub duration_ms: u64,
    pub format: String,
    pub byte_size: u64,
    pub sha256_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct AudioTransition {
    pub from: RecordingPhase,
    pub to: RecordingPhase,
    pub event: String,
    pub at_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct AudioSnapshot {
    pub phase: RecordingPhase,
    pub manifests: Vec<RecordingManifest>,
    pub transitions: Vec<AudioTransition>,
    /// True while an unfinalized recording exists (Recording/Persisting/
    /// Interrupted-from-Recording). Shells use it to warn before discarding.
    pub has_unfinalized_recording: bool,
}

struct Inner {
    phase: RecordingPhase,
    manifests: Vec<RecordingManifest>,
    transitions: Vec<AudioTransition>,
    interrupted_from_recording: bool,
    /// Where playback returns on stop. Playback is a read-only activity: it
    /// must never change what the user is otherwise allowed to do — stopping
    /// playback started from PermissionDenied lands back on PermissionDenied,
    /// never on a phase that grants recording authority.
    playback_return_phase: Option<RecordingPhase>,
}

#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
pub struct AudioLifecycle {
    inner: Mutex<Inner>,
}

impl Default for AudioLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
impl AudioLifecycle {
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                phase: RecordingPhase::Idle,
                manifests: Vec::new(),
                transitions: Vec::new(),
                interrupted_from_recording: false,
                playback_return_phase: None,
            }),
        }
    }

    /// Restart path: reload previously persisted manifests. Phase stays Idle
    /// (never a phantom Recording); permission must be re-reported.
    #[cfg_attr(feature = "uniffi", uniffi::constructor)]
    pub fn with_manifests(manifests: Vec<RecordingManifest>) -> Self {
        let lc = Self::new();
        lc.inner.lock().unwrap().manifests = manifests;
        lc
    }

    /// Shell reports the platform permission result.
    pub fn on_permission(&self, granted: bool, now_ms: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        let to = if granted {
            RecordingPhase::Ready
        } else {
            RecordingPhase::PermissionDenied
        };
        // Legal from Idle/PermissionDenied/Ready (re-report); never mid-flight.
        match g.phase {
            RecordingPhase::Idle | RecordingPhase::PermissionDenied | RecordingPhase::Ready => {
                transition(&mut g, to, "permission", now_ms);
                true
            }
            _ => false,
        }
    }

    /// Record is reachable ONLY from Ready or Recorded (a new take).
    /// From PermissionDenied this is a rejected no-op: no state change,
    /// no file, no entry — the shell shows the explanation.
    pub fn on_record_start(&self, now_ms: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.phase {
            RecordingPhase::Ready | RecordingPhase::Recorded => {
                transition(&mut g, RecordingPhase::Recording, "record_start", now_ms);
                true
            }
            _ => false,
        }
    }

    pub fn on_record_stop(&self, now_ms: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.phase != RecordingPhase::Recording {
            return false;
        }
        transition(&mut g, RecordingPhase::Persisting, "record_stop", now_ms);
        true
    }

    /// Atomic persistence: the manifest appears only here, together with the
    /// Recorded phase. `sha256_hex(bytes)` provides the content hash.
    #[allow(clippy::too_many_arguments)]
    pub fn on_persisted(
        &self,
        id: String,
        created_at_ms: u64,
        duration_ms: u64,
        format: String,
        byte_size: u64,
        sha256_hex: String,
        now_ms: u64,
    ) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.phase != RecordingPhase::Persisting {
            return false;
        }
        g.manifests.push(RecordingManifest {
            id,
            created_at_ms,
            duration_ms,
            format,
            byte_size,
            sha256_hex,
        });
        g.interrupted_from_recording = false;
        transition(&mut g, RecordingPhase::Recorded, "persisted", now_ms);
        true
    }

    /// Persist failure: NO manifest, explicit Failed state with the reason.
    pub fn on_persist_failed(&self, reason: String, now_ms: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.phase != RecordingPhase::Persisting {
            return false;
        }
        g.interrupted_from_recording = false;
        transition(
            &mut g,
            RecordingPhase::Failed { reason },
            "persist_failed",
            now_ms,
        );
        true
    }

    /// Playback is independent of microphone permission: legal from Idle,
    /// PermissionDenied, Ready, and Recorded whenever a persisted manifest
    /// exists (integrity of the underlying file is the shell's check).
    pub fn on_play_start(&self, now_ms: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.manifests.is_empty() {
            return false;
        }
        match g.phase {
            RecordingPhase::Idle
            | RecordingPhase::PermissionDenied
            | RecordingPhase::Ready
            | RecordingPhase::Recorded => {
                g.playback_return_phase = Some(g.phase.clone());
                transition(&mut g, RecordingPhase::Playing, "play_start", now_ms);
                true
            }
            _ => false,
        }
    }

    /// The second action stops playback, returning to the phase playback
    /// started from — never granting authority playback didn't have.
    pub fn on_play_stop(&self, now_ms: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.phase != RecordingPhase::Playing {
            return false;
        }
        let back = g
            .playback_return_phase
            .take()
            .unwrap_or(RecordingPhase::Recorded);
        transition(&mut g, back, "play_stop", now_ms);
        true
    }

    /// OS interruption (call, route change, backgrounding policy) while
    /// Recording or Playing.
    pub fn on_interruption(&self, now_ms: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        match g.phase {
            RecordingPhase::Recording => {
                g.interrupted_from_recording = true;
                transition(&mut g, RecordingPhase::Interrupted, "interruption", now_ms);
                true
            }
            RecordingPhase::Playing => {
                g.interrupted_from_recording = false;
                transition(&mut g, RecordingPhase::Interrupted, "interruption", now_ms);
                true
            }
            _ => false,
        }
    }

    /// Explicit recovery from an interruption. An interrupted recording was
    /// never persisted, so recovery from recording lands on Ready; recovery
    /// from playback returns to the phase playback started from (preserving
    /// the no-authority-gain rule even across interruptions).
    pub fn on_interruption_ended(&self, now_ms: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        if g.phase != RecordingPhase::Interrupted {
            return false;
        }
        let to = if g.interrupted_from_recording {
            g.playback_return_phase = None;
            RecordingPhase::Ready
        } else {
            g.playback_return_phase
                .take()
                .unwrap_or(RecordingPhase::Ready)
        };
        g.interrupted_from_recording = false;
        transition(&mut g, to, "interruption_ended", now_ms);
        true
    }

    /// Recover from Failed back to Ready (operator acknowledges the error).
    pub fn on_failure_acknowledged(&self, now_ms: u64) -> bool {
        let mut g = self.inner.lock().unwrap();
        if !matches!(g.phase, RecordingPhase::Failed { .. }) {
            return false;
        }
        transition(
            &mut g,
            RecordingPhase::Ready,
            "failure_acknowledged",
            now_ms,
        );
        true
    }

    /// Read-only. Never mutates lifecycle state.
    pub fn snapshot(&self) -> AudioSnapshot {
        let g = self.inner.lock().unwrap();
        AudioSnapshot {
            phase: g.phase.clone(),
            manifests: g.manifests.clone(),
            transitions: g.transitions.clone(),
            has_unfinalized_recording: matches!(
                g.phase,
                RecordingPhase::Recording | RecordingPhase::Persisting
            ) || (g.phase == RecordingPhase::Interrupted
                && g.interrupted_from_recording),
        }
    }

    pub fn phase(&self) -> RecordingPhase {
        self.inner.lock().unwrap().phase.clone()
    }
}

fn transition(g: &mut Inner, to: RecordingPhase, event: &str, at_ms: u64) {
    g.transitions.push(AudioTransition {
        from: g.phase.clone(),
        to: to.clone(),
        event: event.to_string(),
        at_ms,
    });
    g.phase = to;
}

/// Deterministic content hash for recording bytes — the portable half of the
/// manifest. Shells may hash natively instead; results must match this.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn sha256_hex(bytes: Vec<u8>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(out, "{b:02x}");
    }
    out
}
