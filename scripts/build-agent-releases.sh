#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${VOIDMETRICS_AGENT_RELEASE_DIR:-${AGENT_RELEASE_DIR:-$ROOT_DIR/releases}}"
PROFILE="${AGENT_BUILD_PROFILE:-release}"
HOST_TARGET="$(rustc -vV | awk '/host:/ { print $2 }')"
DEFAULT_TARGETS=(
  "aarch64-apple-darwin"
  "x86_64-apple-darwin"
  "x86_64-unknown-linux-gnu"
  "aarch64-unknown-linux-gnu"
  "x86_64-pc-windows-msvc"
  "i686-pc-windows-msvc"
  "aarch64-pc-windows-msvc"
)
TARGETS=()

print_help() {
  cat <<'EOF'
Usage:
  ./scripts/build-agent-releases.sh [--all] [--profile release] [target...]

Options:
  --all                  Build the default cross-platform matrix.
  --profile <name>       Cargo profile name. Defaults to "release".
  -h, --help             Show this help.

Examples:
  ./scripts/build-agent-releases.sh
  ./scripts/build-agent-releases.sh --all
  ./scripts/build-agent-releases.sh x86_64-pc-windows-msvc
EOF
}

ensure_target_installed() {
  local target="$1"
  if ! rustup target list --installed | grep -qx "$target"; then
    echo "Rust target '$target' is not installed. Installing..."
    rustup target add "$target"
  fi
}

target_env_name() {
  echo "$1" | tr '[:lower:]-' '[:upper:]_'
}

zig_target_for_rust_target() {
  case "$1" in
    x86_64-unknown-linux-gnu) echo "x86_64-linux-gnu" ;;
    aarch64-unknown-linux-gnu) echo "aarch64-linux-gnu" ;;
    *) return 1 ;;
  esac
}

build_agent() {
  local target="$1"
  local zig_target
  local target_env
  local target_cc_env

  if [[ "$target" == *"windows-msvc" ]]; then
    if ! cargo xwin --version >/dev/null 2>&1; then
      echo "cargo-xwin is required to build target '$target' on this host." >&2
      exit 1
    fi
    (
      cd "$ROOT_DIR"
      env "PATH=$ROOT_DIR/scripts:$PATH" cargo xwin build --profile "$PROFILE" --target "$target"
    )
    return 0
  fi

  if zig_target="$(zig_target_for_rust_target "$target")"; then
    if ! command -v zig >/dev/null 2>&1; then
      echo "zig is required to build target '$target' on this host." >&2
      exit 1
    fi

    target_env="$(target_env_name "$target")"
    target_cc_env="${target//-/_}"
    (
      cd "$ROOT_DIR"
      env \
        "VOIDMETRICS_ZIG_TARGET=$zig_target" \
        "CC=$ROOT_DIR/scripts/zig-cc.sh" \
        "CXX=$ROOT_DIR/scripts/zig-cxx.sh" \
        "AR=$ROOT_DIR/scripts/zig-ar.sh" \
        "CC_${target_cc_env}=$ROOT_DIR/scripts/zig-cc.sh" \
        "CXX_${target_cc_env}=$ROOT_DIR/scripts/zig-cxx.sh" \
        "AR_${target_cc_env}=$ROOT_DIR/scripts/zig-ar.sh" \
        "CARGO_TARGET_${target_env}_LINKER=$ROOT_DIR/scripts/zig-linker.sh" \
        cargo build --profile "$PROFILE" --target "$target"
    )
    return 0
  fi

  (cd "$ROOT_DIR" && cargo build --profile "$PROFILE" --target "$target")
}

write_unix_launcher() {
  local file="$1"
  local mode_label="$2"
  local token_placeholder="$3"

  cat >"$file" <<EOF
#!/usr/bin/env bash
set -euo pipefail

CORE_URL="\${1:-ws://127.0.0.1:3000/ws}"
TOKEN="\${2:-$token_placeholder}"
DIR="\$(cd "\$(dirname "\${BASH_SOURCE[0]}")" && pwd)"

export VOIDMETRICS_CORE_URL="\$CORE_URL"
export VOIDMETRICS_AGENT_TOKEN="\$TOKEN"

echo "Starting VoidMetrics agent ($mode_label)"
exec "\$DIR/voidmetrics-agent" --daemon
EOF
  chmod +x "$file"
}

write_windows_launcher() {
  local file="$1"
  local mode_label="$2"
  local token_placeholder="$3"

  cat >"$file" <<EOF
param(
  [string]\$CoreUrl = "ws://127.0.0.1:3000/ws",
  [string]\$Token = "$token_placeholder",
  [switch]\$Daemon
)

\$env:VOIDMETRICS_CORE_URL = \$CoreUrl
\$env:VOIDMETRICS_AGENT_TOKEN = \$Token
\$Agent = Join-Path \$PSScriptRoot "voidmetrics-agent.exe"

Write-Host "Starting VoidMetrics agent ($mode_label)"
if (\$Daemon) {
  & \$Agent --daemon
} else {
  & \$Agent
}
EOF
}

write_env_example() {
  local file="$1"

  cp "$ROOT_DIR/agent.env.example" "$file"
}

while [ $# -gt 0 ]; do
  case "$1" in
    --all|--matrix)
      TARGETS=("${DEFAULT_TARGETS[@]}")
      shift
      ;;
    --profile)
      PROFILE="${2:-}"
      if [ -z "$PROFILE" ]; then
        echo "Missing profile name after --profile" >&2
        exit 1
      fi
      shift 2
      ;;
    -h|--help)
      print_help
      exit 0
      ;;
    *)
      TARGETS+=("$1")
      shift
      ;;
  esac
done

if [ ${#TARGETS[@]} -eq 0 ]; then
  TARGETS=("$HOST_TARGET")
fi

mkdir -p "$OUT_DIR"

for target in "${TARGETS[@]}"; do
  ensure_target_installed "$target"
  echo "Building agent for $target (profile: $PROFILE)"
  build_agent "$target"

  binary="$ROOT_DIR/target/$target/$PROFILE/voidmetrics-agent"
  archive_name="voidmetrics-agent-$target"
  [ "$PROFILE" != "release" ] && archive_name="$archive_name-$PROFILE"
  archive="$OUT_DIR/$archive_name.tar.gz"

  if [[ "$target" == *"windows"* ]]; then
    binary="$binary.exe"
    archive="$OUT_DIR/$archive_name.zip"
  fi

  if [ ! -f "$binary" ]; then
    echo "Binary not found: $binary" >&2
    exit 1
  fi

  tmp_dir="$(mktemp -d)"
  cp "$binary" "$tmp_dir/voidmetrics-agent${binary##*voidmetrics-agent}"
  write_env_example "$tmp_dir/agent.env.example"

  if [[ "$target" == *"windows"* ]]; then
    write_windows_launcher "$tmp_dir/start-local.ps1" "local token" "paste-local-token-here"
    write_windows_launcher "$tmp_dir/start-external.ps1" "external token" "paste-external-token-here"
  else
    write_unix_launcher "$tmp_dir/start-local.sh" "local token" "paste-local-token-here"
    write_unix_launcher "$tmp_dir/start-external.sh" "external token" "paste-external-token-here"
  fi

  if [[ "$archive" == *.zip ]]; then
    (cd "$tmp_dir" && zip -qr "$archive" .)
  else
    tar -czf "$archive" -C "$tmp_dir" .
  fi

  rm -rf "$tmp_dir"
  echo "Wrote $archive"
done
