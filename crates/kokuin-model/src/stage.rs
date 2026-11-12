use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Confidence;

/// A phase of the boot, in the order in which phases start.
///
/// Phases can overlap: services keep starting while the graphical session comes up,
/// and the kernel keeps logging after it started the init system.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// From power-on to the firmware starting the bootloader (UEFI or BIOS).
    Firmware,
    /// The bootloader (GRUB, systemd-boot...) choosing and loading the kernel.
    Bootloader,
    /// The kernel initializing the hardware and itself, until it runs the first process.
    Kernel,
    /// The early userspace from the initramfs, until it switches to the real root
    /// filesystem. Absent when the system boots without an initramfs.
    Initramfs,
    /// The init system (PID 1) setting up the base system: mounts, devices, hostname...
    Init,
    /// Services being started by the init system or its service supervisor.
    Services,
    /// Display stack coming up: graphics driver, display manager or compositor.
    Graphical,
    /// The user session starting, until the desktop is ready and the system settled.
    Session,
    /// Everything after the session is ready.
    Runtime,
    /// A value from a newer schema version. Never produced by this version.
    #[serde(other)]
    Unknown,
}

impl Stage {
    /// Every stage produced by this version, in boot order.
    pub const ALL: [Stage; 9] = [
        Stage::Firmware,
        Stage::Bootloader,
        Stage::Kernel,
        Stage::Initramfs,
        Stage::Init,
        Stage::Services,
        Stage::Graphical,
        Stage::Session,
        Stage::Runtime,
    ];
}

/// The time span of one [`Stage`] during this boot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StageSpan {
    /// The stage this span describes.
    pub stage: Stage,
    /// Start of the stage on the boot clock, in ns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ns: Option<i64>,
    /// End of the stage on the boot clock, in ns. Absent if the stage did not end
    /// before the capture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ns: Option<i64>,
    /// How much `start_ns` and `end_ns` can be trusted.
    pub confidence: Confidence,
    /// Identifier of the explanation of this stage in the knowledge base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_in_boot_order() {
        assert!(Stage::ALL.is_sorted());
    }

    #[test]
    fn unknown_value_deserializes_as_unknown() {
        let stage: Stage = serde_json::from_str("\"stage_from_the_future\"").unwrap();
        assert_eq!(stage, Stage::Unknown);
    }
}
