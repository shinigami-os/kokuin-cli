+++
title = "The kernel starts"
summary = "The first line of the kernel log: the moment kokuin uses as time zero."
related = ["stage.kernel", "kernel.kmsg", "kernel.cmdline"]
+++

`Linux version 6.x.y-... (compiler) #1 SMP ...` is the first message of the kernel
log. It tells which kernel booted, how it was built and when.

Before this line, the kernel already did a few things: decompressing itself and, on
UEFI, possibly running its EFI stub. But this is where the kernel's clock starts:
`dmesg` shows it at `0.000000`. kokuin uses it as **time zero** of its boot clock;
the firmware and the bootloader happen at negative times.

`uname -r` shows the release of the running kernel, and `/proc/version` the full
line.
