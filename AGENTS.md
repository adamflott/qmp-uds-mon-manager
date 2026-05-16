# AGENTS.md

Guidance for coding agents working in this repository.

## Project Shape

This is a Rust 2024 project named `qmp-uds-mon-manager`. It provides a Unix
domain socket HTTP API and QMP proxy for serializing access to QEMU monitor
sockets.

Important files:

- `src/bin/qmp-uds-mon-manager.rs`: main daemon.
- `src/bin/qmp-parallel.rs`: qmp-uds-mon-manager-mediated concurrency demo.
- `src/bin/qmp-direct.rs`: direct-to-QEMU comparison demo.
- `scripts/demo.sh`: end-to-end demo that starts QEMU, compares direct access,
  then uses qmp-uds-mon-manager with transparent socket takeover.
- `scripts/register.sh`, `scripts/query.sh`, `scripts/unregister.sh`: small
  manual helper scripts.
- `README.md`: user-facing documentation. Keep examples synchronized with CLI
  help and JSON request/response shapes.

There is no `src/main.rs`; all runnable programs are bin targets.

## Commands

Use these before handing work back:

```sh
cargo fmt
cargo check --bins
cargo test
cargo doc --no-deps --bins
bash -n scripts/demo.sh
```

Useful CLI sanity checks:

```sh
cargo run --bin qmp-uds-mon-manager -- --help
cargo run --bin qmp-parallel -- --help
cargo run --bin qmp-direct -- --help
```

The demo script may not run in constrained environments because QEMU can fail
to bind Unix sockets with `Operation not permitted`. Do not treat that as a code
failure if the static checks above pass.

## Implementation Notes

The daemon serves HTTP only over a Unix domain socket using axum + hyper. Each
registered VM has one persistent backend QMP connection and one worker task.
All HTTP passthrough requests and front-side QMP client requests go through a
single Tokio `mpsc` queue per VM. Preserve that single-owner backend connection
model.

The server validates QMP greetings and backend messages with the `qapi` crate,
but raw passthrough commands are dynamic JSON because qapi command types are
compile-time typed.

Front-side QMP clients receive the cached QEMU greeting. Their
`qmp_capabilities` command is answered locally because the backend connection is
already negotiated during registration. Later commands are forwarded through the
VM queue.

The server intentionally does not handle out-of-band QMP execution. Responses
are assumed to arrive in command order because the manager serializes backend
commands.

## Socket Takeover Safety

`move_qmp_socket: true` is a sensitive path. Be conservative.

Current intended behavior:

- Registration treats `qmp_socket` as the original/public QEMU socket path.
- `client_socket` may be omitted. If supplied with `move_qmp_socket: true`, it
  must equal `qmp_socket`.
- The daemon verifies `qmp_socket` exists and is a Unix socket.
- The daemon renames `qmp_socket` to a generated hidden backend path in the same
  directory. Keeping the path in the same directory preserves atomic rename
  semantics and avoids cross-filesystem moves.
- The daemon then binds its client-facing socket at the original `qmp_socket`
  path.
- If anything appears at the original path after the move and before bind, the
  daemon refuses to overwrite it.
- On unregister, the daemon removes its client socket and renames the hidden
  backend socket back to the original path.
- Restore refuses to overwrite anything at the original path.
- If registration fails after moving the socket, the daemon attempts to restore
  the socket before returning the error.

Do not replace these checks with broad remove/overwrite behavior. Never use
`rm -f` in Rust code for paths supplied by users; always validate expected file
type and ownership assumptions first.

## CLI Conventions

All binaries use `clap` derive and named options, not positional arguments.
Keep options order-independent and update README examples when flags change.

Current notable flags:

- `qmp-uds-mon-manager --socket-path <MANAGER_SOCKET>`
- `qmp-uds-mon-manager --queue-depth <QUEUE_DEPTH>`
- `qmp-parallel --manager-socket ... --vm-id-prefix ... --qmp-socket-template ... --client-socket-template ... --vm-count ... --clients-per-vm ... [--move-qmp-socket] [--qmp-json ...]`
- `qmp-direct --qmp-socket ... --clients ... [--timeout-ms ...] [--qmp-json ...]`

## Logging

Use `tracing`, not `println!` or `eprintln!`, for operational output. The demo
binaries should log summaries even when individual clients fail. Client
connect/send/read failures should be counted and logged, not used to abort the
whole test early.

Default filters are set through `tracing_subscriber::EnvFilter`; `RUST_LOG` is
the expected way to increase verbosity.

## Error Handling Expectations

The daemon has crate-level:

```rust
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![forbid(unsafe_code)]
```

Preserve this style. Avoid `unwrap`/`expect`, broad panics, and unsafe code.

For demo binaries:

- Continue running all client tasks even when individual clients fail.
- Log final totals for success and failure.
- `qmp-parallel` may still return an error for setup/registration/cleanup
  failures, but not for per-client QMP I/O failures alone.
- `qmp-direct` returns success after logging the summary even if some clients
  failed, because the failure rate is the test signal.

## Documentation

When behavior changes, update `README.md` and this file. In particular, keep
these in sync:

- binary names,
- CLI flags,
- registration JSON (`qmp_socket`, optional `client_socket`,
  `move_qmp_socket`),
- list response fields (`id`, `qmp_socket`, `client_socket`,
  `moved_qmp_socket`),
- demo script behavior.

## Dependencies

Prefer the existing stack:

- axum/hyper/hyper-util for HTTP-over-UDS,
- tokio for async runtime, UDS, and tasks,
- qapi for QMP validation/framing-related types,
- serde/serde_json for dynamic JSON commands,
- clap derive for CLI,
- tracing/tracing-subscriber for logs.

Do not add a new framework or argument parser unless there is a strong reason.
