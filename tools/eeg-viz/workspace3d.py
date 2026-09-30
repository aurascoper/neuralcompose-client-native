# /// script
# requires-python = ">=3.11"
# dependencies = ["pyvista", "numpy", "scipy"]
# ///
"""3D EEG session viewer: time x frequency x log power, one surface per channel.

The spectrogram is computed HERE (scipy STFT); the verdict markers are the
recorded channelHealth annotations from the Rust core. If a mains verdict has
no visible ridge at its mainsLineHz, one of the two has a bug.

Usage:  uv run workspace3d.py SESSION.eeg.jsonl [SESSION.turns.jsonl]
                              [--screenshot out.png | --self-check]
        uv run workspace3d.py SESSION.eeg.jsonl --export OUT.npz [--session-id ID]
        uv run workspace3d.py --view OUT.npz [--screenshot out.png]

Export writes the spectrogram grid + recorded verdicts as a neutral npz
artifact; --view renders such an artifact and computes nothing.
"""

import argparse
import hashlib
import json
import sys
from pathlib import Path

import numpy as np
from scipy import signal

CHANNELS = ["TP9", "AF7", "AF8", "TP10"]  # frozen wire order (contracts/constants.json)
NOMINAL_HZ = 256.0


def read_capture(path):
    """[(timestamp_s, [ch0..ch3])] straight from the capture lines, file order."""
    samples = []
    with open(path) as f:
        for raw in f:
            raw = raw.strip()
            if not raw:
                continue
            for s in json.loads(json.loads(raw)["payload"]):
                samples.append((s["timestamp"], s["channels"]))
    return samples


def read_turn_health(path):
    """[(turn_index, channel, last_source_timestamp, record)] as recorded; no derivation."""
    out = []
    with open(path) as f:
        for raw in f:
            raw = raw.strip()
            if not raw:
                continue
            turn = json.loads(raw)
            for rec in turn.get("channelHealth") or []:
                out.append(
                    (turn["index"], rec["channel"], rec["window"].get("lastSourceTimestamp"), rec)
                )
    return out


def actual_rate_hz(times):
    """Samples over elapsed span, gaps > 1 s excluded.

    NOT a median of per-sample diffs: the muse bridge stamps a drained batch
    with one clock read, so intra-batch diffs are microseconds and a median
    lands on the burst cluster (measured: 292 kHz from a real capture whose
    true rate was 256 Hz). The span survives burst timestamps; only real
    stream outages are excluded. Sibling trap recorded in lejepa's
    compute_actual_sample_rate (2026-08-07), which hit the inverse case."""
    if len(times) < 2:
        return NOMINAL_HZ
    d = np.diff(times)
    gap = d > 1.0
    span = float(times[-1] - times[0] - d[gap].sum())
    n = len(times) - 1 - int(gap.sum())
    return n / span if span > 0 else NOMINAL_HZ


def spectrograms(times, data, fs):
    """(f, t_abs, [logP per channel]) — ~1 s windows, 50% overlap, log10 power."""
    nperseg = min(int(round(fs)), data.shape[0])
    out = []
    for i in range(data.shape[1]):
        f, t, sxx = signal.spectrogram(data[:, i], fs=fs, nperseg=nperseg, noverlap=nperseg // 2)
        out.append(np.log10(sxx + 1e-20))
    return f, t + times[0], out


def sha256_of(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def health_records(health):
    """Flatten recorded turn-health tuples into plain dicts (for the npz artifact)."""
    return [
        {"turn": turn_idx, "channel": channel, "t": ts,
         "status": rec["annotation"]["status"], "verdict": rec["annotation"]["verdict"],
         "mainsLineHz": rec["annotation"].get("mainsLineHz")}
        for turn_idx, channel, ts, rec in health
    ]


def export_npz(times, data, health, fs, sources, session_id, out_path):
    """Serialize the spectrogram grid + recorded verdicts as a neutral artifact."""
    f, t, logs = spectrograms(times, data, fs)
    meta = {
        "session_id": session_id,
        "time_range": [float(times[0]), float(times[-1])],
        "channels": CHANNELS,
        "nominal_hz": NOMINAL_HZ,
        "actual_hz": fs,
        "sources": sources,
        "verdicts": health_records(health),
        "tool": "workspace3d.py export v1",
    }
    np.savez(out_path, power=np.array(logs, dtype=np.float32),
             freqs=f, times=t, meta_json=json.dumps(meta))
    print(f"wrote {out_path}: power {np.array(logs).shape}, session_id={session_id}")


def load_npz(npz_path):
    """(f, t, logs, health-tuples, stamp, meta) from an exported artifact; no computation."""
    d = np.load(npz_path)
    meta = json.loads(str(d["meta_json"]))
    logs = [d["power"][i] for i in range(d["power"].shape[0])]
    health = [
        (v["turn"], v["channel"], v["t"],
         {"annotation": {"status": v["status"], "verdict": v["verdict"],
                         "mainsLineHz": v.get("mainsLineHz")}})
        for v in meta["verdicts"]
    ]
    stamp = f"artifact sha256 {sha256_of(npz_path)}\nsession {meta['session_id']}"
    return d["freqs"], d["times"], logs, health, stamp, meta


def render_grids(f, t, logs, health, stamp, screenshot=None):
    import pyvista as pv

    lo = min(lp.min() for lp in logs)
    hi = max(lp.max() for lp in logs)
    zspan = 0.25 * max(t[-1] - t[0], f[-1] - f[0])  # so the warp reads at any duration
    zscale = zspan / max(hi - lo, 1e-12)
    tgrid, fgrid = np.meshgrid(t, f)

    off = screenshot is not None
    pl = pv.Plotter(shape=(2, 2), off_screen=off, window_size=(1600, 1200))
    for i, name in enumerate(CHANNELS):
        pl.subplot(i // 2, i % 2)
        lp = logs[i]
        grid = pv.StructuredGrid(tgrid, fgrid, (lp - lo) * zscale)
        grid["log10 power"] = lp.ravel(order="F")
        pl.add_mesh(
            grid, scalars="log10 power", cmap="viridis", clim=(lo, hi),
            show_scalar_bar=(i == 0),
            scalar_bar_args={"title": "log10 power", "label_font_size": 10, "title_font_size": 12},
        )
        pl.add_text(name, font_size=12)
        ztop = (hi - lo) * zscale
        for turn_idx, channel, ts, rec in health:
            if channel != name or ts is None:
                continue
            ann = rec["annotation"]
            color = "green" if ann["status"] == "healthy" else "red"
            pl.add_mesh(pv.Line((ts, f[0], ztop), (ts, f[-1], ztop)), color=color, line_width=3)
            pl.add_point_labels(
                [(ts, f[-1], ztop)], [f"t{turn_idx} {ann['verdict']}"],
                font_size=10, text_color=color, shape=None, always_visible=True,
                show_points=False,
            )
            mains_hz = ann.get("mainsLineHz")
            if mains_hz is not None and "mains" in str(ann["verdict"]).lower():
                pl.add_mesh(
                    pv.Line((t[0], mains_hz, ztop), (t[-1], mains_hz, ztop)),
                    color=color, line_width=2,
                )
        pl.show_grid(xtitle="time (s)", ytitle="freq (Hz)", ztitle="log P", font_size=8)
    pl.subplot(0, 0)
    pl.add_text(
        "spectrogram: computed by this tool | verdicts: recorded by core\n" + stamp,
        position="lower_left", font_size=6,
    )
    pl.link_views()
    if off:
        pl.screenshot(screenshot)
        pl.close()
        return screenshot
    pl.show()


def render(times, data, health, fs, stamp, screenshot=None):
    f, t, logs = spectrograms(times, data, fs)
    return render_grids(f, t, logs, health, stamp, screenshot=screenshot)


def load(eeg_path, turns_path):
    samples = read_capture(eeg_path)
    if not samples:
        sys.exit(f"{eeg_path}: no samples")
    times = np.array([s[0] for s in samples])
    data = np.array([s[1] for s in samples])
    health = read_turn_health(turns_path) if turns_path.exists() else []
    if not turns_path.exists():
        print(f"note: {turns_path} not found; surfaces only", file=sys.stderr)
    fs = actual_rate_hz(times)
    print(f"sample rate: contract {NOMINAL_HZ:g} Hz, actual {fs:.3f} Hz")
    if abs(fs - NOMINAL_HZ) / NOMINAL_HZ > 0.01:
        print(f"WARNING: actual rate differs from contract by >1%", file=sys.stderr)
    stamp = f"eeg sha256 {sha256_of(eeg_path)}"
    if turns_path.exists():
        stamp += f"\nturns sha256 {sha256_of(turns_path)}"
    return times, data, health, fs, stamp


def self_check():
    import tempfile

    fs, dur = 256.0, 8.0
    n = int(fs * dur)
    ts = np.arange(n) / fs
    freqs = [10.0, 15.0, 20.0, 25.0]
    chans = np.column_stack([20.0 * np.sin(2 * np.pi * fq * ts) for fq in freqs])
    lines = []
    for i in range(0, n, 64):
        chunk = [{"timestamp": ts[j], "channels": list(chans[j])} for j in range(i, min(i + 64, n))]
        lines.append(json.dumps({
            "sequence": i // 64 + 1, "receivedAtMonotonicMs": i,
            "acceptedSampleCount": len(chunk), "payload": json.dumps(chunk),
        }))
    turn = {"index": 0, "channelHealth": [
        {"channel": "TP9",
         "window": {"sampleCount": n, "lastSourceTimestamp": ts[-1]},
         "derived": {"rmsMicrovolts": 14.1, "mainsPower": 0.0},
         "annotation": {"status": "healthy", "verdict": "ok", "mainsLineHz": None}},
        {"channel": "AF7",
         "window": {"sampleCount": n, "lastSourceTimestamp": 4.0},
         "derived": {"rmsMicrovolts": 40.0, "mainsPower": 0.9},
         "annotation": {"status": "degraded", "verdict": "mains-pickup", "mainsLineHz": 60.0}},
    ]}
    with tempfile.TemporaryDirectory() as d:
        eeg = Path(d) / "s.eeg.jsonl"
        turns = Path(d) / "s.turns.jsonl"
        eeg.write_text("\n".join(lines) + "\n")
        turns.write_text(json.dumps(turn) + "\n")
        times, data, health, rate, stamp = load(eeg, turns)
        assert data.shape == (n, 4), data.shape
        assert abs(rate - fs) / fs < 0.001, rate
        # burst-stamped timestamps (the muse bridge's pattern: a drained batch
        # shares one clock read) must still estimate the true rate from span
        burst = ts.copy()
        for i in range(0, n, 8):
            burst[i:i + 8] = ts[i]  # whole batch collapses onto one stamp
        burst_rate = actual_rate_hz(burst)
        assert abs(burst_rate - fs) / fs < 0.05, f"burst rate {burst_rate}"
        assert len(health) == 2 and health[1][3]["annotation"]["mainsLineHz"] == 60.0
        assert "sha256" in stamp and sha256_of(eeg) in stamp
        f, t, logs = spectrograms(times, data, rate)
        for i, fq in enumerate(freqs):  # each surface must ridge at its sine frequency
            peak = f[np.argmax(logs[i].mean(axis=1))]
            assert abs(peak - fq) <= 1.0, (CHANNELS[i], peak, fq)
        out = Path(d) / "out.png"
        render(times, data, health, rate, stamp, screenshot=str(out))
        assert out.stat().st_size > 0
        # export -> view round trip: view must render from the artifact alone
        npz = Path(d) / "s.spectrogram.npz"
        export_npz(times, data, health, rate,
                   {"eeg_sha256": sha256_of(eeg), "turns_sha256": sha256_of(turns)},
                   "self-check-session", npz)
        f2, t2, logs2, health2, stamp2, meta = load_npz(npz)
        assert meta["session_id"] == "self-check-session"
        assert meta["time_range"] == [float(times[0]), float(times[-1])]
        assert len(health2) == 2 and health2[1][3]["annotation"]["mainsLineHz"] == 60.0
        np.testing.assert_allclose(logs2[0], logs[0], rtol=1e-6)  # float32 round trip
        out2 = Path(d) / "out2.png"
        render_grids(f2, t2, logs2, health2, stamp2, screenshot=str(out2))
        assert out2.stat().st_size > 0
    print("self-check ok")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("eeg", nargs="?", help="<id>.eeg.jsonl capture payload")
    ap.add_argument("turns", nargs="?", help="<sid>.turns.jsonl (default: sibling of eeg)")
    ap.add_argument("--screenshot", metavar="OUT.png", help="render off-screen to file")
    ap.add_argument("--self-check", action="store_true")
    ap.add_argument("--export", metavar="OUT.npz", help="write the neutral artifact, no render")
    ap.add_argument("--session-id", help="session id stamped into the export (default: eeg stem)")
    ap.add_argument("--view", metavar="IN.npz", help="render a previously exported artifact")
    args = ap.parse_args()
    if args.self_check:
        self_check()
        return
    if args.view:
        f, t, logs, health, stamp, _meta = load_npz(Path(args.view))
        out = render_grids(f, t, logs, health, stamp, screenshot=args.screenshot)
        if out:
            print(out)
        return
    if not args.eeg:
        ap.error("eeg path required (or --self-check / --view)")
    eeg = Path(args.eeg)
    turns = Path(args.turns) if args.turns else eeg.with_name(
        eeg.name.replace(".eeg.jsonl", ".turns.jsonl")
    )
    if args.export:
        times, data, health, fs, _stamp = load(eeg, turns)
        sources = {"eeg_sha256": sha256_of(eeg),
                   "turns_sha256": sha256_of(turns) if turns.exists() else None}
        session_id = args.session_id or eeg.name.replace(".eeg.jsonl", "")
        export_npz(times, data, health, fs, sources, session_id, args.export)
        return
    out = render(*load(eeg, turns), screenshot=args.screenshot)
    if out:
        print(out)


if __name__ == "__main__":
    main()
