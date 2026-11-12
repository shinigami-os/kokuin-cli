use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{BootInfo, Event, Milestone, Process, SourceReport, StageSpan};

/// Everything kokuin knows about one boot. This is the root of the JSON format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BootRecord {
    /// Version of the format, as `MAJOR.MINOR`. A new minor version only adds optional
    /// fields and enum values; a new major version may break consumers.
    pub schema_version: String,
    /// Which boot this is, and on what system.
    pub boot: BootInfo,
    /// How this record was made.
    pub capture: CaptureInfo,
    /// What happened with each data source: what could be read, and why not.
    pub sources: Vec<SourceReport>,
    /// Time span of each boot phase, in boot order. Phases that did not happen are
    /// absent.
    pub stages: Vec<StageSpan>,
    /// Key moments of the boot, in chronological order.
    pub milestones: Vec<Milestone>,
    /// Processes that ran during the boot, sorted by start time.
    pub processes: Vec<Process>,
    /// Everything that happened, in the best-known chronological order.
    pub events: Vec<Event>,
}

impl BootRecord {
    /// Creates an empty record of the current schema version.
    pub fn new(boot: BootInfo, capture: CaptureInfo) -> Self {
        Self {
            schema_version: crate::SCHEMA_VERSION.to_owned(),
            boot,
            capture,
            sources: Vec::new(),
            stages: Vec::new(),
            milestones: Vec::new(),
            processes: Vec::new(),
            events: Vec::new(),
        }
    }
}

/// How a record was made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CaptureInfo {
    /// Name of the program that made the record (`kokuin-cli`).
    pub tool: String,
    /// Version of that program.
    pub tool_version: String,
    /// How the data was collected.
    pub method: CaptureMethod,
    /// Wall-clock time when the record was written, in RFC 3339 format.
    pub captured_at: String,
    /// Why the recording stopped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_reason: Option<EndReason>,
}

/// How the data of a record was collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMethod {
    /// Reconstructed after the boot from what the system still remembers (logs,
    /// firmware tables, running processes). Processes that already exited are missing.
    PostHoc,
    /// Recorded live during the boot by `kokuin-init`.
    Recorder,
    /// Recorded live with the extra kernel tracing of deep mode.
    RecorderDeep,
    /// A value from a newer schema version. Never produced by this version.
    #[serde(other)]
    Unknown,
}

/// Why a recording stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// The session became ready and the settle time elapsed.
    SessionReady,
    /// The maximum recording time elapsed before the session was ready.
    Timeout,
    /// The user stopped the recording.
    Manual,
    /// A post-hoc capture: the data is a snapshot taken at `captured_at`.
    Snapshot,
    /// A value from a newer schema version. Never produced by this version.
    #[serde(other)]
    Unknown,
}
