+++
title = "Power-on and CPU reset"
summary = "What happens between pressing the power button and the first firmware instruction."
related = ["stage.firmware", "firmware.fpdt"]
+++

Pressing the power button asks the power supply to start. Once voltages are stable,
the chipset releases the CPU from **reset**, and the CPU starts executing at a fixed
address (the "reset vector"), mapped to the firmware flash chip.

Before that, on modern PCs, small processors embedded in the chipset (Intel
Management Engine, AMD Platform Security Processor) already ran their own firmware.
That part is invisible to the operating system.

The first moment kokuin can place on its timeline is **the end of the CPU reset**,
as recorded by the firmware in the FPDT table (`firmware_start_ns`). Everything
before it, from the button press, is not measurable by software.
