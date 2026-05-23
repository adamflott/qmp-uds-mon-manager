#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![forbid(unsafe_code)]

//! Unix-socket HTTP and QMP proxy for serializing access to QEMU monitors.
//!
//! QEMU's QMP monitor is a single JSON-line protocol stream. This server keeps
//! one persistent connection to each registered backend monitor and exposes two
//! front doors:
//!
//! - an HTTP API, served only over a Unix domain socket, for registration,
//!   removal, listing, and raw passthrough;
//! - one QMP-compatible Unix socket per registered VM for normal QMP clients.
//!
//! All requests for a VM are sent to one Tokio channel. A single worker owns the
//! backend QMP connection and processes that channel sequentially.

use std::{
    collections::HashMap,
    fs, io,
    os::unix::fs::FileTypeExt,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    Json, Router,
    extract::{Path as AxumPath, State},
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use clap::Parser;
use hyper::body::Incoming;
use hyper_util::{rt::TokioExecutor, rt::TokioIo, server::conn::auto::Builder};
use qapi::{qmp::QapiCapabilities, qmp::QmpMessageAny};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::{RwLock, mpsc, oneshot},
    task::JoinHandle,
};
use tower::Service;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{EnvFilter, fmt};

/// Command-line options for the qmp-uds-mon-manager server.
#[derive(Debug, Parser, Clone)]
#[command(author, version, about)]
struct Args {
    /// Request queue depth
    #[arg(short = 'q', long, value_name = "QUEUE_DEPTH", default_value = "128")]
    queue_depth: usize,

    /// Unix domain socket path for the HTTP management API.
    #[arg(
        short = 's',
        long,
        value_name = "MANAGER_SOCKET",
        default_value = "/tmp/qmp-uds-mon-manager.sock"
    )]
    socket_path: PathBuf,
}

#[derive(Clone)]
struct AppState {
    args: Args,

    /// Registered VMs keyed by caller-provided id.
    vms: Arc<RwLock<HashMap<String, VmHandle>>>,
}

impl AppState {
    fn new(args: Args) -> Self {
        Self {
            args,
            vms: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

/// Runtime state for one managed QEMU instance.
struct VmHandle {
    /// Public QEMU monitor socket path supplied at registration time.
    qmp_socket: PathBuf,
    /// Real QEMU monitor socket path used by the backend worker.
    backend_qmp_socket: PathBuf,
    /// Manager-owned socket exposed to QMP clients.
    client_socket: PathBuf,
    /// Socket rename state when qmp-uds-mon-manager hides the real QEMU socket.
    socket_move: Option<SocketMove>,
    /// Queue shared by the HTTP passthrough and all client-facing QMP sockets.
    commands: mpsc::Sender<QmpRequest>,
    /// Listener task for `client_socket`; aborted when the VM is removed.
    client_listener: JoinHandle<()>,
}

/// Reversible move of QEMU's real QMP socket path.
#[derive(Debug, Clone)]
struct SocketMove {
    /// Original path clients expect to use.
    original: PathBuf,
    /// Generated backend path that only qmp-uds-mon-manager uses while registered.
    backend: PathBuf,
}

/// One queued QMP command and the one-shot response path back to its caller.
struct QmpRequest {
    command: Value,
    response: oneshot::Sender<Result<QmpResponse, QmpError>>,
}

#[derive(Clone)]
struct QmpClientProxy {
    /// Cached greeting from QEMU, replayed to every front-side QMP client.
    greeting: Arc<Value>,
    commands: mpsc::Sender<QmpRequest>,
}

/// Response shape returned by the HTTP passthrough endpoint.
#[derive(Debug, Serialize)]
struct QmpResponse {
    /// Final QMP response, usually containing either `return` or `error`.
    response: Value,
    /// Events observed before the command response arrived.
    events: Vec<Value>,
}

/// Public representation of one registered VM.
#[derive(Debug, Serialize)]
struct VmInfo {
    id: String,
    qmp_socket: PathBuf,
    client_socket: PathBuf,
    moved_qmp_socket: bool,
}

#[derive(Debug, Deserialize)]
struct RegisterVm {
    /// The real QEMU monitor socket this manager connects to.
    qmp_socket: PathBuf,
    /// The manager-owned socket QMP clients should connect to.
    ///
    /// When `move_qmp_socket` is true, this may be omitted and defaults to
    /// `qmp_socket`. If it is provided in that mode, it must equal `qmp_socket`.
    client_socket: Option<PathBuf>,
    /// Move `qmp_socket` aside and expose qmp-uds-mon-manager at the original path.
    #[serde(default)]
    move_qmp_socket: bool,
}

#[derive(Debug)]
enum AppError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    Qmp(QmpError),
}

/// Errors that can occur while speaking to a QMP backend or client.
#[derive(Debug)]
enum QmpError {
    Io(io::Error),
    Json(serde_json::Error),
    Protocol(String),
    Disconnected,
}

impl From<io::Error> for QmpError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<serde_json::Error> for QmpError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<QmpError> for AppError {
    fn from(value: QmpError) -> Self {
        Self::Qmp(value)
    }
}

impl From<io::Error> for AppError {
    fn from(value: io::Error) -> Self {
        Self::Qmp(value.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Conflict(message) => (StatusCode::CONFLICT, message),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
            Self::Qmp(error) => match error {
                QmpError::Io(error) => (StatusCode::BAD_GATEWAY, format!("QMP I/O error: {error}")),
                QmpError::Json(error) => {
                    (StatusCode::BAD_GATEWAY, format!("QMP JSON error: {error}"))
                }
                QmpError::Protocol(message) => (StatusCode::BAD_GATEWAY, message),
                QmpError::Disconnected => (
                    StatusCode::BAD_GATEWAY,
                    "QMP worker disconnected".to_string(),
                ),
            },
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let args = Args::parse();
    let socket_path = args.socket_path.clone();

    remove_stale_socket(&socket_path)?;

    let listener = UnixListener::bind(&socket_path)?;
    let app = Router::new()
        .route("/vms", get(list_vms))
        .route("/vms/{id}", put(register_vm).delete(remove_vm))
        .route("/vms/{id}/qmp", post(qmp_passthrough))
        .with_state(AppState::new(args));

    info!(socket = %socket_path.display(), "management HTTP API listening");
    serve_uds(listener, app).await?;

    Ok(())
}

/// Initialize process-wide structured logging.
///
/// `RUST_LOG` controls verbosity. Without it, qmp-uds-mon-manager logs at `info`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("qmp_uds_mon_manager=info"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}

/// Serve axum over a Unix domain socket.
///
/// axum does not provide a single `serve_uds` helper, so each accepted
/// `UnixStream` is wrapped for hyper and handed to a cloned `Router`.
async fn serve_uds(listener: UnixListener, app: Router) -> io::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let app = app.clone();

        tokio::spawn(async move {
            debug!("accepted HTTP management connection");
            let service = hyper::service::service_fn(move |request: Request<Incoming>| {
                let mut app = app.clone();

                async move { app.call(request).await }
            });

            let stream = TokioIo::new(stream);
            let builder = Builder::new(TokioExecutor::new());

            if let Err(error) = builder
                .serve_connection_with_upgrades(stream, service)
                .await
            {
                warn!(%error, "HTTP management connection ended with error");
            }
        });
    }
}

/// Return registered VM metadata sorted by id for stable output.
async fn list_vms(State(state): State<AppState>) -> Json<Vec<VmInfo>> {
    let vms = state.vms.read().await;
    let mut infos = vms
        .iter()
        .map(|(id, vm)| VmInfo {
            id: id.clone(),
            qmp_socket: vm.qmp_socket.clone(),
            client_socket: vm.client_socket.clone(),
            moved_qmp_socket: vm.socket_move.is_some(),
        })
        .collect::<Vec<_>>();

    infos.sort_by(|left, right| left.id.cmp(&right.id));
    Json(infos)
}

/// Register a VM and immediately connect to its backend QMP socket.
///
/// Registration fails if the backend cannot complete the QMP greeting and
/// `qmp_capabilities` negotiation. This keeps the registry from containing
/// entries that cannot process commands.
async fn register_vm(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<RegisterVm>,
) -> Result<impl IntoResponse, AppError> {
    if id.trim().is_empty() {
        return Err(AppError::BadRequest("VM id must not be empty".to_string()));
    }

    let client_socket = resolve_client_socket(&body)?;

    info!(
        vm_id = %id,
        qmp_socket = %body.qmp_socket.display(),
        client_socket = %client_socket.display(),
        move_qmp_socket = body.move_qmp_socket,
        "registering VM"
    );
    let mut vms = state.vms.write().await;
    if vms.contains_key(&id) {
        return Err(AppError::Conflict(format!("VM '{id}' already exists")));
    }

    if !body.move_qmp_socket && body.qmp_socket == client_socket {
        return Err(AppError::BadRequest(
            "qmp_socket and client_socket must be different paths".to_string(),
        ));
    }

    let socket_move = if body.move_qmp_socket {
        Some(move_qmp_socket_for_registration(&body.qmp_socket)?)
    } else {
        None
    };
    let backend_qmp_socket = socket_move
        .as_ref()
        .map(|move_info| move_info.backend.clone())
        .unwrap_or_else(|| body.qmp_socket.clone());

    let args = state.args;
    let handle = match start_vm_worker(
        &args,
        body.qmp_socket.clone(),
        backend_qmp_socket,
        client_socket.clone(),
        socket_move.clone(),
    )
    .await
    {
        Ok(handle) => handle,
        Err(error) => {
            if let Some(move_info) = &socket_move
                && let Err(restore_error) = restore_moved_qmp_socket(move_info)
            {
                error!(
                    ?restore_error,
                    original = %move_info.original.display(),
                    backend = %move_info.backend.display(),
                    "failed to restore QMP socket after registration failure"
                );
            }
            return Err(error.into());
        }
    };
    vms.insert(id.clone(), handle);

    info!(vm_id = %id, "VM registered");

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": id,
            "qmp_socket": body.qmp_socket,
            "client_socket": client_socket,
            "moved_qmp_socket": body.move_qmp_socket,
        })),
    ))
}

/// Remove a VM, stop its client-facing listener, and unlink its socket.
async fn remove_vm(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> Result<StatusCode, AppError> {
    let removed = state.vms.write().await.remove(&id);
    match removed {
        Some(vm) => {
            info!(
                vm_id = %id,
                client_socket = %vm.client_socket.display(),
                qmp_socket = %vm.qmp_socket.display(),
                backend_qmp_socket = %vm.backend_qmp_socket.display(),
                "removing VM"
            );
            vm.client_listener.abort();
            if let Err(error) = remove_stale_socket(&vm.client_socket) {
                warn!(
                    %error,
                    client_socket = %vm.client_socket.display(),
                    "failed to remove client socket"
                );
            }
            if let Some(move_info) = &vm.socket_move {
                restore_moved_qmp_socket(move_info).map_err(QmpError::from)?;
                info!(
                    original = %move_info.original.display(),
                    backend = %move_info.backend.display(),
                    "restored moved QMP socket"
                );
            }
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err(AppError::NotFound(format!("VM '{id}' does not exist"))),
    }
}

/// Send one raw QMP command through the same queue used by QMP clients.
async fn qmp_passthrough(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
    Json(command): Json<Value>,
) -> Result<Json<QmpResponse>, AppError> {
    validate_qmp_command(&command)?;
    debug!(vm_id = %id, command = %command, "received HTTP QMP passthrough request");

    let sender = {
        let vms = state.vms.read().await;
        vms.get(&id)
            .map(|vm| vm.commands.clone())
            .ok_or_else(|| AppError::NotFound(format!("VM '{id}' does not exist")))?
    };

    let (response, receiver) = oneshot::channel();
    sender
        .send(QmpRequest { command, response })
        .await
        .map_err(|_| QmpError::Disconnected)?;

    let response = receiver.await.map_err(|_| QmpError::Disconnected)??;
    Ok(Json(response))
}

/// Start the serialized backend worker and the client-facing QMP listener.
///
/// The returned `VmHandle` contains cloneable queue senders for callers, but
/// the spawned worker owns the only backend QMP connection.
async fn start_vm_worker(
    args: &Args,
    qmp_socket: PathBuf,
    backend_qmp_socket: PathBuf,
    client_socket: PathBuf,
    socket_move: Option<SocketMove>,
) -> Result<VmHandle, QmpError> {
    info!(
        qmp_socket = %qmp_socket.display(),
        backend_qmp_socket = %backend_qmp_socket.display(),
        client_socket = %client_socket.display(),
        "starting VM worker"
    );
    let mut qmp = QmpConnection::connect(&backend_qmp_socket).await?;
    let greeting = Arc::new(qmp.greeting().clone());
    let (sender, mut receiver) = mpsc::channel::<QmpRequest>(args.queue_depth);
    let remove_stale_client_socket = socket_move.is_none();
    let client_listener = start_qmp_client_listener(
        client_socket.clone(),
        QmpClientProxy {
            greeting,
            commands: sender.clone(),
        },
        remove_stale_client_socket,
    )
    .await?;

    tokio::spawn(async move {
        while let Some(request) = receiver.recv().await {
            debug!(command = %request.command, "forwarding QMP command to backend");
            let result = qmp.execute(request.command).await;
            let should_stop = matches!(result, Err(QmpError::Io(_) | QmpError::Json(_)));
            if let Err(error) = &result {
                warn!(?error, "QMP backend request failed");
            }
            let _ = request.response.send(result);

            if should_stop {
                error!("stopping VM worker after backend connection failure");
                break;
            }
        }
        info!("VM worker stopped");
    });

    Ok(VmHandle {
        qmp_socket,
        backend_qmp_socket,
        client_socket,
        socket_move,
        commands: sender,
        client_listener,
    })
}

/// Persistent connection to a real QEMU QMP monitor.
struct QmpConnection {
    stream: BufReader<UnixStream>,
    greeting: Value,
}

impl QmpConnection {
    /// Connect to QEMU, validate its greeting with qapi, and negotiate caps.
    async fn connect(path: &Path) -> Result<Self, QmpError> {
        info!(qmp_socket = %path.display(), "connecting to backend QMP socket");
        let stream = UnixStream::connect(path).await?;
        let mut connection = Self {
            stream: BufReader::new(stream),
            greeting: Value::Null,
        };

        let greeting = connection.read_json_line().await?;
        let capabilities: QapiCapabilities = serde_json::from_value(greeting.clone())?;
        connection.greeting = greeting;
        debug!(qmp_socket = %path.display(), "received backend QMP greeting {:?}", capabilities);

        let caps = qapi::Execute::<qapi::qmp::qmp_capabilities, u32>::with_command(
            qapi::qmp::qmp_capabilities { enable: None },
        );
        let caps = serde_json::to_value(caps)?;
        let response = connection.execute(caps).await?;

        if response.response.get("return").is_none() {
            return Err(QmpError::Protocol(format!(
                "QMP capability negotiation failed: {}",
                response.response
            )));
        }

        info!(qmp_socket = %path.display(), "backend QMP capability negotiation complete");
        Ok(connection)
    }

    fn greeting(&self) -> &Value {
        &self.greeting
    }

    /// Execute one command and collect any events that arrive before response.
    ///
    /// Since the manager never enables QMP out-of-band execution on the backend,
    /// QEMU responses arrive in command order and no request id is required for
    /// matching. The worker still validates all backend messages as QMP messages
    /// using qapi's generated types.
    async fn execute(&mut self, command: Value) -> Result<QmpResponse, QmpError> {
        self.write_json_line(&command).await?;

        let mut events = Vec::new();

        loop {
            let message = self.read_json_line().await?;
            let _typed: QmpMessageAny = serde_json::from_value(message.clone())?;

            if message.get("event").is_some() {
                events.push(message);
                continue;
            }

            if message.get("return").is_some() || message.get("error").is_some() {
                return Ok(QmpResponse {
                    response: message,
                    events,
                });
            }

            return Err(QmpError::Protocol(format!(
                "unexpected QMP message: {message}"
            )));
        }
    }

    /// Read one JSON-line message from the backend QMP socket.
    async fn read_json_line(&mut self) -> Result<Value, QmpError> {
        let mut line = String::new();
        let bytes = self.stream.read_line(&mut line).await?;

        if bytes == 0 {
            return Err(QmpError::Protocol("QMP socket closed".to_string()));
        }

        Ok(serde_json::from_str(&line)?)
    }

    /// Write one JSON-line message to the backend QMP socket.
    async fn write_json_line(&mut self, value: &Value) -> Result<(), QmpError> {
        let mut line = serde_json::to_vec(value)?;
        line.push(b'\n');

        let stream = self.stream.get_mut();
        stream.write_all(&line).await?;
        stream.flush().await?;

        Ok(())
    }
}

/// Bind the QMP-compatible socket that normal clients use.
async fn start_qmp_client_listener(
    client_socket: PathBuf,
    proxy: QmpClientProxy,
    remove_stale_client_socket: bool,
) -> Result<JoinHandle<()>, QmpError> {
    if remove_stale_client_socket {
        remove_stale_socket(&client_socket)?;
    } else if client_socket.try_exists()? {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "{} appeared after QEMU socket was moved; refusing to overwrite it",
                client_socket.display()
            ),
        )
        .into());
    }

    let listener = UnixListener::bind(&client_socket)?;
    info!(client_socket = %client_socket.display(), "QMP client socket listening");

    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    debug!("accepted QMP client connection");
                    let proxy = proxy.clone();
                    tokio::spawn(async move {
                        if let Err(error) = serve_qmp_client(stream, proxy).await {
                            warn!(?error, "QMP client connection ended with error");
                        }
                    });
                }
                Err(error) => {
                    error!(%error, "QMP client listener failed");
                    break;
                }
            }
        }
    });

    Ok(handle)
}

/// Serve one front-side QMP client connection.
///
/// Each client sees a normal QMP greeting and performs its own
/// `qmp_capabilities` handshake. That handshake is answered locally because the
/// backend connection was already negotiated during registration. All later
/// commands are forwarded through the VM queue.
async fn serve_qmp_client(stream: UnixStream, proxy: QmpClientProxy) -> Result<(), QmpError> {
    let mut stream = BufReader::new(stream);
    write_json_line(stream.get_mut(), &proxy.greeting).await?;
    debug!("sent QMP greeting to client");

    loop {
        let command = match read_json_line(&mut stream).await {
            Ok(command) => {
                debug!(command = %command, "received QMP client command");
                command
            }
            Err(QmpError::Protocol(message)) if message == "QMP socket closed" => {
                debug!("QMP client disconnected");
                return Ok(());
            }
            Err(error) => return Err(error),
        };

        validate_qmp_command_value(&command).map_err(QmpError::Protocol)?;

        if is_qmp_capabilities(&command) {
            let response = local_qmp_capabilities_response(&command);
            write_json_line(stream.get_mut(), &response).await?;
            debug!("answered client qmp_capabilities locally");
            continue;
        }

        let (response, receiver) = oneshot::channel();
        proxy
            .commands
            .send(QmpRequest { command, response })
            .await
            .map_err(|_| QmpError::Disconnected)?;

        let response = receiver.await.map_err(|_| QmpError::Disconnected)??;
        for event in response.events {
            write_json_line(stream.get_mut(), &event).await?;
        }
        write_json_line(stream.get_mut(), &response.response).await?;
    }
}

/// Read one JSON-line message from a front-side QMP client.
async fn read_json_line(stream: &mut BufReader<UnixStream>) -> Result<Value, QmpError> {
    let mut line = String::new();
    let bytes = stream.read_line(&mut line).await?;

    if bytes == 0 {
        return Err(QmpError::Protocol("QMP socket closed".to_string()));
    }

    Ok(serde_json::from_str(&line)?)
}

/// Write one JSON-line message to a front-side QMP client.
async fn write_json_line(stream: &mut UnixStream, value: &Value) -> Result<(), QmpError> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');

    stream.write_all(&line).await?;
    stream.flush().await?;

    Ok(())
}

/// Return whether a command is the per-client QMP capability handshake.
fn is_qmp_capabilities(command: &Value) -> bool {
    command
        .get("execute")
        .and_then(Value::as_str)
        .is_some_and(|execute| execute == "qmp_capabilities")
}

/// Build a local `qmp_capabilities` success response, preserving `id` if set.
fn local_qmp_capabilities_response(command: &Value) -> Value {
    let mut response = serde_json::Map::from_iter([("return".to_string(), json!({}))]);

    if let Some(id) = command.get("id") {
        response.insert("id".to_string(), id.clone());
    }

    Value::Object(response)
}

/// Validate HTTP passthrough commands and map validation failures to HTTP.
fn validate_qmp_command(command: &Value) -> Result<(), AppError> {
    validate_qmp_command_value(command).map_err(AppError::BadRequest)
}

/// Validate that a JSON value is shaped like a QMP command.
fn validate_qmp_command_value(command: &Value) -> Result<(), String> {
    let Some(object) = command.as_object() else {
        return Err("QMP command body must be a JSON object".to_string());
    };

    if !object.contains_key("execute") && !object.contains_key("exec-oob") {
        return Err("QMP command body must contain 'execute' or 'exec-oob'".to_string());
    }

    Ok(())
}

/// Resolve the client-facing socket path from a registration request.
fn resolve_client_socket(body: &RegisterVm) -> Result<PathBuf, AppError> {
    match (body.move_qmp_socket, body.client_socket.as_ref()) {
        (true, None) => Ok(body.qmp_socket.clone()),
        (true, Some(client_socket)) if client_socket == &body.qmp_socket => {
            Ok(client_socket.clone())
        }
        (true, Some(client_socket)) => Err(AppError::BadRequest(format!(
            "client_socket must equal qmp_socket when move_qmp_socket is true; got {} and {}",
            client_socket.display(),
            body.qmp_socket.display()
        ))),
        (false, Some(client_socket)) => Ok(client_socket.clone()),
        (false, None) => Err(AppError::BadRequest(
            "client_socket is required unless move_qmp_socket is true".to_string(),
        )),
    }
}

/// Move QEMU's socket path to a generated backend path in the same directory.
///
/// Keeping the backend path in the same directory makes the rename atomic and
/// avoids cross-filesystem moves. The generated path is intentionally not part
/// of the HTTP response or VM list output.
fn move_qmp_socket_for_registration(original: &Path) -> io::Result<SocketMove> {
    ensure_socket_path(original)?;

    let parent = original.parent().unwrap_or_else(|| Path::new("."));
    let file_name = original.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no file name", original.display()),
        )
    })?;
    let file_name = file_name.to_string_lossy();

    for attempt in 0..100 {
        let backend = parent.join(format!(
            ".{file_name}.qmp-uds-mon-manager.{}.{}.sock",
            std::process::id(),
            attempt
        ));

        if backend.exists() {
            continue;
        }

        fs::rename(original, &backend)?;
        info!(
            original = %original.display(),
            backend = %backend.display(),
            "moved QEMU QMP socket for qmp-uds-mon-manager ownership"
        );

        return Ok(SocketMove {
            original: original.to_path_buf(),
            backend,
        });
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "could not generate unused backend socket path for {}",
            original.display()
        ),
    ))
}

/// Restore a moved QMP socket to its original path.
fn restore_moved_qmp_socket(move_info: &SocketMove) -> io::Result<()> {
    if move_info.original.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "{} exists; refusing to overwrite it while restoring {}",
                move_info.original.display(),
                move_info.backend.display()
            ),
        ));
    }

    ensure_socket_path(&move_info.backend)?;
    fs::rename(&move_info.backend, &move_info.original)
}

/// Verify that a path exists and is a Unix socket.
fn ensure_socket_path(path: &Path) -> io::Result<()> {
    let metadata = fs::metadata(path)?;
    if metadata.file_type().is_socket() {
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{} exists and is not a Unix socket", path.display()),
    ))
}

/// Remove a stale Unix socket without overwriting unrelated files.
fn remove_stale_socket(path: &Path) -> io::Result<()> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(());
    };

    if metadata.file_type().is_socket() {
        fs::remove_file(path)?;
        return Ok(());
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("{} exists and is not a Unix socket", path.display()),
    ))
}
