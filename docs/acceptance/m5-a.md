# Acceptance record — M5-A: connection-generation freshness (2026-07-28)

First deliberate divergence from the Expo oracle. After M5-A the Rust
contract is authoritative where the oracle was demonstrably wrong: transport
state from one connection is never evidence about another.

## Contract revision

```
socket N receives sample       → Live
socket N closes                → Closed
socket N+1 opens               → OpenNoData
cached traces remain visible   → still OpenNoData
invalid frame arrives          → still OpenNoData
first valid frame on N+1       → Live
```

Retry budget: a WebSocket handshake proves nothing. `Opened` no longer
resets attempts; the FIRST ACCEPTED FRAME of a generation does (and clears a
latched give-up). An open-immediately-close server therefore exhausts the
budget (`1000 → 2000 → 4000 ms → GiveUp/Error`) instead of retrying forever.

New FFI surface: `StreamMonitor.stream_snapshot(now_ms) -> StreamSnapshot`
`{connection_generation, received_on_current_connection, total_received,
last_received_at_current_ms, last_received_at_any_ms, cached_sample_count,
reconnect_attempts, phase}`. `ChannelSnapshot` (display cache) is unchanged
and intentionally survives reconnects.

## Evidence (all observed 2026-07-28 on the M4 Mac)

| Class | Result |
|---|---|
| Rust contract suite | **46 tests green** under both feature sets (39 prior + 7 operator-required regressions in `tests/m5a_generation.rs`: prev-gen freshness cannot authorize Live; cached samples survive without changing phase; invalid frames establish nothing; open-immediately-close reaches GiveUp; first accepted sample resets the budget; generations strictly monotonic; read paths never mutate freshness). Gate 4 suite updated where v0 semantics changed. clippy `-D warnings` + fmt clean. |
| Swift iOS Simulator XCTest | 2/2 passed (iPhone 17 / iOS 26.5) with new assertions: `.openNoData` after reopen, `connectionGeneration == 2`, `reconnectAttempts` 1 → 0 on first accepted frame. |
| Kotlin host-JVM | 2/2 passed (JNA + host cdylib) with the same generation assertions. |
| Live stub probe | Transcript after `/control/drop?ms=2500`: `Closed → retry 1000ms → Connecting → OpenNoData → Closed → retry 2000ms → Connecting → OpenNoData → Live`. Two v0 defects visibly absent: no phantom `Live` from inherited freshness after reconnect, and the backoff ladder escalates instead of resetting on each handshake. |
| SwiftUI observation | iPhone 17 simulator: green `OPEN` → (drop) → sustained orange **`OPEN · NO DATA`** on the reconnected-but-paused socket with cached traces still displayed (counter frozen at 848) → `/control/resume` → green `OPEN`, counter advancing. Under v0 the same state displayed green `OPEN`. |

## Non-claims

Unchanged from m4-start: no Android device/Compose UI, no physical iPhone,
no web client. The Compose shell (M5-B) starts only from this corrected
contract.
