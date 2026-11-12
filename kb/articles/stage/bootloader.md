+++
title = "Stage: bootloader"
summary = "A small program chooses which system to boot, loads the kernel and the initramfs into memory and starts the kernel."
related = ["bootloader.grub", "bootloader.systemd_boot", "bootloader.efistub", "firmware.exit_boot_services", "stage.kernel"]
+++

The firmware does not know how to start Linux. It knows how to start a
**bootloader**: on UEFI, an `.efi` program stored on the EFI System Partition (ESP),
a small FAT partition.

The bootloader:

1. shows a menu (or not) and picks an entry;
2. loads the kernel image (`vmlinuz`) and usually an **initramfs** into memory;
3. gives the kernel its **command line** (`root=...`, `quiet`...);
4. jumps into the kernel.

On UEFI, the moment the firmware gives up control of the machine is called
**ExitBootServices**. kokuin uses it as the boundary between the bootloader and the
kernel.

Common bootloaders are GRUB, systemd-boot, rEFInd and Limine. The kernel can also be
started directly by the firmware, without a bootloader (EFI stub).
