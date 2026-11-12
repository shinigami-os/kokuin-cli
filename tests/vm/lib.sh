# Settings and helpers shared by the test VM scripts. Sourced, not run.
#
# Everything the VMs need (downloaded images, disks, firmware variables, TPM
# state, SSH key) lives in $KOKUIN_VM_DIR, outside the repository, so it
# survives `cargo clean` and reboots.

KOKUIN_VM_DIR="${KOKUIN_VM_DIR:-$HOME/Documents/VMs}"
IMAGES_DIR="$KOKUIN_VM_DIR/images"
SSH_KEY="$KOKUIN_VM_DIR/kokuin-ssh/id_ed25519"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
DISTROS=(alpine ubuntu)
VM_USER=kokuin

die() {
    echo "error: $*" >&2
    exit 1
}

log() {
    echo "==> $*" >&2
}

# Sets the settings of one test VM:
#   IMAGE_URL, IMAGE_FILE        cloud image to download
#   CHECKSUM_TOOL, CHECKSUM      how to verify it
#   SSH_PORT                     host port forwarded to the guest SSH server
#   MEMORY, DISK_SIZE            guest resources
#   SECURE_BOOT                  1 to boot with Secure Boot (Microsoft keys)
load_distro() {
    case "$1" in
    alpine)
        # GRUB + OpenRC + musl. Alpine publishes cloud images for x86_64 UEFI
        # with cloud-init; it is not signed, so no Secure Boot.
        IMAGE_URL=https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/cloud/generic_alpine-3.24.1-x86_64-uefi-cloudinit-r0.qcow2
        CHECKSUM_TOOL=sha512sum
        CHECKSUM=ed976ef40de1f73adcb0a3b253ec9e73e43c408208fcc3c30dcdf7a69b91a387a4777f88c6b72345123edf3832d7cb49403ecce28ec84d496d4b3bad6fbd0923
        SSH_PORT=2231
        MEMORY=2G
        DISK_SIZE=8G
        SECURE_BOOT=0
        ;;
    ubuntu)
        # shim + GRUB + systemd + glibc, booted with Secure Boot like most PCs.
        IMAGE_URL=https://cloud-images.ubuntu.com/releases/26.04/release-20261016/ubuntu-26.04-server-cloudimg-amd64.img
        CHECKSUM_TOOL=sha256sum
        CHECKSUM=5645d0318ad21f643ebb1ac4567514620e9893380b07b0201fda163baa5b7eb5
        SSH_PORT=2232
        MEMORY=4G
        DISK_SIZE=16G
        SECURE_BOOT=1
        ;;
    *)
        die "unknown VM '$1' (known: ${DISTROS[*]})"
        ;;
    esac
    IMAGE_FILE="${IMAGE_URL##*/}"
    INSTANCE_DIR="$KOKUIN_VM_DIR/kokuin-$1"
}

# Prints the first existing file among the arguments.
first_existing() {
    local candidate
    for candidate in "$@"; do
        if [[ -f $candidate ]]; then
            echo "$candidate"
            return 0
        fi
    done
    return 1
}

# Sets OVMF_CODE and OVMF_VARS, the UEFI firmware and its variable store
# template. OVMF_CODE and OVMF_VARS in the environment take precedence.
find_firmware() {
    local secure_boot="$1"
    if [[ -n ${OVMF_CODE:-} && -n ${OVMF_VARS:-} ]]; then
        return 0
    fi
    if [[ $secure_boot == 1 ]]; then
        # Debian/Ubuntu, Fedora, Arch.
        OVMF_CODE=$(first_existing \
            /usr/share/OVMF/OVMF_CODE_4M.secboot.fd \
            /usr/share/edk2/ovmf/OVMF_CODE.secboot.fd \
            /usr/share/edk2/x64/OVMF_CODE.secboot.4m.fd) ||
            die "no Secure Boot OVMF firmware found: install ovmf, or set OVMF_CODE and OVMF_VARS"
        OVMF_VARS=$(first_existing \
            /usr/share/OVMF/OVMF_VARS_4M.ms.fd \
            /usr/share/edk2/ovmf/OVMF_VARS.secboot.fd \
            /usr/share/edk2/x64/OVMF_VARS.4m.fd) ||
            die "no OVMF variable store with Microsoft keys found: set OVMF_VARS"
    else
        OVMF_CODE=$(first_existing \
            /usr/share/OVMF/OVMF_CODE_4M.fd \
            /usr/share/edk2/ovmf/OVMF_CODE.fd \
            /usr/share/edk2/x64/OVMF_CODE.4m.fd) ||
            die "no OVMF firmware found: install ovmf, or set OVMF_CODE and OVMF_VARS"
        OVMF_VARS=$(first_existing \
            /usr/share/OVMF/OVMF_VARS_4M.fd \
            /usr/share/edk2/ovmf/OVMF_VARS.fd \
            /usr/share/edk2/x64/OVMF_VARS.4m.fd) ||
            die "no OVMF variable store found: set OVMF_VARS"
    fi
}

# Fails with a hint when a command is missing.
require() {
    local command="$1" hint="$2"
    command -v "$command" >/dev/null || die "$command not found: $hint"
}

# Creates the SSH key used to log into the test VMs, once.
ensure_ssh_key() {
    if [[ ! -f $SSH_KEY ]]; then
        mkdir -p "$(dirname "$SSH_KEY")"
        ssh-keygen -q -t ed25519 -N '' -C kokuin-test-vm -f "$SSH_KEY"
    fi
}

# Options for ssh and scp: the dedicated key, and no host key checks, since
# the guest key changes each time a VM is recreated.
ssh_options() {
    echo -i "$SSH_KEY" -o IdentitiesOnly=yes -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR
}
