+++
title = "Secure Boot"
summary = "The UEFI firmware only runs boot programs signed by a trusted key."
related = ["firmware.uefi", "firmware.measured_boot", "bootloader.shim"]
+++

With **Secure Boot** enabled, the UEFI firmware checks the signature of every program
it runs (drivers from add-on cards, bootloaders) against keys stored in its
variables:

- **PK** (Platform Key): the owner of the machine, who controls the KEK list;
- **KEK** (Key Exchange Keys): keys allowed to update db and dbx;
- **db**: certificates and hashes of allowed programs;
- **dbx**: revoked ones.

Most PCs ship with Microsoft keys only. Linux distributions therefore boot through
**shim**, a small first-stage loader signed by Microsoft, which embeds the
distribution's own key and checks the next stage with it.

Secure Boot verifies; it does not record. Measured boot (TPM) records.
