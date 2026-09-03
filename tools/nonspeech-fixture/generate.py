#!/usr/bin/env python3
"""Emit the non-speech-guard fixture from a real session log.

The labels come from `tools/spoken-loop/converse.py`'s OWN regex, not from a
reading of the Rust. A fixture labelled by a second implementation of the same
prose would agree with the port by construction and prove nothing — the same
argument `tests/env_conformance.rs` makes about `env_v1.json`.

Usage:
    python3 tools/nonspeech-fixture/generate.py \
        ~/Documents/NeuralCompose/InteractionLogs/session-1788413094.turns.jsonl \
        > crates/neuralcompose-hypnagogic/tests/fixtures/nonspeech_v1.json
"""
import datetime
import hashlib
import json
import pathlib
import re
import sys

# Verbatim from converse.py:404. Do not "improve" it: it is the reference.
NON_SPEECH = re.compile(r"[\[(][^\])]*[\])]")


def is_non_speech(heard: str) -> bool:
    return not heard or not NON_SPEECH.sub("", heard).strip()


def main() -> int:
    log = pathlib.Path(sys.argv[1]).expanduser()
    raw = log.read_bytes()
    rows = [json.loads(l) for l in raw.decode().splitlines() if l.strip()]

    cases = [
        {"index": r["index"], "heard": r["heard"], "nonSpeech": is_non_speech(r["heard"])}
        for r in rows
    ]

    longest = run = 0
    for c in cases:
        run = run + 1 if c["nonSpeech"] else 0
        longest = max(longest, run)

    json.dump(
        {
            "schemaId": "neuralcompose.hypnagogic.nonspeech-guard.v1",
            "source": {
                "path": str(log).replace(str(pathlib.Path.home()), "~"),
                "fileSha256": hashlib.sha256(raw).hexdigest(),
                "mtime": datetime.datetime.fromtimestamp(
                    log.stat().st_mtime, datetime.timezone.utc
                ).isoformat(),
                "labelledBy": "tools/spoken-loop/converse.py:404",
                "readOn": datetime.date.today().isoformat(),
            },
            "note": (
                "Every heard value from the session, labelled by the Python "
                "reference's own guard. The speech cases matter as much as the "
                "non-speech ones: a guard that swallowed real utterances would "
                "pass a fixture holding only the noise. The source was an "
                "open-ended session (--turns 0) still appending when it was "
                "read, so a regeneration yielding more cases and a different "
                "digest is expected, not corruption."
            ),
            "coverage": {
                "turns": len(cases),
                "nonSpeech": sum(c["nonSpeech"] for c in cases),
                "speech": sum(not c["nonSpeech"] for c in cases),
                "longestNonSpeechRun": longest,
            },
            "cases": cases,
        },
        sys.stdout,
        indent=2,
    )
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
