+++
title = "shim"
summary = "A small first-stage bootloader signed by Microsoft, so Linux can boot with Secure Boot enabled."
related = ["firmware.secure_boot", "bootloader.grub"]

[[references]]
title = "shim source code"
url = "https://github.com/rhboot/shim"
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
