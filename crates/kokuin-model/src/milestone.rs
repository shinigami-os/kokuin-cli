use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Confidence, EventId};

/// A key moment of the boot, in boot order. Each kind appears at most once in a
/// record.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum MilestoneKind {
    /// The firmware starts running, right after the CPU leaves reset (FPDT `ResetEnd`).
    FirmwareStart,
    /// The firmware starts loading the bootloader image (FPDT `OsLoaderLoadImageStart`).
    BootloaderLoad,
    /// The firmware starts executing the bootloader (FPDT `OsLoaderStartImageStart`).
    BootloaderStart,
    /// The firmware hands the machine over to the OS (end of UEFI `ExitBootServices`).
    ExitBootServices,
    /// The Linux kernel starts. Always at `0` on the boot clock.
    KernelStart,
    /// The kernel runs the initramfs `/init`.
    InitramfsStart,
    /// The real root filesystem is mounted.
    RootMounted,
    /// The init system of the real root filesystem starts as PID 1.
    InitStart,
    /// The init system starts launching services.
    ServicesStart,
    /// A graphics driver is ready to display something (DRM/KMS driver initialized).
    GraphicsReady,
    /// The display manager (login screen) starts.
    DisplayManagerStart,
    /// The user session starts (login done, or automatic login).
    SessionStart,
    /// The compositor (or X server) of the user session starts.
    CompositorStart,
    /// The desktop is usable: the session is up and the system has settled.
    SessionReady,
    /// The recording stopped.
    RecordingEnd,
    /// A value from a newer schema version. Never produced by this version.
    #[serde(other)]
    Unknown,
}

/// A key moment of the boot, placed on the boot clock.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Milestone {
    /// Which key moment this is.
    pub kind: MilestoneKind,
    /// When it happened on the boot clock, in ns.
    pub t_ns: i64,
    /// How much `t_ns` can be trusted.
    pub confidence: Confidence,
    /// The event this milestone was detected from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<EventId>,
    /// Identifier of the explanation of this milestone in the knowledge base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_id: Option<String>,
}
