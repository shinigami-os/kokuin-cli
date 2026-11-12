#!/usr/bin/env bash
# Downloads the cloud images of the test VMs and verifies their checksums.
#
# Usage: fetch.sh [VM...]   (default: all of them)
set -euo pipefail
source "$(dirname "$0")/lib.sh"

require curl "install curl"

fetch() {
    load_distro "$1"
    local target="$IMAGES_DIR/$IMAGE_FILE"
    if [[ -f $target ]]; then
        log "$1: $IMAGE_FILE already downloaded"
        return 0
    fi
    mkdir -p "$IMAGES_DIR"
    log "$1: downloading $IMAGE_URL"
    curl --fail --location --proto '=https' --tlsv1.2 --continue-at - \
        --output "$target.part" "$IMAGE_URL"
    log "$1: verifying the checksum"
    if ! echo "$CHECKSUM  $target.part" | "$CHECKSUM_TOOL" --check --quiet; then
        rm -f "$target.part"
        die "$1: checksum mismatch, the download was removed"
    fi
    mv "$target.part" "$target"
}

for distro in "${@:-${DISTROS[@]}}"; do
    fetch "$distro"
done
