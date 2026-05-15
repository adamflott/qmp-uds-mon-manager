curl --unix-socket /tmp/qmp-uds-mon-manager.sock \
  -X PUT http://localhost/vms/vm1 \
  -H 'content-type: application/json' \
  -d '{"qmp_socket":"/tmp/qemu-vm1.qmp","client_socket":"/tmp/vm1-managed.qmp"}' \
  -v