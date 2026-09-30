#!/usr/bin/env python3
"""Emit every session-log fixture from ONE read of ONE log.

Both fixtures pin the same source file. Generating them from separate scripts
would let the two `fileSha256` values drift apart -- especially against a live
`--turns 0` session, which is exactly how the source for these was captured. So
they are emitted together, from a single read, and the shared digest is a
property of the code rather than a rule someone has to remember.

Usage:
    python3 tools/session-fixtures/generate.py \
        ~/Documents/NeuralCompose/InteractionLogs/session-1788413094.turns.jsonl \
        crates/neuralcompose-hypnagogic/tests/fixtures
"""
import datetime
import hashlib
import json
import pathlib
import re
import sys

# Verbatim from converse.py:404. Do not "improve" it: it is the reference the
# nonspeech fixture is labelled by.
NON_SPEECH = re.compile(r"[\[(][^\])]*[\])]")


def is_non_speech(heard: str) -> bool:
    return not heard or not NON_SPEECH.sub("", heard).strip()


def source_block(log: pathlib.Path, raw: bytes) -> dict:
    return {
        "path": str(log).replace(str(pathlib.Path.home()), "~"),
        "fileSha256": hashlib.sha256(raw).hexdigest(),
        "mtime": datetime.datetime.fromtimestamp(
            log.stat().st_mtime, datetime.timezone.utc
        ).isoformat(),
        "readOn": datetime.date.today().isoformat(),
        "writerLive": False,
        "writerNote": (
            "The session (PID 103860, --mode reflective --turns 0 --mic) had "
            "ended and the file had stopped growing before this read. An "
            "earlier pin of this same path was taken at 55 turns while the "
            "writer was still appending; that is why a committed digest for "
            "this path moved once."
        ),
    }


def nonspeech_fixture(rows: list, src: dict) -> dict:
    cases = [
        {"index": r["index"], "heard": r["heard"], "nonSpeech": is_non_speech(r["heard"])}
        for r in rows
    ]
    longest = run = 0
    for c in cases:
        run = run + 1 if c["nonSpeech"] else 0
        longest = max(longest, run)
    return {
        "schemaId": "neuralcompose.hypnagogic.nonspeech-guard.v1",
        "source": src,
        "note": (
            "Every heard value from the session, labelled by the Python "
            "reference's own guard (converse.py:404). The speech cases matter "
            "as much as the non-speech ones: a guard that swallowed real "
            "utterances would pass a fixture holding only the noise."
        ),
        "labelledBy": "tools/spoken-loop/converse.py:404",
        "coverage": {
            "turns": len(cases),
            "nonSpeech": sum(c["nonSpeech"] for c in cases),
            "speech": sum(not c["nonSpeech"] for c in cases),
            "longestNonSpeechRun": longest,
        },
        "cases": cases,
    }


def repetition_fixture(rows: list, src: dict) -> dict:
    """The repetition corpus.

    Unlike the nonspeech fixture there is NO reference implementation to label
    against -- nothing upstream classifies a turn as repetitive. So this fixture
    pins the raw replies plus the two REGIONS, and the region boundary is a human
    judgement recorded here with its reason rather than a computed label. The
    test asserts behaviour across those regions; it never asserts the floor.
    """
    healthy = [r["index"] for r in rows if not is_non_speech(r["heard"])]
    stuck = [r["index"] for r in rows if r["index"] >= 14]
    return {
        "schemaId": "neuralcompose.hypnagogic.repetition-floor.v1",
        "source": src,
        "note": (
            "Replays a session the non-speech guard now prevents. That is the "
            "point: non-speech tags are not the only route to a fixed point, "
            "and a genuinely stuck conversation with real input would look the "
            "same to this detector while the tag filter would not help. "
            "selfSimilarity is carried alongside because it is the metric that "
            "was ALREADY computed and logged here and does NOT separate these "
            "regions -- see the tables in the plan. Keeping it in the corpus "
            "lets the next reader re-derive that rejection instead of taking "
            "it on trust."
        ),
        "regions": {
            "healthy": healthy,
            "stuck": stuck,
            "rationale": (
                "healthy = the turns whose input was real speech (1-13, minus "
                "the two noise turns 0 and 7 interleaved among them). stuck = "
                "turn 14 onward, an unbroken run of room noise in which the "
                "same two utterances recur near-verbatim. 'healthy' is "
                "generous: those turns were themselves drifting, so they are "
                "the good region only by comparison, and any floor fitted to "
                "them is fitted to a degrading reference."
            ),
        },
        "coverage": {
            "turns": len(rows),
            "healthy": len(healthy),
            "stuck": len(stuck),
        },
        "cases": [
            {
                "index": r["index"],
                "heard": r["heard"],
                "spokenText": r.get("spokenText"),
                "selfSimilarity": r.get("selfSimilarity"),
            }
            for r in rows
        ],
    }


def main() -> int:
    log = pathlib.Path(sys.argv[1]).expanduser()
    out = pathlib.Path(sys.argv[2])
    raw = log.read_bytes()
    rows = [json.loads(l) for l in raw.decode().splitlines() if l.strip()]
    src = source_block(log, raw)

    for name, doc in (
        ("nonspeech_v1.json", nonspeech_fixture(rows, src)),
        ("repetition_v1.json", repetition_fixture(rows, src)),
    ):
        path = out / name
        path.write_text(json.dumps(doc, indent=2) + "\n")
        print(f"{path}  ({doc['coverage']['turns']} turns)", file=sys.stderr)
    print(f"both pinned to {src['fileSha256']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
