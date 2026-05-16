An example of running the demo:

```
> ./scripts/demo.sh | ansifilter | tee -a demo.log
[demo] using work directory /tmp/qmp-uds-mon-manager-demo.WmcLDi
[demo] configuration: QEMU_BIN=qemu-system-x86_64 VM_COUNT=2 CLIENTS=20 TIMEOUT_MS=5000
[demo] building qmp-uds-mon-manager, qmp-parallel, and qmp-direct
[demo] starting 2 QEMU process(es)
[demo] started QEMU 0 pid 71416, qmp=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp
[demo] started QEMU 1 pid 71421, qmp=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp
[demo] running direct-to-QEMU comparison against /tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp
[demo] direct clients may fail or time out; the summary is the useful signal
2026-05-16T02:31:42.796242Z  INFO starting qmp-direct demo qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp clients=20 timeout_ms=5000
2026-05-16T02:31:42.796849Z  WARN direct QMP client failed client_id=1 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.796912Z  WARN direct QMP client failed client_id=4 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.796991Z  WARN direct QMP client failed client_id=2 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797021Z  WARN direct QMP client failed client_id=5 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797041Z  WARN direct QMP client failed client_id=3 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797065Z  WARN direct QMP client failed client_id=9 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797082Z  WARN direct QMP client failed client_id=10 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797099Z  WARN direct QMP client failed client_id=11 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797116Z  WARN direct QMP client failed client_id=12 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797132Z  WARN direct QMP client failed client_id=13 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797148Z  WARN direct QMP client failed client_id=14 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797165Z  WARN direct QMP client failed client_id=16 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797182Z  WARN direct QMP client failed client_id=17 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797197Z  WARN direct QMP client failed client_id=18 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797213Z  WARN direct QMP client failed client_id=19 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797228Z  WARN direct QMP client failed client_id=7 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797246Z  WARN direct QMP client failed client_id=6 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797261Z  WARN direct QMP client failed client_id=15 elapsed_ms=0 error=Connection refused (os error 61)
2026-05-16T02:31:42.797693Z  INFO direct QMP client succeeded client_id=0 elapsed_ms=1
2026-05-16T02:31:42.798453Z  INFO direct QMP client succeeded client_id=8 elapsed_ms=1
2026-05-16T02:31:42.798481Z  INFO qmp-direct demo finished elapsed_ms=2 total=20 succeeded=2 failed=18
2026-05-16T02:31:42.798500Z  WARN qmp-direct completed with client failures total=20 succeeded=2 failed=18
[demo] starting qmp-uds-mon-manager at /tmp/qmp-uds-mon-manager-demo.WmcLDi/qmp-uds-mon-manager.sock
2026-05-16T02:31:43.198059Z  INFO management HTTP API listening socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qmp-uds-mon-manager.sock
[demo] running qmp-uds-mon-manager-mediated case
[demo] clients connect to the original QEMU-looking paths in /tmp/qmp-uds-mon-manager-demo.WmcLDi
2026-05-16T02:31:43.551858Z  INFO starting qmp-parallel demo manager_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qmp-uds-mon-manager.sock vm_count=2 clients_per_vm=20
2026-05-16T02:31:43.553259Z  INFO registering VM vm_id=vm-0 qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp client_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp move_qmp_socket=true
2026-05-16T02:31:43.553513Z  INFO moved QEMU QMP socket for qmp-uds-mon-manager ownership original=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp backend=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm0.qmp.qmp-uds-mon-manager.71429.0.sock
2026-05-16T02:31:43.553580Z  INFO starting VM worker qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp backend_qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm0.qmp.qmp-uds-mon-manager.71429.0.sock client_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp
2026-05-16T02:31:43.553601Z  INFO connecting to backend QMP socket qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm0.qmp.qmp-uds-mon-manager.71429.0.sock
2026-05-16T02:31:43.554524Z  INFO backend QMP capability negotiation complete qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm0.qmp.qmp-uds-mon-manager.71429.0.sock
2026-05-16T02:31:43.554708Z  INFO QMP client socket listening client_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp
2026-05-16T02:31:43.554747Z  INFO VM registered vm_id=vm-0
2026-05-16T02:31:43.555010Z  INFO registered VM with qmp-uds-mon-manager vm_id=vm-0 qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp client_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp
2026-05-16T02:31:43.555459Z  INFO registering VM vm_id=vm-1 qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp client_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp move_qmp_socket=true
2026-05-16T02:31:43.555715Z  INFO moved QEMU QMP socket for qmp-uds-mon-manager ownership original=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp backend=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm1.qmp.qmp-uds-mon-manager.71429.0.sock
2026-05-16T02:31:43.555736Z  INFO starting VM worker qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp backend_qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm1.qmp.qmp-uds-mon-manager.71429.0.sock client_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp
2026-05-16T02:31:43.555751Z  INFO connecting to backend QMP socket qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm1.qmp.qmp-uds-mon-manager.71429.0.sock
2026-05-16T02:31:43.556150Z  INFO backend QMP capability negotiation complete qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm1.qmp.qmp-uds-mon-manager.71429.0.sock
2026-05-16T02:31:43.556266Z  INFO QMP client socket listening client_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp
2026-05-16T02:31:43.556281Z  INFO VM registered vm_id=vm-1
2026-05-16T02:31:43.556359Z  INFO registered VM with qmp-uds-mon-manager vm_id=vm-1 qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp client_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp
2026-05-16T02:31:43.557771Z  INFO QMP client succeeded vm_id=vm-0 client_id=9 elapsed_ms=1
2026-05-16T02:31:43.557877Z  INFO QMP client succeeded vm_id=vm-1 client_id=6 elapsed_ms=1
2026-05-16T02:31:43.558036Z  INFO QMP client succeeded vm_id=vm-0 client_id=6 elapsed_ms=1
2026-05-16T02:31:43.558126Z  INFO QMP client succeeded vm_id=vm-1 client_id=8 elapsed_ms=1
2026-05-16T02:31:43.558261Z  INFO QMP client succeeded vm_id=vm-0 client_id=3 elapsed_ms=1
2026-05-16T02:31:43.558393Z  INFO QMP client succeeded vm_id=vm-1 client_id=0 elapsed_ms=1
2026-05-16T02:31:43.558505Z  INFO QMP client succeeded vm_id=vm-0 client_id=12 elapsed_ms=1
2026-05-16T02:31:43.558656Z  INFO QMP client succeeded vm_id=vm-1 client_id=1 elapsed_ms=2
2026-05-16T02:31:43.558727Z  INFO QMP client succeeded vm_id=vm-0 client_id=4 elapsed_ms=2
2026-05-16T02:31:43.558866Z  INFO QMP client succeeded vm_id=vm-1 client_id=4 elapsed_ms=2
2026-05-16T02:31:43.558945Z  INFO QMP client succeeded vm_id=vm-0 client_id=8 elapsed_ms=2
2026-05-16T02:31:43.559062Z  INFO QMP client succeeded vm_id=vm-1 client_id=2 elapsed_ms=2
2026-05-16T02:31:43.559191Z  INFO QMP client succeeded vm_id=vm-0 client_id=16 elapsed_ms=2
2026-05-16T02:31:43.559269Z  INFO QMP client succeeded vm_id=vm-1 client_id=18 elapsed_ms=2
2026-05-16T02:31:43.559363Z  INFO QMP client succeeded vm_id=vm-0 client_id=2 elapsed_ms=2
2026-05-16T02:31:43.559389Z  INFO QMP client succeeded vm_id=vm-1 client_id=14 elapsed_ms=2
2026-05-16T02:31:43.559485Z  INFO QMP client succeeded vm_id=vm-0 client_id=5 elapsed_ms=3
2026-05-16T02:31:43.559590Z  INFO QMP client succeeded vm_id=vm-1 client_id=15 elapsed_ms=2
2026-05-16T02:31:43.559668Z  INFO QMP client succeeded vm_id=vm-0 client_id=19 elapsed_ms=3
2026-05-16T02:31:43.559721Z  INFO QMP client succeeded vm_id=vm-1 client_id=9 elapsed_ms=3
2026-05-16T02:31:43.559830Z  INFO QMP client succeeded vm_id=vm-0 client_id=14 elapsed_ms=3
2026-05-16T02:31:43.559885Z  INFO QMP client succeeded vm_id=vm-1 client_id=5 elapsed_ms=3
2026-05-16T02:31:43.560014Z  INFO QMP client succeeded vm_id=vm-0 client_id=17 elapsed_ms=3
2026-05-16T02:31:43.560123Z  INFO QMP client succeeded vm_id=vm-1 client_id=11 elapsed_ms=3
2026-05-16T02:31:43.560203Z  INFO QMP client succeeded vm_id=vm-0 client_id=7 elapsed_ms=3
2026-05-16T02:31:43.560341Z  INFO QMP client succeeded vm_id=vm-1 client_id=16 elapsed_ms=3
2026-05-16T02:31:43.560412Z  INFO QMP client succeeded vm_id=vm-0 client_id=11 elapsed_ms=3
2026-05-16T02:31:43.560522Z  INFO QMP client succeeded vm_id=vm-1 client_id=19 elapsed_ms=3
2026-05-16T02:31:43.560612Z  INFO QMP client succeeded vm_id=vm-0 client_id=10 elapsed_ms=4
2026-05-16T02:31:43.560705Z  INFO QMP client succeeded vm_id=vm-1 client_id=3 elapsed_ms=4
2026-05-16T02:31:43.560783Z  INFO QMP client succeeded vm_id=vm-0 client_id=1 elapsed_ms=4
2026-05-16T02:31:43.560886Z  INFO QMP client succeeded vm_id=vm-1 client_id=13 elapsed_ms=4
2026-05-16T02:31:43.560990Z  INFO QMP client succeeded vm_id=vm-0 client_id=18 elapsed_ms=4
2026-05-16T02:31:43.561085Z  INFO QMP client succeeded vm_id=vm-1 client_id=12 elapsed_ms=4
2026-05-16T02:31:43.561159Z  INFO QMP client succeeded vm_id=vm-0 client_id=0 elapsed_ms=4
2026-05-16T02:31:43.561293Z  INFO QMP client succeeded vm_id=vm-1 client_id=10 elapsed_ms=4
2026-05-16T02:31:43.561369Z  INFO QMP client succeeded vm_id=vm-0 client_id=13 elapsed_ms=4
2026-05-16T02:31:43.561466Z  INFO QMP client succeeded vm_id=vm-1 client_id=7 elapsed_ms=4
2026-05-16T02:31:43.561560Z  INFO QMP client succeeded vm_id=vm-0 client_id=15 elapsed_ms=5
2026-05-16T02:31:43.561621Z  INFO QMP client succeeded vm_id=vm-1 client_id=17 elapsed_ms=4
2026-05-16T02:31:43.561631Z  INFO qmp-parallel client phase finished elapsed_ms=5 requested_vms=2 registered_vms=2 registration_failed=0 total=40 succeeded=40 failed=0
2026-05-16T02:31:43.561818Z  INFO removing VM vm_id=vm-1 client_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp backend_qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm1.qmp.qmp-uds-mon-manager.71429.0.sock
2026-05-16T02:31:43.561977Z  INFO restored moved QMP socket original=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm1.qmp backend=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm1.qmp.qmp-uds-mon-manager.71429.0.sock
2026-05-16T02:31:43.562037Z  INFO unregistered VM from qmp-uds-mon-manager vm_id=vm-1
2026-05-16T02:31:43.562065Z  INFO VM worker stopped
2026-05-16T02:31:43.562129Z  INFO removing VM vm_id=vm-0 client_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp backend_qmp_socket=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm0.qmp.qmp-uds-mon-manager.71429.0.sock
2026-05-16T02:31:43.562235Z  INFO restored moved QMP socket original=/tmp/qmp-uds-mon-manager-demo.WmcLDi/qemu-vm0.qmp backend=/tmp/qmp-uds-mon-manager-demo.WmcLDi/.qemu-vm0.qmp.qmp-uds-mon-manager.71429.0.sock
2026-05-16T02:31:43.562278Z  INFO unregistered VM from qmp-uds-mon-manager vm_id=vm-0
2026-05-16T02:31:43.562285Z  INFO qmp-parallel demo finished requested_vms=2 registered_vms=2 registration_failed=0 cleanup_failed=0 total_clients=40 clients_succeeded=40 clients_failed=0
2026-05-16T02:31:43.562292Z  INFO VM worker stopped
[demo] demo complete
[demo] stopping qmp-uds-mon-manager pid 71429
[demo] stopping QEMU pid 71416
[demo] stopping QEMU pid 71421
```