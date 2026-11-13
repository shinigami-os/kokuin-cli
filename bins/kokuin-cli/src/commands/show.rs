//! `show`: a summary of one boot.

use anstream::println;
use kokuin_model::{
    BootRecord, CaptureMethod, EndReason, Event, EventKind, FirmwareKind, MilestoneKind as M,
    SourceStatus, StageSpan,
};

use crate::{
    Failure,
    output::{self, CONFIDENCE_LEGEND, confidence_mark, duration, paint, style, time},
    records,
};

const BAR_WIDTH: usize = 40;
const SLOWEST: usize = 10;

pub fn run(boot: Option<String>, json: bool) -> Result<(), Failure> {
    let record = records::load(boot.as_deref())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&record)?);
        return Ok(());
    }
    header(&record);
    stages(&record);
    milestones(&record);
    slowest(&record);
    sources(&record);
    println!();
    println!("{}", paint(style::DIM, CONFIDENCE_LEGEND));
    println!(
        "{}",
        paint(
            style::DIM,
            "Details: `kokuin-cli timeline`. Explanations: `kokuin-cli explain <id>`."
        )
    );
    Ok(())
}

fn header(record: &BootRecord) {
    let boot = &record.boot;
    let when = boot.started_at.as_deref().unwrap_or(&boot.boot_id);
    println!("{}", paint(style::TITLE, format!("Boot of {when}")));
    println!();

    let os = boot
        .os
        .as_ref()
        .and_then(|os| os.pretty_name.clone().or(os.name.clone()));
    let mut system = vec![
        os.unwrap_or_else(|| "unknown distribution".into()),
        format!("Linux {}", boot.kernel.release),
    ];
    system.extend(boot.hostname.clone());
    println!("  System      {}", system.join(" · "));

    let machine = &boot.machine;
    let mut hardware = Vec::new();
    if let Some(dmi) = &machine.dmi {
        let model = [dmi.sys_vendor.as_deref(), dmi.product_name.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if !model.is_empty() {
            hardware.push(model.join(" "));
        }
    }
    let firmware = match (machine.firmware, &machine.uefi) {
        (FirmwareKind::Uefi, Some(uefi)) => {
            let mut text = "UEFI".to_owned();
            if let Some(version) = &uefi.spec_version {
                text.push_str(&format!(" {version}"));
            }
            if let Some(vendor) = &uefi.vendor {
                text.push_str(&format!(" ({vendor})"));
            }
            if let Some(secure_boot) = uefi.secure_boot {
                text.push_str(if secure_boot {
                    ", Secure Boot on"
                } else {
                    ", Secure Boot off"
                });
            }
            text
        }
        (FirmwareKind::Bios, _) => "legacy BIOS".to_owned(),
        _ => "firmware unknown".to_owned(),
    };
    hardware.push(firmware);
    hardware.push(machine.arch.clone());
    println!("  Machine     {}", hardware.join(" · "));
    println!("  Boot chain  {}", boot_chain(record).join(" → "));

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
    println!("  Record      {capture}{end}");
}

/// The components that took part in the boot, in order.
fn boot_chain(record: &BootRecord) -> Vec<String> {
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
    if record
        .stages
        .iter()
        .any(|s| s.stage == kokuin_model::Stage::Initramfs)
    {
        chain.push("initramfs".into());
    }
    chain.extend(
        boot.init
            .as_ref()
            .map(|i| i.name.clone().unwrap_or(i.id.clone())),
    );
    for kind in [M::DisplayManagerStart, M::CompositorStart] {
        let process = milestone_event(record, kind)
            .and_then(|event| event.process)
            .and_then(|id| record.processes.iter().find(|p| p.id == id));
        chain.extend(process.map(|p| p.comm.clone()));
    }
    chain
}

fn milestone_event(record: &BootRecord, kind: M) -> Option<&Event> {
    let id = record.milestones.iter().find(|m| m.kind == kind)?.event?;
    record.events.iter().find(|e| e.id == id)
}

fn stages(record: &BootRecord) {
    println!();
    println!("{}", paint(style::HEADING, "Stages"));
    let times = record
        .stages
        .iter()
        .flat_map(|s| [s.start_ns, s.end_ns])
        .chain(record.milestones.iter().map(|m| Some(m.t_ns)))
        .flatten();
    let (Some(min), Some(max)) = (times.clone().min(), times.max()) else {
        println!("  no stage could be timed");
        return;
    };
    let span = (max - min).max(1) as f64;
    let column = |t: i64| (((t - min) as f64 / span) * BAR_WIDTH as f64).round() as usize;

    for stage in &record.stages {
        let name = output::stage(stage.stage);
        let (bar, timing) = match stage.start_ns {
            Some(start) => {
                let end = stage.end_ns.unwrap_or(max);
                let from = column(start).min(BAR_WIDTH - 1);
                let to = column(end).clamp(from + 1, BAR_WIDTH);
                let bar = format!(
                    "{}{}{}",
                    " ".repeat(from),
                    "█".repeat(to - from),
                    " ".repeat(BAR_WIDTH - to)
                );
                let length = stage.end_ns.map_or_else(
                    || "…".to_owned(),
                    |end| duration((end - start).max(0) as u64),
                );
                (
                    bar,
                    format!(
                        "{:>10}{} {:>9}",
                        time(start),
                        confidence_mark(stage.confidence),
                        length
                    ),
                )
            }
            None => (
                " ".repeat(BAR_WIDTH),
                format!("{:>21}", paint(style::DIM, "not measured")),
            ),
        };
        println!(
            "  {:<11} {} {}",
            name,
            paint(stage_style(stage), bar),
            timing
        );
    }

    let at = |kind| {
        record
            .milestones
            .iter()
            .find(|m| m.kind == kind)
            .map(|m| m.t_ns)
    };
    if let Some(ready) = at(M::SessionReady) {
        let mut line = format!(
            "Desktop ready {} after the kernel started",
            duration(ready.max(0) as u64)
        );
        if let Some(power_on) = at(M::FirmwareStart) {
            line.push_str(&format!(
                ", {} after the firmware started",
                duration((ready - power_on).max(0) as u64)
            ));
        }
        println!("  {}", paint(style::HEADING, line));
    }
}

fn stage_style(stage: &StageSpan) -> anstyle::Style {
    use anstyle::AnsiColor::*;
    let color = match stage.stage {
        kokuin_model::Stage::Firmware => Magenta,
        kokuin_model::Stage::Bootloader => Blue,
        kokuin_model::Stage::Kernel => Cyan,
        kokuin_model::Stage::Initramfs => Green,
        kokuin_model::Stage::Init => Yellow,
        kokuin_model::Stage::Services => Red,
        kokuin_model::Stage::Graphical => BrightMagenta,
        kokuin_model::Stage::Session => BrightBlue,
        _ => BrightBlack,
    };
    color.on_default()
}

fn milestones(record: &BootRecord) {
    println!();
    println!("{}", paint(style::HEADING, "Milestones"));
    for milestone in &record.milestones {
        let detail = match milestone.event {
            Some(id) => record
                .events
                .iter()
                .find(|e| e.id == id)
                .map(|e| paint(style::DIM, &e.title))
                .unwrap_or_default(),
            None => paint(style::DIM, "estimated: no event marks this moment"),
        };
        println!(
            "  {:>10}{} {:<26} {detail}",
            time(milestone.t_ns),
            confidence_mark(milestone.confidence),
            output::milestone_label(milestone.kind)
        );
    }
}

fn slowest(record: &BootRecord) {
    println!();
    println!("{}", paint(style::HEADING, "Slowest steps"));
    let steps = slowest_of(record, |e| e.kind != EventKind::Initcall);
    if steps.is_empty() {
        println!(
            "  {}",
            paint(style::DIM, "no step durations were measured in this record")
        );
    }
    print_steps(&steps, true);

    // Recorded with `initcall_debug` (a deep boot): the kernel stage in detail.
    let initcalls = slowest_of(record, |e| e.kind == EventKind::Initcall);
    if !initcalls.is_empty() {
        println!();
        println!("{}", paint(style::HEADING, "Slowest kernel init functions"));
        print_steps(&initcalls, false);
    }
}

/// The longest events that pass `keep`, longest first.
fn slowest_of(record: &BootRecord, keep: impl Fn(&Event) -> bool) -> Vec<&Event> {
    let mut timed: Vec<&Event> = record
        .events
        .iter()
        .filter(|e| e.dur_ns.is_some_and(|d| d > 0) && keep(e))
        .collect();
    timed.sort_by_key(|e| std::cmp::Reverse(e.dur_ns));
    timed.truncate(SLOWEST);
    timed
}

fn print_steps(steps: &[&Event], with_stage: bool) {
    for event in steps {
        let stage = if with_stage {
            format!("{:<10} ", output::stage(event.stage))
        } else {
            String::new()
        };
        println!(
            "  {:>9}  {stage}{}",
            duration(event.dur_ns.unwrap_or_default()),
            event.title
        );
    }
}

fn sources(record: &BootRecord) {
    let problems: Vec<_> = record
        .sources
        .iter()
        .filter(|s| s.status != SourceStatus::Ok)
        .collect();
    if problems.is_empty() {
        return;
    }
    println!();
    println!("{}", paint(style::HEADING, "Incomplete data"));
    for source in problems {
        println!(
            "  {} {:<13} {}",
            output::status_symbol(source.status),
            source.id,
            paint(style::DIM, source.reason.as_deref().unwrap_or(""))
        );
    }
}
