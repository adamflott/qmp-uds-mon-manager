curl --unix-socket /tmp/qmp-uds-mon-manager.sock \
  -X DELETE http://localhost/vms/vm1 \
  -v