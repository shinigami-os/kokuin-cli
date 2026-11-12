+++
title = "Stage: kernel"
summary = "Linux initializes itself and the hardware, then starts the first process."
related = ["kernel.start", "kernel.cmdline", "kernel.initcall", "kernel.run_init", "kernel.kmsg", "stage.initramfs"]
+++

The kernel starts with almost nothing: one CPU, memory it does not know yet, no
drivers. In order, it:

1. decompresses itself and sets up memory management, using the memory map given by
   the firmware;
2. reads the firmware's description of the hardware (ACPI tables on PCs, a device
   tree on many ARM boards);
3. starts the other CPUs;
4. runs its **initcalls**: hundreds of initialization functions, including the
   built-in drivers, which detect the hardware (PCI, USB, disks, graphics);
5. unpacks the initramfs, if there is one;
6. starts the first userspace process, **PID 1**.

Everything the kernel says during this stage goes to its log buffer, the one `dmesg`
shows. On the boot clock used by kokuin, the kernel starts at `0`.

The kernel stage ends when PID 1 starts. The kernel keeps working afterwards, loading
modules and detecting hot-plugged devices for example, but from then on, userspace
drives the boot.
