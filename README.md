# VoidMetrics Agent

Репозиторий агента `voidmetrics-agent`.

## Назначение

Агент подключается к `core` по WebSocket:

- `WS /ws`

Агент передает:

- информацию о хосте
- системные метрики
- список процессов
- результаты команд от `core`

## Автоматическая установка

Последовательность:

1. Внешний слой вызывает `GET /api/setup`.
2. Внешний слой вызывает `POST /api/agent-bootstrap`.
3. `core` возвращает `agent_id`, `agent_token`, `agent_ws_url`.
4. Внешний слой скачивает архив через `GET /api/agent-releases/{file}`.
5. Внешний слой запускает агент с полученными `agent_id`, `agent_token`, `agent_ws_url`.
6. Агент подключается к `WS /ws`.
7. Агент появляется в `GET /api/agents` и `GET /api/agents/{id}`.

## Токены

- `bootstrap token` используется только для вызова `POST /api/agent-bootstrap`
- `agent_token` используется только агентом при подключении к `WS /ws`
- `core` не отдает shared install-token агенту
- `core` выдает агенту только новый `agent_token` через bootstrap endpoint

Legacy fallback:

- `VOIDMETRICS_LOCAL_AGENT_TOKEN`
- `VOIDMETRICS_EXTERNAL_AGENT_TOKEN`

Они остаются совместимыми, но основной поток установки теперь bootstrap-based.

## Bootstrap endpoint

Запрос:

```http
POST /api/agent-bootstrap
Authorization: Bearer <BOOTSTRAP_TOKEN>
Content-Type: application/json
```

Body:

```json
{
  "name": "node-01",
  "labels": ["prod"],
  "install_mode": "standalone",
  "access_scope": "external"
}
```

Ответ:

```json
{
  "agent_id": "uuid",
  "agent_token": "vmk_agent_xxx",
  "access_scope": "external",
  "agent_ws_url": "wss://host/ws",
  "desired_config": {}
}
```

## Запуск агента

Ручной запуск:

```bash
./voidmetrics-agent --core-url wss://YOUR_HOST/ws --auth-key YOUR_AGENT_TOKEN --agent-id YOUR_AGENT_ID
```

Фоновый режим:

```bash
./voidmetrics-agent --daemon --core-url wss://YOUR_HOST/ws --auth-key YOUR_AGENT_TOKEN --agent-id YOUR_AGENT_ID
```

Через env:

```bash
export VOIDMETRICS_CORE_URL=wss://YOUR_HOST/ws
export VOIDMETRICS_AGENT_TOKEN=YOUR_AGENT_TOKEN
export VOIDMETRICS_AGENT_ID=YOUR_AGENT_ID
./voidmetrics-agent --daemon
```

Windows PowerShell:

```powershell
$env:VOIDMETRICS_CORE_URL = "wss://YOUR_HOST/ws"
$env:VOIDMETRICS_AGENT_TOKEN = "YOUR_AGENT_TOKEN"
$env:VOIDMETRICS_AGENT_ID = "YOUR_AGENT_ID"
.\voidmetrics-agent.exe --daemon
```

## HTTP и WS endpoints

- `GET /api/setup`
- `POST /api/agent-bootstrap`
- `GET /api/agent-releases`
- `GET /api/agent-releases/{file}`
- `WS /ws`
- `GET /api/agents`
- `GET /api/agents/{id}`
- `GET /api/external/agents`
- `GET /api/external/agents/{id}`

Внешний read-only API:

```text
Authorization: Bearer <VOIDMETRICS_EXTERNAL_AGENT_TOKEN>
```

## Срез состояния

Текущий JSON-срез машины:

- `GET /api/agents/{id}`
- `GET /api/external/agents/{id}`

Поле `metrics` содержит:

- CPU
- RAM
- Swap
- Disk
- Network
- GPU

## Параметры запуска

CLI:

- `--core-url <ws-url>`
- `--auth-key <token>`
- `--daemon`
- `--agent-id <uuid>`
- `--agent-id-file <path>`

Env:

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

## Сборка

```bash
cargo build --release
```

```bash
cargo run -- --help
```

```bash
./scripts/build-agent-releases.sh
```

```bash
./scripts/build-agent-releases.sh --all
```

## Docker

```bash
docker build -t voidmetrics-agent .
```

```bash
docker run --rm \
  -e VOIDMETRICS_CORE_URL=wss://YOUR_HOST/ws \
  -e VOIDMETRICS_AGENT_TOKEN=YOUR_AGENT_TOKEN \
  -e VOIDMETRICS_AGENT_ID=YOUR_AGENT_ID \
  voidmetrics-agent
```
