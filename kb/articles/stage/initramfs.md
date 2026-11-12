+++
title = "Stage: initramfs"
summary = "A tiny temporary system in memory prepares access to the real root filesystem, then hands over to it."
related = ["kernel.initramfs", "kernel.root_mount", "kernel.run_init", "stage.init"]
+++

To start the real init system, the kernel needs the root filesystem. Sometimes it
cannot mount it alone: the disk driver is a module stored *on* that disk, the
filesystem is encrypted (LUKS), on LVM or RAID, or found by its UUID.

The **initramfs** solves this: an archive (cpio) loaded into memory by the bootloader
with the kernel. The kernel unpacks it into a RAM filesystem and runs its `/init`. That
small program loads the needed modules, asks for the disk password if necessary,
mounts the real root filesystem, then calls `switch_root`: it deletes the initramfs
and runs the real init system, which keeps PID 1.

Distributions build it with a generator: dracut (Fedora), mkinitcpio (Arch),
initramfs-tools (Debian, Ubuntu), mkinitfs (Alpine), booster... Kira builds its own
small BusyBox-based one.

A system whose kernel has the drivers for its root disk built in can boot without an
initramfs. That stage is then absent.
