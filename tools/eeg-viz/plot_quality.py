# /// script
# requires-python = ">=3.11"
# dependencies = ["matplotlib"]
# ///
"""Render a session's recorded EEG artifacts as one quality figure.

Draws ONLY what the Rust core recorded: raw samples from `<id>.eeg.jsonl`
(wire-time axis) and the per-turn channel-health records from
`<sid>.turns.jsonl` (rmsMicrovolts, mainsPower, status, verdict — anchored at
each window's lastSourceTimestamp on the same axis). Nothing is recomputed
here: if this picture disagrees with a logged verdict, one of the two has a
bug, and that is the point.

Usage:  uv run plot_quality.py SESSION.eeg.jsonl [SESSION.turns.jsonl] [-o out.png]
        uv run plot_quality.py --self-check
"""

import argparse
import json
import sys
from pathlib import Path

CHANNELS = ["TP9", "AF7", "AF8", "TP10"]  # frozen wire order (contracts/constants.json)
STATUS_COLORS = {"healthy": "tab:green"}  # anything else draws red: it needs eyes


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


def render(samples, health, title, out_path):
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    t = [s[0] for s in samples]
    fig, axes = plt.subplots(len(CHANNELS), 1, sharex=True, figsize=(14, 9))
    for i, (ax, name) in enumerate(zip(axes, CHANNELS)):
        ax.plot(t, [s[1][i] for s in samples], lw=0.4, color="tab:blue")
        ax.set_ylabel(f"{name}\nµV")
        for turn_idx, channel, ts, rec in health:
            if channel != name or ts is None:
                continue
            status = rec["annotation"]["status"]
            color = STATUS_COLORS.get(status, "tab:red")
            ax.axvline(ts, color=color, lw=1, alpha=0.7)
            label = (
                f"t{turn_idx} {rec['annotation']['verdict']}\n"
                f"rms {rec['derived']['rmsMicrovolts']:.1f}"
            )
            mains = rec["derived"].get("mainsPower")
            if mains is not None:
                label += f"\nmains {mains:.2g}"
            ax.annotate(
                label, (ts, ax.get_ylim()[1]), fontsize=6, color=color,
                ha="left", va="top", rotation=0, annotation_clip=True,
            )
    axes[-1].set_xlabel("wire time (s since stream start)")
    fig.suptitle(f"{title} — recorded values only, nothing recomputed")
    fig.tight_layout()
    fig.savefig(out_path, dpi=130)
    return out_path


def self_check():
    # Two capture lines, three samples, one turn record: parse and assert exact values.
    cap = [
        {"sequence": 1, "receivedAtMonotonicMs": 0, "acceptedSampleCount": 2,
         "payload": json.dumps([
             {"timestamp": 0.0, "channels": [1.0, 2.0, 3.0, 4.0]},
             {"timestamp": 0.5, "channels": [5.0, 6.0, 7.0, 8.0]},
         ])},
        {"sequence": 2, "receivedAtMonotonicMs": 4, "acceptedSampleCount": 1,
         "payload": json.dumps([{"timestamp": 1.0, "channels": [9.0, 10.0, 11.0, 12.0]}])},
    ]
    turn = {"index": 3, "channelHealth": [{
        "channel": "AF7",
        "window": {"sampleCount": 3, "lastSourceTimestamp": 1.0},
        "derived": {"rmsMicrovolts": 6.5, "mainsPower": None},
        "annotation": {"status": "healthy", "verdict": "ok", "mainsLineHz": None},
    }]}
    import tempfile

    with tempfile.TemporaryDirectory() as d:
        eeg = Path(d) / "s.eeg.jsonl"
        turns = Path(d) / "s.turns.jsonl"
        eeg.write_text("".join(json.dumps(l) + "\n" for l in cap))
        turns.write_text(json.dumps(turn) + "\n")
        samples = read_capture(eeg)
        assert len(samples) == 3 and samples[0] == (0.0, [1.0, 2.0, 3.0, 4.0]), samples
        assert samples[2][1][3] == 12.0
        health = read_turn_health(turns)
        assert len(health) == 1
        idx, ch, ts, rec = health[0]
        assert (idx, ch, ts) == (3, "AF7", 1.0)
        assert rec["derived"]["rmsMicrovolts"] == 6.5
        assert rec["derived"]["mainsPower"] is None  # None must survive, never become 0.0
        out = render(samples, health, "self-check", Path(d) / "out.png")
        assert out.stat().st_size > 0
    print("self-check ok")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("eeg", nargs="?", help="<id>.eeg.jsonl capture payload")
    ap.add_argument("turns", nargs="?", help="<sid>.turns.jsonl (default: sibling of eeg)")
    ap.add_argument("-o", "--out", help="output PNG (default: <id>.quality.png beside input)")
    ap.add_argument("--self-check", action="store_true")
    args = ap.parse_args()
    if args.self_check:
        self_check()
        return
    if not args.eeg:
        ap.error("eeg path required (or --self-check)")
    eeg = Path(args.eeg)
    turns = Path(args.turns) if args.turns else eeg.with_name(
        eeg.name.replace(".eeg.jsonl", ".turns.jsonl")
    )
    samples = read_capture(eeg)
    if not samples:
        sys.exit(f"{eeg}: no samples")
    health = read_turn_health(turns) if turns.exists() else []
    if not turns.exists():
        print(f"note: {turns} not found; waveforms only", file=sys.stderr)
    out = Path(args.out) if args.out else eeg.with_name(
        eeg.name.replace(".eeg.jsonl", ".quality.png")
    )
    render(samples, health, eeg.stem.replace(".eeg", ""), out)
    print(out)


if __name__ == "__main__":
    main()
