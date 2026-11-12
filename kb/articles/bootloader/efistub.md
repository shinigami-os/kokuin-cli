+++
title = "EFI stub: booting without a bootloader"
summary = "The Linux kernel can be an UEFI program itself, started directly by the firmware."
related = ["stage.bootloader", "firmware.boot_entries", "bootloader.systemd_boot"]

[[references]]
title = "Linux documentation: The EFI Boot Stub"
url = "https://docs.kernel.org/admin-guide/efi-stub.html"
+++

With `CONFIG_EFI_STUB`, the kernel image is also a valid UEFI program. A UEFI boot
entry can point directly to the kernel, with the command line as argument:

```
efibootmgr --create --label "Linux" --loader /vmlinuz-linux \
  --unicode 'root=UUID=... rw initrd=\initramfs-linux.img'
```

The firmware then starts the kernel itself: no menu, no bootloader stage. The kernel's
EFI stub loads the initramfs, calls ExitBootServices and continues booting.

A **Unified Kernel Image** (UKI) goes further: one `.efi` file containing the stub,
the kernel, the initramfs and the command line, which can be signed as a whole for
Secure Boot.
