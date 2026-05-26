# VoidMetrics Agent

Репозиторий агента `voidmetrics-agent`.

## Состав

- исходный код агента на Rust
- скрипт сборки архивов
- готовые архивы в [releases](./releases)

## Назначение

Агент подключается к `core` по WebSocket:

- `WS /ws`

После подключения агент передает:

- информацию о хосте
- системные метрики
- список процессов
- результаты команд от `core`

## Токен

Текущая схема авторизации:

- `core` не выдает токен агенту через HTTP
- агент не получает токен автоматически от `core`
- токен должен быть передан агенту внешним установщиком, скриптом, env, secret store или файлом
- `core` только проверяет токен при подключении на `/ws`

Сейчас используются общие install-token значения:

- локальный токен для внутреннего контура
- внешний токен для внешних машин

Индивидуальный токен на каждого агента в текущей реализации не выдается.

## Автоматическая установка

Последовательность работы:

1. Внешний слой вызывает `GET /api/setup`.
2. Внешний слой вызывает `GET /api/agent-releases`.
3. Внешний слой скачивает архив через `GET /api/agent-releases/{file}`.
4. Внешний слой берет токен из своего секрета.
5. Внешний слой запускает агент.
6. Агент подключается к `WS /ws`.
7. Агент появляется в `GET /api/agents` и `GET /api/agents/{id}`.

Регистрация записи в реестре может выполняться отдельно через:

- `POST /api/registered-agents`

Этот вызов создает запись агента, но не выдает токен.

## Запуск

Локальный контур:

```bash
./voidmetrics-agent --core-url ws://YOUR_CORE_HOST:3000/ws --auth-key YOUR_LOCAL_TOKEN
```

Внешний контур:

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

## HTTP и WS endpoints

### Подключение агента

- `GET /api/setup`
- `GET /api/agent-releases`
- `GET /api/agent-releases/{file}`
- `WS /ws`

### Операторский контур

- `GET /api/agents`
- `GET /api/agents/{id}`
- `WS /live`

### Внешний read-only контур

- `GET /api/external/agents`
- `GET /api/external/agents/{id}`

Заголовок для внешнего read-only API:

```text
Authorization: Bearer <VOIDMETRICS_EXTERNAL_AGENT_TOKEN>
```

## Срез статистики

Текущий JSON-срез машины:

- `GET /api/agents/{id}`
- `GET /api/external/agents/{id}`

Поля `metrics` содержат:

- CPU
- RAM
- Swap
- Disk
- Network
- GPU

Отдельные HTTP endpoints для CPU, RAM и Disk не используются.

## Параметры запуска

CLI:

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
  -e VOIDMETRICS_CORE_URL=ws://host.docker.internal:3000/ws \
  -e VOIDMETRICS_AGENT_TOKEN=YOUR_TOKEN \
  voidmetrics-agent
```
