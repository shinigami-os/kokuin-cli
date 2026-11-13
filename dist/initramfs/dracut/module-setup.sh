#!/bin/bash
# kokuin's dracut module: starts kokuin's boot recorder at the very beginning of the
# initramfs, when the kernel command line has kokuin.record=1.
# Installed by `kokuin-cli enable` in /usr/lib/dracut/modules.d/95kokuin/.

check() {
    require_binaries /usr/bin/kokuin-cli || return 1
    # Only when asked for (add_dracutmodules), never by itself.
    return 255
}

depends() {
    return 0
}

install() {
    inst_binary /usr/bin/kokuin-cli
    if [ -f /etc/kokuin/config.toml ]; then
        inst_simple /etc/kokuin/config.toml
    fi
    if dracut_module_included "systemd"; then
        inst_simple "$moddir/kokuin-recorder.service" "$systemdsystemunitdir/kokuin-recorder.service"
        $SYSTEMCTL -q --root "$initdir" add-wants sysinit.target kokuin-recorder.service
    else
        inst_hook cmdline 00 "$moddir/kokuin-start.sh"
    fi
}
