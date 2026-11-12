# kokuin boot record format

A **boot record** is everything kokuin knows about one boot, from power-on to a
usable desktop. `kokuin-cli` writes it as JSON. Any program can read it, and the
graphical `kokuin` app is built on it.

- JSON Schema: [`schema/boot-record.schema.json`](../schema/boot-record.schema.json)
  (draft 2020-12). Every field has a description there.
- Full example: [`examples/boot-record.example.json`](examples/boot-record.example.json)
  (a Kira Linux boot in QEMU: UEFI, GRUB, runit, greetd, sway).
- Rust types: the `kokuin-model` crate. The schema is generated from it, and a test
  fails if the committed schema is out of date.

This document explains what the schema cannot: the meaning of times, how phases and
milestones relate, and the identifiers that have a fixed meaning.

## Where records come from

| Command | Gives |
|---|---|
| `kokuin-cli capture [-o FILE] [--raw DIR]` | A record of the current boot, rebuilt from what the system still remembers. `--raw` also saves the data it was built from (see below). |
| `kokuin-cli show [RECORD] --json` | The same, or a record file (`-` for standard input), or a record rebuilt from a `--raw` folder. |
| `kokuin-cli timeline [RECORD] --json [filters]` | Only the `events` array, filtered (`--stage`, `--source`, `--kind`, `--grep`). |
| `kokuin-cli explain <explain_id> --json` | One knowledge base article: `id`, `title`, `summary`, `related`, `references`, Markdown `body`. Without an id: the list of articles. |
| `kokuin-cli status --json` | What can be read on this system, with the reason for each missing source and the kernel options it needs. |
| `kokuin-cli schema` | The JSON Schema. |

Run them as root to get every source: the TPM log, the kernel log and some firmware
data are not readable by normal users.

### Raw snapshots

`capture --raw DIR` saves everything kokuin read, so the record can be rebuilt later,
on another machine, or with newer rules (`kokuin-cli show DIR`):

- `DIR/root/`: the files that were read, at their system path (`root/proc/1/stat`,
  `root/sys/kernel/security/tpm0/binary_bios_measurements`);
- `DIR/snapshot.json`: the rest. The clocks (`boot_epoch_unix_ns`,
  `clock_ticks_per_second`), when and by which process it was saved, the commands run
  and their output, directory listings, link targets and times, and read errors.

A snapshot holds the command lines of every running process: look at it before
sharing it. Its layout has its own version (`format`), separate from the record's.

### Boot recorder files

While a recorded boot runs, the recorder (`kokuin-cli record --boot`) watches it.
When it stops, it publishes what it saw in `/run/kokuin/recorder/`, where the
sources read it, so a raw snapshot of that boot includes it:

- `kmsg`: the whole kernel log of the boot, in the `/dev/kmsg` format.
- `processes.jsonl`: one JSON object per line, `event` telling which:

| `event` | Fields | Meaning |
|---|---|---|
| `started` | `t_ns`, `method` (`proc_events` or `polling`), `handover_ns` | The recorder started watching. With `polling`, times are approximate and very short processes are missing. `handover_ns`: when kokuin-init handed over to the real init (it starts the recorder just before), which marks `init_start`. |
| `running` | `start_ns`, `pid`, `ppid`, details | A process already running when the recorder started. |
| `fork` | `t_ns`, `pid`, `ppid` | A process was created. It runs its parent's program until it runs another. |
| `exec` | `t_ns`, `pid`, details | A process started another program. |
| `exit` | `t_ns`, `pid`, `exit_code` or `exit_signal` | A process ended. |
| `lost` | `t_ns` | The kernel dropped events. |
| `stopped` | `t_ns`, `reason` (an `end_reason`) | The recorder stopped. |

The details are `comm`, `cmdline`, `exe`, `uid` and `cgroup`, all optional: a
process can exit before the recorder reads them. Times are on the boot clock.

Planned: records saved for each boot by the boot recorder, in
`/var/lib/kokuin/boots/<boot_id>/record.json` next to a `raw/` directory with the
original data, and listed by `kokuin-cli list`. Consumers should go through
`kokuin-cli` rather than read that directory, which may change.

## Versioning

`schema_version` is `MAJOR.MINOR`. The current version is **0.2**.

- A new **minor** version only adds things: optional fields, enum values, `data` keys.
  A consumer written for `X.Y` reads any `X.Z`.
- A new **major** version may rename or remove things. Check the major version and
  refuse records you were not written for.
- While the major version is `0`, the format is still being designed and a minor
  version may break things. This will stop at `1.0`.

Changes:

- **0.2**: event kind `recording` (the boot recorder starting and stopping), source
  `proc_events` filled by the boot recorder, `capture.method` `recorder` in use.

## Conventions

**Times** are integers in nanoseconds on the **boot clock**, in fields ending with
`_ns`:

- `0` is the start of the Linux kernel (the `0.000000` of `dmesg`).
- Positive values are after it, on the kernel monotonic clock, which is also the
  clock of process events.
- Negative values are before the kernel: firmware and bootloader. The firmware has
  its own clock (ACPI FPDT, counting from the CPU reset), converted with
  `boot_ns = firmware_ns - exitbootservice_end_ns` (FPDT values from `/sys/firmware/acpi/fpdt/boot/`). This assumes the kernel starts
  right when the firmware hands over, which is true to within a few milliseconds, so
  these times have `confidence: approximate`.
- The time between pressing the power button and the end of the CPU reset is not
  measurable by software. The first known moment is the `firmware_start` milestone.

Durations (`dur_ns`) are unsigned nanoseconds.

**Confidence**: every time comes with a `confidence`:

| Value | Meaning | Examples |
|---|---|---|
| `exact` | Read from a clock by the component itself | kernel log timestamps, process events |
| `approximate` | Derived from a clock, with a known error | firmware times, `/proc` start times (10 ms resolution) |
| `inferred` | No clock: placed from the order of events | TPM measurements, EFI variables |

An event with `confidence: inferred` has no `t_ns`. Its position in `events` is
still meaningful: `events` is sorted in the best-known chronological order.

**Absent fields**: an optional field that is absent means "unknown" or "not
applicable", never zero or empty. Empty lists are omitted too, except the top-level
ones.

**Enumerations** are `snake_case` strings. Each one has an `unknown` value, never
written by kokuin, that Rust consumers use for values added in a newer minor
version. Consumers in other languages must handle unexpected values too, for
example by displaying them as they are.

**References** between objects use numeric ids, unique within one record:
`events[].id`, `processes[].id`. Events and milestones refer to processes and events
by these ids. A process id is **not** a PID: the kernel reuses PIDs during a boot,
so `pid` is kept as a separate field.

## Structure

```
BootRecord
├─ schema_version
├─ boot        BootInfo: boot_id, kernel, os, machine (firmware, uefi, dmi), bootloader, init
├─ capture     CaptureInfo: tool, method (post_hoc | recorder | recorder_deep), end_reason
├─ sources[]   SourceReport: what each data source gave, or why it could not be read
├─ stages[]    StageSpan: start/end of each boot phase
├─ milestones[] Milestone: key moments
├─ processes[] Process: every process seen, with parent links
└─ events[]    Event: everything that happened, chronologically
```

## Stages

Boot phases, in the order in which they start:

```
firmware → bootloader → kernel → initramfs → init → services → graphical → session → runtime
```

| Stage | From | To |
|---|---|---|
| `firmware` | `firmware_start` | `bootloader_load` |
| `bootloader` | `bootloader_load` | `exit_boot_services` / `kernel_start` |
| `kernel` | `kernel_start` | `initramfs_start`, or `init_start` without an initramfs |
| `initramfs` | `initramfs_start` | `init_start` (switch to the real root) |
| `init` | `init_start` | `services_start` |
| `services` | `services_start` | end of the boot-time services, often unknown |
| `graphical` | `display_manager_start` (or `compositor_start` without a display manager) | `session_start` |
| `session` | `session_start` | `session_ready` |
| `runtime` | `session_ready` | end of the capture |

Stages that did not happen are absent from `stages` (for example `initramfs` on a
system booted without one). Stages **can overlap**: services keep starting while
the login screen appears. An `end_ns` that is absent means that the stage was still
running at the end of the capture, or that its end is not detectable.

Each event and process has a `stage`: the phase it is attributed to. A kernel
message printed while services start is attributed to `services`. Its `source` (`kmsg`)
still says that the kernel wrote it.

## Milestones

Each milestone `kind` appears at most once, in chronological order. `event` points
to the event the milestone was detected from, when there is one.

| Kind | Detected from |
|---|---|
| `firmware_start` | FPDT `firmware_start_ns` (ACPI name: `ResetEnd`) |
| `bootloader_load` | FPDT `bootloader_load_ns` (`OsLoaderLoadImageStart`) |
| `bootloader_start` | FPDT `bootloader_launch_ns` (`OsLoaderStartImageStart`) |
| `exit_boot_services` | FPDT `exitbootservice_end_ns` (`ExitBootServicesExit`) |
| `kernel_start` | always `0` |
| `initramfs_start` | kernel log `Run /init as init process` |
| `root_mounted` | kernel log, mount of the root filesystem |
| `init_start` | the init of the real root filesystem starting as PID 1 |
| `services_start` | init-system specific (runit stage 2, systemd `basic.target`, OpenRC `default` runlevel...) |
| `graphics_ready` | kernel log, DRM driver initialized |
| `display_manager_start` | a known display manager process starting |
| `session_start` | first process of a normal user (uid ≥ 1000): login, or automatic login |
| `compositor_start` | a known compositor / X server process of a normal user starting |
| `session_ready` | the session shell (panel, bar, desktop shell) running and, when recording live, the system settled (CPU and disk quiet) |
| `recording_end` | the recorder stopping, `settle` seconds after `session_ready` |

## Well-known identifiers

### Sources (`sources[].id`, `events[].source`)

| Id | Data | Typical `data` keys |
|---|---|---|
| `fpdt` | ACPI Firmware Performance Data Table (`/sys/firmware/acpi/fpdt`) | `field`, `firmware_ns` |
| `tpm_eventlog` | TPM measured-boot event log (`/sys/kernel/security/tpm0/binary_bios_measurements`) | `pcr`, `event_type`, `path`, `variable` |
| `efivars` | EFI variables (`/sys/firmware/efi/efivars`) | `variable`, `value`, `description`, `path` |
| `dmi` | SMBIOS/DMI tables (`/sys/class/dmi/id`) | — |
| `kmsg` | Kernel log (`/dev/kmsg`, or the boot recorder's copy) | `seq`, `facility`, plus parsed values (`driver`, `device`, `fstype`...) |
| `proc_events` | Processes seen by the boot recorder (`/run/kokuin/recorder/processes.jsonl`): it gives `processes`, including those that exited | — |
| `procfs` | Processes still running at capture time (`/proc`), for post-hoc records | `pid` |
| `runit`, `systemd`, `openrc` | The init system's own view of services | `service`, `state` |

`raw_files` paths are relative to the record directory.

### Components (`boot.bootloader.id`, `boot.init.id`)

Bootloaders: `grub`, `systemd-boot`, `refind`, `limine`, `efistub` (the firmware
starts the kernel directly), `shim` (Secure Boot first stage, followed by another
bootloader).
Init systems: `runit`, `systemd`, `openrc`, `sysvinit`, `s6`, `dinit`, `busybox`.

### Explanations (`explain_id`)

Most objects carry an `explain_id`, such as `firmware.fpdt`, `bootloader.grub` or
`init.runit_stage2`. It is the identifier of an article of the knowledge base
shipped inside `kokuin-cli`, written for people learning how Linux boots. The form is
`<area>.<topic>`, lowercase with underscores. Areas: `stage`, `firmware`,
`bootloader`, `kernel`, `init`, `graphics`, `session`.

`kokuin-cli explain <explain_id>` prints the article; with `--json`, it returns its
title, Markdown body and related articles so a GUI can display it. An
`explain_id` without an article is not an error: show nothing.
