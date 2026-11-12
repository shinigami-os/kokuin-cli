+++
title = "Measured boot and the TPM event log"
summary = "Each component of the boot chain is hashed into the TPM before it runs, and a log of these measurements tells what ran, in order."
related = ["firmware.secure_boot", "bootloader.grub_commands", "stage.firmware", "stage.bootloader"]

[[references]]
title = "TCG PC Client Platform Firmware Profile"
url = "https://trustedcomputinggroup.org/resource/pc-client-specific-platform-firmware-profile-specification/"

[[references]]
title = "GRUB manual: Measured Boot"
url = "https://www.gnu.org/software/grub/manual/grub/html_node/Measured-Boot.html"
+++

A **TPM** (Trusted Platform Module) is a security chip, or a function of the CPU's
firmware, with registers called **PCRs** (Platform Configuration Registers). A PCR
cannot be set: it can only be *extended*, `PCR = hash(PCR + new data)`. Its final
value therefore depends on every measurement, in order.

With **measured boot**, every component measures the next one before running it:
the firmware measures its own parts, its settings, the bootloader; the bootloader
measures the kernel, its command line and its configuration. Each measurement is
also written to an **event log** that says what was measured. Linux shows it in
`/sys/kernel/security/tpm0/binary_bios_measurements` (root only).

The PCRs are used for security (unlocking a disk only if the boot chain was not
modified, for example). For kokuin, the event log is valuable for a different
reason: it is the most detailed record of what happened before the kernel, in order.
It has **no timestamps**, so kokuin places its events by their order only
(`confidence: inferred`).

Common PCRs: 0 firmware code, 1 firmware settings, 2 option ROMs (add-on cards),
4 boot programs (bootloader, kernel), 5 partition table, 7 Secure Boot state,
8 and 9 GRUB commands and files.
