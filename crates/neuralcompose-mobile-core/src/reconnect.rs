//! Pure reconnect policy. The SHELL owns timers and sockets; this module only
//! decides. Pinned to the Expo client's `LiveApiClient.ts`: the attempt
//! counter increments BEFORE the backoff computation, so the delays are
//! exactly 1000, 2000, 4000 ms, and a 4th failure gives up.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ReconnectDecision {
    RetryAfterMs { delay_ms: u64 },
    GiveUp,
}

pub const MAX_ATTEMPTS: u32 = 3;
pub const BASE_MS: u64 = 500;
pub const CAP_MS: u64 = 30_000;

/// `completed_attempts` = how many connection attempts have already failed
/// (i.e. the value AFTER the increment-on-close).
pub fn next_reconnect(completed_attempts: u32) -> ReconnectDecision {
    next_reconnect_with(completed_attempts, MAX_ATTEMPTS, BASE_MS, CAP_MS)
}

pub fn next_reconnect_with(
    completed_attempts: u32,
    max_attempts: u32,
    base_ms: u64,
    cap_ms: u64,
) -> ReconnectDecision {
    if completed_attempts > max_attempts {
        return ReconnectDecision::GiveUp;
    }
    let exp = completed_attempts.min(63);
    let delay = base_ms.saturating_mul(1u64 << exp);
    ReconnectDecision::RetryAfterMs {
        delay_ms: delay.min(cap_ms),
    }
}
