+++
title = "Starting the other CPUs"
summary = "The kernel boots on one CPU core, then starts all the others."
related = ["stage.kernel"]
+++

Only one CPU core (the "bootstrap processor") runs the firmware, the bootloader and
the beginning of the kernel. Once its basic setup is done, the kernel starts the
other cores ("application processors") one by one: `smp: Brought up 1 node, 8 CPUs`.

From then on, the kernel can do work in parallel, for example probe devices
asynchronously.
