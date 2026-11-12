+++
title = "ExitBootServices: the firmware hands over"
summary = "The UEFI call after which the firmware stops managing the machine and the operating system takes control."
related = ["firmware.uefi", "firmware.fpdt", "stage.bootloader", "stage.kernel"]
+++

Until `ExitBootServices()` is called, the UEFI firmware owns the machine: it manages
memory, timers and devices, and offers services (files, screen, keyboard) that the
bootloader uses.

When the bootloader (or the kernel's own EFI stub) is ready to start the kernel, it
calls `ExitBootServices()`. From then on, only the "runtime services" remain, like
reading and writing EFI variables. The OS owns the hardware.

This is the boundary kokuin uses between the bootloader and the kernel. The firmware
records it in the FPDT, and the TPM log often contains an "Exit Boot Services
Invocation" event.
