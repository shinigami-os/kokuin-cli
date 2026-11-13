//! Boot records rebuilt from real snapshots, saved with `kokuin-cli capture --raw` in
//! the test VMs (`tests/vm/`) and kept in `tests/fixtures/snapshots/`.
//!
//! Every snapshot goes through the common checks. The checks of one snapshot pin
//! what is known about that boot.

use std::path::{Path, PathBuf};

use kokuin_kb::Kb;
use kokuin_model::{
    BootRecord, CaptureInfo, CaptureMethod, Confidence, EndReason, Event, EventKind,
    MilestoneKind as M, SourceStatus,
};
use kokuin_sources::Context;

fn snapshots_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/snapshots")
}

fn build(dir: &Path) -> BootRecord {
    let ctx = Context::from_snapshot(dir)
        .unwrap_or_else(|err| panic!("cannot read the snapshot {}: {err}", dir.display()));
    let capture = CaptureInfo {
        tool: "test".into(),
        tool_version: "0".into(),
        method: CaptureMethod::PostHoc,
        captured_at: String::new(),
        end_reason: None,
    };
    kokuin_analyze::build_record(kokuin_analyze::collect(&ctx), capture, Kb::embedded())
}

fn snapshot(name: &str) -> BootRecord {
    build(&snapshots_dir().join(name))
}

fn status(record: &BootRecord, source: &str) -> SourceStatus {
    record
        .sources
        .iter()
        .find(|s| s.id == source)
        .unwrap_or_else(|| panic!("no report for {source}"))
        .status
}

fn reason<'a>(record: &'a BootRecord, source: &str) -> &'a str {
    record
        .sources
        .iter()
        .find(|s| s.id == source)
        .and_then(|s| s.reason.as_deref())
        .unwrap_or_default()
}

fn milestone_event(record: &BootRecord, kind: M) -> Option<&Event> {
    let id = record.milestones.iter().find(|m| m.kind == kind)?.event?;
    record.events.iter().find(|e| e.id == id)
}

/// The program of the process behind a milestone.
fn milestone_program(record: &BootRecord, kind: M) -> Option<&str> {
    let process = milestone_event(record, kind)?.process?;
    let process = record.processes.iter().find(|p| p.id == process)?;
    Some(&process.comm)
}

fn milestone_time(record: &BootRecord, kind: M) -> Option<i64> {
    Some(record.milestones.iter().find(|m| m.kind == kind)?.t_ns)
}

/// What a recorded boot has that a capture after the fact does not.
fn assert_recorded(record: &BootRecord) {
    assert_eq!(status(record, "proc_events"), SourceStatus::Ok);
    assert_eq!(status(record, "procfs"), SourceStatus::Ok);
    let end = milestone_event(record, M::RecordingEnd).expect("the recording end");
    assert_eq!(
        kokuin_sources::proc_events::end_reason(end),
        Some(EndReason::SessionReady)
    );
    let exited = record
        .processes
        .iter()
        .filter(|p| p.end_ns.is_some())
        .count();
    assert!(exited > 500, "{exited} processes that exited");
    // Kernel messages were moved onto the boot clock, keeping their own time.
    let moved = record
        .events
        .iter()
        .filter(|e| e.source == "kmsg")
        .all(|e| e.data.contains_key("kmsg_ns"));
    assert!(moved);
    // Desktop ready comes from the recorded process start: an exact time.
    let ready = record
        .milestones
        .iter()
        .find(|m| m.kind == M::SessionReady)
        .unwrap();
    assert_eq!(ready.confidence, Confidence::Exact);
}

fn has_title(record: &BootRecord, start: &str) -> bool {
    record.events.iter().any(|e| e.title.starts_with(start))
}

#[test]
fn every_snapshot_gives_a_consistent_record() {
    let kb = Kb::embedded();
    let mut checked = 0;
    for entry in std::fs::read_dir(snapshots_dir()).unwrap() {
        let dir = entry.unwrap().path();
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let record = build(&dir);
        checked += 1;

        for event in &record.events {
            assert!(
                !event.title.is_empty() && !event.title.contains('\n'),
                "{name}: titles fit on one line: {:?}",
                event.title
            );
        }
        let explain_ids = record
            .events
            .iter()
            .filter_map(|e| e.explain_id.as_deref())
            .chain(record.stages.iter().filter_map(|s| s.explain_id.as_deref()))
            .chain(
                record
                    .sources
                    .iter()
                    .filter_map(|s| s.explain_id.as_deref()),
            );
        for id in explain_ids {
            assert!(kb.article(id).is_some(), "{name}: no article {id}");
        }
        for kind in [M::DisplayManagerStart, M::CompositorStart] {
            if record.milestones.iter().any(|m| m.kind == kind) {
                assert!(
                    milestone_program(&record, kind).is_some(),
                    "{name}: {kind:?} is linked to its program"
                );
            }
        }
        assert!(
            record.milestones.iter().any(|m| m.kind == M::SessionReady),
            "{name}: the desktop was ready"
        );
        // Init runs from the root filesystem: the clocks of the kernel log and of the
        // processes must agree on that.
        if let (Some(root), Some(init)) = (
            milestone_time(&record, M::RootMounted),
            milestone_time(&record, M::InitStart),
        ) {
            assert!(
                root <= init,
                "{name}: root mounted at {root}, init at {init}"
            );
        }
        let json = serde_json::to_string(&record).unwrap();
        let back: BootRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back, record, "{name}: the record survives JSON");
    }
    assert!(
        checked >= 2,
        "the snapshots are in {}",
        snapshots_dir().display()
    );
}

/// Alpine 3.24: OVMF without Secure Boot, GRUB, initramfs, OpenRC, greetd, sway. Its
/// `virt` kernel has no TPM driver and no FPDT support.
#[test]
fn alpine_openrc_boot() {
    let record = snapshot("alpine-3.24-qemu");
    let boot = &record.boot;
    assert_eq!(boot.init.as_ref().unwrap().id, "openrc");
    assert_eq!(boot.bootloader.as_ref().unwrap().id, "grub");
    assert_eq!(boot.machine.uefi.as_ref().unwrap().secure_boot, Some(false));

    assert_eq!(status(&record, "efivars"), SourceStatus::Ok);
    assert_eq!(status(&record, "openrc"), SourceStatus::Ok);
    assert_eq!(status(&record, "tpm_eventlog"), SourceStatus::Unavailable);
    assert_eq!(status(&record, "fpdt"), SourceStatus::Unavailable);
    assert!(reason(&record, "fpdt").contains("does not provide an FPDT table"));

    // OpenRC services are linked to their daemon through its pidfile.
    assert_eq!(
        milestone_program(&record, M::DisplayManagerStart),
        Some("greetd")
    );
    assert_eq!(milestone_program(&record, M::CompositorStart), Some("sway"));
    assert_eq!(milestone_program(&record, M::SessionReady), Some("swaybar"));
}

/// Ubuntu 26.04: OVMF with Secure Boot, shim, GRUB, initramfs, systemd, greetd, sway.
#[test]
fn ubuntu_secure_boot_with_shim() {
    let record = snapshot("ubuntu-26.04-qemu");
    let boot = &record.boot;
    assert_eq!(boot.init.as_ref().unwrap().id, "systemd");
    let bootloader = boot.bootloader.as_ref().unwrap();
    assert_eq!(bootloader.id, "grub");
    assert!(bootloader.name.as_deref().unwrap().contains("shim"));
    assert_eq!(boot.machine.uefi.as_ref().unwrap().secure_boot, Some(true));

    for source in ["tpm_eventlog", "efivars", "dmi", "kmsg", "systemd"] {
        assert_eq!(status(&record, source), SourceStatus::Ok, "{source}");
    }
    assert!(reason(&record, "fpdt").contains("does not provide an FPDT table"));

    // The TPM log tells the whole chain, from the firmware to the kernel command line.
    assert!(has_title(
        &record,
        "Firmware measured the Secure Boot state: enabled"
    ));
    assert!(has_title(
        &record,
        r"Firmware measured the boot program \EFI\ubuntu\shimx64.efi"
    ));
    assert!(has_title(&record, "Shim measured the SBAT level"));
    assert!(has_title(
        &record,
        "Shim measured the Machine Owner Keys (MokList)"
    ));
    assert!(has_title(
        &record,
        r"Firmware measured the boot program \EFI\ubuntu\grubx64.efi"
    ));
    assert!(has_title(&record, "GRUB ran: linux /vmlinuz-"));
    assert!(has_title(
        &record,
        "Firmware measured a program a bootloader loaded from memory"
    ));
    assert!(has_title(
        &record,
        "Kernel EFI stub measured its command line"
    ));
    assert!(has_title(&record, "Kernel EFI stub measured the initramfs"));
    assert!(!has_title(
        &record,
        "Secure Boot: a program was verified with a key from SbatLevel"
    ));

    for kind in [
        M::KernelStart,
        M::InitramfsStart,
        M::RootMounted,
        M::InitStart,
        M::ServicesStart,
        M::DisplayManagerStart,
        M::SessionStart,
        M::CompositorStart,
        M::SessionReady,
    ] {
        assert!(milestone_event(&record, kind).is_some(), "{kind:?}");
    }
    assert_eq!(
        milestone_program(&record, M::DisplayManagerStart),
        Some("greetd")
    );
}

/// The Alpine VM, recorded by kokuin-init and the recorder.
#[test]
fn alpine_recorded_boot() {
    let record = snapshot("alpine-3.24-qemu-recorded");
    assert_recorded(&record);
    // OpenRC keeps no time of its start: the handover by kokuin-init tells it.
    let init = milestone_event(&record, M::InitStart).unwrap();
    assert!(init.title.starts_with("kokuin-init started the recorder"));
}

/// The Ubuntu VM, recorded by kokuin-init and the recorder.
#[test]
fn ubuntu_recorded_boot() {
    let record = snapshot("ubuntu-26.04-qemu-recorded");
    assert_recorded(&record);
    // systemd's own exact time wins over the handover.
    let init = milestone_event(&record, M::InitStart).unwrap();
    assert_eq!(init.source, "systemd");
    assert_eq!(
        milestone_program(&record, M::DisplayManagerStart),
        Some("greetd")
    );
}

/// The same Ubuntu VM, rebuilt after the fact, then recorded live.
#[test]
fn diff_of_a_rebuilt_and_a_recorded_boot() {
    let rebuilt = snapshot("ubuntu-26.04-qemu");
    let recorded = snapshot("ubuntu-26.04-qemu-recorded");

    let same = kokuin_analyze::diff(&recorded, &recorded);
    assert!(same.steps.is_empty() && same.new_programs.is_empty());
    assert!(same.milestones.iter().all(|m| m.delta_ns == Some(0)));

    let diff = kokuin_analyze::diff(&rebuilt, &recorded);
    let ready = diff
        .milestones
        .iter()
        .find(|m| m.kind == M::SessionReady)
        .unwrap();
    assert!(ready.a_ns.is_some() && ready.b_ns.is_some());
    // Milestones are in time order.
    let times: Vec<i64> = diff
        .milestones
        .iter()
        .filter_map(|m| m.b_ns.or(m.a_ns))
        .collect();
    assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
    // Only the recording sees the short-lived programs.
    assert!(diff.new_programs.len() > 50, "{}", diff.new_programs.len());
    // Services are matched by name: steps are not all new or gone.
    assert!(
        diff.steps
            .iter()
            .any(|s| s.a_ns.is_some() && s.b_ns.is_some())
    );
}

/// Boots armed with `kokuin-cli deep`: kernel init functions timed, every program
/// named by the kernel's trace.
#[test]
fn deep_boots() {
    for name in ["alpine-3.24-qemu-deep", "ubuntu-26.04-qemu-deep"] {
        let record = snapshot(name);
        assert_recorded(&record);
        let cmdline = record.boot.kernel.cmdline.as_deref().unwrap_or_default();
        assert!(cmdline.contains("kokuin.deep=1"), "{name}");
        let initcalls = record
            .events
            .iter()
            .filter(|e| e.kind == EventKind::Initcall && e.dur_ns.is_some())
            .count();
        assert!(initcalls > 100, "{name}: {initcalls} timed initcalls");
        // initcall_debug's lines did not push the start of the log out.
        assert_eq!(status(&record, "kmsg"), SourceStatus::Ok, "{name}");
        let unnamed = record
            .processes
            .iter()
            .filter(|p| p.comm.is_empty())
            .count();
        assert_eq!(unnamed, 0, "{name}: every program named by the trace");
    }
}

/// A boot that went through kokuin's EFI probe: the stages before the kernel timed.
#[test]
fn probed_boot() {
    let record = snapshot("alpine-3.24-qemu-probe");
    assert_recorded(&record);
    let time = |kind| milestone_time(&record, kind).unwrap_or_else(|| panic!("{kind:?}"));
    let power_on = time(M::FirmwareStart);
    let probe = time(M::BootloaderLoad);
    let grub = time(M::BootloaderStart);
    assert!(power_on < probe && probe < grub && grub < 0);
    // The firmware took seconds; the probe itself, milliseconds.
    assert!(probe - power_on > 1_000_000_000);
    assert!(grub - probe < 100_000_000);
    let measured = |stage| {
        record
            .stages
            .iter()
            .find(|s| s.stage == stage)
            .is_some_and(|s| s.start_ns.is_some() && s.end_ns.is_some())
    };
    assert!(measured(kokuin_model::Stage::Firmware));
    assert!(measured(kokuin_model::Stage::Bootloader));
    let started = milestone_event(&record, M::BootloaderStart).unwrap();
    assert!(started.title.contains("Alpine (GRUB)"), "{}", started.title);
}
