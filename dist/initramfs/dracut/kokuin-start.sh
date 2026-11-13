#!/bin/sh
# kokuin's dracut hook, for initramfs images without systemd: starts kokuin's boot
# recorder in the background when the kernel command line has kokuin.record=1.
if getargbool 0 kokuin.record; then
    /usr/bin/kokuin-cli record --initramfs </dev/null >/dev/null 2>&1 &
fi
