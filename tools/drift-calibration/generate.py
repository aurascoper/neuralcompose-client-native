#!/usr/bin/env python3
"""Emit the drift-ceiling calibration fixture from two live runs.

The ceiling is a property of the EMBEDDER, not of the loop, so it cannot be
reasoned out — it has to be measured against the model that will actually run.
An earlier default of 0.45 was picked from the theoretical 0..1 range and from a
test embedder whose vectors were orthogonal; real sentence embeddings sit in a
much narrower cone, and 0.45 fired on none of these twenty turns.

Reproduce both runs first (llama-server on 8080 with the generation model):

    LD_LIBRARY_PATH=$LLAMA/build-cpu/bin \
    NC_EMBED_MODEL=~/models/bge-small-en-v1.5-f32.gguf \
      ./target/debug/neuralcompose-hypnagogic --mode reflective --turns 11 --json \
      < tools/drift-calibration/on-topic.txt  > /tmp/on.jsonl
    ... same with off-topic.txt > /tmp/off.jsonl

Then:

    python3 tools/drift-calibration/generate.py \
        bge-small-en-v1.5-f32 /tmp/on.jsonl /tmp/off.jsonl \
        > crates/neuralcompose-hypnagogic/tests/fixtures/drift_calibration_v1.json
"""
import datetime
import json
import pathlib
import sys


def drifts(path: pathlib.Path) -> list:
    out = []
    for line in path.read_text().splitlines():
        if not line.startswith("{"):
            continue
        row = json.loads(line)
        if row.get("topicDrift") is not None:
            out.append({"index": row["index"], "heard": row["heard"], "drift": row["topicDrift"]})
    return out


def main() -> int:
    embedder, on_path, off_path = sys.argv[1], pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
    on, off = drifts(on_path), drifts(off_path)
    on_max = max(c["drift"] for c in on)
    off_min = min(c["drift"] for c in off)
    json.dump(
        {
            "schemaId": "neuralcompose.hypnagogic.drift-calibration.v1",
            "source": {
                "embedder": embedder,
                # From the INPUT file, not from `on[0]`: turn 0 has no drift
                # (it IS the anchor) and so is filtered out of `on` entirely.
                # Reading it from the parsed rows silently reports the second
                # utterance as the anchor.
                "anchor": pathlib.Path("tools/drift-calibration/on-topic.txt")
                .read_text()
                .splitlines()[0],
                "inputs": [
                    "tools/drift-calibration/on-topic.txt",
                    "tools/drift-calibration/off-topic.txt",
                ],
                "measuredOn": datetime.date.today().isoformat(),
            },
            "note": (
                "Twenty live turns through the real embedder, one anchor. Thin, "
                "and stated as thin: it establishes that the two classes separate "
                "and roughly where, not a robust threshold. A different "
                "NC_EMBED_MODEL invalidates every number here."
            ),
            "separation": {
                "onTopicMax": on_max,
                "offTopicMin": off_min,
                "midpoint": (on_max + off_min) / 2,
            },
            "onTopic": on,
            "offTopic": off,
        },
        sys.stdout,
        indent=2,
    )
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
