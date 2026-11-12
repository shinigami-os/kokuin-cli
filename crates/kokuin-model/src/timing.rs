use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How much a time (or a time span) can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Read from a clock by the component itself (kernel log timestamp, process event,
    /// firmware performance record).
    Exact,
    /// Derived from a clock, with a known imprecision: for example a firmware time
    /// converted to the boot clock, or a process start time with a resolution of one
    /// clock tick.
    Approximate,
    /// No clock at all: the position was deduced from the order of events (for example
    /// a TPM measurement placed between two timed events).
    Inferred,
}
