use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{CaptureMethod, MilestoneKind, Stage};

/// The comparison of two boots, A (before) and B (after): `kokuin-cli diff A B`.
///
/// Every change is "B minus A": a positive `delta_ns` means later, or longer, in B.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BootDiff {
    /// Version of the format, as for [`crate::BootRecord`].
    pub schema_version: String,
    /// The first boot.
    pub a: DiffSide,
    /// The second boot.
    pub b: DiffSide,
    /// Each milestone of either boot, in boot order.
    pub milestones: Vec<MilestoneChange>,
    /// Each stage of either boot, in boot order, with its duration.
    pub stages: Vec<StageChange>,
    /// Timed steps (services, kernel init functions...) whose duration changed by
    /// at least 10 ms, or that took at least 10 ms in only one of the boots. Sorted
    /// by the size of the change, largest first.
    pub steps: Vec<StepChange>,
    /// Programs run in B but not in A, sorted.
    pub new_programs: Vec<String>,
    /// Programs run in A but not in B, sorted.
    pub gone_programs: Vec<String>,
}

/// Which boot one side of a comparison is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DiffSide {
    /// The kernel's id of the boot.
    pub boot_id: String,
    /// When it started, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// The kernel release.
    pub kernel_release: String,
    /// How it was recorded.
    pub method: CaptureMethod,
}

/// A milestone in both boots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MilestoneChange {
    /// Which milestone.
    pub kind: MilestoneKind,
    /// When it happened in A, if it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a_ns: Option<i64>,
    /// When it happened in B, if it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_ns: Option<i64>,
    /// `b_ns - a_ns`, when both are known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_ns: Option<i64>,
}

/// A stage in both boots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StageChange {
    /// Which stage.
    pub stage: Stage,
    /// Its duration in A, if measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a_ns: Option<i64>,
    /// Its duration in B, if measured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_ns: Option<i64>,
    /// `b_ns - a_ns`, when both are known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta_ns: Option<i64>,
}

/// A timed step, matched between the two boots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StepChange {
    /// What matches the step between boots: `service:<name>`, `initcall:<function>`,
    /// or `<source>:<title>`.
    pub key: String,
    /// A title for it (from B if it is there, else from A).
    pub title: String,
    /// Its duration in A, if it ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a_ns: Option<i64>,
    /// Its duration in B, if it ran.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_ns: Option<i64>,
    /// `b_ns - a_ns`, counting a missing side as 0.
    pub delta_ns: i64,
}
