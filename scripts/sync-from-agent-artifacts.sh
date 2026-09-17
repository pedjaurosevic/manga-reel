#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
if [[ -f scripts/restore-sources.py && -f .srcb64/manifest.json ]]; then
  python3 scripts/restore-sources.py
fi
if [[ -f scripts/unpack-bundle.py && -d .bundle ]]; then
  python3 scripts/unpack-bundle.py
fi
if [[ -f scripts/bootstrap-assets.py ]]; then
  python3 scripts/bootstrap-assets.py || true
fi
echo "Sources restored under $ROOT"
