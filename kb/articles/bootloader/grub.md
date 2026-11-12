+++
title = "GRUB"
summary = "The GNU GRand Unified Bootloader, the most common Linux bootloader."
related = ["bootloader.grub_commands", "stage.bootloader", "bootloader.shim", "kernel.cmdline"]

[[references]]
title = "GRUB manual"
url = "https://www.gnu.org/software/grub/manual/grub/grub.html"
+++

**GRUB** is a small operating system of its own: it has drivers for disks and
filesystems (ext4, btrfs, XFS, LVM, LUKS...), a shell, and a scripting language. On
UEFI it is a program like `\EFI\<distro>\grubx64.efi`; on BIOS it is split between
the MBR and the space after it.

At boot, GRUB:

1. reads its configuration, `/boot/grub/grub.cfg`;
2. shows the menu (unless hidden) and waits for the timeout;
3. runs the commands of the chosen entry, typically:
   `linux /vmlinuz-... root=UUID=... ro quiet` (load the kernel with its command
   line), `initrd /initramfs-...` (load the initramfs), then `boot`.

`grub.cfg` is generated: edit `/etc/default/grub` (for example
`GRUB_CMDLINE_LINUX_DEFAULT`), then run `grub-mkconfig -o /boot/grub/grub.cfg`
(`update-grub` on Debian and Ubuntu).

GRUB adds `BOOT_IMAGE=<kernel path>` to the kernel command line, which is one of the
ways kokuin recognizes it.
