+++
title = "How kokuin records a boot"
summary = "kokuin-init runs first, starts the recorder, then becomes the real init; the recorder watches every process and the kernel log until the desktop is ready."
related = ["init.pid1", "init.proc_events", "kernel.kmsg", "kernel.cmdline", "init.procfs"]
+++

After a boot, most of what happened is gone: processes that exited leave no trace,
and the kernel log may have overwritten its first messages. To see everything,
kokuin records the boot while it happens. `sudo kokuin-cli enable` turns this on.

## Starting before everything else

The kernel starts one program, PID 1, and everything else descends from it. Which
program is set by the `init=` kernel parameter (default: `/sbin/init`).
`kokuin-cli enable` adds `init=/usr/lib/kokuin/kokuin-init` to the kernel command
line, through the bootloader configuration (GRUB: `/etc/default/grub`).

With an initramfs, the kernel first starts the initramfs's own `/init`. Once the real
root filesystem is mounted, that program starts the `init=` program from it.

`kokuin-init` is tiny. It does two things:

1. it starts the recorder (`kokuin-cli record`) in the background;
2. it **replaces itself** with the real init (`exec`). The real init keeps PID 1 and
   never knows anything happened.

If anything goes wrong (the recorder is missing, for example), only the recording
is lost: the real init always starts.

## Watching

The recorder subscribes to the kernel's **process events** (see
`init.proc_events`): each process created, each program started, each exit, with
its exact time. For each new program, it reads `/proc/<pid>/` right away (command
line, user, control group) before the process can exit. Without
`CONFIG_PROC_EVENTS`, it compares `/proc` listings every 50 ms instead.

It also reads the kernel log from its first message, and keeps all of it.

## Two clocks

The kernel stamps its log with the scheduler clock, while process times use the
monotonic clock. They usually agree. In a virtual machine, or when a time daemon
like chrony corrects the time, they drift apart by a fraction of a second. The
recorder measures the gap each time it reads a new kernel message, and kokuin uses
it to put everything on one timeline.

## Stopping and saving

The recorder stops `settle_secs` (30 s) after the desktop is ready, or at
`timeout_secs` (180 s) after the kernel started. Both are set in
`/etc/kokuin/config.toml`. It then saves the boot in
`/var/lib/kokuin/boots/<boot id>/`:

- `record.json`: the boot record;
- `raw/`: everything it was built from.

It keeps the last `keep_boots` (10) boots. See them with `kokuin-cli list`, and one
of them with `kokuin-cli show last`.

## What it costs

The recorder waits for the kernel to wake it, so it takes very little time. Its
messages, `kokuin: ...`, are in the kernel log of the boot it recorded.
`sudo kokuin-cli disable` turns the recording off again.
