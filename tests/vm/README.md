# Test VMs

Two QEMU VMs with UEFI firmware (OVMF) and a software TPM (swtpm). Like a real
PC, they leave FPDT timestamps, EFI variables and a TPM event log, which this
laptop or a BIOS machine cannot always provide.

| VM | Boot chain | Init | libc | Secure Boot | SSH port |
|---|---|---|---|---|---|
| `alpine` | OVMF → GRUB → Linux | OpenRC | musl | off | 2231 |
| `ubuntu` | OVMF → shim → GRUB → Linux → initramfs | systemd | glibc | on | 2232 |

Both end on a sway desktop started by greetd, which logs the user `kokuin`
(password `kokuin`) in automatically.

## Requirements

```sh
sudo apt install qemu-system-x86 qemu-utils ovmf swtpm swtpm-tools cloud-image-utils
rustup target add x86_64-unknown-linux-musl
```

## Use

```sh
tests/vm/fetch.sh                 # download and verify the images (about 1 GB)
tests/vm/run.sh alpine            # boot; the first boot installs the desktop, then reboots
tests/vm/push.sh alpine           # build kokuin-cli and kokuin-init and install them (another terminal)
tests/vm/ssh.sh alpine sudo kokuin-cli show
```

`run.sh VM --headless` runs without a window, with the serial console in the
terminal (`Ctrl-a x` stops the VM). `run.sh VM --reset` starts again from the
downloaded image.

## Where the files go

Everything is in `$KOKUIN_VM_DIR` (default `~/Documents/VMs`), outside the
repository:

- `images/`: downloaded images, never modified.
- `kokuin-<vm>/`: one VM. Its disk is a layer on top of the image, plus its
  own EFI variables, TPM state, cloud-init seed and serial log.
- `kokuin-ssh/`: the SSH key used by `ssh.sh` and `push.sh`.

The firmware is looked up at the Debian, Fedora and Arch locations. Set
`OVMF_CODE` and `OVMF_VARS` to use other files.
