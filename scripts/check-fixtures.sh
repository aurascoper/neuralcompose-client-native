#!/usr/bin/env bash
# Fixture drift guard: regenerate the Rust-owned fixtures into a temp dir and
# diff against the frozen contracts/fixtures/ copies.
set -euo pipefail
cd "$(dirname "$0")/.."

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cargo run --quiet --example gen_fixtures -p neuralcompose-mobile-core -- "$tmp"

diff -u "contracts/fixtures/eeg-frame-batch-8.json" "$tmp/eeg-frame-batch-8.json"
echo "fixtures: no drift"
