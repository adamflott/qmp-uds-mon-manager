#!/usr/bin/env bash
#
# End-to-end qmp-uds-mon-manager demo.
#
# This script starts a set of temporary QEMU processes, first shows direct
# concurrent QMP access against a real QEMU monitor socket, then asks
# qmp-uds-mon-manager to move each real QEMU QMP socket aside and recreate the
# original socket path as a client-facing proxy. Clients then connect to the
# original QEMU-looking paths without knowing qmp-uds-mon-manager is in the middle.

set -Eeuo pipefail

QEMU_BIN="${QEMU_BIN:-qemu-system-x86_64}"
VM_COUNT="${VM_COUNT:-2}"
CLIENTS="${CLIENTS:-20}"
TIMEOUT_MS="${TIMEOUT_MS:-5000}"
QMP_JSON="${QMP_JSON:-{\"execute\":\"query-list\"}}"

RUST_LOG="${RUST_LOG:-info}"
KEEP_WORKDIR="${KEEP_WORKDIR:-0}"

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
WORKDIR="${WORKDIR:-$(mktemp -d "/tmp/qmp-uds-mon-manager-demo.XXXXXX")}"
MANAGER_SOCKET="${WORKDIR}/qmp-uds-mon-manager.sock"

declare -a QEMU_PIDS=()
MANAGER_PID=""

log() {
  printf '[demo] %s\n' "$*"
}

die() {
  printf '[demo] error: %s\n' "$*" >&2
  exit 1
}

wait_for_socket() {
  local socket_path="$1"
  local label="$2"
  local attempts="${3:-100}"
  local pid="${4:-}"

  for _ in $(seq 1 "${attempts}"); do
    if [[ -S "${socket_path}" ]]; then
      return 0
    fi

    if [[ -n "${pid}" ]] && ! kill -0 "${pid}" 2>/dev/null; then
      die "${label} process exited before creating socket at ${socket_path}"
    fi

    sleep 0.1
  done

  die "timed out waiting for ${label} socket at ${socket_path}"
}

qmp_socket() {
  local index="$1"
  printf '%s/qemu-vm%s.qmp' "${WORKDIR}" "${index}"
}

cleanup() {
  local status=$?
  trap - EXIT

  if [[ -n "${MANAGER_PID}" ]] && kill -0 "${MANAGER_PID}" 2>/dev/null; then
    log "stopping qmp-uds-mon-manager pid ${MANAGER_PID}"
    kill "${MANAGER_PID}" 2>/dev/null || true
    wait "${MANAGER_PID}" 2>/dev/null || true
  fi

  for pid in "${QEMU_PIDS[@]}"; do
    if kill -0 "${pid}" 2>/dev/null; then
      log "stopping QEMU pid ${pid}"
      kill "${pid}" 2>/dev/null || true
      wait "${pid}" 2>/dev/null || true
    fi
  done

  if [[ "${KEEP_WORKDIR}" == "1" ]]; then
    log "keeping work directory ${WORKDIR}"
  else
    rm -rf -- "${WORKDIR}"
  fi

  exit "${status}"
}

trap cleanup EXIT INT TERM

require_commands() {
  command -v cargo >/dev/null 2>&1 || die "cargo is required"
  command -v "${QEMU_BIN}" >/dev/null 2>&1 || die "${QEMU_BIN} is required; set QEMU_BIN=/path/to/qemu-system-*"
}

build_binaries() {
  log "building qmp-uds-mon-manager, qmp-parallel, and qmp-direct"
  cargo build --bins
}

start_qemus() {
  log "starting ${VM_COUNT} QEMU process(es)"
  for index in $(seq 0 "$((VM_COUNT - 1))"); do
    local socket_path
    socket_path="$(qmp_socket "${index}")"

    "${QEMU_BIN}" \
      -machine none \
      -nodefaults \
      -display none \
      -monitor none \
      -serial none \
      -parallel none \
      -qmp "unix:${socket_path},server=on,wait=off" &

    local pid=$!
    QEMU_PIDS+=("${pid}")
    log "started QEMU ${index} pid ${pid}, qmp=${socket_path}"
    wait_for_socket "${socket_path}" "QEMU ${index} QMP" 100 "${pid}"
  done
}

run_direct_case() {
  log "running direct-to-QEMU comparison against $(qmp_socket 0)"
  log "direct clients may fail or time out; the summary is the useful signal"

  RUST_LOG="${RUST_LOG}" "${REPO_DIR}/target/debug/qmp-direct" \
    --qmp-socket "$(qmp_socket 0)" \
    --clients "${CLIENTS}" \
    --timeout-ms "${TIMEOUT_MS}" \
    --qmp-json "${QMP_JSON}" || true
}

start_manager() {
  log "starting qmp-uds-mon-manager at ${MANAGER_SOCKET}"
  RUST_LOG="${RUST_LOG}" "${REPO_DIR}/target/debug/qmp-uds-mon-manager" \
    --socket-path "${MANAGER_SOCKET}" &

  MANAGER_PID=$!
  wait_for_socket "${MANAGER_SOCKET}" "qmp-uds-mon-manager"
}

run_manager_case() {
  log "running qmp-uds-mon-manager-mediated case"
  log "clients connect to the original QEMU-looking paths in ${WORKDIR}"

  RUST_LOG="${RUST_LOG}" "${REPO_DIR}/target/debug/qmp-parallel" \
    --manager-socket "${MANAGER_SOCKET}" \
    --vm-id-prefix vm \
    --qmp-socket-template "${WORKDIR}/qemu-vm{i}.qmp" \
    --client-socket-template "${WORKDIR}/qemu-vm{i}.qmp" \
    --move-qmp-socket \
    --vm-count "${VM_COUNT}" \
    --clients-per-vm "${CLIENTS}" \
    --qmp-json "${QMP_JSON}"
}

main() {
  require_commands

  if [[ "${VM_COUNT}" -lt 1 ]]; then
    die "VM_COUNT must be at least 1"
  fi

  if [[ "${CLIENTS}" -lt 1 ]]; then
    die "CLIENTS must be at least 1"
  fi

  cd "${REPO_DIR}"
  log "using work directory ${WORKDIR}"
  log "configuration: QEMU_BIN=${QEMU_BIN} VM_COUNT=${VM_COUNT} CLIENTS=${CLIENTS} TIMEOUT_MS=${TIMEOUT_MS}"

  build_binaries
  start_qemus
  run_direct_case
  start_manager
  run_manager_case

  log "demo complete"
}

main "$@"
