#!/usr/bin/env bash
set -euo pipefail

if [ -z "${VOIDMETRICS_ZIG_TARGET:-}" ]; then
  echo "VOIDMETRICS_ZIG_TARGET is not set" >&2
  exit 1
fi

args=()
has_target=0
skip_next=0

for arg in "$@"; do
  if [ "$skip_next" -eq 1 ]; then
    args+=("$VOIDMETRICS_ZIG_TARGET")
    skip_next=0
    has_target=1
    continue
  fi

  case "$arg" in
    --target=*)
      args+=("--target=$VOIDMETRICS_ZIG_TARGET")
      has_target=1
      ;;
    --target|-target)
      args+=("$arg")
      skip_next=1
      ;;
    *)
      args+=("$arg")
      ;;
  esac
done

if [ "$has_target" -eq 0 ]; then
  args=(-target "$VOIDMETRICS_ZIG_TARGET" "${args[@]}")
fi

exec zig c++ "${args[@]}"
