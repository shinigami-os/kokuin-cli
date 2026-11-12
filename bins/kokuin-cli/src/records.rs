//! Where the record a command works on comes from.

use std::{io::Read, path::Path};

use kokuin_kb::Kb;
use kokuin_model::{
    BootRecord, CaptureInfo, CaptureMethod, EndReason, MilestoneKind, SourceStatus,
};
use kokuin_sources::{Context, format_wall_clock, proc_events};

use crate::Failure;

/// Builds a record of the boot `ctx` describes: from what the boot recorder saw if it
/// ran, else from what the system still remembers.
pub fn build(ctx: &Context) -> BootRecord {
    let collected = kokuin_analyze::collect(ctx);
    let capture = CaptureInfo {
        tool: env!("CARGO_PKG_NAME").to_owned(),
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        method: CaptureMethod::PostHoc,
        captured_at: format_wall_clock(ctx.now_unix_ns().unwrap_or_default()),
        end_reason: Some(EndReason::Snapshot),
    };
    let mut record = kokuin_analyze::build_record(collected, capture, Kb::embedded());
    if let Some(reason) = recording_end(&record) {
        record.capture.method = CaptureMethod::Recorder;
        record.capture.end_reason = Some(reason);
    }
    record
}

/// Why the boot recorder stopped, when the boot was recorded.
fn recording_end(record: &BootRecord) -> Option<EndReason> {
    let event = record
        .milestones
        .iter()
        .find(|m| m.kind == MilestoneKind::RecordingEnd)?
        .event?;
    let event = record.events.iter().find(|e| e.id == event)?;
    proc_events::end_reason(event)
}

/// Reads a record file (`-` for the standard input), rebuilds the record of a
/// snapshot directory, or captures the current boot when no path is given.
pub fn load(path: Option<&Path>) -> Result<BootRecord, Failure> {
    let ctx = match path {
        None => Context::live(),
        Some(dir) if dir.is_dir() => Context::from_snapshot(dir)?,
        Some(file) => return read_file(file),
    };
    let record = build(&ctx);
    warn_about_missing_sources(&record);
    Ok(record)
}

fn read_file(path: &Path) -> Result<BootRecord, Failure> {
    let text = if path == Path::new("-") {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        text
    } else {
        std::fs::read_to_string(path)
            .map_err(|err| Failure(format!("cannot read {}: {err}", path.display())))?
    };
    let record: BootRecord = serde_json::from_str(&text).map_err(|err| {
        Failure(format!(
            "{} is not a kokuin boot record: {err}",
            path.display()
        ))
    })?;
    let major = |version: &str| version.split('.').next().map(str::to_owned);
    if major(&record.schema_version) != major(kokuin_model::SCHEMA_VERSION) {
        return Err(Failure(format!(
            "{} uses format version {}, this kokuin-cli reads version {}",
            path.display(),
            record.schema_version,
            kokuin_model::SCHEMA_VERSION
        )));
    }
    Ok(record)
}

/// Tells on the error output when sources could not be read, so a partial result is
/// not mistaken for a complete one.
pub fn warn_about_missing_sources(record: &BootRecord) {
    let missing = record
        .sources
        .iter()
        .filter(|s| matches!(s.status, SourceStatus::Unavailable | SourceStatus::Failed))
        .count();
    if missing > 0 {
        let style = crate::output::style::WARNING;
        anstream::eprintln!(
            "{}note:{} {missing} data source(s) could not be read, run `kokuin-cli status` to see why",
            style.render(),
            style.render_reset()
        );
    }
}
