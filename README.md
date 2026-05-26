# VoidMetrics Agent

Standalone repository for the VoidMetrics host agent. This repo contains only the Rust agent, cross-platform release packaging, and a short API guide for the main VoidMetrics server.

## What Is Included

- Rust source for `voidmetrics-agent`
- release packaging script for macOS, Linux, and Windows
- GitHub Actions workflow for tagged releases
- ready-made release archives in [releases](./releases)
- quick run guide and main server endpoint reference

## Supported Release Targets

- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-pc-windows-msvc`
- `i686-pc-windows-msvc`
- `aarch64-pc-windows-msvc`

Current prebuilt archives committed in this repo:

- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `x86_64-pc-windows-msvc`

Extra Windows variants are produced by the GitHub Actions release workflow.

## Quick Start

### 1. Download A Release

Use one of the archives from [releases](./releases) or from GitHub Releases after tagging.

### 2. Unpack It

Each release archive contains:

- `voidmetrics-agent` or `voidmetrics-agent.exe`
- `start-local`
- `start-external`
- `agent.env.example`

### 3. Point The Agent To Core

Local operator setup:

```bash
./voidmetrics-agent --core-url ws://YOUR_CORE_HOST:3000/ws --auth-key YOUR_LOCAL_TOKEN
```

External/public setup:

```bash
./voidmetrics-agent --core-url wss://YOUR_PUBLIC_HOST/ws --auth-key YOUR_EXTERNAL_TOKEN
```

Background mode:

```bash
./voidmetrics-agent --daemon --core-url ws://YOUR_CORE_HOST:3000/ws --auth-key YOUR_TOKEN
```

### 4. Or Run Through Environment Variables

Linux or macOS:

```bash
export VOIDMETRICS_CORE_URL=ws://YOUR_CORE_HOST:3000/ws
export VOIDMETRICS_AGENT_TOKEN=YOUR_TOKEN
./voidmetrics-agent --daemon
```

Windows PowerShell:

```powershell
$env:VOIDMETRICS_CORE_URL = "ws://YOUR_CORE_HOST:3000/ws"
$env:VOIDMETRICS_AGENT_TOKEN = "YOUR_TOKEN"
.\voidmetrics-agent.exe --daemon
```

## Windows Launchers

Windows archives include:

- `start-local.ps1`
- `start-external.ps1`

Examples:

```powershell
.\start-local.ps1 -CoreUrl "ws://YOUR_CORE_HOST:3000/ws" -Token "YOUR_LOCAL_TOKEN"
```

```powershell
.\start-external.ps1 -CoreUrl "wss://YOUR_PUBLIC_HOST/ws" -Token "YOUR_EXTERNAL_TOKEN" -Daemon
```

## Agent Configuration

Important runtime options:

- `--core-url <ws-url>`: override `VOIDMETRICS_CORE_URL`
- `--auth-key <token>`: override `VOIDMETRICS_AGENT_TOKEN`
- `--daemon`: run in background
- `--agent-id <uuid>`: pin the agent ID
- `--agent-id-file <path>`: where the agent stores its ID

Important env vars:

- `VOIDMETRICS_CORE_URL`
- `VOIDMETRICS_AGENT_TOKEN`
- `VOIDMETRICS_AGENT_TOKEN_FILE`
- `VOIDMETRICS_AGENT_ID`
- `VOIDMETRICS_AGENT_ID_FILE`
- `VOIDMETRICS_INTERVAL_SECONDS`
- `VOIDMETRICS_RECONNECT_SECONDS`
- `VOIDMETRICS_PROCESS_LIMIT`
- `VOIDMETRICS_SEND_ALL_PROCESSES`
- `VOIDMETRICS_PROCESS_INTERVAL_SECONDS`
- `VOIDMETRICS_INCLUDE_COMMAND`
- `VOIDMETRICS_INCLUDE_PATHS`
- `VOIDMETRICS_INCLUDE_ENVIRONMENT_COUNT`
- `VOIDMETRICS_MAX_COMMAND_LENGTH`
- `VOIDMETRICS_MAX_PATH_LENGTH`

## Main Server Endpoints

The agent itself only needs the WebSocket endpoint:

- `GET /ws` - main agent connection endpoint

Useful operator endpoints on the main server:

- `GET /api/setup` - returns token setup state and the agent WebSocket path
- `GET /api/agent-releases` - list downloadable agent archives
- `GET /api/agent-releases/{file}` - download a concrete agent archive
- `GET /api/agents` - list live agents
- `GET /api/agents/{id}` - get one live agent
- `POST /api/agents/{id}/commands` - send an agent command
- `POST /api/agents/{id}/shutdown` - stop a connected agent
- `GET /api/agents/{id}/history?metric=<name>&range=<range>` - time-series history
- `GET /api/agents/{id}/history.csv?metric=<name>&range=<range>` - export history CSV
- `GET /api/agents/{id}/export.csv` - export current agent snapshot CSV

Registry and grouping endpoints on the main server:

- `GET /api/registry`
- `GET /api/agent-groups`
- `POST /api/agent-groups`
- `DELETE /api/agent-groups/{id}`
- `GET /api/registered-agents`
- `POST /api/registered-agents`
- `GET /api/registered-agents/{id}`
- `DELETE /api/registered-agents/{id}`
- `POST /api/registered-agents/{id}/config`
- `POST /api/registered-agents/{id}/move`

External read-only API:

- `GET /api/external/agents`
- `GET /api/external/agents/{id}`

For external endpoints use:

```text
Authorization: Bearer <VOIDMETRICS_EXTERNAL_AGENT_TOKEN>
```

## Agent Commands Accepted By Core

These actions are accepted by `POST /api/agents/{id}/commands`:

- `set_process_stream`
- `reconnect`
- `refresh_inventory`
- `diagnose`
- `docker_ps`
- `logs`

## Build From Source

Local build:

```bash
cargo build --release
```

Run help:

```bash
cargo run -- --help
```

## Build Release Archives

Current host only:

```bash
./scripts/build-agent-releases.sh
```

Full matrix:

```bash
./scripts/build-agent-releases.sh --all
```

Build output goes to `./releases` by default.

Requirements for cross-builds:

- `rustup`
- `zip`
- `zig` for non-host Linux cross-compilation
- `cargo-xwin` for `*-pc-windows-msvc`

## GitHub Releases

This repo ships a workflow in `.github/workflows/release.yml`.

- `workflow_dispatch` builds archives and uploads them as artifacts
- pushing a tag like `v0.1.0` builds archives and publishes a GitHub Release

## Docker

Build image:

```bash
docker build -t voidmetrics-agent .
```

Run image:

```bash
docker run --rm \
  -e VOIDMETRICS_CORE_URL=ws://host.docker.internal:3000/ws \
  -e VOIDMETRICS_AGENT_TOKEN=YOUR_TOKEN \
  voidmetrics-agent
```
