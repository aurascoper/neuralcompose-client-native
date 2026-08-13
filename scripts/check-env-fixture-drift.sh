#!/usr/bin/env bash
# Re-reads WorldModel/env.py and checks the conformance fixture was generated
# from the version it claims.
#
# WHY THIS EXISTS AT ALL. The fixture is generated FROM env.py, so Python is the
# reference by construction rather than by test. If env.py changes, the fixture
# goes stale and BOTH the Rust and the Swift conformance tests keep passing
# against an obsolete reference, with nothing detecting it. Three conformant
# implementations make that drift LESS likely to be noticed, not more — the
# agreement is reassuring precisely when it has stopped meaning anything.
#
# FAILS when the sibling checkout is absent. It does not skip, for the same
# reason the conformance test does not.
set -euo pipefail
cd "$(dirname "$0")/.."

FIXTURE=crates/neuralcompose-hypnagogic/tests/fixtures/env_v1.json
SRC_DIR="${NEURALCOMPOSE_DIR:-$HOME/src/NeuralCompose}"

field() { python3 -c "import json,sys;print(json.load(open('$FIXTURE'))['source'][sys.argv[1]] or '')" "$1"; }

if [ ! -f "$FIXTURE" ]; then
  echo "env-fixture drift: FAILED — $FIXTURE is missing" >&2
  exit 1
fi
REL="$(field path)"; PINNED="$(field fileSha256)"; COMMIT="$(field commit)"
ENV_PY="$SRC_DIR/$REL"

if [ ! -f "$ENV_PY" ]; then
  echo "env-fixture drift: FAILED — no $ENV_PY" >&2
  echo "  Set NEURALCOMPOSE_DIR, or clone it. This check does not skip." >&2
  exit 1
fi

ACTUAL="$(sha256sum "$ENV_PY" | cut -d' ' -f1)"
if [ "$ACTUAL" != "$PINNED" ]; then
  echo "env-fixture drift: FAILED — $REL has changed since the fixture was generated" >&2
  echo "  pinned $PINNED" >&2
  echo "  actual $ACTUAL" >&2
  echo "  The Rust AND Swift conformance tests are both still green against a stale" >&2
  echo "  reference. Regenerate:" >&2
  echo "    ~/src/NeuralCompose/.venv-muse/bin/python \\" >&2
  echo "        tools/worldmodel-fixture/generate.py > $FIXTURE" >&2
  echo "  then re-run the Swift suite in the NeuralCompose worktree." >&2
  exit 1
fi

echo "env-fixture drift: clean ($REL matches ${PINNED:0:12}…${COMMIT:+, pinned at ${COMMIT:0:12}})"
