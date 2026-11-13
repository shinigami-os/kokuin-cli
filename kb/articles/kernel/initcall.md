+++
title = "Initcalls"
summary = "The hundreds of initialization functions the kernel runs at boot, including every built-in driver."
related = ["stage.kernel", "kernel.cmdline", "kernel.pci"]

[[references]]
title = "Linux documentation: kernel parameters (initcall_debug)"
url = "https://docs.kernel.org/admin-guide/kernel-parameters.html"
+++

Each part of the kernel that needs to initialize something registers an
**initcall**. At boot, the kernel runs them by level: core, postcore, arch,
subsys, fs, device, late. The `device` level holds the built-in drivers.

With the `initcall_debug` kernel parameter, each one is logged with its duration:

```
initcall pci_init+0x0/0x40 returned 0 after 1523 usecs
```

This is the best way to find which driver slows the kernel stage down. kokuin's deep
mode (`sudo kokuin-cli deep`) adds this parameter for one boot. Initcalls that wait for hardware (probing a
disk, a USB hub, a missing device) are usual suspects.
