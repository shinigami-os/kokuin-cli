+++
title = "systemd"
summary = "The init system of most mainstream distributions: services, dependencies, logging and much more."
related = ["init.pid1", "stage.init", "stage.services"]

[[references]]
title = "bootup(7): the systemd boot process"
url = "https://www.freedesktop.org/software/systemd/man/latest/bootup.html"
+++

**systemd** manages **units**: services, mounts, devices, sockets, timers, and
**targets** (groups of units that mark a state, like `sysinit.target` or
`graphical.target`). Each unit declares its dependencies and ordering, and systemd
starts as much as possible in parallel.

A typical boot: `sysinit.target` (mounts, udev, swap...) → `basic.target` →
`multi-user.target` (services) → `graphical.target` (display manager).

systemd records when each unit started and became active. `systemd-analyze blame`
and `systemd-analyze critical-chain` use these times, and kokuin reads them as well.
It runs in the initramfs too on many distributions ("Switching root").
