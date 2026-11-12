+++
title = "Mounting the root filesystem"
summary = "The disk partition holding the system becomes /."
related = ["stage.initramfs", "init.remount_rw", "kernel.cmdline"]
+++

The **root filesystem** is mounted at `/`: it holds the programs, the libraries
and the configuration. It is chosen by the `root=` kernel parameter, by device name
(`/dev/nvme0n1p2`), by UUID or by label.

Without an initramfs, the kernel mounts it itself; with one, the initramfs does, then
switches to it. It is usually mounted read-only first, so it can be checked
(`fsck`), and remounted read-write by the init system.

kokuin takes the first disk filesystem mounted during boot as the root filesystem.
This is right in the usual case; a system that mounts another disk first would
mislead it.
