//! Where the record a command works on comes from.

use std::{io::Read, path::Path};

use kokuin_kb::Kb;
use kokuin_model::{
    BootRecord, CaptureInfo, CaptureMethod, EndReason, Event, FirmwareKind, MilestoneKind,
    SourceStatus, Stage,
};
use kokuin_sources::{Context, format_wall_clock, proc_events};

use crate::{Failure, boots};

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
        let cmdline = record.boot.kernel.cmdline.as_deref().unwrap_or_default();
        record.capture.method = if kokuin_bootcfg::is_deep_boot(cmdline) {
            CaptureMethod::RecorderDeep
        } else {
            CaptureMethod::Recorder
        };
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

/// The record a command works on. `boot` is, in order:
///
/// - `-`: a record on the standard input;
/// - a file: a record;
/// - a folder: a recorded boot's folder, or a snapshot saved by `capture --raw`
///   (the record is rebuilt from it);
/// - otherwise a recorded boot: `last`, `-2`, or the start of its id.
///
/// Without `boot`: the current boot's recording if it was recorded, else a record
/// rebuilt now from what the system still remembers.
pub fn load(boot: Option<&str>) -> Result<BootRecord, Failure> {
    let ctx = match boot {
        None => match boots::current() {
            Some(recorded) => return Ok(recorded.record),
            None => Context::live(),
        },
        Some("-") => {
            let mut text = String::new();
            std::io::stdin().read_to_string(&mut text)?;
            return parse(&text, "the standard input");
        }
        Some(path) if Path::new(path).is_file() => return read_file(Path::new(path)),
        Some(path) if Path::new(path).join(boots::RECORD_FILE).is_file() => {
            return boots::load(Path::new(path)).map(|b| b.record);
        }
        Some(path) if Path::new(path).is_dir() => Context::from_snapshot(path)?,
        Some(selector) => return boots::find(selector).map(|b| b.record),
    };
    let record = build(&ctx);
    warn_about_missing_sources(&record);
    Ok(record)
}

/// Reads a record file.
pub fn read_file(path: &Path) -> Result<BootRecord, Failure> {
    let text = std::fs::read_to_string(path)
        .map_err(|err| Failure(format!("cannot read {}: {err}", path.display())))?;
    parse(&text, &path.display().to_string())
}

/// Parses a record, refusing a format version this kokuin-cli does not read.
fn parse(text: &str, origin: &str) -> Result<BootRecord, Failure> {
    let record: BootRecord = serde_json::from_str(text)
        .map_err(|err| Failure(format!("{origin} is not a kokuin boot record: {err}")))?;
    let major = |version: &str| version.split('.').next().map(str::to_owned);
    if major(&record.schema_version) != major(kokuin_model::SCHEMA_VERSION) {
        return Err(Failure(format!(
            "{origin} uses format version {}, this kokuin-cli reads version {}",
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

/// How the record was made, in words.
pub fn capture_wording(record: &BootRecord) -> String {
    let capture = match record.capture.method {
        CaptureMethod::PostHoc => "rebuilt after boot: processes that already exited are missing",
        CaptureMethod::Recorder => "recorded live during boot",
        CaptureMethod::RecorderDeep => {
            "recorded live during boot, with kernel tracing (which makes it a little slower)"
        }
        CaptureMethod::Unknown => "unknown method",
    };
    let end = match record.capture.end_reason {
        Some(EndReason::Timeout) => " (stopped before the desktop was ready)",
        _ => "",
    };
    format!("{capture}{end}")
}

/// The components that took part in the boot, in order.
pub fn boot_chain(record: &BootRecord) -> Vec<String> {
    let boot = &record.boot;
    let mut chain = vec![match boot.machine.firmware {
        FirmwareKind::Uefi => "UEFI".to_owned(),
        FirmwareKind::Bios => "BIOS".to_owned(),
        FirmwareKind::Unknown => "firmware".to_owned(),
    }];
    chain.extend(
        boot.bootloader
            .as_ref()
            .map(|b| b.name.clone().unwrap_or(b.id.clone())),
    );
    chain.push(format!("Linux {}", boot.kernel.release));
    if record.stages.iter().any(|s| s.stage == Stage::Initramfs) {
        chain.push("initramfs".into());
    }
    chain.extend(
        boot.init
            .as_ref()
            .map(|i| i.name.clone().unwrap_or(i.id.clone())),
    );
    for kind in [
        MilestoneKind::DisplayManagerStart,
        MilestoneKind::CompositorStart,
    ] {
        let process = milestone_event(record, kind)
            .and_then(|event| event.process)
            .and_then(|id| record.processes.iter().find(|p| p.id == id));
        chain.extend(process.map(|p| p.comm.clone()));
    }
    chain
}

/// The event marking a milestone, if there is one.
pub fn milestone_event(record: &BootRecord, kind: MilestoneKind) -> Option<&Event> {
    let id = record.milestones.iter().find(|m| m.kind == kind)?.event?;
    record.events.iter().find(|e| e.id == id)
}
