+++
title = "shim"
summary = "A small first-stage bootloader signed by Microsoft, so Linux can boot with Secure Boot enabled."
related = ["firmware.secure_boot", "bootloader.grub"]

[[references]]
title = "shim source code"
url = "https://github.com/rhboot/shim"

[[references]]
title = "SBAT: UEFI Secure Boot Advanced Targeting"
url = "https://github.com/rhboot/shim/blob/main/SBAT.md"
+++

Most PCs only trust Microsoft keys for Secure Boot. Distributions cannot have every
GRUB and kernel build signed by Microsoft, so they use **shim**: a tiny bootloader
(`shimx64.efi`), reviewed and signed by Microsoft once, that embeds the
distribution's own certificate.

The firmware checks shim; shim checks GRUB (`grubx64.efi`) and the kernel against
the distribution's certificate or the user's own keys (**MOK**, Machine Owner Keys,
managed with `mokutil`).

When the boot entry points to `shimx64.efi`, the boot chain has one more step:
firmware → shim → GRUB → kernel.

## What shim leaves in the TPM log

With a TPM, shim measures what it decides with, so the log shows it:

- **MokList**, **MokListX**: the Machine Owner Keys, and the ones revoked.
- **MokListTrusted**: whether the kernel also trusts the Machine Owner Keys.
- **SbatLevel**: the SBAT level. Each bootloader build carries a generation
  number per component (`grub,4`); shim refuses any program older than this level.
  Revoking a vulnerable GRUB this way avoids adding every old build to `dbx`.
- The certificate each program was verified with: the one built into shim, or a
  Machine Owner Key.
