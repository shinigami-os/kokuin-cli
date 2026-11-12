+++
title = "EFI variables"
summary = "Settings stored by the UEFI firmware in non-volatile memory, shared between the firmware, the bootloader and the OS."
related = ["firmware.boot_entries", "firmware.secure_boot", "firmware.uefi"]

[[references]]
title = "Linux documentation: efivarfs"
url = "https://docs.kernel.org/filesystems/efivarfs.html"
+++

UEFI firmwares store settings in **variables** kept in flash memory (NVRAM). Each
variable has a name and a vendor GUID. Linux shows them as files in
`/sys/firmware/efi/efivars/`, named `<Name>-<GUID>`. Each file starts with 4 bytes
of attributes, followed by the value.

Variables kokuin reads:

- `BootOrder`, `Boot####`, `BootCurrent`: the boot entries;
- `Timeout`: how long the firmware boot menu waits;
- `SecureBoot`, `SetupMode`: Secure Boot state;
- `LoaderInfo`, `LoaderTimeInitUSec`, `LoaderTimeExecUSec`,
  `LoaderEntrySelected`...: written by bootloaders that follow the systemd
  Boot Loader Interface (systemd-boot, and some others).

`efibootmgr` reads and changes the boot entries from Linux.
