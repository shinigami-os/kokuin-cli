+++
title = "Remounting the root filesystem read-write"
summary = "The root filesystem is first mounted read-only for checking, then made writable."
related = ["kernel.root_mount", "stage.init"]
+++

The `ro` kernel parameter makes the root filesystem mounted read-only at first. This
lets the init system check it with `fsck` while nothing writes to it. Then it runs
`mount -o remount,rw /`, and the kernel logs `re-mounted`.

Until that moment, nothing can be written to the disk: logs stay in memory, and
programs needing to write fail.
