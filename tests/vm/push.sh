#!/usr/bin/env bash
# Builds kokuin-cli and kokuin-init as static (musl) binaries, and the EFI probe, and
# installs them in a running test VM where a package would put them:
# /usr/bin/kokuin-cli, /usr/lib/kokuin/kokuin-init and /usr/lib/kokuin/kokuin-probe.efi.
#
# Usage: push.sh VM
set -euo pipefail
source "$(dirname "$0")/lib.sh"

[[ $# -eq 1 ]] || die "usage: push.sh VM"
load_distro "$1"
[[ -f $SSH_KEY ]] || die "no SSH key yet: start the VM with tests/vm/run.sh first"

if ! command -v cargo >/dev/null && [[ -f $HOME/.cargo/env ]]; then
    source "$HOME/.cargo/env"
fi
require cargo "install Rust with rustup (and: rustup target add x86_64-unknown-uefi)"

target=x86_64-unknown-linux-musl
log "building kokuin-cli and kokuin-init for $target"
cargo build --quiet --release --locked --target "$target" -p kokuin-cli -p kokuin-init \
    --manifest-path "$REPO_DIR/Cargo.toml"
built="$REPO_DIR/target/$target/release"
log "building the EFI probe"
cargo build --quiet --release --locked --target x86_64-unknown-uefi -p kokuin-probe \
    --manifest-path "$REPO_DIR/Cargo.toml"
probe="$REPO_DIR/target/x86_64-unknown-uefi/release/kokuin-probe.efi"

log "installing them in the $1 VM"
# shellcheck disable=SC2046 # the options are meant to be split
scp $(ssh_options) -P "$SSH_PORT" "$built/kokuin-cli" "$built/kokuin-init" "$probe" \
    "$VM_USER@127.0.0.1:/tmp/"
"$SCRIPT_DIR/ssh.sh" "$1" "sudo install -m 0755 /tmp/kokuin-cli /usr/bin/kokuin-cli &&
    sudo install -D -m 0755 /tmp/kokuin-init /usr/lib/kokuin/kokuin-init &&
    sudo install -m 0644 /tmp/kokuin-probe.efi /usr/lib/kokuin/kokuin-probe.efi"
