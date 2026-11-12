+++
title = "Swap"
summary = "Disk space used as an extension of memory, enabled by the init system."
related = ["stage.init"]
+++

**Swap** is a partition or a file where the kernel moves memory pages that are not
used much, to free RAM. The init system enables it from `/etc/fstab` (`swapon -a`),
and the kernel logs `Adding 8388604k swap on /dev/nvme0n1p3`.

A swap partition also stores the memory image during hibernation.
