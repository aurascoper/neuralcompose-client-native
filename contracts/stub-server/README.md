# Gate 4 reference stub

Checked in **verbatim** from the stub the Expo client's Gate 4 acceptance run
was executed against (2026-07-28, NeuralCompose PR #45 @ `886b25f`). Do not
edit casually — the Rust core's `gate4_probe` example and the shells' manual
acceptance runs all replay against this exact artifact.

```sh
npm ci
node server.mjs
```

Serves on `http://127.0.0.1:8787`:

- `GET /api/diagnostics`, `/api/health`, `/api/classifier`, `/api/pipeline-mode`
- `WS /api/eeg/stream` — 8-sample batches every 31.25 ms (256 Hz), sine
  fixture per channel
- Controls:
  - `POST /control/pause` — socket stays open, samples and heartbeat stop
    (the open-but-silent case that must present as STALE, not OPEN)
  - `POST /control/resume`
  - `POST /control/drop?ms=2500` — closes sockets, rejects reconnects for the
    window (exercises CLOSED → CONNECTING → LIVE with backoff 1s, 2s)
  - `GET /control/state` — `{streaming, acceptingStreams, websocketClients, packetsReceived, lastHeartbeat}`
