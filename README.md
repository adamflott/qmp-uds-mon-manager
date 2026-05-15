# qmp-uds-mon-manager

`qmp-uds-mon-manager` is a small Rust service that makes a single QEMU Machine
Protocol (QMP) Unix domain socket usable by multiple clients. The daemon is
referred to as qmp-uds-mon-manager throughout this README when describing the runtime
service.

QEMU's monitor socket is a single protocol stream. Multiple independent
clients cannot safely connect to and issue commands against the same monitor
socket directly. `qmp-uds-mon-manager` keeps one persistent backend connection to each
registered QEMU monitor and exposes a manager-owned Unix domain socket for
clients. Client commands are queued and executed sequentially against QEMU, then
the response is returned to the client that made the request.

The project also exposes an HTTP API over a Unix domain socket for registration,
introspection, removal, and raw QMP passthrough.

## Features

- Manages multiple QEMU instances.
- Maintains one persistent backend QMP connection per registered VM.
- Exposes one client-facing QMP Unix socket per registered VM.
- Allows concurrent QMP clients to connect to the same manager-owned socket.
- Serializes requests before sending them to the real QEMU monitor.
- Provides an HTTP-over-UDS management API.
- Includes a `qmp-parallel` helper to demonstrate concurrent client access.
- Includes a `qmp-direct` helper and `scripts/demo.sh` for comparison demos.

## Status

This project is early-stage software. It is useful for experimenting with QMP
request serialization, but it is not yet hardened for production.

Current limitations:

- Registered VMs are kept in memory only.
- No authentication or authorization is implemented.
- QMP events are returned only when they are observed while waiting for a
  command response. (e.g. no out-of-band events)
- The client-facing QMP socket handles `qmp_capabilities` locally, then forwards
  later commands to the backend QEMU connection.

## Requirements

- Rust 1.95 or newer
- A Unix-like operating system with Unix domain sockets
- QEMU configured with a QMP Unix domain socket

## Quickstart

### Build

```sh
cargo build --release
```

Run checks:

```sh
cargo check
cargo test
```

### Running the Server

Start the management HTTP API on a Unix domain socket:

```sh
./target/release/qmp-uds-mon-manager --socket-path /tmp/qmp-uds-mon-manager.sock
```

If no socket path is provided, the server defaults to:

```text
/tmp/qmp-uds-mon-manager.sock
```

### Booting a QEMU Instance

Boot a Debian 13 VM with QMP enabled:
image: https://cloud.debian.org/images/cloud/trixie/latest/debian-13-generic-amd64.qcow2

```sh
qemu-system-x86_64 -qmp unix:/tmp/qemu-vm1.qmp,server=on,wait=off -name vm1 -d guest_errors,unimp -serial stdio -drive file=$HOME/Downloads/debian-13-generic-amd64.qcow2,media=disk,if=virtio -nic user,model=virtio
```


The server removes stale socket files at startup when the path already exists
and is a Unix socket. It refuses to overwrite non-socket files.

### Registering a QEMU Instance

Register a VM by providing:

- `qmp_socket`: the real QEMU QMP Unix socket.
- `client_socket`: the manager-owned Unix socket that clients should use. This
  is required unless `move_qmp_socket` is true.

```sh
curl --unix-socket /tmp/qmp-uds-mon-manager.sock \
  -X PUT http://localhost/vms/vm1 \
  -H 'content-type: application/json' \
  -d '{"qmp_socket":"/tmp/qemu-vm1.qmp","client_socket":"/tmp/vm1-managed.qmp"}'
```

After registration, ordinary QMP clients can connect to:

```text
/tmp/vm1-managed.qmp
```

The manager connects once to `/tmp/qemu-vm1.qmp` and queues all client commands
through that backend connection.

To keep clients using the original QEMU socket path, register with
`move_qmp_socket: true`. In that mode, qmp-uds-mon-manager validates that `qmp_socket`
is a Unix socket, renames it to a generated hidden backend path in the same
directory, and binds its client-facing socket at the original path. When the VM
is unregistered, qmp-uds-mon-manager removes its client-facing socket and renames the
real QEMU socket back to the original path.

`scripts/register.sh`:

```sh
curl --unix-socket /tmp/qmp-uds-mon-manager.sock \
  -X PUT http://localhost/vms/vm1 \
  -H 'content-type: application/json' \
  -d '{"qmp_socket":"/tmp/qemu-vm1.qmp","move_qmp_socket":true}'
```

### Querying the Status of a VM

`scripts/query.sh`:

```sh
echo '{"execute":"query-status"}' | nc -U /tmp/vm1-managed.qmp
{"QMP":{"capabilities":["oob"],"version":{"package":"","qemu":{"major":11,"micro":0,"minor":0}}}}
{"return":{"running":true,"status":"running"}}
```

### Unregistering a QEMU Instance

`scripts/unregister.sh`:

```sh
curl --unix-socket /tmp/qmp-uds-mon-manager.sock \
-X DELETE http://localhost/vms/vm1
```

## HTTP API

The HTTP API is available only through the management Unix domain socket.

### List VMs

```sh
curl --unix-socket /tmp/qmp-uds-mon-manager.sock \
  http://localhost/vms
```

Example response:

```json
[
  {
    "id": "vm1",
    "qmp_socket": "/tmp/qemu-vm1.qmp",
    "client_socket": "/tmp/vm1-managed.qmp",
    "moved_qmp_socket": false
  }
]
```

### Register VM

```http
PUT /vms/{id}
```

Request body:

```json
{
  "qmp_socket": "/tmp/qemu-vm1.qmp",
  "client_socket": "/tmp/vm1-managed.qmp",
  "move_qmp_socket": false
}
```

When `move_qmp_socket` is true, `client_socket` may be omitted. If it is
provided, it must be the same path as `qmp_socket`.

### Remove VM

```sh
curl --unix-socket /tmp/qmp-uds-mon-manager.sock \
  -X DELETE http://localhost/vms/vm1
```

Removing a VM aborts its client-facing listener and removes the manager-owned
client socket file. If `move_qmp_socket` was used, unregistering also restores
the real QEMU socket to the original `qmp_socket` path.

### HTTP QMP Passthrough

The HTTP passthrough endpoint remains available as a convenience API:

```sh
curl --unix-socket /tmp/qmp-uds-mon-manager.sock \
  -X POST http://localhost/vms/vm1/qmp \
  -H 'content-type: application/json' \
  -d '{"execute":"query-status"}'
```

Example response:

```json
{
  "response": {
    "return": {
      "running": true,
      "singlestep": false,
      "status": "running"
    }
  },
  "events": []
}
```

## QMP Client Socket Flow

For each registered VM, `qmp-uds-mon-manager` creates a QMP-compatible Unix socket at
`client_socket`.

Each client connection follows this flow:

1. Client connects to the manager-owned socket.
2. Server sends the cached QMP greeting from the real QEMU monitor.
3. Client sends `{"execute":"qmp_capabilities"}`.
4. Server answers locally with `{"return":{}}`.
5. Client sends normal QMP commands.
6. Server queues each command through the single backend QEMU connection.
7. Server returns the QEMU response to the requesting client.

This lets many clients connect concurrently while preserving QEMU's sequential
monitor semantics.

## Parallel Client Demo

`qmp-parallel` demonstrates the intended use case. It registers one or more VMs,
starts many concurrent QMP clients per VM against each manager-owned
`client_socket`, sends the same command from each client, and unregisters the
VMs at the end.

```sh
cargo run --bin qmp-parallel -- \
  --manager-socket /tmp/qmp-uds-mon-manager.sock \
  --vm-id-prefix vm \
  --qmp-socket-template '/tmp/qemu-vm{i}.qmp' \
  --client-socket-template '/tmp/vm{i}-managed.qmp' \
  --vm-count 4 \
  --clients-per-vm 20
```

With a custom command:

```sh
cargo run --bin qmp-parallel -- \
  --qmp-json '{"execute":"query-version"}' \
  --clients-per-vm 20 \
  --vm-count 4 \
  --client-socket-template '/tmp/vm{i}-managed.qmp' \
  --qmp-socket-template '/tmp/qemu-vm{i}.qmp' \
  --vm-id-prefix vm \
  --manager-socket /tmp/qmp-uds-mon-manager.sock
```

To demonstrate transparent socket takeover, pass the same QEMU socket template
as both the backend and client template and enable `--move-qmp-socket`:

```sh
cargo run --bin qmp-parallel -- \
  --manager-socket /tmp/qmp-uds-mon-manager.sock \
  --vm-id-prefix vm \
  --qmp-socket-template '/tmp/qemu-vm{i}.qmp' \
  --client-socket-template '/tmp/qemu-vm{i}.qmp' \
  --move-qmp-socket \
  --vm-count 4 \
  --clients-per-vm 20
```

Options:

```text
qmp-parallel \
  --manager-socket <MANAGER_SOCKET> \
  --vm-id-prefix <VM_ID_PREFIX> \
  --qmp-socket-template <QEMU_QMP_SOCKET_TEMPLATE> \
  --client-socket-template <CLIENT_SOCKET_TEMPLATE> \
  [--move-qmp-socket] \
  --vm-count <VM_COUNT> \
  --clients-per-vm <CLIENTS_PER_VM> \
  [--qmp-json <QMP_JSON>]
```

For multi-VM runs, both socket templates must include `{i}`. The demo replaces
`{i}` with a zero-based VM index and registers VM ids as
`<vm-id-prefix>-<index>`. For single-VM runs, the socket paths may be exact
paths without `{i}` and the VM id is exactly `<vm-id-prefix>`.

## Direct QEMU Comparison Demo

`qmp-direct` runs the same style of concurrent QMP client workload directly
against one real QEMU monitor socket. It does not use qmp-uds-mon-manager. This is
useful for showing why the daemon exists: direct concurrent clients may fail or
time out, while `qmp-parallel` can route those clients through a manager-owned
socket and serialize backend access.

```sh
cargo run --bin qmp-direct -- \
  --qmp-socket /tmp/qemu-vm1.qmp \
  --clients 20 \
  --timeout-ms 5000
```

With a custom command:

```sh
cargo run --bin qmp-direct -- \
  --qmp-json '{"execute":"query-version"}' \
  --timeout-ms 5000 \
  --clients 20 \
  --qmp-socket /tmp/qemu-vm1.qmp
```

Options:

```text
qmp-direct \
  --qmp-socket <QEMU_QMP_SOCKET> \
  --clients <CLIENTS> \
  [--timeout-ms <TIMEOUT_MS>] \
  [--qmp-json <QMP_JSON>]
```

The default client timeout is 5000 ms.

## End-to-End Demo Script

`scripts/demo.sh` builds the project, starts temporary QEMU instances, runs the
direct QEMU comparison first, starts qmp-uds-mon-manager, and then runs `qmp-parallel`
with `--move-qmp-socket`. The daemon safely moves each real QEMU QMP socket
aside and recreates the original path as a qmp-uds-mon-manager client socket.

```sh
scripts/demo.sh
```

Useful environment variables:

```sh
VM_COUNT=2 CLIENTS=20 TIMEOUT_MS=5000 scripts/demo.sh
```

```sh
QEMU_BIN=qemu-system-aarch64 \
QMP_JSON='{"execute":"query-version"}' \
scripts/demo.sh
```

The socket swap is reversible:

1. QEMU starts with sockets such as `/tmp/.../qemu-vm0.qmp`.
2. The script runs `qmp-direct` against the first real QEMU socket.
3. `qmp-parallel` registers each real QEMU socket with `move_qmp_socket: true`.
4. qmp-uds-mon-manager renames each real QEMU socket to a generated hidden backend path
   and creates client sockets at the original `/tmp/.../qemu-vm0.qmp` paths.
5. Clients connect to the original paths, unaware they are talking to
   qmp-uds-mon-manager.
6. After unregistering, qmp-uds-mon-manager restores the real QEMU sockets to their
   original paths and the script stops all child processes.

Set `KEEP_WORKDIR=1` to keep the temporary directory for inspection.

## Architecture

At a high level, each VM has:

- A persistent backend QMP connection to the real QEMU monitor.
- An internal Tokio channel used as a request queue.
- A manager-owned QMP Unix socket listener for client connections.
- Optional HTTP passthrough access through the management socket.

The backend worker owns the real QMP connection. All HTTP passthrough requests
and all QMP client socket requests send work to the same queue. This keeps QMP
execution sequential even when many clients are active.

## Security Notes

The service currently assumes trusted local clients. Do not expose the
management socket or client sockets to untrusted users.

Recommended deployment precautions:

- Place sockets in a directory with restrictive permissions.
- Use Unix filesystem permissions to control access.
- Run the service with the least privileges needed to reach the QEMU sockets.

## License

MIT OR Apache-2.0
