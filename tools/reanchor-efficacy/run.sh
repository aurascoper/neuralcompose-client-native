#!/usr/bin/env bash
# A/B the re-anchoring mechanism. See PREREGISTRATION.md — read it first; the
# criterion is fixed there and this script only collects the data.
#
# Needs llama-server on 8080 (generation) and the binary built with the real
# embedder backend:
#   LLAMA_CPP_DIR=~/src/llama.cpp LLAMA_CPP_LIB_DIR=~/src/llama.cpp/build-cpu/bin \
#     cargo build -p neuralcompose-hypnagogic
# A plain `cargo test` rebuilds it as a stub and every run then dies at startup.
set -u -o pipefail

REPO=/home/aurascoper/src/neuralcompose-client-native-nonspeech-guard
OUT="${1:?usage: run.sh <output-dir>}"
RUNS="${2:-5}"
BIN="$REPO/target/debug/neuralcompose-hypnagogic"
export LD_LIBRARY_PATH=/home/aurascoper/src/llama.cpp/build-cpu/bin
export NC_EMBED_MODEL=/home/aurascoper/models/bge-small-en-v1.5-f32.gguf

mkdir -p "$OUT"

# Build it here rather than trusting whatever is in target/. A plain `cargo
# test` anywhere else rebuilds this binary as a stub -- build.rs keys off
# LLAMA_CPP_DIR and cargo reruns it whenever that variable changes -- and a
# rebuild landing mid-experiment silently swaps the binary between runs. That
# happened: it killed a batch after run 1 and the first sign was an "Unavailable"
# in a stderr file nobody was reading.
export LLAMA_CPP_DIR=/home/aurascoper/src/llama.cpp
export LLAMA_CPP_LIB_DIR=/home/aurascoper/src/llama.cpp/build-cpu/bin
cargo build --manifest-path "$REPO/Cargo.toml" -p neuralcompose-hypnagogic -j 4 \
  || { echo "build failed" >&2; exit 1; }

one() {  # arm ceiling input tag
  "$BIN" --mode reflective --turns 11 --json \
    --drift-ceiling "$2" --repetition-floor 0 \
    < "$3" > "$OUT/$4.jsonl" 2> "$OUT/$4.err"
  # A stub binary, a dead server or a bad flag all produce a short file and a
  # zero exit from the pipeline. Fail loudly instead of collecting a batch of
  # near-empty runs and averaging them.
  if grep -q "Unavailable" "$OUT/$4.err"; then
    echo "FATAL: $4 ran against a stub binary (embedder Unavailable)" >&2
    exit 1
  fi
  local n
  n=$(grep -c '^{' "$OUT/$4.jsonl" || true)
  if [ "$n" -ne 11 ]; then
    echo "FATAL: $4 logged $n turns, expected 11" >&2
    exit 1
  fi
}

for r in $(seq 1 "$RUNS"); do
  echo "run $r/$RUNS" >&2
  one framed   0.18 "$REPO/tools/drift-calibration/off-topic.txt" "framed-$r"
  one unframed 0    "$REPO/tools/drift-calibration/off-topic.txt" "unframed-$r"
done

# Positive control: if on-topic replies do not outscore off-topic ones, the
# outcome measure is dead and a null result above means nothing.
one control-framed   0.18 "$REPO/tools/drift-calibration/on-topic.txt" "control-framed"
one control-unframed 0    "$REPO/tools/drift-calibration/on-topic.txt" "control-unframed"

echo "done -> $OUT" >&2
