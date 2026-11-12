+++
title = "UEFI boot entries"
summary = "BootOrder and Boot#### variables tell the firmware which programs to try, and in what order."
related = ["firmware.efivars", "firmware.uefi", "stage.bootloader"]
+++

A UEFI firmware does not look for a boot sector. It reads its **boot entries**:
variables named `Boot0000`, `Boot0001`... Each one contains a description ("Kira
Linux", "Windows Boot Manager"), the disk partition and the path of an `.efi`
program, and optional arguments.

`BootOrder` lists the entries to try, in order. `BootCurrent` tells which one booted
the running system. If no entry works, the firmware falls back to the default path
`\EFI\BOOT\BOOTX64.EFI` of each disk, which is how USB installers boot.

Installers create an entry with `efibootmgr` (or `grub-install`, `bootctl
install`). Run `efibootmgr -v` to see yours.
