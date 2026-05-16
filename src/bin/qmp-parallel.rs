//! Demonstration client for qmp-uds-mon-manager's client-facing QMP socket.
//!
//! The program registers one or more VMs through qmp-uds-mon-manager's HTTP-over-UDS
//! API, then starts many independent QMP clients per VM. Clients for a given VM
//! all connect to that VM's manager-owned `client_socket`. Each client performs
//! the normal QMP greeting and capability handshake before sending the same
//! command.
//!
//! This is intentionally a small protocol-level client instead of a benchmark
//! harness. Its purpose is to show that concurrent clients can use one
//! QMP-looking socket while qmp-uds-mon-manager serializes backend access to each QEMU.

use std::{io, path::PathBuf, time::Instant};

use clap::Parser;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    task::JoinSet,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{EnvFilter, fmt};

#[tokio::main]
async fn main() {
    init_tracing();

    if let Err(error) = run().await {
        error!(%error, "qmp-parallel failed");
        std::process::exit(1);
    }
}

/// Initialize process-wide structured logging.
///
/// `RUST_LOG` controls verbosity. Without it, the demo logs at `info`.
fn init_tracing() {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("qmp_parallel=info"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}

async fn run() -> Result<(), String> {
    let config = Config::try_from(CliArgs::parse())?;

    info!(
        manager_socket = %config.manager_socket.display(),
        vm_count = config.vms.len(),
        clients_per_vm = config.clients_per_vm,
        "starting qmp-parallel demo"
    );

    let mut registered_vms = Vec::new();
    let mut registration_errors = Vec::new();
    for vm in &config.vms {
        if let Err(error) = register_vm(&config.manager_socket, vm).await {
            warn!(
                vm_id = %vm.id,
                manager_socket = %config.manager_socket.display(),
                %error,
                "failed to register VM with qmp-uds-mon-manager"
            );
            registration_errors.push(format!("{}: {error}", vm.id));
            continue;
        }
        registered_vms.push(vm.clone());

        info!(
            vm_id = %vm.id,
            qmp_socket = %vm.qmp_socket.display(),
            client_socket = %vm.client_socket.display(),
            "registered VM with qmp-uds-mon-manager"
        );
    }

    let started = Instant::now();
    let mut tasks = JoinSet::new();

    // Each task is a separate QMP client connection. The manager accepts these
    // concurrently, but forwards commands to the backend QEMU monitor in order.
    for vm in &config.vms {
        for client_id in 0..config.clients_per_vm {
            let vm_id = vm.id.clone();
            let client_socket = vm.client_socket.clone();
            let command = config.command.clone();

            tasks.spawn(async move {
                let started = Instant::now();
                debug!(
                    vm_id = %vm_id,
                    client_id,
                    client_socket = %client_socket.display(),
                    "starting QMP client"
                );
                let response = run_qmp_client(&client_socket, command).await;
                (vm_id, client_id, started.elapsed(), response)
            });
        }
    }

    let mut succeeded = 0usize;
    let mut failed = 0usize;

    while let Some(result) = tasks.join_next().await {
        match result {
            Ok((vm_id, client_id, elapsed, Ok(response))) => {
                succeeded += 1;
                info!(
                    vm_id = %vm_id,
                    client_id,
                    elapsed_ms = elapsed.as_millis(),
                    "QMP client succeeded"
                );
                debug!(vm_id = %vm_id, client_id, response, "QMP client response");
            }
            Ok((vm_id, client_id, elapsed, Err(error))) => {
                failed += 1;
                warn!(
                    vm_id = %vm_id,
                    client_id,
                    elapsed_ms = elapsed.as_millis(),
                    %error,
                    "QMP client failed"
                );
            }
            Err(error) => {
                failed += 1;
                warn!(%error, "QMP client task failed");
            }
        }
    }

    let total = succeeded + failed;
    info!(
        elapsed_ms = started.elapsed().as_millis(),
        requested_vms = config.vms.len(),
        registered_vms = registered_vms.len(),
        registration_failed = registration_errors.len(),
        total,
        succeeded,
        failed,
        "qmp-parallel client phase finished"
    );

    if failed > 0 {
        warn!(
            total,
            succeeded, failed, "qmp-parallel completed with client failures"
        );
    }

    // Best-effort cleanup is part of the test contract. Preserve the client
    // failure signal, but still report cleanup problems when they happen.
    let cleanup_errors = cleanup_registered_vms(&config.manager_socket, &registered_vms).await;

    info!(
        requested_vms = config.vms.len(),
        registered_vms = registered_vms.len(),
        registration_failed = registration_errors.len(),
        cleanup_failed = cleanup_errors.len(),
        total_clients = total,
        clients_succeeded = succeeded,
        clients_failed = failed,
        "qmp-parallel demo finished"
    );

    match (registration_errors.is_empty(), cleanup_errors.is_empty()) {
        (true, true) => Ok(()),
        (false, true) => Err(format!(
            "failed to register one or more VMs: {}",
            registration_errors.join("; ")
        )),
        (true, false) => Err(format!(
            "failed to unregister one or more VMs: {}",
            cleanup_errors.join("; ")
        )),
        (false, false) => Err(format!(
            "failed to register one or more VMs: {}; failed to unregister one or more VMs: {}",
            registration_errors.join("; "),
            cleanup_errors.join("; ")
        )),
    }
}

/// Unregister every VM that was successfully registered by this run.
async fn cleanup_registered_vms(
    manager_socket: &PathBuf,
    registered_vms: &[VmConfig],
) -> Vec<String> {
    let mut cleanup_errors = Vec::new();
    for vm in registered_vms.iter().rev() {
        match unregister_vm(manager_socket, vm).await {
            Ok(()) => info!(vm_id = %vm.id, "unregistered VM from qmp-uds-mon-manager"),
            Err(error) => {
                warn!(vm_id = %vm.id, %error, "failed to unregister VM");
                cleanup_errors.push(format!("{}: {error}", vm.id));
            }
        }
    }
    cleanup_errors
}

/// Register the demo VM using qmp-uds-mon-manager's HTTP management API.
async fn register_vm(manager_socket: &PathBuf, vm: &VmConfig) -> Result<(), io::Error> {
    debug!(vm_id = %vm.id, "sending VM registration request");
    let body = serde_json::to_vec(&json!({
        "qmp_socket": vm.qmp_socket,
        "client_socket": vm.client_socket,
        "move_qmp_socket": vm.move_qmp_socket,
    }))?;

    let response = send_http_request(
        manager_socket,
        &format!("PUT /vms/{} HTTP/1.1", vm.id),
        Some(&body),
    )
    .await?;

    parse_http_response(&response).map(|_| ())
}

/// Remove the demo VM and let the server unlink the client-facing socket.
async fn unregister_vm(manager_socket: &PathBuf, vm: &VmConfig) -> Result<(), io::Error> {
    debug!(vm_id = %vm.id, "sending VM unregister request");
    let response = send_http_request(
        manager_socket,
        &format!("DELETE /vms/{} HTTP/1.1", vm.id),
        None,
    )
    .await?;

    parse_http_response(&response).map(|_| ())
}

/// Run one independent QMP client against the manager-owned client socket.
async fn run_qmp_client(client_socket: &PathBuf, command: Value) -> Result<String, io::Error> {
    let stream = UnixStream::connect(client_socket).await?;
    let mut stream = BufReader::new(stream);
    debug!(client_socket = %client_socket.display(), "connected to client-facing QMP socket");

    // qmp-uds-mon-manager replays QEMU's greeting so normal QMP clients can start the
    // same way they would when connected directly to QEMU.
    let greeting = read_json_line(&mut stream).await?;
    if greeting.get("QMP").is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("expected QMP greeting, got {greeting}"),
        ));
    }
    debug!("received QMP greeting");

    // The front-side handshake is per client and is answered by qmp-uds-mon-manager.
    // The backend QEMU connection was already negotiated during registration.
    write_json_line(stream.get_mut(), &json!({ "execute": "qmp_capabilities" })).await?;
    let capabilities = read_json_line(&mut stream).await?;
    if capabilities.get("return").is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("qmp_capabilities failed: {capabilities}"),
        ));
    }
    debug!("completed front-side QMP capability negotiation");

    debug!(command = %command, "sending QMP command");
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

/// Send a minimal HTTP/1.1 request over qmp-uds-mon-manager's Unix socket.
async fn send_http_request(
    manager_socket: &PathBuf,
    request_line: &str,
    body: Option<&[u8]>,
) -> Result<Vec<u8>, io::Error> {
    let body = body.unwrap_or_default();
    let mut stream = UnixStream::connect(manager_socket).await?;
    debug!(
        manager_socket = %manager_socket.display(),
        request_line,
        body_len = body.len(),
        "sending HTTP request"
    );
    let request = format!(
        "{request_line}\r\n\
         Host: qmp-uds-mon-manager\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n",
        body.len()
    );

    stream.write_all(request.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    Ok(response)
}

/// Return the response body for any 2xx HTTP status.
fn parse_http_response(response: &[u8]) -> Result<String, io::Error> {
    let response = std::str::from_utf8(response)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let Some((head, body)) = response.split_once("\r\n\r\n") else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "HTTP response did not contain a header/body separator",
        ));
    };

    let status = head.lines().next().unwrap_or_default();
    if !status.contains(" 2") {
        return Err(io::Error::other(format!("{status}: {body}")));
    }

    Ok(body.to_string())
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

/// Command-line options for the concurrent QMP client demo.
#[derive(Debug, Parser)]
#[command(author, version, about)]
struct CliArgs {
    /// qmp-uds-mon-manager HTTP management Unix socket.
    #[arg(short = 'm', long, value_name = "MANAGER_SOCKET")]
    manager_socket: PathBuf,

    /// VM id or VM id prefix. Multi-VM runs produce `<prefix>-<index>`.
    #[arg(short = 'p', long, value_name = "VM_ID_PREFIX")]
    vm_id_prefix: String,

    /// Real QEMU QMP socket path or template. Multi-VM runs require `{i}`.
    #[arg(short = 'q', long, value_name = "QEMU_QMP_SOCKET_TEMPLATE")]
    qmp_socket_template: String,

    /// qmp-uds-mon-manager client-facing socket path or template. Multi-VM runs require `{i}`.
    #[arg(short = 's', long, value_name = "CLIENT_SOCKET_TEMPLATE")]
    client_socket_template: String,

    /// Ask qmp-uds-mon-manager to move the real QEMU socket aside and serve clients at the original path.
    #[arg(long)]
    move_qmp_socket: bool,

    /// Number of VMs to register for this run.
    #[arg(short = 'n', long, value_name = "VM_COUNT")]
    vm_count: usize,

    /// Number of concurrent QMP clients to run per VM.
    #[arg(short = 'c', long, value_name = "CLIENTS_PER_VM")]
    clients_per_vm: usize,

    /// Optional raw QMP command JSON. Defaults to `{"execute":"query-status"}`.
    #[arg(short = 'j', long, value_name = "QMP_JSON")]
    qmp_json: Option<String>,
}

/// Command-line configuration for the demo run.
struct Config {
    manager_socket: PathBuf,
    vms: Vec<VmConfig>,
    clients_per_vm: usize,
    command: Value,
}

/// Concrete socket paths for one registered VM.
#[derive(Clone)]
struct VmConfig {
    id: String,
    qmp_socket: PathBuf,
    client_socket: PathBuf,
    move_qmp_socket: bool,
}

impl TryFrom<CliArgs> for Config {
    type Error = String;

    fn try_from(args: CliArgs) -> Result<Self, Self::Error> {
        if args.vm_count == 0 {
            return Err("VM count must be greater than zero".to_string());
        }

        if args.clients_per_vm == 0 {
            return Err("clients-per-vm must be greater than zero".to_string());
        }

        let command = match args.qmp_json {
            Some(raw) => serde_json::from_str(&raw)
                .map_err(|error| format!("invalid QMP JSON command: {error}"))?,
            None => json!({ "execute": "query-status" }),
        };

        let mut vms = Vec::with_capacity(args.vm_count);

        for index in 0..args.vm_count {
            vms.push(VmConfig {
                id: format_vm_id(&args.vm_id_prefix, index, args.vm_count),
                qmp_socket: render_template(
                    &args.qmp_socket_template,
                    index,
                    args.vm_count,
                    "qemu-qmp-socket-template",
                )?,
                client_socket: render_template(
                    &args.client_socket_template,
                    index,
                    args.vm_count,
                    "client-socket-template",
                )?,
                move_qmp_socket: args.move_qmp_socket,
            });
        }

        Ok(Self {
            manager_socket: args.manager_socket,
            vms,
            clients_per_vm: args.clients_per_vm,
            command,
        })
    }
}

/// Build a VM id, preserving the exact prefix when only one VM is requested.
fn format_vm_id(prefix: &str, index: usize, vm_count: usize) -> String {
    if vm_count == 1 {
        prefix.to_string()
    } else {
        format!("{prefix}-{index}")
    }
}

/// Render a socket template for one VM.
///
/// Templates for multi-VM runs must include `{i}`. A single-VM run may use an
/// exact path without a placeholder.
fn render_template(
    template: &str,
    index: usize,
    vm_count: usize,
    name: &str,
) -> Result<PathBuf, String> {
    if template.contains("{i}") {
        return Ok(PathBuf::from(template.replace("{i}", &index.to_string())));
    }

    if vm_count == 1 {
        return Ok(PathBuf::from(template));
    }

    Err(format!(
        "{name} must contain '{{i}}' when VM count is greater than one"
    ))
}
