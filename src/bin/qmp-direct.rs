//! Direct QEMU QMP concurrency demo.
//!
//! This binary intentionally bypasses qmp-uds-mon-manager. It starts many independent
//! QMP clients against one real QEMU monitor socket and sends the same command
//! from each client.
//!
//! The expected contrast is with `qmp-parallel`: direct concurrent access to a
//! QEMU QMP socket can fail or time out because the monitor is a single protocol
//! stream, while qmp-uds-mon-manager gives clients a QMP-looking socket and serializes
//! backend access.

use std::{io, path::PathBuf, time::Duration, time::Instant};

use clap::Parser;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    task::JoinSet,
    time::timeout,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() {
    init_tracing();

    if let Err(error) = run().await {
        error!(%error, "qmp-direct failed");
        std::process::exit(1);
    }
}

/// Initialize process-wide structured logging.
///
/// `RUST_LOG` controls verbosity. Without it, the demo logs at `info`.
fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("qmp_direct=info"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}

async fn run() -> Result<(), String> {
    let config = Config::try_from(CliArgs::parse())?;

    info!(
        qmp_socket = %config.qmp_socket.display(),
        clients = config.clients,
        timeout_ms = config.timeout.as_millis(),
        "starting qmp-direct demo"
    );

    let started = Instant::now();
    let mut tasks = JoinSet::new();

    for client_id in 0..config.clients {
        let qmp_socket = config.qmp_socket.clone();
        let command = config.command.clone();
        let timeout_duration = config.timeout;

        tasks.spawn(async move {
            let started = Instant::now();
            debug!(
                client_id,
                qmp_socket = %qmp_socket.display(),
                "starting direct QMP client"
            );
            let response = timeout(
                timeout_duration,
                run_direct_qmp_client(&qmp_socket, command),
            )
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("direct QMP client timed out after {timeout_duration:?}"),
                )
            })
            .and_then(|result| result);

            (client_id, started.elapsed(), response)
        });
    }

    let mut succeeded = 0usize;
    let mut failed = 0usize;

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((client_id, elapsed, Ok(response))) => {
                succeeded += 1;
                info!(
                    client_id,
                    elapsed_ms = elapsed.as_millis(),
                    "direct QMP client succeeded"
                );
                debug!(client_id, response, "direct QMP client response");
            }
            Ok((client_id, elapsed, Err(error))) => {
                failed += 1;
                warn!(
                    client_id,
                    elapsed_ms = elapsed.as_millis(),
                    %error,
                    "direct QMP client failed"
                );
            }
            Err(error) => {
                failed += 1;
                warn!(%error, "direct QMP client task failed");
            }
        }
    }

    let total = succeeded + failed;
    info!(
        elapsed_ms = started.elapsed().as_millis(),
        total, succeeded, failed, "qmp-direct demo finished"
    );

    if failed > 0 {
        warn!(
            total,
            succeeded, failed, "qmp-direct completed with client failures"
        );
    }

    Ok(())
}

/// Run one independent QMP client against a real QEMU monitor socket.
async fn run_direct_qmp_client(qmp_socket: &PathBuf, command: Value) -> Result<String, io::Error> {
    let stream = UnixStream::connect(qmp_socket).await?;
    let mut stream = BufReader::new(stream);
    debug!(qmp_socket = %qmp_socket.display(), "connected to real QEMU QMP socket");

    let greeting = read_json_line(&mut stream).await?;
    if greeting.get("QMP").is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected QMP greeting, got {greeting}"),
        ));
    }
    debug!("received QMP greeting");

    write_json_line(stream.get_mut(), &json!({ "execute": "qmp_capabilities" })).await?;
    let capabilities = read_json_line(&mut stream).await?;
    if capabilities.get("return").is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("qmp_capabilities failed: {capabilities}"),
        ));
    }
    debug!("completed QMP capability negotiation");

    debug!(command = %command, "sending QMP command directly to QEMU");
    write_json_line(stream.get_mut(), &command).await?;

    let mut events = Vec::new();
    loop {
        let message = read_json_line(&mut stream).await?;
        if message.get("event").is_some() {
            events.push(message);
            continue;
        }

        if message.get("return").is_some() || message.get("error").is_some() {
            if events.is_empty() {
                debug!(response = %message, "received QMP response");
                return Ok(message.to_string());
            }

            debug!(
                response = %message,
                event_count = events.len(),
                "received QMP response with preceding events"
            );
            return Ok(json!({
                "response": message,
                "events": events,
            })
            .to_string());
        }

        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unexpected QMP message: {message}"),
        ));
    }
}

/// Read one JSON-line QMP message.
async fn read_json_line(stream: &mut BufReader<UnixStream>) -> Result<Value, io::Error> {
    let mut line = String::new();
    let bytes = stream.read_line(&mut line).await?;

    if bytes == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "socket closed",
        ));
    }

    serde_json::from_str(&line).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Write one JSON-line QMP message.
async fn write_json_line(stream: &mut UnixStream, value: &Value) -> Result<(), io::Error> {
    let mut line = serde_json::to_vec(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    line.push(b'\n');

    stream.write_all(&line).await?;
    stream.flush().await
}

/// Command-line options for the direct QEMU QMP client demo.
#[derive(Debug, Parser)]
#[command(author, version, about)]
struct CliArgs {
    /// Real QEMU QMP socket path.
    #[arg(short = 'q', long, value_name = "QEMU_QMP_SOCKET")]
    qmp_socket: PathBuf,

    /// Number of concurrent QMP clients to run.
    #[arg(short = 'c', long, value_name = "CLIENTS")]
    clients: usize,

    /// Per-client timeout in milliseconds.
    #[arg(short = 't', long, value_name = "TIMEOUT_MS", default_value_t = 5_000)]
    timeout_ms: u64,

    /// Optional raw QMP command JSON. Defaults to `{"execute":"query-status"}`.
    #[arg(short = 'j', long, value_name = "QMP_JSON")]
    qmp_json: Option<String>,
}

/// Command-line configuration for the demo run.
struct Config {
    qmp_socket: PathBuf,
    clients: usize,
    timeout: Duration,
    command: Value,
}

impl TryFrom<CliArgs> for Config {
    type Error = String;

    fn try_from(args: CliArgs) -> Result<Self, Self::Error> {
        if args.clients == 0 {
            return Err("clients must be greater than zero".to_string());
        }

        if args.timeout_ms == 0 {
            return Err("timeout-ms must be greater than zero".to_string());
        }

        let command = match args.qmp_json {
            Some(raw) => serde_json::from_str(&raw)
                .map_err(|error| format!("invalid QMP JSON command: {error}"))?,
            None => json!({ "execute": "query-status" }),
        };

        Ok(Self {
            qmp_socket: args.qmp_socket,
            clients: args.clients,
            timeout: Duration::from_millis(args.timeout_ms),
            command,
        })
    }
}
