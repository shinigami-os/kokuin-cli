# TO DELETE: notes on Kira's initramfs, for kira-base

> Temporary file. It records problems found in `kira-base` while working on kokuin,
> to fix there. Delete it once they are fixed.

Found on 2026-11-13 in `shinigami-os/kira-base`, `runit/1-initramfs` (the `/init` of
the initramfs built by `kira-base`'s Makefile).

## 1. `init=` is ignored

`boot_root` ends with:

```sh
exec switch_root /newroot /sbin/runit-init
```

The program after `switch_root` is hard-coded. Every common initramfs (dracut,
mkinitcpio, initramfs-tools, Alpine's mkinitfs) starts the program named by the
`init=` kernel parameter instead, `/sbin/init` by default. Consequences:

- `kokuin-cli enable` adds `init=/usr/lib/kokuin/kokuin-init` to the kernel command
  line; on Kira it has no effect, so **no Kira boot is recorded**. Deep mode
  (`kokuin-cli deep`) depends on it as well. The EFI probe does not.
- `init=/bin/sh`, the usual way to repair a system that does not boot, does not work
  either.

Suggested fix, in `runit/1-initramfs`:

```sh
INIT=$(tr ' ' '\n' < /proc/cmdline | grep '^init=' | tail -n 1 | cut -d= -f2-)
[ -n "$INIT" ] || INIT=/sbin/runit-init
```

then, in `boot_root`:

```sh
exec switch_root /newroot "$INIT"
```

(`tail -n 1`: when the parameter is given twice, the last one wins, as in the
kernel.)

## 2. A "preflight" runs runit-init inside the initramfs

Before `switch_root`, `boot_root` runs:

```sh
echo "[initramfs] preflight: /sbin/runit-init at $ROOT"
ls -la /newroot/sbin/runit-init
mount | grep newroot
/newroot/sbin/runit-init
echo "[initramfs] preflight exec rc=$?"
```

This looks like debugging left behind. Running `runit-init` when it is not PID 1
makes it act as `init 0` / `init 6` (asking a running runit to halt or reboot),
not as an init. Here no runit is running yet, so it most likely just fails. If it is
indeed leftover debugging, these five lines can go.
