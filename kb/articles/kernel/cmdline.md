+++
title = "Kernel command line"
summary = "Parameters given by the bootloader to the kernel: root filesystem, init, logging, debugging options."
related = ["bootloader.grub", "kernel.run_init", "kernel.initcall"]

[[references]]
title = "Linux documentation: kernel parameters"
url = "https://docs.kernel.org/admin-guide/kernel-parameters.html"
+++

The bootloader passes a line of text to the kernel, for example:

```
BOOT_IMAGE=/vmlinuz-6.17 root=UUID=3b9e... ro quiet init=/sbin/runit-init
```

- `root=` tells where the root filesystem is;
- `ro` mounts it read-only first, so it can be checked; the init system remounts it
  read-write;
- `quiet` hides most kernel messages on the console (they are still in `dmesg`);
- `init=` chooses the program to run as PID 1, `rdinit=` the one in the initramfs;
- `initcall_debug` logs every initcall with its duration;
- `log_buf_len=` enlarges the kernel log buffer.

Parameters the kernel does not know are passed to the init system. Read the current
one in `/proc/cmdline`.
