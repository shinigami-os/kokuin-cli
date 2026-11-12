+++
title = "The kernel starts the first process"
summary = "The kernel runs PID 1: the initramfs /init, or the init system itself."
related = ["init.pid1", "stage.init", "stage.initramfs", "kernel.cmdline"]
+++

The last thing the kernel does on its own is start the first userspace process,
which gets the process id 1: `Run /sbin/init as init process`.

- With an initramfs, it runs its `/init` (or the program given by `rdinit=`).
- Without one, it mounts the root filesystem itself (from `root=`) and runs `init=`,
  or tries `/sbin/init`, `/etc/init`, `/bin/init` and `/bin/sh` in that order.

If no init can be started, the kernel panics with "No working init found".
