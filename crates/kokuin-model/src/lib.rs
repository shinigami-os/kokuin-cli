//! Data model of a kokuin boot record.
//!
//! A [`BootRecord`] is everything kokuin knows about one boot, from power-on to a
//! usable desktop. It is serialized as JSON and is the contract between
//! `kokuin-cli` (which produces it) and its consumers (the `kokuin` GUI, scripts).
//!
//! Conventions shared by every type of this crate:
//! - Times are integers in nanoseconds on the **boot clock**: `0` is the start of the
//!   Linux kernel, negative values are before the kernel (firmware, bootloader).
//!   Fields holding such a time end with `_ns`.
//! - An optional field that is absent from the JSON means "unknown" or "not
//!   applicable", never zero.
//! - Enumerations are `snake_case` strings. Consumers must accept values they do not
//!   know (they deserialize as `unknown` here): new values can appear in a minor
//!   version of the schema.
//!
//! See `docs/format.md` for the full description of the format.

mod diff;
mod event;
mod milestone;
mod process;
mod record;
mod source;
mod stage;
mod system;
mod timing;

pub use diff::{BootDiff, DiffSide, MilestoneChange, StageChange, StepChange};
pub use event::{Event, EventId, EventKind, LogLevel};
pub use milestone::{Milestone, MilestoneKind};
pub use process::{Process, ProcessId};
pub use record::{BootRecord, CaptureInfo, CaptureMethod, EndReason};
pub use source::{SourceReport, SourceStatus};
pub use stage::{Stage, StageSpan};
pub use system::{
    BootInfo, ComponentInfo, DmiInfo, FirmwareKind, KernelInfo, MachineInfo, OsInfo, UefiInfo,
};
pub use timing::Confidence;

/// Version of the JSON format, as `MAJOR.MINOR`.
///
/// A new minor version only adds things (optional fields, enum values), so a consumer
/// written for `X.Y` can read any `X.Z`. A new major version may break consumers.
pub const SCHEMA_VERSION: &str = "0.4";

/// Returns the JSON Schema describing a serialized [`BootRecord`].
pub fn json_schema() -> schemars::Schema {
    schemars::schema_for!(BootRecord)
}

/// Returns the JSON Schema describing a serialized [`BootDiff`].
pub fn diff_json_schema() -> schemars::Schema {
    schemars::schema_for!(BootDiff)
}
