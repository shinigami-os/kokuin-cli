use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{Confidence, ProcessId, Stage};

/// Identifier of an [`Event`], unique within one record.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct EventId(pub u64);

/// What an [`Event`] describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A log message (kernel log, init system log...).
    Log,
    /// A timestamp reported by the firmware (ACPI FPDT).
    FirmwareTiming,
    /// Something the firmware or bootloader measured into the TPM before running or
    /// using it (measured boot).
    Measurement,
    /// A firmware setting or EFI variable (boot order, Secure Boot state...).
    Variable,
    /// A kernel initialization function (initcall), with its duration when known.
    Initcall,
    /// A process started (exec).
    ProcessStart,
    /// A process exited.
    ProcessExit,
    /// A service changed state (`data.state` holds the new state).
    Service,
    /// A filesystem was mounted.
    Mount,
    /// A device was detected, or a driver bound to it.
    Device,
    /// kokuin's boot recorder started or stopped watching the boot.
    Recording,
    /// A value from a newer schema version. Never produced by this version.
    #[serde(other)]
    Unknown,
}

/// Severity of a log message, as defined by syslog.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// The system is unusable.
    Emerg,
    /// Action must be taken immediately.
    Alert,
    /// Critical condition.
    Crit,
    /// Error.
    Err,
    /// Warning.
    Warning,
    /// Normal but significant condition.
    Notice,
    /// Informational.
    Info,
    /// Debug message.
    Debug,
}

/// One thing that happened during the boot.
///
/// In a record, events are sorted in the best-known chronological order, including
/// events without a time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    /// Identifier, unique within the record.
    pub id: EventId,
    /// When it happened on the boot clock, in ns. Absent when no clock was available
    /// (the position in the event list still gives the order).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t_ns: Option<i64>,
    /// How long it lasted, in ns, for events that have a duration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dur_ns: Option<u64>,
    /// How much `t_ns` and `dur_ns` can be trusted.
    pub confidence: Confidence,
    /// The boot phase this event is attributed to.
    pub stage: Stage,
    /// Identifier of the data source that produced this event: the `id` of one of the
    /// record `sources`.
    pub source: String,
    /// What this event describes.
    pub kind: EventKind,
    /// Short human-readable summary.
    pub title: String,
    /// The original message, as written by the component, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Severity, for log messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<LogLevel>,
    /// The process this event is about or comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<ProcessId>,
    /// Identifier of the explanation of this event in the knowledge base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_id: Option<String>,
    /// Source-specific details. The keys used by each source are listed in
    /// `docs/format.md`.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub data: Map<String, Value>,
}

impl Event {
    /// Creates an event with the required fields; the optional ones are empty.
    pub fn new(
        id: EventId,
        stage: Stage,
        source: impl Into<String>,
        kind: EventKind,
        title: impl Into<String>,
        t_ns: Option<i64>,
        confidence: Confidence,
    ) -> Self {
        Self {
            id,
            t_ns,
            dur_ns: None,
            confidence,
            stage,
            source: source.into(),
            kind,
            title: title.into(),
            message: None,
            level: None,
            process: None,
            explain_id: None,
            data: Map::new(),
        }
    }
}
