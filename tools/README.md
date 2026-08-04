# tools/

Development tools. **None is a product surface**, none ships in any app, and
none is on the SwiftPM/Gradle/Cargo target graph.

Two are EEG bridges; `spoken-loop` is a demo and is described at the bottom.

The golden-capture gate assumes `Muse → bridge → /api/eeg/stream → app`, but
nothing in this repository spoke that first hop: the macOS app consumes Muse
internally and never re-exports it, and the only server here was the Gate 4
*synthetic* stub. These fill that gap so the gate can run against a real
headband.

| Tool | Path | Muse link |
| --- | --- | --- |
| `muse-ble-bridge` | Python + BrainFlow | **Direct Bluetooth.** No phone, no Mind Monitor. |
| `muse-osc-bridge` | Node | Mind Monitor OSC over UDP. Fallback when BLE is unavailable. |

Both publish the frozen wire contract on `/api/eeg/stream`: either one sample
or an array, `timestamp` in **seconds since stream start** (never wall
clock), exactly four microvolt channels in fixed TP9/AF7/AF8/TP10 order.

Both bind all interfaces so a phone on the same LAN can reach them — which
also means anyone on that LAN can read the stream. **Trusted networks only.**
Neither writes to disk: persistence is the client's job, so raw EEG leaves no
trace in either bridge.

## Direct BLE (preferred)

```sh
cd tools/muse-ble-bridge
python3 -m venv .venv && ./.venv/bin/pip install brainflow websockets
./.venv/bin/python bridge.py          # ws://0.0.0.0:8788/api/eeg/stream
```

Power the headband on and leave it unpaired from other apps. On macOS BLE
matches by name, so `MUSE_MAC` is optional. Grant the terminal Bluetooth
permission on first run.

## Mind Monitor OSC (fallback)

```sh
cd tools/muse-osc-bridge && npm install
node bridge.mjs                        # OSC :5000 → ws :8788
```

Mind Monitor → Settings → OSC Stream Target = this machine's LAN IP, port
5000. Only `/muse/eeg` is consumed; `/muse/acc`, `/muse/gyro` and `/muse/batt`
are ignored so continuing non-EEG traffic cannot make a dead EEG substream
look alive.

## `spoken-loop` — a demo, not a bridge

One spoken turn on Linux: mic → whisper.cpp (CPU) → llama-server (Vulkan, HTTP)
→ Kokoro-82M → speakers. Unlike the bridges it publishes no contract and touches
no headband.

```sh
cd tools/spoken-loop                       # must be run from its own directory
./turn.sh                                  # speak, then press Enter
./turn.sh ~/src/whisper.cpp/samples/jfk.wav   # or feed a WAV and skip the mic
```

**A fresh checkout does not run**: `.venv` and `models/` are gitignored and must
be created first — see that directory's README.

Read `tools/spoken-loop/README.md` first. Three things it establishes that are
easy to get wrong by assumption:

- **It promotes no support-matrix row and must not be cited as if it did.** A
  working spoken demo is exactly the artifact that later gets read as evidence
  the platform works.
- **It links onnxruntime** (for Kokoro). The single-runtime rule governs the
  shipped binary, not this; the product's runtime decision is **deferred and
  unmade**, and this dependency graph is not a precedent for it.
- **Exactly one process may hold a Vulkan context.** llama-server holds it;
  whisper is built `-DGGML_VULKAN=OFF` and Kokoro's onnxruntime has no GPU
  provider. PR #30's device lock is process-local and cannot serialise across
  processes.
