#!/usr/bin/env bash
# Fixture drift guard: regenerate the Rust-owned fixtures into a temp dir and
# diff against the frozen contracts/fixtures/ copies.
set -euo pipefail
cd "$(dirname "$0")/.."

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cargo run --quiet --example gen_fixtures -p neuralcompose-mobile-core -- "$tmp"

diff -u "contracts/fixtures/eeg-frame-batch-8.json" "$tmp/eeg-frame-batch-8.json"

# The M7-B corpus is compiled into the crate with include_bytes! and its
# identity is derived from its content, so the committed artifact and the
# compiled value must be provably the same bytes — not merely believed to be.
diff -u "contracts/generation-eval/m7b-corpus-v1.json" "$tmp/m7b-corpus-v1.json"

echo "fixtures: no drift"
