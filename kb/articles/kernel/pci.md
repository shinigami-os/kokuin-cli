+++
title = "PCI device enumeration"
summary = "The kernel scans the PCI buses to discover the devices inside the machine."
related = ["stage.kernel", "kernel.initcall", "kernel.usb"]
+++

Most devices inside a PC (graphics, network, NVMe disks, USB and SATA controllers)
are on **PCI Express** buses. The kernel scans them, reads each device's vendor and
device identifiers, and gives each one to the driver that declared it supports it.
Each device is named by its address, like `0000:03:00.0` (domain:bus:device.function).

`lspci` lists them, `lspci -k` shows which driver uses each one. Drivers built as
modules are loaded later, by udev.
