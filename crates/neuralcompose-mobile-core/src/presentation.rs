//! Truthful stream presentation. Semantic state crosses the FFI; shells
//! localize. The English formatters exist to (a) enforce parity with the Expo
//! client's Jest oracle strings and (b) serve as a lazy default for shells.
//!
//! Gate 4 lesson, pinned here: socket state alone is not health — an open
//! socket with no recent samples presents as STALE, never OPEN.

/// The operator-specified stream phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum StreamPhase {
    Connecting,
    OpenNoData,
    Live,
    Stale { age_ms: u64 },
    Closed,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum StreamTone {
    Ok,
    Stale,
    Connecting,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Presentation {
    pub phase: StreamPhase,
    pub tone: StreamTone,
    /// `floor(age_ms / 1000)` when the phase is `Stale`, else `None`.
    pub age_s: Option<u64>,
    /// "Stream silent — no samples for Ns (socket still open)"
    pub show_silent_banner: bool,
    /// "Stream disconnected — showing last cached data"
    pub show_disconnected_banner: bool,
}

pub fn present(phase: StreamPhase) -> Presentation {
    match phase {
        StreamPhase::Live => Presentation {
            phase,
            tone: StreamTone::Ok,
            age_s: None,
            show_silent_banner: false,
            show_disconnected_banner: false,
        },
        StreamPhase::OpenNoData => Presentation {
            phase,
            tone: StreamTone::Stale,
            age_s: None,
            show_silent_banner: false,
            show_disconnected_banner: false,
        },
        StreamPhase::Stale { age_ms } => Presentation {
            phase,
            tone: StreamTone::Stale,
            age_s: Some(age_ms / 1000),
            show_silent_banner: true,
            show_disconnected_banner: false,
        },
        StreamPhase::Connecting => Presentation {
            phase,
            tone: StreamTone::Connecting,
            age_s: None,
            show_silent_banner: false,
            show_disconnected_banner: false,
        },
        StreamPhase::Closed | StreamPhase::Error => Presentation {
            phase,
            tone: StreamTone::Down,
            age_s: None,
            show_silent_banner: false,
            show_disconnected_banner: true,
        },
    }
}

/// English pill label — must reproduce the Expo Jest oracle strings exactly.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn format_label_en(p: Presentation) -> String {
    match p.phase {
        StreamPhase::Live => "OPEN".to_string(),
        StreamPhase::OpenNoData => "OPEN · NO DATA".to_string(),
        StreamPhase::Stale { age_ms } => format!("STALE {}s", age_ms / 1000),
        StreamPhase::Connecting => "CONNECTING".to_string(),
        StreamPhase::Closed => "CLOSED".to_string(),
        StreamPhase::Error => "ERROR".to_string(),
    }
}

/// English banner — must reproduce the Expo Jest oracle strings exactly.
#[cfg_attr(feature = "uniffi", uniffi::export)]
pub fn format_banner_en(p: Presentation) -> Option<String> {
    if p.show_silent_banner {
        if let StreamPhase::Stale { age_ms } = p.phase {
            return Some(format!(
                "Stream silent — no samples for {}s (socket still open)",
                age_ms / 1000
            ));
        }
    }
    if p.show_disconnected_banner {
        return Some("Stream disconnected — showing last cached data".to_string());
    }
    None
}
