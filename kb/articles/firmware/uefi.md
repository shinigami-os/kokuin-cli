+++
title = "UEFI"
summary = "The modern PC firmware standard: a small operating system that boots your operating system."
related = ["stage.firmware", "firmware.bios", "firmware.efivars", "firmware.exit_boot_services", "firmware.boot_entries", "firmware.kokuin_probe"]

[[references]]
title = "UEFI specifications"
url = "https://uefi.org/specifications"
+++

**UEFI** (Unified Extensible Firmware Interface) replaced the PC BIOS. It is a
specification; most implementations derive from Intel's open-source **EDK II**
(also used by QEMU as "OVMF").

A UEFI firmware boots in phases:

- **SEC** (security): first instructions, uses the CPU cache as temporary RAM;
- **PEI** (pre-EFI initialization): initializes the memory;
- **DXE** (driver execution environment): loads drivers for the devices;
- **BDS** (boot device selection): reads the boot entries and starts the chosen one.

Unlike the BIOS, UEFI understands partitions (GPT) and the FAT filesystem: it loads a
bootloader file (`\EFI\<distro>\grubx64.efi` for example) from the **EFI System
Partition**. It also offers services to the bootloader (files, screen, memory) until
**ExitBootServices**, and keeps a few "runtime services" for the OS, like access to
EFI variables.

Linux shows that it booted through UEFI with the `/sys/firmware/efi` directory.
