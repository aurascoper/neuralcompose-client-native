#!/usr/bin/env bash
# Re-reads neural-memory-server's EvidenceClass enum and checks it still matches
# the list this repository's provenance mapping was built against (ADR-003).
#
# THIS IS DATED EVIDENCE PLUS A CONSISTENCY GATE, NOT A LIVE CHECK. `cargo test`
# here cannot see the sibling repository, and there is no runtime oracle either:
# its MCP surface exposes no tool returning the enum, because
# crates/neural-memory-store/src/write.rs clamps every agent write to
# agentInference before the digest is computed. So the mapping is bound in CI to
# a committed fixture, and this script is what re-derives that fixture from
# source on demand. Run it before changing the mapping in provenance.rs.
#
# It FAILS when the sibling checkout is absent. It does not skip: a check that
# passes when its ground truth is missing reports green for exactly the state it
# exists to detect (the argument tests/dialectic_conformance.rs already makes).
set -euo pipefail
cd "$(dirname "$0")/.."

RECORD=contracts/provenance/fixtures/evidence-class-names.json
SERVER_DIR="${NEURAL_MEMORY_SERVER_DIR:-$HOME/src/neural-memory-server}"
jq_field() { python3 -c "import json,sys;d=json.load(open('$RECORD'));print(d['upstream'][sys.argv[1]])" "$1"; }
TERMS="$SERVER_DIR/$(jq_field path)"

if [ ! -d "$SERVER_DIR" ]; then
  echo "evidence-class drift: FAILED — no checkout at $SERVER_DIR" >&2
  echo "  Set NEURAL_MEMORY_SERVER_DIR, or clone it. This check does not skip." >&2
  exit 1
fi
if [ ! -f "$TERMS" ]; then
  echo "evidence-class drift: FAILED — $TERMS is missing" >&2
  echo "  The enum moved. Update upstream.path in $RECORD after finding it." >&2
  exit 1
fi

# One snapshot: the hash and the dirty state must describe the same moment, or
# they can disagree about a tree that changed between two git invocations.
status="$(git -C "$SERVER_DIR" status --porcelain=v2 --branch)"
head_oid="$(awk '/^# branch\.oid /{print $3}' <<<"$status")"
if grep -qv '^#' <<<"$status"; then
  echo "evidence-class drift: NOTE — $SERVER_DIR has uncommitted changes;" >&2
  echo "  the variants below may not be what commit $head_oid contains." >&2
fi

# The variants between `enum EvidenceClass {` and its closing brace, PascalCase
# as written, then camelCased the way `#[serde(rename_all = "camelCase")]` does.
actual="$(
  awk '/^pub enum EvidenceClass \{/{f=1;next} f&&/^\}/{exit} f' "$TERMS" \
    | grep -oE '^\s{4}[A-Z][A-Za-z0-9]*,' \
    | tr -d ' ,' \
    | sed 's/^\(.\)/\l\1/'
)"

expected="$(python3 -c "import json;print('\n'.join(json.load(open('$RECORD'))['upstream']['evidenceClasses']))")"
recorded_commit="$(jq_field commit)"
field() { python3 -c "import json,sys;print(json.load(open('$RECORD'))[sys.argv[1]].get(sys.argv[2],''))" "$1" "$2"; }
blob_sha() { git -C "$SERVER_DIR" show "$1:$2" | sha256sum | cut -d' ' -f1; }

if ! git -C "$SERVER_DIR" cat-file -e "$recorded_commit^{commit}" 2>/dev/null; then
  echo "evidence-class drift: FAILED — $recorded_commit is not in $SERVER_DIR" >&2
  echo "  Fetch it, or re-pin upstream.commit to a pushed commit after re-reading." >&2
  exit 1
fi

# The pin is on CONTENT, not on the commit ID. Each pinned file is hashed twice:
# at the recorded commit (is the record itself true?) and at HEAD (has the file
# changed since?). A moved commit with identical bytes cannot have moved the enum
# or the write clamp, so it passes with a note instead of asking for a re-pin.
fail=0
for section in upstream agentWritableClasses; do
  path="$(field "$section" path)"
  pinned="$(field "$section" fileSha256)"
  if [ -z "$pinned" ]; then
    echo "evidence-class drift: FAILED — $section.fileSha256 is missing in $RECORD" >&2
    fail=1; continue
  fi
  at_record="$(blob_sha "$recorded_commit" "$path")"
  if [ "$at_record" != "$pinned" ]; then
    echo "evidence-class drift: FAILED — $path at $recorded_commit digests to" >&2
    echo "  $at_record, but $section.fileSha256 pins $pinned." >&2
    echo "  Either the record is wrong or the commit is not what it was read from." >&2
    fail=1; continue
  fi
  at_head="$(blob_sha "$head_oid" "$path")"
  if [ "$at_head" != "$pinned" ]; then
    echo "evidence-class drift: FAILED — $path changed since $recorded_commit" >&2
    echo "  Re-read $section from source, then update the record and the mapping together." >&2
    fail=1
  fi
done

if ! diff -u <(echo "$expected") <(echo "$actual") ; then
  echo "evidence-class drift: FAILED — the enum no longer matches $RECORD" >&2
  echo "  Update the mapping in crates/neuralcompose-mobile-core/src/provenance.rs" >&2
  echo "  AND the fixture, together. The mapping is the thing that must not drift." >&2
  fail=1
fi
[ "$fail" -eq 0 ] || exit 1

if [ "$head_oid" != "$recorded_commit" ]; then
  echo "evidence-class drift: NOTE — checkout is at $head_oid, record at $recorded_commit;" >&2
  echo "  pinned files are byte-identical. Re-pinning upstream.commit is optional." >&2
fi
echo "evidence-class drift: clean (5 variants, $SERVER_DIR at $head_oid)"
