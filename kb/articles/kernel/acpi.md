+++
title = "ACPI"
summary = "Tables and bytecode in which the PC firmware describes the hardware and how to manage its power."
related = ["stage.kernel", "firmware.fpdt", "kernel.pci"]
+++

Many devices of a PC cannot be discovered by probing: the embedded controller, the
power buttons, the battery, the fans, the way interrupts are routed. The firmware
describes them in **ACPI tables**. Some tables are plain data (MADT lists the CPUs,
FPDT holds boot timings), others (DSDT, SSDT) contain bytecode (AML) that the
kernel's ACPI interpreter executes to manage power and devices.

`ACPI: Interpreter enabled` means the kernel can now use that description. Buggy
ACPI tables are a common source of boot warnings and slow boots. Linux shows the
tables in `/sys/firmware/acpi/tables/`.
