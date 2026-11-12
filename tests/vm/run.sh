#!/usr/bin/env bash
# Boots a test VM: UEFI firmware (OVMF) with a software TPM (swtpm), so the
# guest has real FPDT timestamps, EFI variables and a TPM event log.
#
# The first boot installs a small graphical desktop (sway started by greetd)
# with cloud-init, then reboots. Every later boot goes to the desktop.
#
# Usage: run.sh VM [--reset] [--headless]
#   --reset     delete the VM disk and state first, to start from the image again
#   --headless  no window: the serial console is on this terminal (Ctrl-a x quits)
set -euo pipefail
source "$(dirname "$0")/lib.sh"

usage() {
    sed -n '2,/^set /{/^set /d; s/^# \{0,1\}//; p}' "$0"
    exit "${1:-0}"
}

distro=""
reset=0
headless=0
for arg in "$@"; do
    case "$arg" in
    --reset) reset=1 ;;
    --headless) headless=1 ;;
    -h | --help) usage ;;
    -*) usage 1 ;;
    *) [[ -z $distro ]] || usage 1; distro="$arg" ;;
    esac
done
[[ -n $distro ]] || usage 1

require qemu-system-x86_64 "install qemu-system-x86"
require qemu-img "install qemu-utils"
require swtpm "install swtpm swtpm-tools"
require cloud-localds "install cloud-image-utils"

load_distro "$distro"
find_firmware "$SECURE_BOOT"
base="$IMAGES_DIR/$IMAGE_FILE"
[[ -f $base ]] || die "$IMAGE_FILE is missing: run tests/vm/fetch.sh $distro first"

if [[ $reset == 1 && -d $INSTANCE_DIR ]]; then
    log "deleting $INSTANCE_DIR"
    rm -rf "$INSTANCE_DIR"
fi

# A new VM: a disk on top of the downloaded image (left untouched), its own
# copy of the firmware variables, and the cloud-init seed.
if [[ ! -f $INSTANCE_DIR/disk.qcow2 ]]; then
    log "creating the $distro VM in $INSTANCE_DIR"
    mkdir -p "$INSTANCE_DIR/tpm"
    qemu-img create -q -f qcow2 -F qcow2 -b "$base" "$INSTANCE_DIR/disk.qcow2" "$DISK_SIZE"
    cp "$OVMF_VARS" "$INSTANCE_DIR/OVMF_VARS.fd"
    ensure_ssh_key
    sed -e "s|@SSH_KEY@|$(cat "$SSH_KEY.pub")|" -e "s|@USER@|$VM_USER|g" \
        "$SCRIPT_DIR/seed/$distro.yaml" >"$INSTANCE_DIR/user-data"
    printf 'instance-id: kokuin-%s\nlocal-hostname: kokuin-%s\n' "$distro" "$distro" \
        >"$INSTANCE_DIR/meta-data"
    cloud-localds "$INSTANCE_DIR/seed.img" "$INSTANCE_DIR/user-data" "$INSTANCE_DIR/meta-data"
fi

# The software TPM. It stops by itself when QEMU disconnects (--terminate).
tpm_socket="$INSTANCE_DIR/tpm/swtpm.sock"
if [[ -f $INSTANCE_DIR/tpm/swtpm.pid ]]; then
    kill "$(cat "$INSTANCE_DIR/tpm/swtpm.pid")" 2>/dev/null || true
fi
rm -f "$tpm_socket"
swtpm socket --tpm2 --tpmstate dir="$INSTANCE_DIR/tpm" \
    --ctrl type=unixio,path="$tpm_socket" --terminate --daemon \
    --pid file="$INSTANCE_DIR/tpm/swtpm.pid" --log file="$INSTANCE_DIR/tpm/swtpm.log"
for _ in $(seq 50); do
    [[ -S $tpm_socket ]] && break
    sleep 0.1
done
[[ -S $tpm_socket ]] || die "swtpm did not start, see $INSTANCE_DIR/tpm/swtpm.log"

if [[ -w /dev/kvm ]]; then
    accel=(-accel kvm -cpu host)
else
    log "no access to /dev/kvm: the VM will be very slow, and boot timings meaningless"
    accel=(-accel tcg -cpu max)
fi

machine="q35"
secure=()
if [[ $SECURE_BOOT == 1 ]]; then
    # Secure Boot firmware needs SMM, and its flash locked to SMM code.
    machine="q35,smm=on"
    secure=(-global driver=cfi.pflash01,property=secure,value=on)
fi

if [[ $headless == 1 ]]; then
    display=(-display none -serial mon:stdio)
else
    display=(-display gtk -serial "file:$INSTANCE_DIR/serial.log")
    log "serial console written to $INSTANCE_DIR/serial.log"
fi

log "booting $distro (SSH: tests/vm/ssh.sh $distro)"
exec qemu-system-x86_64 \
    -name "kokuin-$distro" \
    -machine "$machine" "${accel[@]}" "${secure[@]}" \
    -smp 2 -m "$MEMORY" \
    -drive if=pflash,format=raw,unit=0,readonly=on,file="$OVMF_CODE" \
    -drive if=pflash,format=raw,unit=1,file="$INSTANCE_DIR/OVMF_VARS.fd" \
    -drive if=virtio,format=qcow2,file="$INSTANCE_DIR/disk.qcow2" \
    -drive if=virtio,format=raw,readonly=on,file="$INSTANCE_DIR/seed.img" \
    -nic user,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:"$SSH_PORT"-:22 \
    -chardev socket,id=tpm,path="$tpm_socket" \
    -tpmdev emulator,id=tpm0,chardev=tpm \
    -device tpm-tis,tpmdev=tpm0 \
    -object rng-random,id=rng,filename=/dev/urandom \
    -device virtio-rng-pci,rng=rng \
    -device virtio-vga \
    -device virtio-tablet-pci \
    -device virtio-keyboard-pci \
    "${display[@]}"
