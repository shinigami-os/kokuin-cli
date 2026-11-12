+++
title = "FPDT: firmware performance timestamps"
summary = "An ACPI table in which the firmware records when it started, when it started the bootloader and when it handed over to the OS."
related = ["firmware.reset", "firmware.exit_boot_services", "stage.firmware", "stage.bootloader"]

[[references]]
title = "Linux ABI documentation: sysfs-firmware-acpi (fpdt)"
url = "https://github.com/torvalds/linux/blob/master/Documentation/ABI/testing/sysfs-firmware-acpi"

[[references]]
title = "ACPI specification, chapter 5 (Firmware Performance Data Table)"
url = "https://uefi.org/specs/ACPI/6.5/05_ACPI_Software_Programming_Model.html"
+++

The **Firmware Performance Data Table** (FPDT) is an ACPI table where the firmware
writes a few timestamps of the boot, in nanoseconds since the CPU reset. Linux shows
them in `/sys/firmware/acpi/fpdt/boot/` (readable by root only):

| File | Moment |
|---|---|
| `firmware_start_ns` | end of the CPU reset: the firmware starts |
| `bootloader_load_ns` | the firmware starts loading the bootloader |
| `bootloader_launch_ns` | the firmware starts executing the bootloader |
| `exitbootservice_start_ns` | the bootloader asks the firmware to step aside |
| `exitbootservice_end_ns` | the firmware has handed the machine over |

A value of `0` means the firmware did not record it.

kokuin converts these times to its boot clock by assuming the kernel starts at
`exitbootservice_end_ns`, which is accurate to within a few milliseconds. They are
marked `approximate`. This is also how `systemd-analyze` measures the "firmware"
and "loader" times.

The table needs the kernel option `CONFIG_ACPI_FPDT`. Not every firmware fills it in.
