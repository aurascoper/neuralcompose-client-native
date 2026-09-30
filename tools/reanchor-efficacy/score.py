#!/usr/bin/env python3
"""Score the re-anchoring A/B. Criterion is fixed in PREREGISTRATION.md.

Usage: python3 score.py <output-dir-from-run.sh>

Embeds with nomic-embed-text-v1.5 on :8082 -- deliberately NOT bge-small, which
is the model that gates the mechanism. If the same model both fired the guard
and scored the result, a positive finding could be that model's geometry rather
than a change in the text.
"""
import glob
import json
import math
import os
import statistics
import sys
import urllib.request

EMBED_URL = "http://127.0.0.1:8082/embedding"
TERMS = ("radiotropic", "biofilm", "biofilms", "microbial", "radiation")


def embed(text: str) -> list:
    req = urllib.request.Request(
        EMBED_URL,
        data=json.dumps({"content": text}).encode(),
        headers={"Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=60) as r:
        payload = json.load(r)
    v = payload[0]["embedding"]
    # llama-server returns [[...]] for a single input.
    while isinstance(v[0], list):
        v = v[0]
    return v


def cosine(a: list, b: list) -> float:
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(x * x for x in b))
    return dot / (na * nb) if na and nb else 0.0


def replies(path: str) -> list:
    out = []
    for line in open(path):
        if not line.startswith("{"):
            continue
        row = json.loads(line)
        # Turn 0 is the anchor; it is not a drifted turn and is excluded.
        if row["index"] == 0:
            continue
        if row.get("spokenText"):
            out.append(row)
    return out


def main() -> int:
    d = sys.argv[1]
    seed = open(
        os.path.join(
            os.path.dirname(os.path.abspath(__file__)), "..", "drift-calibration", "off-topic.txt"
        )
    ).read().splitlines()[0]
    seed_vec = embed(seed)
    print(f"seed: {seed!r}\n")

    def score_run(path):
        sims, hits, n = [], 0, 0
        for row in replies(path):
            t = row["spokenText"]
            sims.append(cosine(embed(t), seed_vec))
            low = t.lower()
            if any(term in low for term in TERMS):
                hits += 1
            n += 1
        return (statistics.mean(sims) if sims else float("nan")), hits, n

    arms = {}
    for arm in ("framed", "unframed"):
        per_run, total_hits, total_n = [], 0, 0
        for p in sorted(glob.glob(os.path.join(d, f"{arm}-*.jsonl"))):
            m, h, n = score_run(p)
            per_run.append(m)
            total_hits += h
            total_n += n
            print(f"  {os.path.basename(p):18} mean={m:.4f}  on-subject {h}/{n}")
        arms[arm] = (per_run, total_hits, total_n)
        print()

    fr, un = arms["framed"][0], arms["unframed"][0]
    d_eff = statistics.mean(fr) - statistics.mean(un)
    # Pooled within-arm SD of per-run means.
    s = math.sqrt(
        (statistics.variance(fr) + statistics.variance(un)) / 2
    ) if len(fr) > 1 and len(un) > 1 else float("nan")

    print("=== PRIMARY: reply-to-seed cosine (nomic) ===")
    print(f"framed    mean {statistics.mean(fr):.4f}   per-run {[f'{x:.4f}' for x in fr]}")
    print(f"unframed  mean {statistics.mean(un):.4f}   per-run {[f'{x:.4f}' for x in un]}")
    print(f"\nd = {d_eff:+.4f}   pooled within-arm SD s = {s:.4f}")
    if d_eff > s:
        verdict = "EFFECT: framing pulls replies back (d > s)"
    elif d_eff < -s:
        verdict = "REVERSE EFFECT: framing pushes replies away (d < -s) -- reconsider §2"
    else:
        verdict = "NO DETECTABLE EFFECT at this sample size (|d| <= s). NOT 'framing does not work'."
    print(f"VERDICT: {verdict}")

    print("\n=== SECONDARY: replies mentioning the subject ===")
    for arm in ("framed", "unframed"):
        _, h, n = arms[arm]
        print(f"  {arm:9} {h}/{n}")

    print("\n=== POSITIVE CONTROL (on-topic input) ===")
    ctl = {}
    for tag in ("control-framed", "control-unframed"):
        p = os.path.join(d, f"{tag}.jsonl")
        if os.path.exists(p):
            m, h, n = score_run(p)
            ctl[tag] = m
            print(f"  {tag:18} mean={m:.4f}  on-subject {h}/{n}")
    if ctl:
        best_off = max(statistics.mean(fr), statistics.mean(un))
        worst_ctl = min(ctl.values())
        print(f"\n  worst on-topic {worst_ctl:.4f}  vs  best off-topic {best_off:.4f}")
        if worst_ctl > best_off:
            print("  MEASURE IS ALIVE: on-topic replies outscore off-topic ones.")
        else:
            print("  *** MEASURE IS DEAD. The primary result above is UNINTERPRETABLE. ***")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
