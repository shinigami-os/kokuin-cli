+++
title = "systemd-boot"
summary = "A simple UEFI-only bootloader that reads one small file per menu entry."
related = ["stage.bootloader", "firmware.efivars", "bootloader.efistub"]

[[references]]
title = "systemd-boot manual"
url = "https://www.freedesktop.org/software/systemd/man/latest/systemd-boot.html"

[[references]]
title = "The Boot Loader Interface"
url = "https://systemd.io/BOOT_LOADER_INTERFACE/"
+++

**systemd-boot** (formerly gummiboot) only works on UEFI and has no filesystem
drivers: the kernels must be on the EFI System Partition (or an XBOOTLDR partition).
Each menu entry is a small text file in `loader/entries/`, with `linux`, `initrd` and
`options` lines. It also finds Unified Kernel Images (`EFI/Linux/*.efi`)
automatically.

It implements the **Boot Loader Interface**: it writes EFI variables that the OS can
read afterwards, such as `LoaderInfo` (its name and version), `LoaderEntrySelected`,
and `LoaderTimeInitUSec` / `LoaderTimeExecUSec` (when it started and when it started
the kernel).
