+++
title = "Stage: init"
summary = "PID 1, the init system, sets up the base of the system before starting services."
related = ["init.pid1", "init.runit", "init.systemd", "init.openrc", "init.udev", "stage.services"]
+++

The first process started on the real root filesystem is the **init system**, with
the process id 1 (PID 1). It is the ancestor of every other process and it must never
exit: if it does, the kernel panics.

Its first job is to make the system usable. The exact order depends on the init
system, but it always includes:

- mounting the virtual filesystems (`/proc`, `/sys`, `/dev`, `/run`);
- starting the device manager (udev), which loads drivers and creates device files;
- checking and remounting the root filesystem read-write, mounting the others
  (`/etc/fstab`);
- setting the hostname, the clock, the console keymap;
- seeding the random number generator, enabling swap.

With runit this is "stage 1" (`/etc/runit/1`), with OpenRC the `sysinit` and `boot`
runlevels, with systemd everything before `sysinit.target`.
