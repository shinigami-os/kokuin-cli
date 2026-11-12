#!/usr/bin/env bash
# Opens a shell in a running test VM, or runs a command in it.
#
# Usage: ssh.sh VM [COMMAND...]
#   ssh.sh alpine
#   ssh.sh ubuntu sudo kokuin-cli show
set -euo pipefail
source "$(dirname "$0")/lib.sh"

[[ $# -ge 1 ]] || die "usage: ssh.sh VM [COMMAND...]"
load_distro "$1"
shift
[[ -f $SSH_KEY ]] || die "no SSH key yet: start the VM with tests/vm/run.sh first"

# shellcheck disable=SC2046 # the options are meant to be split
exec ssh $(ssh_options) -p "$SSH_PORT" "$VM_USER@127.0.0.1" "$@"
