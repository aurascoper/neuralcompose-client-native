#!/usr/bin/env bash
# Secret / private-endpoint scan. Tracked content only; the stub server is
# exempt because it declares loopback fixtures by design.
set -euo pipefail
cd "$(dirname "$0")/.."

pattern='sk-ant-|sk-[A-Za-z0-9]{32}|tskey-|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----|100\.([0-9]{1,3})\.([0-9]{1,3})\.([0-9]{1,3})'
if git grep -nEI "$pattern" -- . ':!contracts/stub-server' ':!scripts/check-no-secrets.sh'; then
  echo "secret scan: FAILED (matches above)" >&2
  exit 1
fi
echo "secret scan: clean"
