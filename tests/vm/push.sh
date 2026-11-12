#!/usr/bin/env bash
# Builds kokuin-cli as a static (musl) binary and installs it in a running
# test VM, as /usr/local/bin/kokuin-cli.
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
require cargo "install Rust with rustup"

target=x86_64-unknown-linux-musl
log "building kokuin-cli for $target"
cargo build --quiet --release --locked --target "$target" -p kokuin-cli \
    --manifest-path "$REPO_DIR/Cargo.toml"
binary="$REPO_DIR/target/$target/release/kokuin-cli"

log "installing it in the $1 VM"
# shellcheck disable=SC2046 # the options are meant to be split
scp $(ssh_options) -P "$SSH_PORT" "$binary" "$VM_USER@127.0.0.1:/tmp/kokuin-cli"
"$SCRIPT_DIR/ssh.sh" "$1" sudo install -m 0755 /tmp/kokuin-cli /usr/local/bin/kokuin-cli
