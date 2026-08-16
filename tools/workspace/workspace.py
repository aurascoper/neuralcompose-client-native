# /// script
# requires-python = ">=3.11"
# dependencies = ["pyvista", "numpy"]
# ///
"""Combined session workspace: three panels, one clock, zero computation.

Renders the three neutral artifacts the repo tools export — spectrogram grid
(workspace3d.py export), latents (latent3d.py export), provenance neighbourhood
(graph3d.py export) — for ONE session, and triggers the missing exports in
their own repos. The render step computes nothing at all; every number drawn
here was computed and recorded by the exporting tool.

Epistemics, stated here the way the ADRs state rungs:
- The latent panel is DERIVED FROM the EEG panel. They will agree — visually,
  temporally, everywhere. That agreement is arithmetic, not evidence.
- Nothing records causation between a spectral event and a banked claim.
  Temporal coincidence across panels is coincidence.
- This is a navigation aid, not an analysis tool. No finding is licensed by
  what it shows.

The safeguard that matters: the viewer stamps all three artifact digests and
REFUSES to render when their session ids or time ranges disagree — a coherent
picture assembled across sessions is the most convincing failure available.
The three panels share no coordinate system (frequency, PCA axes and layout
axes are unrelated), so cameras are deliberately not linked; the one honest
shared axis is session time.

Usage:  uv run workspace.py SESSION_ID [--dir InteractionLogs] [--refresh]
                            [--screenshot out.png | --self-check]
"""

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

import numpy as np

DEFAULT_DIR = Path("~/Documents/NeuralCompose/InteractionLogs").expanduser()
REPOS = {
    "client_native": Path("~/src/neuralcompose-client-native").expanduser(),
    "lejepa": Path("~/src/neuralcompose-eeg-lejepa").expanduser(),
    "memory_server": Path("~/src/neural-memory-server").expanduser(),
}
CHANNELS = ["TP9", "AF7", "AF8", "TP10"]
TIME_TOLERANCE_S = 2.0
CLASS_COLORS = {
    "observed": "#2e7d32", "derivedDeterministically": "#1565c0",
    "humanDecision": "#f9a825", "agentInference": "#8e24aa", "externalClaim": "#795548",
}


def sha256_of(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def run(cmd, cwd):
    print(f"+ {' '.join(map(str, cmd))}  (cwd {cwd})", file=sys.stderr)
    r = subprocess.run(list(map(str, cmd)), cwd=cwd, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(f"export failed ({r.returncode}):\n{r.stdout}\n{r.stderr}")


def transcribe_csv(eeg_path, csv_path):
    """Mechanical .eeg.jsonl -> contract CSV transcription. Recorded values only,
    reshaped; no resampling, no filtering, no derivation."""
    with open(eeg_path) as f, open(csv_path, "w") as out:
        out.write("timestamp," + ",".join(CHANNELS) + "\n")
        for raw in f:
            raw = raw.strip()
            if not raw:
                continue
            for s in json.loads(json.loads(raw)["payload"]):
                out.write(f'{s["timestamp"]!r},' + ",".join(repr(v) for v in s["channels"]) + "\n")


def utc_anchor(session_dir, session_id):
    """(utc_start_iso, utc_end_iso) from the capture manifest, or None when absent."""
    man = session_dir / f"{session_id}.eeg.manifest.json"
    if not man.exists():
        return None
    m = json.loads(man.read_text())
    start_ms = m.get("startedAtMs")
    dur_s = m.get("durationSeconds")
    if start_ms is None:
        return None
    from datetime import datetime, timedelta, timezone

    t0 = datetime.fromtimestamp(start_ms / 1000, tz=timezone.utc)
    t1 = t0 + timedelta(seconds=dur_s if dur_s is not None else 24 * 3600)
    fmt = "%Y-%m-%dT%H:%M:%SZ"
    return t0.strftime(fmt), t1.strftime(fmt)


def ensure_artifacts(session_id, base_dir, repos, refresh):
    """Locate or trigger the three exports; return their paths + unverified notes."""
    eeg = base_dir / f"{session_id}.eeg.jsonl"
    if not eeg.exists():
        sys.exit(f"{eeg}: no such capture")
    ws = base_dir / f"{session_id}.workspace"
    ws.mkdir(exist_ok=True)
    spec = ws / f"{session_id}.spectrogram.npz"
    lat = ws / f"{session_id}.latents.npz"
    graph = ws / f"{session_id}.graph.npz"
    csv = ws / f"{session_id}.csv"
    unverified = []

    if refresh or not spec.exists():
        run(["uv", "run", "tools/eeg-viz/workspace3d.py", eeg,
             "--export", spec, "--session-id", session_id], repos["client_native"])
    if refresh or not lat.exists():
        transcribe_csv(eeg, csv)
        run(["uv", "run", "scripts/latent3d.py", "export", csv,
             "--session-id", session_id, "-o", lat], repos["lejepa"])
    anchor = utc_anchor(base_dir, session_id)
    if refresh or not graph.exists():
        cmd = ["uv", "run", "tools/graph3d.py", "--export", graph, "--session-id", session_id]
        if anchor:
            cmd += ["--utc-start", anchor[0], "--utc-end", anchor[1]]
        run(cmd, repos["memory_server"])
    if not anchor:
        unverified.append("graph time-range vs session (no capture manifest anchor; "
                          "locator match only)")
    manifest = {
        "session_id": session_id,
        "eeg_sha256": sha256_of(eeg),
        "csv_sha256": sha256_of(csv) if csv.exists() else None,
        "csv_note": "mechanical transcription of the capture payload; recorded values only",
    }
    (ws / "manifest.json").write_text(json.dumps(manifest, indent=1))
    return spec, lat, graph, unverified


def load_artifacts(spec_path, lat_path, graph_path):
    spec = np.load(spec_path)
    lat = np.load(lat_path)
    graph = np.load(graph_path)
    spec_meta = json.loads(str(spec["meta_json"]))
    graph_meta = json.loads(str(graph["meta_json"]))
    return spec, spec_meta, lat, graph, graph_meta


def gate(spec_meta, lat, graph_meta):
    """Refuse on positive disagreement; return notes for checks that are impossible."""
    ids = {
        "spectrogram": spec_meta["session_id"],
        "latents": str(lat["session_id"]),
        "graph": graph_meta["session_id"],
    }
    if len(set(ids.values())) != 1:
        sys.exit("REFUSING to render: session ids disagree: "
                 + ", ".join(f"{k}={v}" for k, v in ids.items()))
    t0, t1 = spec_meta["time_range"]
    wt = lat["window_times"]
    if len(wt) and (wt.min() < t0 - TIME_TOLERANCE_S or wt.max() > t1 + TIME_TOLERANCE_S):
        sys.exit(f"REFUSING to render: latent window times [{wt.min():.1f}, {wt.max():.1f}] s "
                 f"fall outside the EEG time range [{t0:.1f}, {t1:.1f}] s")


def render(spec, spec_meta, lat, graph, graph_meta, stamps, unverified, screenshot=None):
    import pyvista as pv

    off = screenshot is not None
    pl = pv.Plotter(shape=(1, 3), off_screen=off, window_size=(2200, 900))

    # Panel 0: spectrogram surfaces, channels stacked along y (recorded grid, drawn as-is)
    pl.subplot(0, 0)
    f = spec["freqs"]
    t = spec["times"]
    power = spec["power"]
    lo, hi = float(power.min()), float(power.max())
    span = float(f[-1] - f[0]) or 1.0
    zscale = 0.25 * span / max(hi - lo, 1e-12)
    tg, fg = np.meshgrid(t, f)
    for i, name in enumerate(CHANNELS):
        yoff = i * span * 1.2
        grid = pv.StructuredGrid(tg, fg + yoff, (power[i] - lo) * zscale)
        grid["log10 power"] = power[i].ravel(order="F")
        pl.add_mesh(grid, scalars="log10 power", cmap="viridis", clim=(lo, hi),
                    show_scalar_bar=False)
        pl.add_point_labels([(float(t[0]), float(f[-1]) + yoff, (hi - lo) * zscale)], [name],
                            font_size=10, shape=None, always_visible=True, show_points=False)
        for v in spec_meta["verdicts"]:
            if v["channel"] != name or v["t"] is None:
                continue
            color = "green" if v["status"] == "healthy" else "red"
            pl.add_mesh(pv.Line((v["t"], f[0] + yoff, (hi - lo) * zscale),
                                (v["t"], f[-1] + yoff, (hi - lo) * zscale)),
                        color=color, line_width=3)
    pl.add_text("EEG  time x freq x power (exported grid)", font_size=10)
    # display scale only (axis labels stay in data units): keep the time span
    # readable against 4 stacked frequency ranges, whatever the session length
    tspan = float(t[-1] - t[0]) or 1.0
    pl.set_scale(xscale=(span * 4 * 1.2) / tspan * 0.8)
    pl.show_grid(xtitle="time (s)", ytitle="freq (Hz, stacked/channel)", ztitle="", font_size=8)

    # Panel 1: latent trajectory (exported projection; view computes nothing)
    pl.subplot(0, 1)
    pl.add_points(np.asarray(lat["proj_control"]), color="#bbbbbb", point_size=6,
                  render_points_as_spheres=True)
    pl.add_points(np.asarray(lat["proj_real"]), scalars=np.asarray(lat["window_times"]),
                  cmap="viridis", point_size=11, render_points_as_spheres=True,
                  scalar_bar_args={"title": "window start (s)", "label_font_size": 8,
                                   "title_font_size": 9})
    pl.add_text("latents  (grey: shuffle control)", font_size=10)
    pl.add_text(
        f"PC1-3 carry {float(lat['evr_pc13_pct']):.1f}% of variance\n"
        f"encoder: {lat['encoder_identity']}\n"
        "derived from the EEG panel -- agreement is arithmetic, not evidence",
        position="lower_left", font_size=6)

    # Panel 2: provenance neighbourhood (exported layout)
    pl.subplot(0, 2)
    n_nodes = len(graph["digests"])
    if n_nodes == 0:
        pl.add_text("0 session-linked claims\n(no live ingestion path from neuralcompose;\n"
                    "an honest empty, not an error)", position=(0.22, 0.55), viewport=True,
                    font_size=10)
    else:
        pos = np.asarray(graph["positions"])
        classes = [str(c) for c in graph["evidence_class"]]
        seeds = np.asarray(graph["seed_mask"])
        for cls in CLASS_COLORS:
            sel = np.array([c == cls for c in classes])
            if sel.any():
                pl.add_mesh(pv.PolyData(pos[sel]), color=CLASS_COLORS[cls],
                            render_points_as_spheres=True, point_size=12,
                            label=f"{cls} ({int(sel.sum())})")
        if seeds.any():
            pl.add_mesh(pv.PolyData(pos[seeds]), color="black", style="points",
                        point_size=20, render_points_as_spheres=False,
                        label=f"session-created ({int(seeds.sum())})")
        edges = np.asarray(graph["edges"])
        if len(edges):
            pts = pos[edges.ravel()]
            lines = np.hstack([[2, 2 * i, 2 * i + 1] for i in range(len(edges))])
            pl.add_mesh(pv.PolyData(pts, lines=lines), color="#9e9e9e", line_width=1.5)
        pl.add_legend(bcolor="white", size=(0.4, 0.25), loc="upper right")
    pl.add_text(f"claims  ({graph_meta['seed_count']} session-created, 1-hop neighbourhood)",
                font_size=10)
    pl.add_text("nothing records causation between a spectral event and a banked claim;\n"
                "temporal coincidence across panels is coincidence",
                position="lower_edge", font_size=6)

    pl.subplot(0, 0)
    banner = "navigation aid, not an analysis tool -- no finding is licensed by what it shows\n"
    banner += "\n".join(f"{k} sha256 {v}" for k, v in stamps.items())
    for u in unverified:
        banner += f"\nunverified: {u}"
    pl.add_text(banner, position="lower_left", font_size=6)

    if off:
        pl.screenshot(screenshot)
        pl.close()
        return screenshot
    pl.show()


def _fixture(dirp, sid_spec="s1", sid_lat="s1", sid_graph="s1"):
    """Three tiny consistent artifacts with the real exporters' schemas."""
    f = np.linspace(0, 128, 20)
    t = np.linspace(0.0, 8.0, 10)
    np.savez(dirp / "spec.npz",
             power=np.random.default_rng(0).random((4, 20, 10)).astype(np.float32),
             freqs=f, times=t,
             meta_json=json.dumps({
                 "session_id": sid_spec, "time_range": [0.0, 8.0], "channels": CHANNELS,
                 "nominal_hz": 256.0, "actual_hz": 256.0, "sources": {},
                 "verdicts": [{"turn": 0, "channel": "TP9", "t": 8.0,
                               "status": "healthy", "verdict": "ok", "mainsLineHz": None}],
                 "tool": "fixture"}))
    n = 5
    np.savez(dirp / "lat.npz", real=np.zeros((n, 128), np.float32),
             control=np.zeros((n, 128), np.float32),
             proj_real=np.random.default_rng(1).random((n, 3)),
             proj_control=np.random.default_rng(2).random((n, 3)),
             evr_pc13_pct=61.5, window_times=np.linspace(0, 4, n),
             session_id=sid_lat, encoder_identity="random-init(seed=0)",
             csv_sha256="0" * 64, window_length=1024, patch_size=64, n_windows=n,
             sample_rate_hz=256.0, sample_rate_diagnostics="{}")
    np.savez(dirp / "graph.npz", positions=np.zeros((0, 3)), digests=np.array([], dtype=str),
             evidence_class=np.array([], dtype=str), claim_prefixes=np.array([], dtype=str),
             edges=np.zeros((0, 2), np.int64), edge_kinds=np.array([], dtype=str),
             seed_mask=np.array([], dtype=bool),
             meta_json=json.dumps({"session_id": sid_graph, "utc_range": None,
                                   "db_copy_sha256": "0" * 64, "snapshot_utc": "fixture",
                                   "store_counts": {}, "seed_count": 0, "tool": "fixture"}))
    return dirp / "spec.npz", dirp / "lat.npz", dirp / "graph.npz"


def self_check():
    import tempfile

    with tempfile.TemporaryDirectory() as d:
        dirp = Path(d)
        spec_p, lat_p, graph_p = _fixture(dirp)
        spec, spec_meta, lat, graph, graph_meta = load_artifacts(spec_p, lat_p, graph_p)
        gate(spec_meta, lat, graph_meta)
        out = dirp / "out.png"
        stamps = {"spectrogram": sha256_of(spec_p), "latents": sha256_of(lat_p),
                  "graph": sha256_of(graph_p)}
        render(spec, spec_meta, lat, graph, graph_meta, stamps,
               ["fixture unverified note"], screenshot=str(out))
        assert out.stat().st_size > 0
        # the refusal must fire on a session-id mismatch
        _fixture(dirp, sid_lat="OTHER")
        spec, spec_meta, lat, graph, graph_meta = load_artifacts(spec_p, lat_p, graph_p)
        try:
            gate(spec_meta, lat, graph_meta)
        except SystemExit as e:
            assert "session ids disagree" in str(e), e
        else:
            raise AssertionError("gate did not refuse mismatched session ids")
        # and on latent times outside the EEG range
        _fixture(dirp)
        np.savez(dirp / "lat.npz", **{**dict(np.load(lat_p).items()),
                                      "window_times": np.array([0.0, 50.0])})
        spec, spec_meta, lat, graph, graph_meta = load_artifacts(spec_p, lat_p, graph_p)
        try:
            gate(spec_meta, lat, graph_meta)
        except SystemExit as e:
            assert "outside the EEG time range" in str(e), e
        else:
            raise AssertionError("gate did not refuse out-of-range latent times")
    print("self-check ok")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("session_id", nargs="?")
    ap.add_argument("--dir", type=Path, default=DEFAULT_DIR, help="session artifact directory")
    ap.add_argument("--refresh", action="store_true", help="re-run the three exports")
    ap.add_argument("--screenshot", metavar="OUT.png")
    ap.add_argument("--self-check", action="store_true")
    for k, v in REPOS.items():
        ap.add_argument(f"--{k.replace('_', '-')}", type=Path, default=v)
    args = ap.parse_args()
    if args.self_check:
        self_check()
        return
    if not args.session_id:
        ap.error("session_id required (or --self-check)")
    repos = {k: getattr(args, k) for k in REPOS}
    spec_p, lat_p, graph_p, unverified = ensure_artifacts(
        args.session_id, args.dir, repos, args.refresh)
    spec, spec_meta, lat, graph, graph_meta = load_artifacts(spec_p, lat_p, graph_p)
    gate(spec_meta, lat, graph_meta)
    stamps = {"spectrogram": sha256_of(spec_p), "latents": sha256_of(lat_p),
              "graph": sha256_of(graph_p)}
    out = render(spec, spec_meta, lat, graph, graph_meta, stamps, unverified,
                 screenshot=args.screenshot)
    if out:
        print(out)


if __name__ == "__main__":
    main()
