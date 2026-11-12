+++
title = "The kernel log (dmesg, /dev/kmsg)"
summary = "The kernel's message buffer: what drivers and subsystems say while the system boots and runs."
related = ["stage.kernel", "kernel.start", "kernel.cmdline"]

[[references]]
title = "Linux ABI: /dev/kmsg"
url = "https://github.com/torvalds/linux/blob/master/Documentation/ABI/testing/dev-kmsg"
+++

The kernel writes its messages to a **ring buffer** in memory. `dmesg` prints it;
`/dev/kmsg` gives the same messages with structure: priority, sequence number,
timestamp in microseconds since the kernel started, and the text.

Two things limit what you can see:

- **Permissions**: with `kernel.dmesg_restrict = 1` (the default on many
  distributions), only root can read it.
- **Size**: the buffer has a fixed size (often 128 KiB to 1 MiB, see the
  `CONFIG_LOG_BUF_SHIFT` kernel option). When it is full, the oldest messages are
  overwritten. On a machine with many devices, or after hours of uptime, the first
  messages of the boot may be gone. The `log_buf_len=4M` kernel parameter enlarges it.

Userspace programs can also write to it (early init systems do, before syslog
runs); these messages have a non-kernel facility.
