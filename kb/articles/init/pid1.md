+++
title = "PID 1"
summary = "The first userspace process: the ancestor of all others, which must never exit."
related = ["stage.init", "kernel.run_init", "init.runit", "init.systemd", "init.openrc"]
+++

The process with id 1 is special:

- it is started by the kernel and is the ancestor of every other process;
- when a process's parent exits, the orphan is re-attached to PID 1, which must
  collect its exit status ("reap" it) so it does not stay as a zombie;
- it receives special signals (Ctrl+Alt+Del, power button) and handles shutdown;
- if it exits or crashes, the kernel panics.

With an initramfs, PID 1 is first the initramfs `/init`, which then *replaces itself*
(`exec`) with the real init system: the process stays the same, PID 1, but runs a
new program.

`ps -p 1` or `cat /proc/1/comm` tells which init system runs your machine.
