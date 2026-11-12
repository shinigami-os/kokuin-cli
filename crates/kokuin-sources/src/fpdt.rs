//! ACPI Firmware Performance Data Table (`/sys/firmware/acpi/fpdt/boot/`).

use kokuin_model::{Confidence, EventKind, MilestoneKind, SourceStatus, Stage};

use crate::{Context, Observation, Source, SourceOutput, report};

const DIR: &str = "/sys/firmware/acpi/fpdt/boot";
const EXPLAIN: &str = "firmware.fpdt";

/// The FPDT boot record.
pub struct Fpdt;

/// One timestamp of the FPDT boot record.
struct Field {
    file: &'static str,
    title: &'static str,
    stage: Stage,
    milestone: Option<MilestoneKind>,
    explain: &'static str,
}

/// In boot order. The kernel exposes each one as a file holding ns since CPU reset.
const FIELDS: [Field; 5] = [
    Field {
        file: "firmware_start_ns",
        title: "Firmware started (the CPU left reset)",
        stage: Stage::Firmware,
        milestone: Some(MilestoneKind::FirmwareStart),
        explain: "firmware.reset",
    },
    Field {
        file: "bootloader_load_ns",
        title: "Firmware started loading the bootloader",
        stage: Stage::Bootloader,
        milestone: Some(MilestoneKind::BootloaderLoad),
        explain: EXPLAIN,
    },
    Field {
        file: "bootloader_launch_ns",
        title: "Firmware started the bootloader",
        stage: Stage::Bootloader,
        milestone: Some(MilestoneKind::BootloaderStart),
        explain: EXPLAIN,
    },
    Field {
        file: "exitbootservice_start_ns",
        title: "Bootloader asked the firmware to hand over (ExitBootServices)",
        stage: Stage::Bootloader,
        milestone: None,
        explain: "firmware.exit_boot_services",
    },
    Field {
        file: "exitbootservice_end_ns",
        title: "Firmware handed the machine over to the OS",
        stage: Stage::Bootloader,
        milestone: Some(MilestoneKind::ExitBootServices),
        explain: "firmware.exit_boot_services",
    },
];

impl Source for Fpdt {
    fn id(&self) -> &'static str {
        "fpdt"
    }

    fn explain_id(&self) -> &'static str {
        EXPLAIN
    }

    fn kernel_options(&self) -> &'static [&'static str] {
        &["CONFIG_ACPI_FPDT"]
    }

    fn collect(&self, ctx: &Context) -> SourceOutput {
        let mut values = Vec::new();
        for field in &FIELDS {
            let path = format!("{DIR}/{}", field.file);
            match ctx.read_text(&path) {
                Ok(text) => values.push(text.trim().parse::<u64>().unwrap_or(0)),
                Err(err) => {
                    let mut report = report::io_error(self.id(), EXPLAIN, &ctx.path(&path), &err);
                    if !ctx.exists(DIR) {
                        report.reason = Some(
                            "the firmware has no FPDT table, or the kernel does not expose it \
                             (needs CONFIG_ACPI_FPDT)"
                                .into(),
                        );
                    }
                    return SourceOutput::empty(report);
                }
            }
        }
        let observations = observations(&values);
        let report = if observations.len() == FIELDS.len() {
            report::ok(self.id(), EXPLAIN)
        } else {
            report::with_status(
                self.id(),
                EXPLAIN,
                SourceStatus::Partial,
                Some("the firmware did not record every timestamp (they are 0)".into()),
            )
        };
        SourceOutput {
            report,
            observations,
            processes: Vec::new(),
        }
    }
}

/// Builds the observations from the raw values, in the order of [`FIELDS`].
///
/// The boot clock origin is the end of ExitBootServices (or its start when the end
/// is missing): `boot_ns = firmware_ns - anchor`.
fn observations(values: &[u64]) -> Vec<Observation> {
    let anchor = [values[4], values[3]].into_iter().find(|&v| v != 0);
    FIELDS
        .iter()
        .zip(values)
        .filter(|&(_, &value)| value != 0)
        .map(|(field, &value)| {
            let t_ns = anchor.map(|anchor| value as i64 - anchor as i64);
            let confidence = if t_ns.is_some() {
                Confidence::Approximate
            } else {
                Confidence::Inferred
            };
            let mut observation =
                Observation::new(EventKind::FirmwareTiming, field.title, t_ns, confidence)
                    .in_stage(field.stage)
                    .explained_by(field.explain)
                    .with("field", field.file)
                    .with("firmware_ns", value);
            observation.milestone = field.milestone;
            observation
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_times_relative_to_exit_boot_services() {
        let found = observations(&[
            350_000_000,
            5_100_000_000,
            5_150_000_000,
            7_400_000_000,
            7_420_000_000,
        ]);
        let times: Vec<_> = found.iter().map(|o| o.t_ns.unwrap()).collect();
        assert_eq!(
            times,
            [
                -7_070_000_000,
                -2_320_000_000,
                -2_270_000_000,
                -20_000_000,
                0
            ]
        );
        assert_eq!(found[0].milestone, Some(MilestoneKind::FirmwareStart));
    }

    #[test]
    fn skips_unrecorded_values_and_keeps_order_without_anchor() {
        let found = observations(&[350_000_000, 0, 0, 0, 0]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].t_ns, None);
        assert_eq!(found[0].confidence, Confidence::Inferred);
    }
}
