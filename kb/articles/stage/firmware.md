+++
title = "Stage: firmware"
summary = "From power-on to the bootloader: the motherboard's own software prepares the machine."
related = ["firmware.reset", "firmware.uefi", "firmware.bios", "firmware.fpdt", "stage.bootloader", "firmware.kokuin_probe"]
+++

When you press the power button, no operating system exists yet. The CPU starts
executing code stored in a flash chip on the motherboard: the **firmware**. It is
**UEFI** on modern PCs and the legacy **BIOS** on older ones.

The firmware:

1. initializes the CPU and the memory controller ("memory training" can take
   seconds, especially after a hardware change);
2. finds and initializes the devices needed to boot: storage, keyboard, screen;
3. reads its boot settings from non-volatile memory (NVRAM): which disk, which
   entry, in which order;
4. loads a bootloader from disk and starts it.

Linux cannot watch this stage: it is not running yet. kokuin rebuilds it afterwards
from the traces the firmware leaves behind: the FPDT timestamps, the TPM measurement
log, and the EFI variables.

The firmware stage is often the longest part of a boot on desktop machines. It is
also the one you can least influence: "fast boot" options in the firmware setup,
fewer devices to initialize, and firmware updates are the usual levers.

Most firmware keeps no record of its own timing. When the FPDT table is missing,
kokuin's EFI probe (`sudo kokuin-cli probe arm`) measures this stage for one boot.
