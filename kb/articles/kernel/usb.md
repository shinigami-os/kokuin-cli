+++
title = "USB devices"
summary = "USB devices are discovered when their controller is ready, and again whenever one is plugged in."
related = ["kernel.pci", "init.udev"]
+++

USB controllers are PCI devices. Once their driver is ready, the kernel asks each
port what is connected: `usb 1-2: new high-speed USB device number 3`. Each device
describes itself (class, vendor, product), and the matching driver binds to it, or
udev loads one.

USB discovery takes time (devices need to power up and answer) but happens in
parallel with the rest of the boot. `lsusb -t` shows the tree.
