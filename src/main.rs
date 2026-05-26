use futures_util::SinkExt;
use futures_util::StreamExt;
use rustls::crypto::ring::default_provider;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env,
    error::Error,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use sysinfo::{Disks, Networks, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind, Users};
use tokio::time::{self, Duration, Instant};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::process::CommandExt as _;
#[cfg(windows)]
use std::os::windows::process::CommandExt as _;

const DEFAULT_CORE_URL: &str = "ws://127.0.0.1:3000/ws";
const DEFAULT_INTERVAL_SECONDS: u64 = 1;
const DEFAULT_RECONNECT_SECONDS: u64 = 3;
const DEFAULT_PROCESS_LIMIT: usize = 220;
const DEFAULT_SEND_ALL_PROCESSES: bool = false;
const DEFAULT_PROCESS_INTERVAL_SECONDS: u64 = 2;
const DEFAULT_INCLUDE_COMMAND: bool = true;
const DEFAULT_INCLUDE_PATHS: bool = true;
const DEFAULT_INCLUDE_ENVIRONMENT_COUNT: bool = true;
const DEFAULT_MAX_COMMAND_LENGTH: usize = 512;
const DEFAULT_MAX_PATH_LENGTH: usize = 256;
const DEFAULT_AGENT_TOKEN_FILE: &str = "/run/voidmetrics/agent_token";
const DEFAULT_AGENT_ID_FILE: &str = ".voidmetrics/agent_id";
const DAEMON_CHILD_ENV: &str = "VOIDMETRICS_DAEMON_CHILD";

type AnyError = Box<dyn Error>;

struct AgentConfig {
    agent_id: Uuid,
    agent_id_path: PathBuf,
    core_url: String,
    agent_token: Option<String>,
    interval: Duration,
    reconnect: Duration,
    process_limit: usize,
    send_all_processes: bool,
    process_interval: Duration,
    include_command: bool,
    include_paths: bool,
    include_environment_count: bool,
    max_command_length: usize,
    max_path_length: usize,
}

#[derive(Default)]
struct CliOptions {
    daemon: bool,
    auth_key: Option<String>,
    core_url: Option<String>,
    agent_id: Option<Uuid>,
    agent_id_file: Option<PathBuf>,
    forwarded_args: Vec<OsString>,
    help: bool,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientFrame {
    Hello {
        agent_id: Uuid,
        token: Option<String>,
        hostname: String,
        os: String,
        arch: String,
        version: String,
        ts: u64,
    },
    MetricsBatch {
        agent_id: Uuid,
        ts: u64,
        metrics: Vec<MetricPoint>,
        processes: Option<Vec<ProcessPoint>>,
    },
    CommandResult {
        agent_id: Uuid,
        result: CommandResult,
    },
}

#[derive(Serialize)]
struct CommandResult {
    id: String,
    action: String,
    ok: bool,
    output: String,
    ts: u64,
}

#[derive(Serialize)]
struct MetricPoint {
    name: String,
    value: f64,
    unit: &'static str,
}

#[derive(Serialize)]
struct ProcessPoint {
    pid: u32,
    parent_pid: Option<u32>,
    name: String,
    command: String,
    exe: String,
    cwd: String,
    root: String,
    user: String,
    effective_user: String,
    group_id: String,
    status: String,
    threads: usize,
    memory_bytes: u64,
    virtual_memory_bytes: u64,
    cpu_usage: f64,
    start_time: u64,
    run_time: u64,
    disk_read_bytes: u64,
    disk_written_bytes: u64,
    total_disk_read_bytes: u64,
    total_disk_written_bytes: u64,
    environment_count: usize,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerFrame {
    Ping {
        ts: Option<u64>,
    },
    Shutdown {
        reason: Option<String>,
    },
    Command {
        id: String,
        action: String,
        enabled: Option<bool>,
    },
}

enum ConnectionExit {
    Disconnected,
    Shutdown,
}

enum ServerAction {
    Continue,
    Reconnect,
    Shutdown,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn parse_cli_args() -> Result<CliOptions, AnyError> {
    let mut cli = CliOptions::default();
    let mut args = env::args_os().skip(1);

    while let Some(arg) = args.next() {
        let value = arg.to_string_lossy();
        match value.as_ref() {
            "-h" | "--help" => {
                cli.help = true;
            }
            "--daemon" => {
                cli.daemon = true;
            }
            "--auth-key" => {
                let Some(next) = args.next() else {
                    return Err("--auth-key requires a value".into());
                };
                let token = next.to_string_lossy().trim().to_string();
                if token.is_empty() {
                    return Err("--auth-key requires a non-empty value".into());
                }
                cli.auth_key = Some(token);
                cli.forwarded_args.push(OsString::from("--auth-key"));
                cli.forwarded_args.push(next);
            }
            "--core-url" => {
                let Some(next) = args.next() else {
                    return Err("--core-url requires a value".into());
                };
                let core_url = next.to_string_lossy().trim().to_string();
                if core_url.is_empty() {
                    return Err("--core-url requires a non-empty value".into());
                }
                cli.core_url = Some(core_url);
                cli.forwarded_args.push(OsString::from("--core-url"));
                cli.forwarded_args.push(next);
            }
            "--agent-id" => {
                let Some(next) = args.next() else {
                    return Err("--agent-id requires a UUID".into());
                };
                let parsed = Uuid::parse_str(next.to_string_lossy().trim())
                    .map_err(|_| "--agent-id must be a UUID")?;
                cli.agent_id = Some(parsed);
                cli.forwarded_args.push(OsString::from("--agent-id"));
                cli.forwarded_args.push(next);
            }
            "--agent-id-file" => {
                let Some(next) = args.next() else {
                    return Err("--agent-id-file requires a path".into());
                };
                let path = PathBuf::from(&next);
                cli.agent_id_file = Some(path);
                cli.forwarded_args.push(OsString::from("--agent-id-file"));
                cli.forwarded_args.push(next);
            }
            other => {
                return Err(format!("unknown argument: {other}").into());
            }
        }
    }

    Ok(cli)
}

fn print_help() {
    println!(
        "VoidMetrics agent\n\n\
Usage:\n  voidmetrics-agent [--daemon] [--core-url <ws-url>] [--auth-key <key>] [--agent-id <uuid>] [--agent-id-file <path>]\n\n\
Options:\n  --daemon            Start the agent as a background process and exit.\n  --core-url          Override VOIDMETRICS_CORE_URL.\n  --auth-key          Override VOIDMETRICS_AGENT_TOKEN.\n  --agent-id          Pin the agent UUID.\n  --agent-id-file     Override the file used to persist the agent UUID.\n  -h, --help          Show this help.\n"
    );
}

fn daemon_child_mode() -> bool {
    env::var(DAEMON_CHILD_ENV).ok().as_deref() == Some("1")
}

#[cfg(unix)]
fn prepare_daemon_command(command: &mut Command) {
    command.process_group(0);
}

#[cfg(windows)]
fn prepare_daemon_command(command: &mut Command) {
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(any(unix, windows)))]
fn prepare_daemon_command(_command: &mut Command) {}

fn spawn_daemon(cli: &CliOptions) -> Result<(), AnyError> {
    let exe = env::current_exe()?;
    let mut command = Command::new(exe);
    command
        .args(&cli.forwarded_args)
        .env(DAEMON_CHILD_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    prepare_daemon_command(&mut command);
    let child = command.spawn()?;
    println!("AGENT DAEMON STARTED pid={}", child.id());
    Ok(())
}

fn core_url(cli: &CliOptions) -> String {
    cli.core_url
        .clone()
        .or_else(|| env::var("VOIDMETRICS_CORE_URL").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_CORE_URL.to_string())
}

fn agent_token(cli: &CliOptions) -> Option<String> {
    if let Some(token) = cli
        .auth_key
        .as_ref()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
    {
        return Some(token);
    }

    if let Some(token) = env::var("VOIDMETRICS_AGENT_TOKEN")
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
    {
        return Some(token);
    }

    let token_path = env::var("VOIDMETRICS_AGENT_TOKEN_FILE")
        .ok()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| DEFAULT_AGENT_TOKEN_FILE.to_string());

    fs::read_to_string(token_path)
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

fn interval_seconds() -> u64 {
    env::var("VOIDMETRICS_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_INTERVAL_SECONDS)
}

fn reconnect_seconds() -> u64 {
    env::var("VOIDMETRICS_RECONNECT_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_RECONNECT_SECONDS)
}

fn env_usize(name: &str, default: usize, min: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value >= min)
        .unwrap_or(default)
}

fn env_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .and_then(|value| match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        })
        .unwrap_or(default)
}

fn process_interval_seconds() -> u64 {
    env::var("VOIDMETRICS_PROCESS_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_PROCESS_INTERVAL_SECONDS)
}

fn configured_agent_id_path(cli: &CliOptions) -> Option<PathBuf> {
    cli.agent_id_file.clone().or_else(|| {
        env::var("VOIDMETRICS_AGENT_ID_FILE")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    })
}

fn configured_agent_id(cli: &CliOptions) -> Option<Uuid> {
    cli.agent_id.or_else(|| {
        env::var("VOIDMETRICS_AGENT_ID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .and_then(|value| match Uuid::parse_str(&value) {
                Ok(agent_id) => Some(agent_id),
                Err(_) => {
                    eprintln!("VOIDMETRICS_AGENT_ID ignored: expected UUID");
                    None
                }
            })
    })
}

fn default_agent_id_path() -> Result<PathBuf, AnyError> {
    if let Some(home) = env::home_dir() {
        return Ok(home.join(DEFAULT_AGENT_ID_FILE));
    }

    let current = env::current_dir()?;
    Ok(current.join(DEFAULT_AGENT_ID_FILE))
}

fn agent_id_path(cli: &CliOptions) -> Result<PathBuf, AnyError> {
    if let Some(path) = configured_agent_id_path(cli) {
        return Ok(path);
    }

    default_agent_id_path()
}

fn load_agent_id(path: &Path, cli: &CliOptions) -> Result<Uuid, AnyError> {
    if let Some(agent_id) = configured_agent_id(cli) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, format!("{agent_id}\n"))?;
        return Ok(agent_id);
    }

    if let Ok(value) = fs::read_to_string(path) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            if let Ok(agent_id) = Uuid::parse_str(trimmed) {
                return Ok(agent_id);
            }
            eprintln!(
                "AGENT ID INVALID: expected UUID in {}, generating a new one",
                path.display()
            );
        }
    }

    let agent_id = Uuid::new_v4();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(path, format!("{agent_id}\n"))?;
    println!("AGENT ID GENERATED path={}", path.display());
    Ok(agent_id)
}

fn install_tls_provider() {
    let _ = default_provider().install_default();
}

#[tokio::main]
async fn main() -> Result<(), AnyError> {
    install_tls_provider();

    let cli = parse_cli_args()?;
    if cli.help {
        print_help();
        return Ok(());
    }

    if cli.daemon && !daemon_child_mode() {
        return spawn_daemon(&cli);
    }

    let agent_id_path = agent_id_path(&cli)?;
    let config = AgentConfig {
        agent_id: load_agent_id(&agent_id_path, &cli)?,
        agent_id_path,
        core_url: core_url(&cli),
        agent_token: agent_token(&cli),
        interval: Duration::from_secs(interval_seconds()),
        reconnect: Duration::from_secs(reconnect_seconds()),
        process_limit: env_usize("VOIDMETRICS_PROCESS_LIMIT", DEFAULT_PROCESS_LIMIT, 1),
        send_all_processes: env_bool("VOIDMETRICS_SEND_ALL_PROCESSES", DEFAULT_SEND_ALL_PROCESSES),
        process_interval: Duration::from_secs(process_interval_seconds()),
        include_command: env_bool("VOIDMETRICS_INCLUDE_COMMAND", DEFAULT_INCLUDE_COMMAND),
        include_paths: env_bool("VOIDMETRICS_INCLUDE_PATHS", DEFAULT_INCLUDE_PATHS),
        include_environment_count: env_bool(
            "VOIDMETRICS_INCLUDE_ENVIRONMENT_COUNT",
            DEFAULT_INCLUDE_ENVIRONMENT_COUNT,
        ),
        max_command_length: env_usize(
            "VOIDMETRICS_MAX_COMMAND_LENGTH",
            DEFAULT_MAX_COMMAND_LENGTH,
            16,
        ),
        max_path_length: env_usize("VOIDMETRICS_MAX_PATH_LENGTH", DEFAULT_MAX_PATH_LENGTH, 16),
    };

    loop {
        let exit = tokio::select! {
            result = run_connection(&config) => match result {
                Ok(exit) => exit,
                Err(error) => {
                    eprintln!("AGENT CONNECTION ERROR: {error}");
                    ConnectionExit::Disconnected
                }
            },
            signal = tokio::signal::ctrl_c() => {
                signal?;
                println!("AGENT STOPPED");
                return Ok(());
            }
        };

        match exit {
            ConnectionExit::Disconnected => {
                println!("AGENT RECONNECT IN {}s", config.reconnect.as_secs());
                time::sleep(config.reconnect).await;
            }
            ConnectionExit::Shutdown => {
                println!("AGENT SHUTDOWN BY SERVER");
                return Ok(());
            }
        }
    }
}

async fn run_connection(config: &AgentConfig) -> Result<ConnectionExit, AnyError> {
    println!("AGENT CONNECTING url={}", config.core_url);

    let (ws_stream, _) = connect_async(&config.core_url).await?;
    let (mut write, mut read) = ws_stream.split();

    let hello = ClientFrame::Hello {
        agent_id: config.agent_id,
        token: config.agent_token.clone(),
        hostname: System::host_name().unwrap_or_else(|| "unknown".into()),
        os: System::name().unwrap_or_else(|| "unknown".into()),
        arch: std::env::consts::ARCH.to_string(),
        version: env!("CARGO_PKG_VERSION").into(),
        ts: now_ms(),
    };

    write
        .send(Message::Text(serde_json::to_string(&hello)?.into()))
        .await?;

    println!("AGENT CONNECTED agent_id={}", config.agent_id);

    let mut sys = System::new_all();
    let mut disks = Disks::new_with_refreshed_list();
    let mut networks = Networks::new_with_refreshed_list();
    let users = Users::new_with_refreshed_list();
    let mut tick = time::interval(config.interval);
    let mut process_stream = true;
    let mut last_process_refresh = Instant::now()
        .checked_sub(config.process_interval)
        .unwrap_or_else(Instant::now);

    loop {
        tokio::select! {
            _ = tick.tick() => {
                sys.refresh_cpu_usage();
                sys.refresh_memory();
                let should_send_processes =
                    process_stream && last_process_refresh.elapsed() >= config.process_interval;

                if should_send_processes {
                    sys.refresh_processes_specifics(
                        ProcessesToUpdate::All,
                        true,
                        process_refresh_kind(config),
                    );
                    last_process_refresh = Instant::now();
                }

                disks.refresh(true);
                networks.refresh(true);
                let disk = disk_usage(&disks);
                let network = network_usage(&networks);
                let gpu = gpu_usage();
                let disk_usage_percent = if disk.total > 0 {
                    disk.used as f64 / disk.total as f64 * 100.0
                } else {
                    0.0
                };
                let load = System::load_average();

                let mut metrics = vec![
                    MetricPoint {
                        name: "cpu_usage".to_string(),
                        value: sys.global_cpu_usage() as f64,
                        unit: "percent",
                    },
                    MetricPoint {
                        name: "load_1".to_string(),
                        value: load.one,
                        unit: "load",
                    },
                    MetricPoint {
                        name: "load_5".to_string(),
                        value: load.five,
                        unit: "load",
                    },
                    MetricPoint {
                        name: "load_15".to_string(),
                        value: load.fifteen,
                        unit: "load",
                    },
                    MetricPoint {
                        name: "ram_used".to_string(),
                        value: sys.used_memory() as f64,
                        unit: "bytes",
                    },
                    MetricPoint {
                        name: "ram_total".to_string(),
                        value: sys.total_memory() as f64,
                        unit: "bytes",
                    },
                    MetricPoint {
                        name: "swap_used".to_string(),
                        value: sys.used_swap() as f64,
                        unit: "bytes",
                    },
                    MetricPoint {
                        name: "swap_total".to_string(),
                        value: sys.total_swap() as f64,
                        unit: "bytes",
                    },
                    MetricPoint {
                        name: "disk_used".to_string(),
                        value: disk.used as f64,
                        unit: "bytes",
                    },
                    MetricPoint {
                        name: "disk_total".to_string(),
                        value: disk.total as f64,
                        unit: "bytes",
                    },
                    MetricPoint {
                        name: "disk_usage_percent".to_string(),
                        value: disk_usage_percent,
                        unit: "percent",
                    },
                    MetricPoint {
                        name: "disk_read_bytes".to_string(),
                        value: disk.read_bytes as f64,
                        unit: "bytes_per_second",
                    },
                    MetricPoint {
                        name: "disk_written_bytes".to_string(),
                        value: disk.written_bytes as f64,
                        unit: "bytes_per_second",
                    },
                    MetricPoint {
                        name: "disk_total_read_bytes".to_string(),
                        value: disk.total_read_bytes as f64,
                        unit: "bytes",
                    },
                    MetricPoint {
                        name: "disk_total_written_bytes".to_string(),
                        value: disk.total_written_bytes as f64,
                        unit: "bytes",
                    },
                    MetricPoint {
                        name: "network_received_bytes".to_string(),
                        value: network.received_bytes as f64,
                        unit: "bytes_per_second",
                    },
                    MetricPoint {
                        name: "network_transmitted_bytes".to_string(),
                        value: network.transmitted_bytes as f64,
                        unit: "bytes_per_second",
                    },
                    MetricPoint {
                        name: "network_total_received_bytes".to_string(),
                        value: network.total_received_bytes as f64,
                        unit: "bytes",
                    },
                    MetricPoint {
                        name: "network_total_transmitted_bytes".to_string(),
                        value: network.total_transmitted_bytes as f64,
                        unit: "bytes",
                    },
                ];

                if let Some(gpu) = gpu {
                    metrics.extend([
                        MetricPoint {
                            name: "gpu_usage_percent".to_string(),
                            value: gpu.usage_percent,
                            unit: "percent",
                        },
                        MetricPoint {
                            name: "gpu_memory_used".to_string(),
                            value: gpu.memory_used,
                            unit: "bytes",
                        },
                        MetricPoint {
                            name: "gpu_memory_total".to_string(),
                            value: gpu.memory_total,
                            unit: "bytes",
                        },
                        MetricPoint {
                            name: "gpu_temperature_celsius".to_string(),
                            value: gpu.temperature_celsius,
                            unit: "celsius",
                        },
                    ]);
                }

                for (index, cpu) in sys.cpus().iter().enumerate() {
                    metrics.push(MetricPoint {
                        name: format!("cpu_core_{index}"),
                        value: cpu.cpu_usage() as f64,
                        unit: "percent",
                    });
                }

                let frame = ClientFrame::MetricsBatch {
                    agent_id: config.agent_id,
                    ts: now_ms(),
                    metrics,
                    processes: if should_send_processes {
                        Some(top_processes(&sys, &users, config))
                    } else {
                        None
                    },
                };

                write
                    .send(Message::Text(serde_json::to_string(&frame)?.into()))
                    .await?;
            }
            message = read.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    match handle_server_message(
                        text.as_str(),
                        config,
                        &mut write,
                        &mut process_stream,
                    ).await? {
                        ServerAction::Continue => {}
                        ServerAction::Reconnect => return Ok(ConnectionExit::Disconnected),
                        ServerAction::Shutdown => return Ok(ConnectionExit::Shutdown),
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    return Ok(ConnectionExit::Disconnected);
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(Box::new(error)),
            }
        }
    }
}

#[derive(Default)]
struct DiskTotals {
    used: u64,
    total: u64,
    read_bytes: u64,
    written_bytes: u64,
    total_read_bytes: u64,
    total_written_bytes: u64,
}

#[derive(Default)]
struct NetworkTotals {
    received_bytes: u64,
    transmitted_bytes: u64,
    total_received_bytes: u64,
    total_transmitted_bytes: u64,
}

#[derive(Default)]
struct GpuTotals {
    usage_percent: f64,
    memory_used: f64,
    memory_total: f64,
    temperature_celsius: f64,
}

fn disk_usage(disks: &Disks) -> DiskTotals {
    let (used, total_space) = disk_capacity(disks);

    disks.iter().fold(
        DiskTotals {
            used,
            total: total_space,
            ..DiskTotals::default()
        },
        |mut total, disk| {
            let usage = disk.usage();

            total.read_bytes = total.read_bytes.saturating_add(usage.read_bytes);
            total.written_bytes = total.written_bytes.saturating_add(usage.written_bytes);
            total.total_read_bytes = total
                .total_read_bytes
                .saturating_add(usage.total_read_bytes);
            total.total_written_bytes = total
                .total_written_bytes
                .saturating_add(usage.total_written_bytes);
            total
        },
    )
}

fn disk_capacity(disks: &Disks) -> (u64, u64) {
    #[cfg(target_os = "macos")]
    {
        if let Some(disk) = disks
            .iter()
            .find(|disk| disk.mount_point() == Path::new("/System/Volumes/Data"))
            .or_else(|| {
                disks
                    .iter()
                    .find(|disk| disk.mount_point() == Path::new("/"))
            })
        {
            let total = disk.total_space();
            return (total.saturating_sub(disk.available_space()), total);
        }
    }

    if let Some(disk) = disks
        .iter()
        .find(|disk| disk.mount_point() == Path::new("/"))
    {
        let total = disk.total_space();
        return (total.saturating_sub(disk.available_space()), total);
    }

    disks.iter().fold((0_u64, 0_u64), |(used, total), disk| {
        let disk_total = disk.total_space();
        (
            used.saturating_add(disk_total.saturating_sub(disk.available_space())),
            total.saturating_add(disk_total),
        )
    })
}

fn network_usage(networks: &Networks) -> NetworkTotals {
    networks
        .iter()
        .fold(NetworkTotals::default(), |mut total, (_name, network)| {
            total.received_bytes = total.received_bytes.saturating_add(network.received());
            total.transmitted_bytes = total
                .transmitted_bytes
                .saturating_add(network.transmitted());
            total.total_received_bytes = total
                .total_received_bytes
                .saturating_add(network.total_received());
            total.total_transmitted_bytes = total
                .total_transmitted_bytes
                .saturating_add(network.total_transmitted());
            total
        })
}

fn gpu_usage() -> Option<GpuTotals> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let mut total = GpuTotals::default();
    let mut count = 0.0;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let values = line
            .split(',')
            .map(|value| value.trim().parse::<f64>().ok())
            .collect::<Vec<_>>();

        let (Some(usage), Some(memory_used), Some(memory_total), Some(temperature)) = (
            values.first().copied().flatten(),
            values.get(1).copied().flatten(),
            values.get(2).copied().flatten(),
            values.get(3).copied().flatten(),
        ) else {
            continue;
        };

        total.usage_percent += usage;
        total.memory_used += memory_used * 1024.0 * 1024.0;
        total.memory_total += memory_total * 1024.0 * 1024.0;
        total.temperature_celsius += temperature;
        count += 1.0;
    }

    if count == 0.0 {
        return None;
    }

    total.usage_percent /= count;
    total.temperature_celsius /= count;
    Some(total)
}

fn process_refresh_kind(config: &AgentConfig) -> ProcessRefreshKind {
    let mut kind = ProcessRefreshKind::nothing()
        .with_cpu()
        .with_memory()
        .with_disk_usage()
        .with_user(UpdateKind::OnlyIfNotSet)
        .with_tasks();

    if config.include_command {
        kind = kind.with_cmd(UpdateKind::OnlyIfNotSet);
    }

    if config.include_paths {
        kind = kind
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cwd(UpdateKind::OnlyIfNotSet)
            .with_root(UpdateKind::OnlyIfNotSet);
    }

    if config.include_environment_count {
        kind = kind.with_environ(UpdateKind::OnlyIfNotSet);
    }

    kind
}

fn top_processes(sys: &System, users: &Users, config: &AgentConfig) -> Vec<ProcessPoint> {
    let parent_pids = parent_pid_map();

    let mut processes = sys
        .processes()
        .values()
        .filter(|process| process.thread_kind().is_none())
        .map(|process| {
            let command = if config.include_command {
                compact_text(
                    if process.cmd().is_empty() {
                        process
                            .exe()
                            .map(|path| path.display().to_string())
                            .unwrap_or_default()
                    } else {
                        process
                            .cmd()
                            .iter()
                            .map(|part| part.to_string_lossy())
                            .collect::<Vec<_>>()
                            .join(" ")
                    },
                    config.max_command_length,
                )
            } else {
                String::new()
            };

            let pid = process.pid().as_u32();
            let parent_pid = parent_pids
                .get(&pid)
                .copied()
                .or_else(|| process.parent().map(|pid| pid.as_u32()))
                .filter(|parent_pid| *parent_pid > 0 && *parent_pid != pid);

            ProcessPoint {
                pid,
                parent_pid,
                name: compact_text(process.name().to_string_lossy(), 96),
                command,
                exe: process_path(process.exe(), config),
                cwd: process_path(process.cwd(), config),
                root: process_path(process.root(), config),
                user: process
                    .user_id()
                    .and_then(|user_id| users.get_user_by_id(user_id))
                    .map(|user| user.name().to_string())
                    .unwrap_or_else(|| "-".to_string()),
                effective_user: process
                    .effective_user_id()
                    .and_then(|user_id| users.get_user_by_id(user_id))
                    .map(|user| user.name().to_string())
                    .unwrap_or_else(|| "-".to_string()),
                group_id: process
                    .group_id()
                    .map(|group_id| group_id.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                status: process.status().to_string(),
                threads: process.tasks().map(|tasks| tasks.len()).unwrap_or(1),
                memory_bytes: process.memory(),
                virtual_memory_bytes: process.virtual_memory(),
                cpu_usage: process.cpu_usage() as f64,
                start_time: process.start_time(),
                run_time: process.run_time(),
                disk_read_bytes: process.disk_usage().read_bytes,
                disk_written_bytes: process.disk_usage().written_bytes,
                total_disk_read_bytes: process.disk_usage().total_read_bytes,
                total_disk_written_bytes: process.disk_usage().total_written_bytes,
                environment_count: if config.include_environment_count {
                    process.environ().len()
                } else {
                    0
                },
            }
        })
        .collect::<Vec<_>>();

    processes.sort_by(|left, right| {
        right
            .cpu_usage
            .partial_cmp(&left.cpu_usage)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.memory_bytes.cmp(&left.memory_bytes))
    });

    if !config.send_all_processes && processes.len() > config.process_limit {
        processes.truncate(config.process_limit);
    }

    processes
}

fn process_path(path: Option<&Path>, config: &AgentConfig) -> String {
    if !config.include_paths {
        return String::new();
    }

    compact_text(
        path.map(|path| path.display().to_string())
            .unwrap_or_default(),
        config.max_path_length,
    )
}

fn compact_text(value: impl AsRef<str>, max_chars: usize) -> String {
    let value = value.as_ref();
    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    let keep = max_chars.saturating_sub(1);
    let mut compacted = value.chars().take(keep).collect::<String>();
    compacted.push('~');
    compacted
}

#[cfg(unix)]
fn parent_pid_map() -> HashMap<u32, u32> {
    let output = Command::new("ps").args(["-axo", "pid=,ppid="]).output();

    let Ok(output) = output else {
        return HashMap::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut values = line.split_whitespace();
            let pid = values.next()?.parse().ok()?;
            let parent_pid = values.next()?.parse().ok()?;
            Some((pid, parent_pid))
        })
        .collect()
}

#[cfg(not(unix))]
fn parent_pid_map() -> HashMap<u32, u32> {
    HashMap::new()
}

async fn handle_server_message(
    text: &str,
    config: &AgentConfig,
    write: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    process_stream: &mut bool,
) -> Result<ServerAction, AnyError> {
    match serde_json::from_str::<ServerFrame>(text) {
        Ok(ServerFrame::Ping { ts }) => {
            if let Some(ts) = ts {
                println!("AGENT SERVER PING ts={ts}");
            }
        }
        Ok(ServerFrame::Shutdown { reason }) => {
            if let Some(reason) = reason {
                println!("AGENT SERVER SHUTDOWN reason={reason}");
            }
            return Ok(ServerAction::Shutdown);
        }
        Ok(ServerFrame::Command {
            id,
            action,
            enabled,
        }) => {
            let (ok, output, server_action) =
                run_agent_command(config, &action, enabled, process_stream);
            send_command_result(write, config, id, action, ok, output).await?;
            return Ok(server_action);
        }
        Err(_) => {
            println!("AGENT SERVER MESSAGE {text}");
        }
    }

    Ok(ServerAction::Continue)
}

fn run_agent_command(
    config: &AgentConfig,
    action: &str,
    enabled: Option<bool>,
    process_stream: &mut bool,
) -> (bool, String, ServerAction) {
    match action {
        "set_process_stream" => {
            *process_stream = enabled.unwrap_or(!*process_stream);
            (
                true,
                format!(
                    "Process stream is {}",
                    if *process_stream {
                        "enabled"
                    } else {
                        "disabled"
                    }
                ),
                ServerAction::Continue,
            )
        }
        "reconnect" => (
            true,
            format!("Reconnect requested. Next target: {}", config.core_url),
            ServerAction::Reconnect,
        ),
        "refresh_inventory" => (
            true,
            format!(
                "Inventory refreshed: host={} os={} arch={} version={}",
                System::host_name().unwrap_or_else(|| "unknown".into()),
                System::name().unwrap_or_else(|| "unknown".into()),
                std::env::consts::ARCH,
                env!("CARGO_PKG_VERSION")
            ),
            ServerAction::Continue,
        ),
        "diagnose" => (
            true,
            diagnose_output(config, *process_stream),
            ServerAction::Continue,
        ),
        "docker_ps" => command_output(
            "docker",
            &[
                "ps",
                "--format",
                "table {{.Names}}\t{{.Image}}\t{{.Status}}\t{{.Ports}}",
            ],
        ),
        "logs" => (
            true,
            format!(
                "Agent is running. id={} core={} interval={}s process_stream={}",
                config.agent_id,
                config.core_url,
                config.interval.as_secs(),
                *process_stream
            ),
            ServerAction::Continue,
        ),
        _ => (
            false,
            format!("Unknown command: {action}"),
            ServerAction::Continue,
        ),
    }
}

fn command_output(program: &str, args: &[&str]) -> (bool, String, ServerAction) {
    match Command::new(program).args(args).output() {
        Ok(output) if output.status.success() => (
            true,
            truncate_output(String::from_utf8_lossy(&output.stdout).trim()),
            ServerAction::Continue,
        ),
        Ok(output) => (
            false,
            truncate_output(String::from_utf8_lossy(&output.stderr).trim()),
            ServerAction::Continue,
        ),
        Err(error) => (
            false,
            format!("{program} is unavailable: {error}"),
            ServerAction::Continue,
        ),
    }
}

fn diagnose_output(config: &AgentConfig, process_stream: bool) -> String {
    let docker = Command::new("docker")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_else(|| "docker unavailable".to_string());

    format!(
        "core={} agent_id={} agent_id_path={} agent_token={} reconnect={}s interval={}s process_stream={} process_limit={} send_all={} process_interval={}s include_command={} include_paths={} include_env_count={} max_command={} max_path={} host={} os={} arch={} docker={}",
        config.core_url,
        config.agent_id,
        config.agent_id_path.display(),
        if config.agent_token.is_some() {
            "configured"
        } else {
            "missing"
        },
        config.reconnect.as_secs(),
        config.interval.as_secs(),
        process_stream,
        config.process_limit,
        config.send_all_processes,
        config.process_interval.as_secs(),
        config.include_command,
        config.include_paths,
        config.include_environment_count,
        config.max_command_length,
        config.max_path_length,
        System::host_name().unwrap_or_else(|| "unknown".into()),
        System::name().unwrap_or_else(|| "unknown".into()),
        std::env::consts::ARCH,
        docker
    )
}

fn truncate_output(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    if value.is_empty() {
        return "command completed with empty output".to_string();
    }
    value.chars().take(4_000).collect()
}

async fn send_command_result(
    write: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    config: &AgentConfig,
    id: String,
    action: String,
    ok: bool,
    output: String,
) -> Result<(), AnyError> {
    let frame = ClientFrame::CommandResult {
        agent_id: config.agent_id,
        result: CommandResult {
            id,
            action,
            ok,
            output,
            ts: now_ms(),
        },
    };

    write
        .send(Message::Text(serde_json::to_string(&frame)?.into()))
        .await?;
    Ok(())
}
