+++
title = "udev: the device manager"
summary = "udev reacts to devices found by the kernel: it loads drivers, creates /dev entries and applies rules."
related = ["stage.init", "kernel.pci", "kernel.usb"]

[[references]]
title = "udev(7)"
url = "https://man7.org/linux/man-pages/man7/udev.7.html"
+++

For each device it detects, the kernel sends a **uevent** to userspace. The
**udev** daemon (`udevd`: eudev on Kira, systemd-udevd elsewhere) receives them and,
following rules in `/usr/lib/udev/rules.d` and `/etc/udev/rules.d`:

- loads the driver module for the device (`modprobe`), which can make the kernel
  find more devices;
- sets permissions and creates the stable names in `/dev/disk/by-uuid/`,
  `/dev/input/by-id/`...;
- loads firmware files, renames network interfaces.

At boot, devices found before udev started are "replayed" with `udevadm trigger`, and
the init system often waits for `udevadm settle`: all events processed. That wait
can be long when a device is slow to answer.
