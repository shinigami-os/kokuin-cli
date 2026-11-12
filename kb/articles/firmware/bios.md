+++
title = "Legacy BIOS"
summary = "The original PC firmware, still emulated by many UEFI machines (CSM, legacy mode)."
related = ["firmware.uefi", "stage.firmware"]
+++

The **BIOS** booted PCs from 1981 until UEFI replaced it. It runs in 16-bit mode,
knows nothing about partitions or filesystems, and boots by loading the first sector
of a disk (512 bytes, the **MBR**) and jumping into it. A bootloader like GRUB must
then load the rest of itself from fixed places on the disk.

Many UEFI machines can still boot this way through a **Compatibility Support
Module** (CSM, "legacy mode"). When Linux is booted like that, `/sys/firmware/efi`
does not exist, there are no EFI variables, and kokuin sees less of the pre-kernel
stages. Some firmwares still provide FPDT timestamps and a TPM log in that mode.
