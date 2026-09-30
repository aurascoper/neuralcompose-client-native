# /// script
# requires-python = ">=3.11"
# dependencies = ["pyvista", "numpy", "websockets"]
# ///
"""Live EEG electrode network: a second subscriber on the bridge's stream.

LIVE — everything here is computed from the wire stream as it arrives; no
artifact, no digest, nothing is recorded by this window. It is a fitting and
watching aid, not an evidence display. The sealed session workspace
(tools/workspace/workspace.py) is the evidence view.

The scene: the four electrodes as spheres on a translucent head. A node
pulses with its live RMS; saturation is a DISTINCT state (red + SAT), not a
bigger sphere, because railing must never read as "large but fine". Edges
are tubes carrying 1-s Pearson correlation computed AFTER subtracting the
instantaneous 4-channel mean (crude common average reference): all four Muse
channels share the FPz reference, and raw broadband correlation would render
every tube warm and call it connectivity. Even after CAR, neighbouring
electrodes couple by proximity — AF7-AF8 > TP9-TP10 is geometry, not finding.

Thresholds mirror nc_eeg_lejepa/src/nc_eeg_lejepa/quality.py: saturation
|sample| > 800 uV absolute, dead channel RMS < 1 uV. Reused, not invented.

It taps the same broadcast the session consumes (bridge.py gathers sends with
return_exceptions=True over a snapshot of the client set, so a viewer coming
or going cannot disturb the session's feed). When the bridge goes away — the
session ended — this window says so and closes.

BOUNDARY, stated precisely: eeg.rs enforces "EEG does not touch the dialectic"
inside the Rust binary, for the paths it covers — this tap (and --status-json)
sit outside them. Nothing structural stops an agent feeding these numbers into
a prompt; on the agent side the rule is maintained by CONVENTION. Agents
polling --status-json should also rate-limit themselves: every call is a
connect/disconnect against the live bridge.

Usage:  uv run live.py [--url ws://127.0.0.1:8788/api/eeg/stream] [--self-check]
"""

import argparse
import itertools
import json
import sys

import numpy as np

CHANNELS = ["TP9", "AF7", "AF8", "TP10"]
RATE = 256.0
KEEP = int(RATE * 5)
WIN = 256                # 1 s window for RMS / correlation / saturation
SAT_UV = 800.0           # quality.py saturation_threshold
DEAD_RMS_UV = 1.0        # quality.py flat_threshold
PAIRS = list(itertools.combinations(range(4), 2))

# Approximate Muse electrode positions on a unit head (x right, y front, z up).
POS = {
    "TP9": (-0.82, -0.35, -0.15),
    "AF7": (-0.38, 0.75, 0.35),
    "AF8": (0.38, 0.75, 0.35),
    "TP10": (0.82, -0.35, -0.15),
}
NODE_COLORS = ["#1565c0", "#2e7d32", "#8e24aa", "#795548"]


class Ring:
    """Fixed-size rolling buffer of the last KEEP samples per channel."""

    def __init__(self):
        self.data = np.zeros((4, KEEP), dtype=np.float64)
        self.total = 0
        self.last_t = 0.0

    def push(self, frames):
        vals = np.array([f["channels"] for f in frames], dtype=np.float64).T
        n = vals.shape[1]
        if n >= KEEP:
            self.data[:] = vals[:, -KEEP:]
        else:
            self.data = np.roll(self.data, -n, axis=1)
            self.data[:, -n:] = vals
        self.total += n
        self.last_t = frames[-1]["timestamp"]

    def window(self):
        return self.data[:, -WIN:]

    def rms(self):
        return np.sqrt((self.window() ** 2).mean(axis=1))

    def saturated(self):
        return np.abs(self.window()).max(axis=1) > SAT_UV

    def car_corr(self):
        """Pairwise Pearson AFTER common-average-reference. Note: CAR biases
        four iid channels toward r = -1/(n-1) = -1/3; the scale is symmetric
        so that reads as cool, not as anything meaningful."""
        w = self.window()
        x = w - w.mean(axis=0, keepdims=True)
        x = x - x.mean(axis=1, keepdims=True)
        std = x.std(axis=1)
        out = {}
        for i, j in PAIRS:
            if std[i] < 1e-9 or std[j] < 1e-9:
                out[(i, j)] = 0.0
            else:
                out[(i, j)] = float((x[i] * x[j]).mean() / (std[i] * std[j]))
        return out


def parse_frames(message):
    d = json.loads(message)
    return d if isinstance(d, list) else [d]


def _corr_color(r):
    """Symmetric [-1,1] -> cool blue .. grey .. warm red."""
    t = (np.clip(r, -1, 1) + 1) / 2
    cool = np.array([0.25, 0.45, 0.95])
    mid = np.array([0.55, 0.55, 0.58])
    warm = np.array([0.95, 0.35, 0.2])
    rgb = mid + (warm - mid) * (t - 0.5) * 2 if t >= 0.5 else mid + (cool - mid) * (0.5 - t) * 2
    return rgb.tolist()


class View:
    """Electrode-network scene: pulsing nodes, correlation tubes, orbiting camera."""

    def __init__(self, url, off_screen=False):
        import pyvista as pv

        self.pv = pv
        self.pl = pv.Plotter(off_screen=off_screen, window_size=(1300, 950),
                             title="NeuralCompose live electrode network")
        self.pl.set_background("#101418")
        head = pv.Sphere(radius=1.0, theta_resolution=48, phi_resolution=48)
        self.pl.add_mesh(head, color="#39424e", opacity=0.16, smooth_shading=True)
        nose = pv.Cone(center=(0, 1.08, 0.05), direction=(0, 1, 0.15),
                       height=0.22, radius=0.09)
        self.pl.add_mesh(nose, color="#39424e", opacity=0.35)

        self.node_actors = []
        for i, name in enumerate(CHANNELS):
            actor = self.pl.add_mesh(pv.Sphere(radius=1.0), color=NODE_COLORS[i],
                                     smooth_shading=True)
            actor.SetPosition(*POS[name])
            actor.SetScale(0.08)
            self.node_actors.append(actor)
            p = np.array(POS[name])
            self.pl.add_point_labels([p * 1.35], [name], font_size=14, shape=None,
                                     always_visible=True, show_points=False,
                                     text_color="#d7dde5")

        self.edge_actors = {}
        for i, j in PAIRS:
            tube = pv.Line(POS[CHANNELS[i]], POS[CHANNELS[j]]).tube(radius=0.018)
            self.edge_actors[(i, j)] = self.pl.add_mesh(tube, color="#888888",
                                                        opacity=0.3,
                                                        smooth_shading=True)

        self.status = self.pl.add_text("waiting for the stream…",
                                       position="upper_left", font_size=11,
                                       color="#d7dde5")
        self.pl.add_text(
            "LIVE — computed from the wire stream; no artifact, no digest, nothing "
            "recorded by this window.\n"
            "Edges: 1-s correlation after common-average-reference (shared FPz "
            "component removed);\n"
            "neighbouring electrodes still couple by proximity — AF7-AF8 > TP9-TP10 "
            "is geometry, not finding.\n"
            f"Node states: pulse = RMS, red SAT = |sample| > {SAT_UV:.0f} uV, grey "
            f"DEAD = RMS < {DEAD_RMS_UV:.0f} uV.  Stream: {url}\n"
            "The sealed session workspace is the evidence view.",
            position="lower_left", font_size=7, color="#8a94a3")
        self.pl.camera_position = [(0.4, -2.6, 1.6), (0, 0, 0), (0, 0, 1)]

    def refresh(self, ring):
        rms = ring.rms()
        sat = ring.saturated()
        corr = ring.car_corr()
        lines = []
        for i, name in enumerate(CHANNELS):
            actor = self.node_actors[i]
            if sat[i]:
                actor.prop.color = "#ff2b2b"
                actor.SetScale(0.16)
                state = "SAT"
            elif rms[i] < DEAD_RMS_UV:
                actor.prop.color = "#5a5f66"
                actor.SetScale(0.05)
                state = "DEAD"
            else:
                actor.prop.color = NODE_COLORS[i]
                # log pulse over the healthy range (~1..800 uV)
                s = 0.06 + 0.10 * np.clip(np.log10(max(rms[i], 1.0)) / np.log10(SAT_UV), 0, 1)
                actor.SetScale(float(s))
                state = "ok"
            lines.append(f"{name} rms {rms[i]:6.1f}  {state}")
        for (i, j), actor in self.edge_actors.items():
            r = corr[(i, j)]
            actor.prop.color = _corr_color(r)
            actor.prop.opacity = 0.15 + 0.75 * abs(r)
        self.status.SetText(
            2, f"wire t {ring.last_t:7.1f}s   samples {ring.total}\n" + "\n".join(lines))
        self.pl.camera.Azimuth(0.25)


def watch(url):
    from websockets.sync.client import connect

    ring = Ring()
    view = View(url)
    announced = False
    try:
        with connect(url) as ws:
            view.pl.show(interactive_update=True, auto_close=False)
            while True:
                try:
                    ring.push(parse_frames(ws.recv(timeout=0.05)))
                except TimeoutError:
                    pass
                if not announced and ring.total > RATE:
                    print(f"connected — receiving, {ring.total} samples, wire t "
                          f"{ring.last_t:.1f}s", flush=True)
                    announced = True
                view.refresh(ring)
                view.pl.update()
    except (ConnectionRefusedError, OSError) as e:
        sys.exit(f"no stream at {url} ({e}) — is a session (or the bridge) running?")
    except Exception as e:
        if type(e).__name__.startswith("ConnectionClosed"):
            print("stream closed — session over", flush=True)
            view.pl.close()
        else:
            raise


def self_check():
    import threading
    import time

    from websockets.sync.client import connect
    from websockets.sync.server import serve

    rng = np.random.default_rng(3)
    stop = threading.Event()

    def handler(ws):
        t = 0.0
        while not stop.is_set():
            batch = []
            for _ in range(8):
                s = 20.0 * np.sin(2 * np.pi * 10.0 * t)
                # ch0=ch1 share the sine (tiny independent noise); ch2, ch3 are
                # independent noise. Post-CAR: r01 must be high AND r23 must be
                # low — a stub that always returns 1 fails the second assert.
                ch = [s + rng.normal(0, 2), s + rng.normal(0, 2),
                      rng.normal(0, 30), rng.normal(0, 30)]
                batch.append({"timestamp": t, "channels": ch})
                t += 1.0 / RATE
            try:
                ws.send(json.dumps(batch))
            except Exception:
                return
            time.sleep(8 / RATE)

    server = serve(handler, "127.0.0.1", 0)
    port = server.socket.getsockname()[1]
    threading.Thread(target=server.serve_forever, daemon=True).start()

    ring = Ring()
    view = View(f"ws://127.0.0.1:{port}", off_screen=True)
    with connect(f"ws://127.0.0.1:{port}") as ws:
        t0 = time.time()
        while time.time() - t0 < 2.5:
            try:
                ring.push(parse_frames(ws.recv(timeout=0.2)))
            except TimeoutError:
                pass
            view.refresh(ring)
    stop.set()
    server.shutdown()
    assert ring.total > RATE, f"only {ring.total} samples flowed"
    corr = ring.car_corr()
    r01, r23 = corr[(0, 1)], corr[(2, 3)]
    assert r01 > 0.9, f"shared-sine pair r={r01:.2f}, expected > 0.9"
    assert r23 < 0.2, f"independent pair r={r23:.2f}, expected < 0.2 (CAR bias ~ -1/3)"
    assert not ring.saturated().any()
    view.pl.close()
    print(f"self-check ok (r01={r01:.2f}, r23={r23:.2f})")


def status_json(url):
    """Agent-facing tap: collect ~1.5 s, print one JSON object, exit.

    Feeds the harness, not the session: read-only, writes nowhere, and the
    eeg.rs boundary (EEG does not touch the dialectic) is unaffected."""
    import time

    from websockets.sync.client import connect

    ring = Ring()
    try:
        with connect(url) as ws:
            t0 = time.time()
            while time.time() - t0 < 1.5:
                try:
                    ring.push(parse_frames(ws.recv(timeout=0.2)))
                except TimeoutError:
                    pass
    except (ConnectionRefusedError, OSError) as e:
        sys.exit(f"no stream at {url} ({e}) — is a session (or the bridge) running?")
    if ring.total == 0:
        sys.exit("connected but no samples arrived in 1.5s")
    rms = ring.rms()
    sat = ring.saturated()
    corr = ring.car_corr()
    print(json.dumps({
        "wire_t": round(ring.last_t, 2),
        "samples": ring.total,
        "channels": {
            name: {"rms_uv": round(float(rms[i]), 1), "sat": bool(sat[i]),
                   "dead": bool(rms[i] < DEAD_RMS_UV)}
            for i, name in enumerate(CHANNELS)
        },
        "car_corr": {f"{CHANNELS[i]}-{CHANNELS[j]}": round(corr[(i, j)], 3)
                     for i, j in PAIRS},
        "thresholds": {"sat_uv": SAT_UV, "dead_rms_uv": DEAD_RMS_UV},
        "note": "LIVE tap over ~1.5s; correlation is common-average-referenced; "
                "not an evidence artifact. May disagree with turn-log verdicts "
                "computed over other windows at other times — both can be right; "
                "disagreement is not a defect.",
    }))


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--url", default="ws://127.0.0.1:8788/api/eeg/stream")
    ap.add_argument("--self-check", action="store_true")
    ap.add_argument("--status-json", action="store_true",
                    help="1.5s tap -> one JSON object on stdout (for agents)")
    args = ap.parse_args()
    if args.self_check:
        self_check()
    elif args.status_json:
        status_json(args.url)
    else:
        watch(args.url)


if __name__ == "__main__":
    main()
