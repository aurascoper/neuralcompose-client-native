#!/usr/bin/env python3
"""Freeze a round's per-reply scores as committed evidence.

Generation is sampled, so re-running `run.sh` produces different replies and
cannot reproduce a round's numbers. The per-reply scores are therefore the
PRIMARY record, not a cache of something recoverable — which is why they are
committed and the ~700K of raw turn logs are not.

Usage: python3 distill.py <round-dir> <label> > results/<label>.json
"""
import glob
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from score import TERMS, cosine, embed, replies  # noqa: E402


def main() -> int:
    d, label = sys.argv[1], sys.argv[2]
    here = os.path.dirname(os.path.abspath(__file__))
    seed = (
        open(os.path.join(here, "..", "drift-calibration", "off-topic.txt"))
        .read()
        .splitlines()[0]
    )
    seed_vec = embed(seed)

    runs = {}
    for path in sorted(glob.glob(os.path.join(d, "*.jsonl"))):
        arm = os.path.basename(path).replace(".jsonl", "")
        rows = []
        for row in replies(path):
            text = row["spokenText"]
            rows.append(
                {
                    "index": row["index"],
                    "heard": row["heard"],
                    "reanchored": row.get("reanchored"),
                    "cosineToSeed": cosine(embed(text), seed_vec),
                    "mentionsSubject": any(t in text.lower() for t in TERMS),
                    "spokenText": text,
                }
            )
        runs[arm] = rows

    json.dump(
        {
            "schemaId": "neuralcompose.hypnagogic.reanchor-efficacy.v1",
            "label": label,
            "seed": seed,
            "scoringEmbedder": "nomic-embed-text-v1.5 (:8082)",
            "note": (
                "Scored with a DIFFERENT embedder from the bge-small that gates "
                "the mechanism: one model both firing the guard and scoring the "
                "result could manufacture a finding from its own geometry. "
                "Turn 0 is the anchor and is excluded from every arm."
            ),
            "runs": runs,
        },
        sys.stdout,
        indent=2,
    )
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
