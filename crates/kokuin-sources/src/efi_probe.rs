//! What kokuin's EFI probe saw before the bootloader (`kokuin-cli probe arm`).
//!
//! The probe leaves a report in an EFI variable: the CPU's time counter (TSC) when
//! it started and when it handed over to the boot entry, and what the firmware set
//! up. The counter counts from power-on; read again here together with the boot
//! clock ([`Context::tsc_sample`]), it places power-on, the probe and the handover on
//! the boot clock: the firmware and bootloader stages, without FPDT.

use kokuin_model::{Confidence, EventKind, LogLevel, MilestoneKind, SourceStatus, Stage};
use kokuin_probe_format::{GUID, Report, VARIABLE};

use crate::{Context, Observation, Source, SourceOutput, report, snapshot::TscSample};

const EXPLAIN: &str = "firmware.kokuin_probe";

/// What kokuin's EFI probe saw.
pub struct EfiProbe;

impl Source for EfiProbe {
    fn id(&self) -> &'static str {
        "efi_probe"
    }

    fn explain_id(&self) -> &'static str {
        EXPLAIN
    }

    fn kernel_options(&self) -> &'static [&'static str] {
        &["CONFIG_EFIVAR_FS"]
    }

    fn collect(&self, ctx: &Context) -> SourceOutput {
        let path = format!("/sys/firmware/efi/efivars/{VARIABLE}-{GUID}");
        let Ok(bytes) = ctx.read_bytes(&path) else {
            return SourceOutput::empty(report::with_status(
                self.id(),
                EXPLAIN,
                SourceStatus::Unavailable,
                Some(
                    "this boot did not go through kokuin's EFI probe (`kokuin-cli probe arm` \
                     before rebooting)"
                        .into(),
                ),
            ));
        };
        // efivarfs files start with the variable's 4 attribute bytes.
        let text = String::from_utf8_lossy(bytes.get(4..).unwrap_or_default());
        let probe = match Report::decode(&text) {
            Ok(probe) => probe,
            Err(err) => {
                return SourceOutput::empty(report::with_status(
                    self.id(),
                    EXPLAIN,
                    SourceStatus::Failed,
                    Some(format!("the probe's report could not be read ({err:?})")),
                ));
            }
        };

        let sample = ctx.tsc_sample().map(|sample| TscSample {
            // Linux's measure of the frequency is the better one; the probe's own
            // is the fallback.
            hz: if sample.hz > 0 {
                sample.hz
            } else {
                probe.tsc_hz.unwrap_or_default()
            },
            ..sample
        });
        let steady = ctx.read_text("/proc/cpuinfo").is_ok_and(|cpuinfo| {
            let flags = |flag| cpuinfo.split_whitespace().any(|word| word == flag);
            flags("constant_tsc") && flags("nonstop_tsc")
        });
        let mut problems = Vec::new();
        if sample.is_none() {
            problems.push("the CPU's time counter could not be read: no times".to_owned());
        } else if !steady {
            problems.push(
                "the CPU's time counter may not tick at a steady rate (no constant_tsc and \
                 nonstop_tsc): times are approximate"
                    .to_owned(),
            );
        }
        if let Some(error) = &probe.handover_error {
            problems.push(format!("the probe could not start the boot entry: {error}"));
        }
        let report = if problems.is_empty() {
            report::ok(self.id(), EXPLAIN)
        } else {
            report::with_status(
                self.id(),
                EXPLAIN,
                SourceStatus::Partial,
                Some(problems.join("; ")),
            )
        };
        let confidence = if steady {
            Confidence::Exact
        } else {
            Confidence::Approximate
        };
        SourceOutput {
            report,
            observations: observations(&probe, sample, confidence),
            processes: Vec::new(),
        }
    }
}

/// The probe's report on the timeline.
fn observations(
    probe: &Report,
    sample: Option<TscSample>,
    confidence: Confidence,
) -> Vec<Observation> {
    let at = |tsc: u64| sample.map(|s| s.boot_ns(tsc));
    let (time_confidence, start) = match at(probe.tsc_start) {
        Some(t) => (confidence, Some(t)),
        None => (Confidence::Inferred, None),
    };
    let mut observations = vec![
        Observation::new(
            EventKind::FirmwareTiming,
            "Power on: the CPU's time counter starts",
            at(0),
            // The counter starts at reset on real machines, at the start of a virtual
            // one: the best guess of power-on, not a measure.
            match time_confidence {
                Confidence::Inferred => Confidence::Inferred,
                _ => Confidence::Approximate,
            },
        )
        .in_stage(Stage::Firmware)
        .explained_by("firmware.reset")
        .marks(MilestoneKind::FirmwareStart),
    ];

    let mut started = Observation::new(
        EventKind::FirmwareTiming,
        format!(
            "The firmware started its first boot program: kokuin's probe ({})",
            probe
                .probe_path
                .as_deref()
                .unwrap_or("\\EFI\\kokuin\\probe.efi")
        ),
        start,
        time_confidence,
    )
    .in_stage(Stage::Bootloader)
    .explained_by(EXPLAIN)
    .marks(MilestoneKind::BootloaderLoad)
    .with("tsc", probe.tsc_start);
    for (key, value) in [
        ("firmware_vendor", probe.firmware_vendor.clone()),
        ("uefi_revision", probe.uefi_revision.clone()),
        ("console", probe.console.clone()),
    ] {
        if let Some(value) = value {
            started = started.with(key, value);
        }
    }
    observations.push(started);

    if let (Some(total), Some(usable)) = (probe.memory_total, probe.memory_usable) {
        observations.push(
            Observation::new(
                EventKind::Variable,
                format!(
                    "The firmware describes {} of memory, {} of it for the system",
                    gibibytes(total),
                    gibibytes(usable)
                ),
                start,
                time_confidence,
            )
            .in_stage(Stage::Firmware)
            .explained_by("kernel.memory_map")
            .with("memory_total", total)
            .with("memory_usable", usable),
        );
    }
    if let Some(count) = probe.loaded_images {
        observations.push(
            Observation::new(
                EventKind::Variable,
                format!("The firmware had loaded {count} programs and drivers"),
                start,
                time_confidence,
            )
            .in_stage(Stage::Firmware)
            .explained_by("firmware.uefi")
            .with("loaded_images", count),
        );
    }

    let entry = probe.handover_entry.map_or_else(
        || "the boot entry".to_owned(),
        |entry| match &probe.handover_description {
            Some(description) => format!("Boot{entry:04X} ({description})"),
            None => format!("Boot{entry:04X}"),
        },
    );
    match &probe.handover_error {
        None => observations.push(
            Observation::new(
                EventKind::FirmwareTiming,
                format!("kokuin's probe started {entry}"),
                probe.tsc_handover.and_then(at),
                time_confidence,
            )
            .in_stage(Stage::Bootloader)
            .explained_by("firmware.boot_entries")
            .marks(MilestoneKind::BootloaderStart),
        ),
        Some(error) => {
            let mut failed = Observation::new(
                EventKind::FirmwareTiming,
                format!(
                    "kokuin's probe could not start {entry} ({error}): the firmware went on \
                     with its boot order"
                ),
                start,
                time_confidence,
            )
            .in_stage(Stage::Bootloader)
            .explained_by(EXPLAIN);
            failed.level = Some(LogLevel::Warning);
            observations.push(failed);
        }
    }
    observations
}

fn gibibytes(bytes: u64) -> String {
    format!("{:.2} GiB", bytes as f64 / f64::from(1u32 << 30))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe() -> Report {
        Report::decode(
            "format=1\ntsc_start=10275561720\ntsc_handover=10363255011\ntsc_hz=2700046500\n\
             firmware_vendor=EDK II\nmemory_total=2419736576\nmemory_usable=2140942336\n\
             handover_entry=0002\nhandover_description=UEFI Misc Device\nloaded_images=115\n",
        )
        .unwrap()
    }

    /// Linux read the counter 20 s after the kernel started; power-on was 5 s before
    /// the kernel.
    fn sample() -> TscSample {
        let hz = 2_700_000_000u64;
        TscSample {
            tsc: 25 * hz,
            mono_ns: 20_000_000_000,
            hz,
        }
    }

    #[test]
    fn places_power_on_the_probe_and_the_handover_before_the_kernel() {
        let found = observations(&probe(), Some(sample()), Confidence::Exact);
        let milestone = |kind| found.iter().find(|o| o.milestone == Some(kind)).unwrap();
        assert_eq!(
            milestone(MilestoneKind::FirmwareStart).t_ns,
            Some(-5_000_000_000)
        );
        let probe_start = milestone(MilestoneKind::BootloaderLoad).t_ns.unwrap();
        let handover = milestone(MilestoneKind::BootloaderStart).t_ns.unwrap();
        // 10275561720 ticks at 2.7 GHz: 3.806 s after power-on, so 1.194 s before
        // the kernel.
        assert_eq!(probe_start / 1_000_000, -1_194);
        assert!(handover > probe_start && handover < 0);
        assert!(found.iter().any(|o| o.title.contains("115 programs")));
        assert!(
            found
                .iter()
                .any(|o| o.title.contains("Boot0002 (UEFI Misc Device)"))
        );
    }

    #[test]
    fn a_failed_handover_is_told() {
        let mut failed = probe();
        failed.handover_error = Some("the firmware could not load it: NOT_FOUND".into());
        let found = observations(&failed, Some(sample()), Confidence::Exact);
        assert!(
            found
                .iter()
                .all(|o| o.milestone != Some(MilestoneKind::BootloaderStart))
        );
        assert!(
            found
                .iter()
                .any(|o| o.title.contains("went on with its boot order"))
        );
    }

    #[test]
    fn without_the_counter_events_have_no_time() {
        let found = observations(&probe(), None, Confidence::Exact);
        assert!(found.iter().all(|o| o.t_ns.is_none()));
    }

    #[test]
    fn a_boot_without_the_probe_says_how_to_get_it() {
        let output = EfiProbe.collect(&Context::with_root("/nonexistent"));
        assert_eq!(output.report.status, SourceStatus::Unavailable);
        assert!(output.report.reason.unwrap().contains("probe arm"));
    }
}
