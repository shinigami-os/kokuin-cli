+++
title = "The kernel unpacks the initramfs"
summary = "The kernel extracts the initramfs archive into a RAM filesystem before starting userspace."
related = ["stage.initramfs", "kernel.run_init"]

[[references]]
title = "Linux documentation: ramfs, rootfs and initramfs"
url = "https://docs.kernel.org/filesystems/ramfs-rootfs-initramfs.html"
+++

The initramfs is a compressed cpio archive. It can be built into the kernel image,
or loaded by the bootloader as a separate file (`initrd` line in GRUB). The kernel
extracts it into **rootfs**, a RAM filesystem (`Trying to unpack rootfs image as
initramfs...`), then frees the memory of the archive (`Freeing initrd memory`).

A big initramfs (many modules and firmware files) takes longer to load from disk by
the bootloader and to decompress by the kernel. Host-only initramfs (only the drivers
of this machine) are smaller and faster.
