use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Confidence, Stage};

/// Identifier of a [`Process`], unique within one record.
///
/// Different from the PID: the kernel reuses PIDs, so one PID can belong to several
/// processes during a boot.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct ProcessId(pub u64);

/// A process that ran during the boot, from its creation (`fork`) to its exit.
///
/// A process can replace its program (`exec`) several times during its life:
/// `comm`, `cmdline` and `exe` describe the last program it ran, and each `exec` is
/// also an event of kind `process_start`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Process {
    /// Identifier, unique within the record.
    pub id: ProcessId,
    /// PID given by the kernel.
    pub pid: u32,
    /// The process that started this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ProcessId>,
    /// PID of the parent, also given when the parent itself is not in the record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ppid: Option<u32>,
    /// Short command name of the last program, as the kernel knows it (at most 15
    /// characters).
    pub comm: String,
    /// Full command line of the last program. Empty when unknown, and for kernel threads.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cmdline: Vec<String>,
    /// Path of the executable of the last program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    /// When the process was created on the boot clock, in ns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ns: Option<i64>,
    /// When the process exited on the boot clock, in ns. Absent if it was still
    /// running at the end of the capture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_ns: Option<i64>,
    /// Exit code, if the process exited normally.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Number of the signal that killed the process, if a signal killed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_signal: Option<i32>,
    /// Name of the service this process belongs to, when the init system tells it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// The boot phase this process is attributed to.
    pub stage: Stage,
    /// How much `start_ns` and `end_ns` can be trusted.
    pub confidence: Confidence,
    /// Identifier of the explanation of this program in the knowledge base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_id: Option<String>,
}
