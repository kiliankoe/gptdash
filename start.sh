#!/usr/bin/env bash
set -a
# shellcheck disable=SC1091
source "$(dirname "$0")/.env"
set +a
exec "$(dirname "$0")/gptdash" "$@"
