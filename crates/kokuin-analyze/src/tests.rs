//! End-to-end test: a Kira-like boot (UEFI, GRUB, initramfs, runit, greetd, sway)
//! rebuilt from synthetic source outputs.

use kokuin_kb::Kb;
use kokuin_model::{
    BootInfo, CaptureInfo, CaptureMethod, ComponentInfo, Confidence, EventKind, FirmwareKind,
    KernelInfo, MachineInfo, MilestoneKind as M, SourceReport, SourceStatus, Stage, UefiInfo,
};
use kokuin_sources::{Observation, ProcessInfo, SourceOutput, system::SystemInfo};

use super::*;

fn report(id: &str) -> SourceReport {
    SourceReport {
        id: id.into(),
        status: SourceStatus::Ok,
        reason: None,
        raw_files: vec![],
        explain_id: None,
    }
}

fn fpdt(title: &str, t_ns: i64, stage: Stage, milestone: M) -> Observation {
    Observation::new(
        EventKind::FirmwareTiming,
        title,
        Some(t_ns),
        Confidence::Approximate,
    )
    .in_stage(stage)
    .marks(milestone)
}

fn tpm(title: &str, stage: Stage, data: &[(&str, &str)]) -> Observation {
    let mut obs =
        Observation::new(EventKind::Measurement, title, None, Confidence::Inferred).in_stage(stage);
    for (key, value) in data {
        obs = obs.with(key, *value);
    }
    obs
}

fn kmsg(seq: u64, t_ms: i64, message: &str) -> Observation {
    let mut obs = Observation::new(
        EventKind::Log,
        message,
        Some(t_ms * 1_000_000),
        Confidence::Exact,
    )
    .with("seq", seq);
    obs.message = Some(message.into());
    obs
}

fn process(pid: u32, ppid: u32, uid: u32, cmdline: &[&str], start_ms: i64) -> ProcessInfo {
    ProcessInfo {
        pid,
        ppid: Some(ppid),
        uid: Some(uid),
        comm: cmdline[0].rsplit('/').next().unwrap().into(),
        cmdline: cmdline.iter().map(|s| s.to_string()).collect(),
        start_ns: Some(start_ms * 1_000_000),
        ..ProcessInfo::default()
    }
}

fn kira_boot() -> Collected {
    let boot = BootInfo {
        boot_id: "test".into(),
        started_at: None,
        hostname: Some("kira".into()),
        kernel: KernelInfo {
            release: "6.17.7-shinigami".into(),
            version: None,
            cmdline: Some("BOOT_IMAGE=/vmlinuz ro".into()),
        },
        os: None,
        machine: MachineInfo {
            arch: "x86_64".into(),
            firmware: FirmwareKind::Uefi,
            uefi: Some(UefiInfo::default()),
            dmi: None,
        },
        bootloader: None,
        init: None,
    };
    let outputs = vec![
        SourceOutput {
            report: report("fpdt"),
            observations: vec![
                fpdt(
                    "firmware start",
                    -7_070_000_000,
                    Stage::Firmware,
                    M::FirmwareStart,
                ),
                fpdt(
                    "bootloader load",
                    -2_320_000_000,
                    Stage::Bootloader,
                    M::BootloaderLoad,
                ),
                fpdt(
                    "exit boot services",
                    0,
                    Stage::Bootloader,
                    M::ExitBootServices,
                ),
            ],
            processes: vec![],
        },
        SourceOutput {
            report: report("tpm_eventlog"),
            observations: vec![
                tpm("Firmware version measured", Stage::Firmware, &[]),
                tpm(
                    "Firmware measured the boot program",
                    Stage::Bootloader,
                    &[
                        ("event_type", "EV_EFI_BOOT_SERVICES_APPLICATION"),
                        ("path", "\\EFI\\kira\\grubx64.efi"),
                    ],
                ),
                tpm(
                    "GRUB ran: linux",
                    Stage::Bootloader,
                    &[("grub_command", "linux /vmlinuz")],
                ),
            ],
            processes: vec![],
        },
        SourceOutput {
            report: report("kmsg"),
            observations: vec![
                kmsg(0, 0, "Linux version 6.17.7-shinigami (clang) #1 SMP"),
                kmsg(1, 1, "efi: EFI v2.7 by EDK II"),
                kmsg(2, 300, "Trying to unpack rootfs image as initramfs..."),
                kmsg(3, 1_100, "Run /init as init process"),
                kmsg(
                    4,
                    1_180,
                    "[drm] Initialized virtio_gpu 0.1.0 for virtio0 on minor 0",
                ),
                kmsg(
                    5,
                    1_390,
                    "EXT4-fs (vda2): mounted filesystem 3b9e ro with ordered data mode.",
                ),
                kmsg(
                    6,
                    4_000,
                    "usb 1-1: new high-speed USB device number 2 using xhci_hcd",
                ),
            ],
            processes: vec![],
        },
        SourceOutput {
            report: SourceReport {
                status: SourceStatus::Partial,
                ..report("procfs")
            },
            observations: vec![],
            processes: vec![
                process(1, 0, 0, &["runit"], 1_100),
                process(401, 1, 0, &["runsvdir", "-P", "/etc/sv"], 2_950),
                process(452, 401, 0, &["runsv", "greetd"], 3_000),
                process(455, 452, 0, &["greetd"], 3_100),
                process(470, 455, 102, &["cage"], 3_200),
                process(600, 401, 0, &["syslogd"], 3_400),
                process(612, 455, 1000, &["sway"], 5_800),
                process(640, 612, 1000, &["waybar"], 6_300),
            ],
        },
        SourceOutput {
            report: report("runit"),
            observations: vec![{
                let mut obs = Observation::new(
                    EventKind::Service,
                    "Service greetd started",
                    Some(3_100_000_000),
                    Confidence::Approximate,
                )
                .with("service", "greetd");
                obs.pid = Some(455);
                obs
            }],
            processes: vec![],
        },
    ];
    Collected {
        system: SystemInfo {
            boot,
            dmi_report: report("dmi"),
        },
        init: Some(ComponentInfo {
            id: "runit".into(),
            name: Some("runit".into()),
            version: None,
            explain_id: None,
        }),
        outputs,
    }
}

fn capture() -> CaptureInfo {
    CaptureInfo {
        tool: "kokuin-cli".into(),
        tool_version: "test".into(),
        method: CaptureMethod::PostHoc,
        captured_at: "2026-11-12T00:00:00Z".into(),
        end_reason: None,
    }
}

#[test]
fn rebuilds_a_kira_boot() {
    let kb = Kb::embedded();
    let record = build_record(kira_boot(), capture(), kb);

    let milestones: Vec<_> = record
        .milestones
        .iter()
        .map(|m| (m.kind, m.t_ns / 1_000_000))
        .collect();
    assert_eq!(
        milestones,
        [
            (M::FirmwareStart, -7_070),
            (M::BootloaderLoad, -2_320),
            (M::ExitBootServices, 0),
            (M::KernelStart, 0),
            (M::InitramfsStart, 1_100),
            (M::GraphicsReady, 1_180),
            (M::RootMounted, 1_390),
            (M::InitStart, 1_390),
            (M::ServicesStart, 2_950),
            (M::DisplayManagerStart, 3_100),
            (M::SessionStart, 5_800),
            (M::CompositorStart, 5_800),
            (M::SessionReady, 6_300),
        ]
    );
    let stages: Vec<_> = record.stages.iter().map(|s| s.stage).collect();
    assert_eq!(
        stages,
        [
            Stage::Firmware,
            Stage::Bootloader,
            Stage::Kernel,
            Stage::Initramfs,
            Stage::Init,
            Stage::Services,
            Stage::Graphical,
            Stage::Session,
            Stage::Runtime
        ]
    );

    // Untimed TPM events are placed at the start of their stage, after the timed ones.
    let titles: Vec<_> = record
        .events
        .iter()
        .take(5)
        .map(|e| e.title.as_str())
        .collect();
    assert_eq!(
        titles,
        [
            "firmware start",
            "Firmware version measured",
            "bootloader load",
            "Firmware measured the boot program",
            "GRUB ran: linux"
        ]
    );
    // Ids follow the timeline, and milestones point to their events.
    assert!(
        record
            .events
            .iter()
            .enumerate()
            .all(|(i, e)| e.id.0 == i as u64 + 1)
    );
    let drm = record
        .milestones
        .iter()
        .find(|m| m.kind == M::GraphicsReady)
        .unwrap();
    let drm_event = &record.events[drm.event.unwrap().0 as usize - 1];
    assert_eq!(drm_event.title, "Graphics driver virtio_gpu is ready");

    // A system service starting during the login screen stays in the services stage;
    // a kernel message at that time belongs to the graphical stage.
    let stage_of = |title: &str| {
        record
            .events
            .iter()
            .find(|e| e.title == title)
            .unwrap()
            .stage
    };
    assert_eq!(stage_of("syslogd started"), Stage::Services);
    assert_eq!(stage_of("cage started"), Stage::Graphical);
    assert_eq!(stage_of("New high-speed USB device"), Stage::Graphical);

    // Processes: tree, services, stages.
    let greetd = record
        .processes
        .iter()
        .find(|p| p.comm == "greetd")
        .unwrap();
    assert_eq!(greetd.service.as_deref(), Some("greetd"));
    assert_eq!(greetd.stage, Stage::Graphical);
    let service_event = record.events.iter().find(|e| e.source == "runit").unwrap();
    assert_eq!(service_event.process, Some(greetd.id));

    // System description.
    let bootloader = record.boot.bootloader.as_ref().unwrap();
    assert_eq!(
        (bootloader.id.as_str(), bootloader.explain_id.as_deref()),
        ("grub", Some("bootloader.grub"))
    );
    assert_eq!(
        record.boot.init.as_ref().unwrap().explain_id.as_deref(),
        Some("init.runit")
    );
    let uefi = record.boot.machine.uefi.as_ref().unwrap();
    assert_eq!(
        (uefi.vendor.as_deref(), uefi.spec_version.as_deref()),
        (Some("EDK II"), Some("2.7"))
    );
    assert_eq!(
        record
            .sources
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["fpdt", "tpm_eventlog", "dmi", "kmsg", "procfs", "runit"]
    );
}

#[test]
fn every_explain_id_has_an_article() {
    let kb = Kb::embedded();
    let record = build_record(kira_boot(), capture(), kb);
    let ids = record
        .events
        .iter()
        .filter_map(|e| e.explain_id.as_deref())
        .chain(
            record
                .processes
                .iter()
                .filter_map(|p| p.explain_id.as_deref()),
        )
        .chain(record.stages.iter().filter_map(|s| s.explain_id.as_deref()))
        .chain(
            record
                .milestones
                .iter()
                .filter_map(|m| m.explain_id.as_deref()),
        );
    for id in ids {
        assert!(kb.article(id).is_some(), "no article for {id}");
    }
}

#[test]
fn source_explanations_have_articles() {
    let kb = Kb::embedded();
    for id in ["runit", "systemd", "openrc"] {
        let source = kokuin_sources::init::adapter(id).unwrap();
        assert!(
            kb.article(source.explain_id()).is_some(),
            "no article for {}",
            source.explain_id()
        );
    }
    for source in kokuin_sources::sources(None) {
        assert!(
            kb.article(source.explain_id()).is_some(),
            "no article for {}",
            source.explain_id()
        );
    }
}

#[test]
fn record_is_valid_json_for_the_schema() {
    let record = build_record(kira_boot(), capture(), Kb::embedded());
    let json = serde_json::to_value(&record).unwrap();
    let back: BootRecord = serde_json::from_value(json).unwrap();
    assert_eq!(back, record);
}
