+++
title = "Physical memory map"
summary = "The firmware tells the kernel which ranges of physical memory are usable RAM and which are reserved."
related = ["stage.kernel", "firmware.uefi"]
+++

Not all physical addresses are RAM: some ranges are reserved by the firmware, used
for ACPI tables, or map device registers. The firmware gives the kernel a **memory
map** (the "e820" table on PC BIOS, the UEFI memory map on UEFI), printed at the
start of the kernel log (`BIOS-provided physical RAM map:` followed by `usable` and
`reserved` ranges).

The kernel builds its memory management on it. The usable total is what `free` and
`/proc/meminfo` later show as total memory, minus what the kernel keeps for itself.
