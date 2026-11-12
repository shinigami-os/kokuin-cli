+++
title = "GRUB commands in the TPM log"
summary = "With measured boot, GRUB records every command it runs, which shows exactly what the bootloader did."
related = ["bootloader.grub", "firmware.measured_boot"]

[[references]]
title = "GRUB manual: Measured Boot"
url = "https://www.gnu.org/software/grub/manual/grub/html_node/Measured-Boot.html"
+++

When GRUB's `tpm` module is loaded (the default on UEFI in most distributions),
GRUB measures:

- every command it executes into **PCR 8**, logged as `grub_cmd: <command>`, and
  the final kernel command line as `kernel_cmdline: ...`;
- every file it loads (kernel, initramfs, configuration, fonts, modules) into
  **PCR 9**.

These measurements end up in the TPM event log, so after boot you can read the exact
list of commands GRUB ran: which `grub.cfg` it read, which menu entry it executed,
which kernel and initramfs it loaded. kokuin shows them as events of the bootloader
stage. They have no timestamps.
