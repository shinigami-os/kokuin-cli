//! Builds [`SourceReport`]s with reasons a user can act on.

use std::{io, path::Path};

use kokuin_model::{SourceReport, SourceStatus};

/// A report with no reason.
pub(crate) fn ok(id: &str, explain_id: &str) -> SourceReport {
    with_status(id, explain_id, SourceStatus::Ok, None)
}

/// A report with a status and a reason.
pub(crate) fn with_status(
    id: &str,
    explain_id: &str,
    status: SourceStatus,
    reason: Option<String>,
) -> SourceReport {
    SourceReport {
        id: id.to_owned(),
        status,
        reason,
        raw_files: Vec::new(),
        explain_id: Some(explain_id.to_owned()),
    }
}

/// A report for a source that could not be read because of `err` on `path`, a
/// system path (`/sys/...`, wherever the context reads it from).
pub(crate) fn io_error(
    id: &str,
    explain_id: &str,
    path: impl AsRef<Path>,
    err: &io::Error,
) -> SourceReport {
    let path = path.as_ref();
    let (status, reason) = match err.kind() {
        io::ErrorKind::NotFound => (
            SourceStatus::Unavailable,
            format!("{} does not exist", path.display()),
        ),
        io::ErrorKind::PermissionDenied => (
            SourceStatus::Unavailable,
            format!("reading {} needs root (permission denied)", path.display()),
        ),
        _ => (
            SourceStatus::Failed,
            format!("cannot read {}: {err}", path.display()),
        ),
    };
    with_status(id, explain_id, status, Some(reason))
}
