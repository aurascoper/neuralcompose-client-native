# tools/

Development bridges. **Neither is a product surface**, neither ships in any
app, and neither is on the SwiftPM/Gradle/Cargo target graph.

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
