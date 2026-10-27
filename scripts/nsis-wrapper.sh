#!/usr/bin/env bash
set -euo pipefail
project="$(cd "$(dirname "$0")/.." && pwd)"
export NSISDIR="$project/.tools/nsis/root/usr/share/nsis"
exec "$project/.tools/nsis/root/usr/bin/makensis" "$@"
