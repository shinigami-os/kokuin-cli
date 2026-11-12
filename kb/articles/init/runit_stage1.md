+++
title = "runit stage 1"
summary = "The one-time setup script of runit: everything that must be done before services start."
related = ["init.runit", "init.runit_stage2", "stage.init"]
+++

`/etc/runit/1` is a shell script run by runit at boot. runit waits for it to finish
before starting stage 2. On Kira and Void it typically mounts the virtual
filesystems, starts udev and waits for devices to settle, checks and remounts the root
filesystem read-write, mounts `/etc/fstab`, sets the hostname, the clock and the
keymap, and seeds the random number generator.

Everything here is sequential: stage 1 is often where a runit system spends most of
its init time. Waiting for udev to settle is a common cost.
