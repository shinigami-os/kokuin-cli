# TO DELETE: handoff to the next chat

> Temporary file: where the work stopped on 2026-11-13, so another chat can finish
> Phase 6b. Delete it once Phase 6b is done.

## Rules to keep following

- **Commits:** Conventional Commits, one line, as `OxoGhost01 <arlenghost@proton.me>`
  (the repo-local git identity; check `git config user.email`). **Never** add a
  `Co-Authored-By` or any attribution line.
- **Branch:** work directly on `main`.
- **Staging:** files by name, never `git add -A`. `.vscode/` stays untracked.
- **Pushing:** allowed once `cargo test` passes entirely, to `origin`
  (`git@github-oxoghost:shinigami-os/kokuin-cli.git`). Never force-push.
- **Code:** touch only what the task needs; clean, reusable code following each
  file's conventions; explore and plan before starting a new problem.
- **Unsafe:** the workspace denies `unsafe`. The only exceptions are the two
  `#[allow(unsafe_code)]` functions that read the TSC (`bins/kokuin-probe` and
  `crates/kokuin-sources/src/context.rs`).
- **Docs and KB:** short sentences, written for learners.

## Tools

- In a tool shell, run `source ~/.cargo/env` before cargo.
- Checks: `cargo test`, `cargo clippy --all-targets`, and for the probe
  `cargo clippy --release --target x86_64-unknown-uefi -p kokuin-probe`. The probe
  is outside `default-members`.
- VMs (`tests/vm/README.md`): `tests/vm/run.sh <vm> --headless`, `tests/vm/push.sh
  <vm>` (builds and installs kokuin-cli, kokuin-init and the EFI probe),
  `tests/vm/ssh.sh <vm> <command>`. Files live in `~/Documents/VMs`.
- **Ubuntu VM:** systemd, dracut, Secure Boot on. Recording is enabled (GRUB
  drop-in) from before this phase.
- **Alpine VM:** OpenRC, mkinitfs, Secure Boot off. Recording is enabled (block in
  `/etc/default/grub`). The EFI probe is installed as `Boot000A`, and a test entry
  `Boot0009 "Alpine (GRUB)"` exists (not in `BootOrder`).
- Both VMs are shut down.

## Phase 6b: recording from the initramfs

The full plan was approved by the user. Its essentials:

- **Scope:** hooks for **dracut** (Ubuntu 25.10+, Fedora...), **initramfs-tools**
  (Debian) and **mkinitcpio** (Arch), tested in VMs: the existing Ubuntu VM, plus
  a **Debian** VM and an **Arch** VM to add.
- **Kira:** instructions only, no code and no tests (see
  `TO-DELETE-kira-initramfs.md`). The user will edit `kira-base` directly.
- **Mechanism:**
  - a hook starts `kokuin-cli record --initramfs` when the kernel command line
    has `kokuin.record=1`;
  - the recorder follows the switch to the real root (`chroot` into
    `/proc/1/root`) and writes a `root_switched` line that marks `init_start`;
  - one recorder at a time: the abstract socket `@kokuin-recorder`, which
    `kokuin-init` checks before starting one.

### Done (all committed; the last 6 commits are not pushed yet)

1. `54303c2` `crates/kokuin-sources/src/proc_events.rs`: `started.from`
   (`init`/`initramfs`), the `root_switched` line, the "initramfs switched to the
   real root" observation marking `init_start`.
2. `378cc5e` `crates/kokuin-recorder`: `root.rs` (`RootFollower`), `instance.rs`
   (the abstract socket claim); `record()` takes `StartedFrom`.
3. `4443183` `kokuin-cli record --initramfs`; `kokuin-init` skips starting a
   recorder when the socket is taken. **Tested by hand** in the Alpine VM from a
   chroot: it followed into `/` and saved the boot in the real `/var/lib/kokuin`.
4. `f799c0e` `kokuin_bootcfg::parameters` now starts with `kokuin.record=1`
   (`RECORD_MARKER`), for both `enable` and `deep`.
5. `564e697` The hook files in `dist/initramfs/`: the shared systemd unit
   `kokuin-recorder.service`, `dracut/` (`module-setup.sh`, `kokuin-start.sh`),
   `initramfs-tools/` (`hook`, `init-top`) and `mkinitcpio/` (`install`, `hook`).
   Also `crates/kokuin-bootcfg/src/initramfs.rs`: detection, plan enable/disable,
   regeneration, unit tests on fake systems. `Edit` gained `executable` and
   `own_folder`, and `Plan.bootloader` became `Plan.title`.
6. `688fcd0` `enable`/`disable` apply the initramfs plan before the GRUB one;
   `status` shows the hook. Dry runs show long new files by their line count.

All 148 tests pass; clippy is clean.

### What I was doing when stopped

Starting step 5 on the **Ubuntu VM**: boot it, `tests/vm/push.sh ubuntu`, then
`sudo kokuin-cli enable`. That installs the dracut module, regenerates the
initramfs, and updates the GRUB drop-in with `kokuin.record=1`. Then check
`lsinitrd | grep kokuin`, reboot, and verify. **Nothing of this ran**: the VM never
started.

### Left to do

1. **Ubuntu (dracut + systemd in the initramfs):**
   - enable, check `lsinitrd`, reboot;
   - check `/run/kokuin/recorder/processes.jsonl` starts with
     `"from":"initramfs"` and has a `root_switched` line;
   - `show last`: the initramfs stage with its processes (udev, dracut hooks…),
     `init_start` exact;
   - `systemctl status kokuin-recorder` after boot, to check the process survived
     the switch to the real root (the unit has `SurviveFinalKillSignal=yes`, and
     `argv[0]` starts with `@`).

   Points to check:
   - is the `dracut` binary at `/usr/bin` or `/usr/sbin` on Ubuntu (detection
     looks in both);
   - does dracut accept the module (`check()` returns 255, so it is only included
     through `/etc/dracut.conf.d/kokuin.conf`);
   - does `$systemdsystemunitdir` exist in module-setup.
2. **Add the Debian and Arch VMs** to `tests/vm/`:
   - `lib.sh` `load_distro`: image URL, checksum, SSH port (2233, 2234), memory,
     Secure Boot off;
   - `seed/debian.yaml` and `seed/arch.yaml` (sway + greetd autologin, as in
     `seed/ubuntu.yaml`; Arch's greetd user is `greeter`, Debian's `_greetd`; to
     check);
   - `tests/vm/README.md` table.

   Images:
   - Debian 13 `debian-13-genericcloud-amd64.qcow2` from
     `https://cloud.debian.org/images/cloud/trixie/latest/`, with its `SHA512SUMS`;
   - Arch `Arch-Linux-x86_64-cloudimg.qcow2` from
     `https://geo.mirror.pkgbuild.com/images/latest/`, with its `.SHA256`.

   Check Arch's image boots through UEFI with OVMF. If not, add a BIOS mode to
   `run.sh`; the EFI probe and efivars don't matter there.
3. **Test initramfs-tools on Debian** and **mkinitcpio on Arch**, as for Ubuntu.
   - Arch: check `/etc/mkinitcpio.conf.d/` drop-ins are read by its mkinitcpio
     version, and whether its `HOOKS` use `systemd` (then the unit path is taken)
     or `base udev`.
   - Check the result with `lsinitramfs` / `lsinitcpio`.
4. **`disable`** in each VM: the hook files are removed, the initramfs is
   regenerated, and the GRUB config is back as before.
5. **Fixtures:** one recorded boot per generator in `tests/fixtures/snapshots/`
   (`ubuntu-26.04-qemu-initramfs`, `debian-13-qemu-initramfs`,
   `arch-qemu-initramfs`). Raw folders are in `/var/lib/kokuin/boots/<id>/raw/`;
   copy them with `ssh.sh <vm> 'sudo tar -C <raw> -cf - .' | tar -C <fixture> -xf
   - --no-same-owner`, then grep them for personal data. Add tests in
   `crates/kokuin-analyze/tests/snapshots.rs`: `from` is initramfs, initramfs-stage
   processes exist, `init_start` comes from `root_switched` with exact confidence.
6. **Docs:**
   - `docs/initramfs.md`, the contract for any initramfs: start `kokuin-cli
     record --initramfs` early, in the background, when `kokuin.record=1` is on
     the command line, with `/proc` mounted and the binary (and
     `/etc/kokuin/config.toml` if present) in the initramfs;
   - a **Kira section** there: in `kira-base/runit/1-initramfs`, right after
     mounting `/proc`, `/sys` and `/dev`, add the same `case " $(cat
     /proc/cmdline) " in *" kokuin.record=1 "*) /usr/bin/kokuin-cli record
     --initramfs </dev/null >/dev/null 2>&1 & ;; esac` as
     `dist/initramfs/initramfs-tools/init-top`; in the Makefile, copy
     `/usr/bin/kokuin-cli` into `$(INITRAMFS_ROOT)/usr/bin/` (a static musl
     binary, nothing else needed);
   - point `TO-DELETE-kira-initramfs.md` to it;
   - KB: a "Recording from the initramfs" section in
     `kb/articles/init/kokuin.md`, and a mention in
     `kb/articles/kernel/initramfs.md`;
   - `docs/format.md`: the `started.from` and `root_switched` lines in the recorder
     files table; `status --json`'s `recorder.initramfs` (`generator`,
     `hook_installed`); a change log entry (format 0.5);
   - `README.md`: one line on the initramfs hook.
7. Run `cargo test` and clippy, commit, push, delete this file.

## Known open points (not part of 6b)

- `TO-DELETE-kira-initramfs.md`: Kira's initramfs ignores `init=`, and has a
  leftover "preflight" that runs `runit-init` before `switch_root`. These are for
  the user to fix in `kira-base`.
- Later phases: s6/dinit adapters, aarch64, a systemd-boot adapter, signing the EFI
  probe for Secure Boot. The flux recipe is done by the user, not here.
- The memory files in `~/.claude/projects/-home-stagiaire-Documents-flowvisu/memory/`
  hold the project's context and the user's rules.
