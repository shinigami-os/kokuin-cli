//! Where the record a command works on comes from.

use std::{io::Read, path::Path, time::SystemTime};

use kokuin_kb::Kb;
use kokuin_model::{BootRecord, CaptureInfo, CaptureMethod, EndReason, SourceStatus};
use kokuin_sources::{Context, format_wall_clock};

use crate::Failure;

/// Rebuilds a record of the current boot from what the system still remembers.
pub fn capture_current_boot() -> BootRecord {
    let ctx = Context::live();
    let collected = kokuin_analyze::collect(&ctx);
    let now_ns = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX));
    let capture = CaptureInfo {
        tool: env!("CARGO_PKG_NAME").to_owned(),
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        method: CaptureMethod::PostHoc,
        captured_at: format_wall_clock(now_ns),
        end_reason: Some(EndReason::Snapshot),
    };
    kokuin_analyze::build_record(collected, capture, Kb::embedded())
}

/// Reads a record file (`-` for the standard input), or captures the current boot
/// when no file is given.
pub fn load(path: Option<&Path>) -> Result<BootRecord, Failure> {
    let Some(path) = path else {
        let record = capture_current_boot();
        warn_about_missing_sources(&record);
        return Ok(record);
    };
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
