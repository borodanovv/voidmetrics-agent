# VoidMetrics Agent

Отдельный репозиторий агента VoidMetrics.

Здесь лежат:

- Rust-исходники `voidmetrics-agent`
- скрипт сборки релизных архивов
- готовые архивы в [releases](./releases)
- краткая документация по подключению агента к `core`

## Что важно понимать

Агенту для работы нужен только WebSocket `core`:

- `WS /ws`

HTTP-часть нужна оператору или внешнему слою только для установки, скачивания архива и чтения текущего состояния.

## Быстрый старт

1. Скачайте архив из [releases](./releases) или из GitHub Releases.
2. Распакуйте его.
3. Запустите агент с URL `core` и токеном.

Локальное подключение:

```bash
./voidmetrics-agent --core-url ws://YOUR_CORE_HOST:3000/ws --auth-key YOUR_LOCAL_TOKEN
```

Публичное подключение:

```bash
./voidmetrics-agent --core-url wss://YOUR_PUBLIC_HOST/ws --auth-key YOUR_EXTERNAL_TOKEN
```

Фоновый режим:

```bash
./voidmetrics-agent --daemon --core-url ws://YOUR_CORE_HOST:3000/ws --auth-key YOUR_TOKEN
```

Через переменные окружения:

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

## Автоматический сценарий установки

Текущий API подходит для автоустановки без ручного копирования команд в UI.

Рекомендуемый поток такой:

1. Внешний слой проверяет `GET /api/setup`.
2. Получает список архивов через `GET /api/agent-releases`.
3. Скачивает нужный файл через `GET /api/agent-releases/{file}`.
4. Берёт install-token из своего секрета, env или secret store.
5. Запускает агент с `--core-url` и `--auth-key`.
6. После подключения агент сам появляется в live-срезе.

Важно:

- `core` не выдаёт install-token через HTTP;
- токен надо передавать установщику из безопасного внешнего контура;
- для текущей версии это правильнее, чем открывать общий токен через публичный API.

Если нужно заранее завести карточку агента в реестре, можно дополнительно вызвать:

- `POST /api/registered-agents`

Пример payload:

```json
{
  "name": "node-01",
  "group_id": "default",
  "labels": ["prod", "eu-west"],
  "desired_config": {
    "interval_seconds": 1,
    "process_interval_seconds": 2,
    "process_limit": 220
  },
  "install_mode": "standalone"
}
```

Этот вызов создаёт запись в реестре, но не выдаёт отдельный токен на агента.

## Какие эндпоинты реально нужны

### Подключение и установка

- `GET /api/setup`
- `GET /api/agent-releases`
- `GET /api/agent-releases/{file}`
- `WS /ws`

### Live API оператора

- `GET /api/agents`
- `GET /api/agents/{id}`
- `WS /live`

### Внешний read-only API

- `GET /api/external/agents`
- `GET /api/external/agents/{id}`

Для внешнего API используйте:

```text
Authorization: Bearer <VOIDMETRICS_EXTERNAL_AGENT_TOKEN>
```

## Один endpoint для статистики

Если нужен единый JSON-срез по машине без лишних routes, используйте:

- операторский контур: `GET /api/agents/{id}`
- внешний контур: `GET /api/external/agents/{id}`

Оба ответа уже содержат общий объект `metrics`, где лежат:

- CPU: `cpu_usage`, `cpu_cores`, `load_1`, `load_5`, `load_15`
- RAM: `ram_used`, `ram_total`, `swap_used`, `swap_total`
- Disk: `disk_used`, `disk_total`, `disk_usage_percent`, `disk_read_bytes`, `disk_written_bytes`
- Network: `network_received_bytes`, `network_transmitted_bytes`
- GPU: `gpu_usage_percent`, `gpu_memory_used`, `gpu_temperature_celsius`

То есть для CPU, RAM и Disk не нужен отдельный набор endpoint-ов: достаточно одного `GET /api/agents/{id}` или `GET /api/external/agents/{id}`.

## Что намеренно не включено в эту документацию

Здесь специально не перечислены:

- history/CSV export routes;
- operator command/shutdown routes;
- group CRUD и служебные registry-маршруты.

Они могут использоваться внутренним UI, но не нужны для обычной установки агента и только перегружают публичную документацию.

## Конфигурация агента

CLI-опции:

- `--core-url <ws-url>`
- `--auth-key <token>`
- `--daemon`
- `--agent-id <uuid>`
- `--agent-id-file <path>`

Основные env:

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

Локально:

```bash
cargo build --release
```

Справка:

```bash
cargo run -- --help
```

Сборка архивов для текущей платформы:

```bash
./scripts/build-agent-releases.sh
```

Полная матрица:

```bash
./scripts/build-agent-releases.sh --all
```

## Docker

Сборка:

```bash
docker build -t voidmetrics-agent .
```

Запуск:

```bash
docker run --rm \
  -e VOIDMETRICS_CORE_URL=ws://host.docker.internal:3000/ws \
  -e VOIDMETRICS_AGENT_TOKEN=YOUR_TOKEN \
  voidmetrics-agent
```
