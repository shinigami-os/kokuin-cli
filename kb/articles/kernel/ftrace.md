+++
title = "Kernel tracing (ftrace)"
summary = "The kernel can record its own events, with precise times, into a memory buffer that tracefs shows as files."
related = ["init.kokuin", "kernel.initcall", "kernel.cmdline", "init.proc_events"]

[[references]]
title = "Linux documentation: ftrace"
url = "https://docs.kernel.org/trace/ftrace.html"

[[references]]
title = "Linux documentation: event tracing"
url = "https://docs.kernel.org/trace/events.html"
+++

The kernel has thousands of **trace events**: fixed points in its code (a process
starting a program, a disk request, a scheduler switch...) where it can write a
record when asked to. The records go into a ring buffer in memory, one per CPU,
with the time and the process that triggered them. This is **ftrace**, the kernel's
own tracer. Tools like `trace-cmd` and `perf` are built on it.

It is controlled through **tracefs**, a filesystem usually mounted on
`/sys/kernel/tracing`:

- `available_events` lists the events;
- writing an event name to `set_event` turns it on;
- `trace` shows the records as text.

```
sh-1352 [001] ..... 2.034567: sched_process_exec: filename=/sbin/openrc pid=1352 old_pid=1352
```

Events can be turned on from the kernel command line too, before any program runs:

- `trace_event=sched:sched_process_exec` turns on that event at boot;
- `trace_clock=mono` stamps records with the monotonic clock, the one process
  times use;
- `trace_buf_size=8M` sets the buffer size, per CPU.

kokuin's deep mode (`sudo kokuin-cli deep`) does that for one boot. It traces every
program started, so that even a process living a millisecond gets its name. At the
end of the boot, the recorder copies `trace` into the boot's raw data.
