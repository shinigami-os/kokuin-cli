+++
title = "Processes seen in /proc"
summary = "Every running process has a directory in /proc, which tells when it started and who started it."
related = ["init.proc_events", "init.pid1"]

[[references]]
title = "proc(5)"
url = "https://man7.org/linux/man-pages/man5/proc.5.html"
+++

`/proc/<pid>/` describes the process `<pid>`: its command line (`cmdline`), program
(`exe`), parent (`stat`), user (`status`), control group (`cgroup`), and when it
started, in clock ticks since boot (field 22 of `stat`, usually 100 ticks per
second, so a 10 ms resolution).

When kokuin rebuilds a boot afterwards, `/proc` only shows processes **still
running**. Short-lived ones (boot scripts, `modprobe`, one-shot services) have
already exited and left no trace. To see them, kokuin must record the boot live.
