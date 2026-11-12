use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Whether a data source could be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    /// Fully read.
    Ok,
    /// Read, but some data is missing (`reason` says what and why).
    Partial,
    /// Not present on this system (`reason` says why: no TPM, BIOS boot, missing
    /// kernel option, not running as root...).
    Unavailable,
    /// Present but reading it failed (`reason` holds the error).
    Failed,
    /// A value from a newer schema version. Never produced by this version.
    #[serde(other)]
    Unknown,
}

/// What happened with one data source during the capture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceReport {
    /// Identifier of the source (for example `fpdt`, `tpm_eventlog`, `kmsg`). Events
    /// reference it in their `source` field. The known identifiers are listed in
    /// `docs/format.md`.
    pub id: String,
    /// Whether the source could be read.
    pub status: SourceStatus,
    /// Human-readable explanation when `status` is not `ok`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Raw files saved from this source, relative to the record directory. They keep
    /// the original data so a record can be analyzed again with better rules.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub raw_files: Vec<String>,
    /// Identifier of the explanation of this source in the knowledge base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_id: Option<String>,
}
