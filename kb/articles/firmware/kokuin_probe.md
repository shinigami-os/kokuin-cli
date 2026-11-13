+++
title = "kokuin's EFI probe"
summary = "A tiny EFI program that runs before the bootloader, for one boot, to time the firmware and note what it set up."
related = ["firmware.uefi", "firmware.boot_entries", "firmware.reset", "firmware.fpdt", "firmware.secure_boot", "init.kokuin"]

[[references]]
title = "UEFI specification: boot manager (BootNext, BootOrder)"
url = "https://uefi.org/specs/UEFI/2.10/03_Boot_Manager.html"
+++

Before Linux starts, nothing records what the firmware does unless the firmware
itself leaves timestamps (the FPDT table, often missing). kokuin's EFI probe fills
that gap for one boot.

## How it runs

`sudo kokuin-cli probe install` copies the probe to the EFI system partition
(`\EFI\kokuin\probe.efi`) and creates a boot entry for it, without adding it to
`BootOrder`: the firmware never starts it on its own. `sudo kokuin-cli probe arm`
sets **`BootNext`** to that entry. `BootNext` means "boot this entry next time,
once": the firmware clears it before starting the program, so only that one boot
goes through the probe. `kokuin-cli deep` arms it too.

The probe then:

1. reads the CPU's **time stamp counter** (TSC), a counter that starts at 0 when the
   CPU is reset and ticks at a fixed rate;
2. notes what the firmware set up: who made it, how much memory it describes, how
   many programs and drivers it loaded;
3. starts the usual boot entry (GRUB, for example), and just before, writes all
   that in an EFI variable, `KokuinProbe`.

If it cannot start that entry, it returns, and the firmware goes on with its boot
order as if the probe had never been there.

## From a counter to a timeline

Once Linux runs, kokuin reads the same counter again, at the same moment as the boot
clock. Knowing how fast the counter ticks, any counter value becomes a time on the
boot clock: power-on (counter 0), the probe starting (the end of the firmware's
work), the probe handing over (the bootloader starting). The firmware and bootloader
stages get real durations.

In a virtual machine, the counter starts when the machine starts, which plays the
part of power-on.

## Secure Boot

With Secure Boot on, the firmware only starts programs signed with a key it trusts.
The probe is not signed, so kokuin does not install it then. (Distributions get
GRUB started through `shim`, signed by Microsoft; shim's own keys are not trusted by
the firmware itself.)
