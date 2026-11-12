//! Boot records rebuilt from real snapshots, saved with `kokuin-cli capture --raw` in
//! the test VMs (`tests/vm/`) and kept in `tests/fixtures/snapshots/`.
//!
//! Every snapshot goes through the common checks. The checks of one snapshot pin
//! what is known about that boot.

use std::path::{Path, PathBuf};

use kokuin_kb::Kb;
use kokuin_model::{
    BootRecord, CaptureInfo, CaptureMethod, Event, MilestoneKind as M, SourceStatus,
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
