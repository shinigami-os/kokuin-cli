//! `diff`: what changed between two boots.

use anstream::println;
use kokuin_model::{BootDiff, CaptureMethod, DiffSide, StepChange};

use crate::{
    Failure,
    output::{self, duration, paint, style, time},
    records,
};

/// How many steps to show in each direction.
const STEPS: usize = 10;
/// How many program names to list.
const PROGRAMS: usize = 25;

pub fn run(a: Option<String>, b: Option<String>, json: bool) -> Result<(), Failure> {
    let a = records::load(Some(a.as_deref().unwrap_or("-2")))?;
    let b = records::load(Some(b.as_deref().unwrap_or("last")))?;
    let diff = kokuin_analyze::diff(&a, &b);
    if json {
        println!("{}", serde_json::to_string_pretty(&diff)?);
        return Ok(());
    }
    header(&diff);
    milestones(&diff);
    stages(&diff);
    steps(&diff);
    programs(&diff);
    Ok(())
}

fn header(diff: &BootDiff) {
    println!("{}", paint(style::TITLE, "What changed between two boots"));
    println!();
    for (name, side) in [("A", &diff.a), ("B", &diff.b)] {
        println!("  {name}  {}", describe(side));
    }
    if diff.a.kernel_release != diff.b.kernel_release {
        println!(
            "  {} Linux {} → {}",
            paint(style::WARNING, "The kernel changed:"),
            diff.a.kernel_release,
            diff.b.kernel_release
        );
    }
}

fn describe(side: &DiffSide) -> String {
    let id: String = side.boot_id.chars().take(8).collect();
    let method = match side.method {
        CaptureMethod::Recorder => "recorded live",
        CaptureMethod::RecorderDeep => "recorded live, with kernel tracing",
        CaptureMethod::PostHoc => "rebuilt after boot",
        CaptureMethod::Unknown => "",
    };
    format!(
        "{}  {id}  Linux {}  {}",
        side.started_at.as_deref().unwrap_or("?"),
        side.kernel_release,
        paint(style::DIM, method)
    )
}

/// A signed change, right-aligned on `width` columns, colored: longer or later is
/// worse. Padded before coloring, so that the colors do not count as columns.
fn change(delta_ns: Option<i64>, width: usize) -> String {
    let (text, style) = match delta_ns {
        None => ("·".to_owned(), style::DIM),
        Some(0) => ("=".to_owned(), style::DIM),
        Some(delta) => {
            let sign = if delta > 0 { "+" } else { "−" };
            let text = format!("{sign}{}", duration(delta.unsigned_abs()));
            (text, if delta > 0 { style::ERROR } else { style::GOOD })
        }
    };
    paint(style, format!("{text:>width$}"))
}

fn optional_time(t_ns: Option<i64>) -> String {
    t_ns.map_or_else(|| "·".to_owned(), time)
}

fn optional_duration(ns: Option<i64>) -> String {
    ns.map_or_else(|| "·".to_owned(), |ns| duration(ns.max(0) as u64))
}

fn milestones(diff: &BootDiff) {
    println!();
    println!(
        "{}",
        paint(
            style::HEADING,
            format!("{:<28} {:>10} {:>10}  {}", "Milestones", "A", "B", "change")
        )
    );
    for m in &diff.milestones {
        println!(
            "  {:<26} {:>10} {:>10}  {}",
            output::milestone_label(m.kind),
            optional_time(m.a_ns),
            optional_time(m.b_ns),
            change(m.delta_ns, 8)
        );
    }
}

fn stages(diff: &BootDiff) {
    println!();
    println!(
        "{}",
        paint(
            style::HEADING,
            format!(
                "{:<28} {:>10} {:>10}  {}",
                "Stages (duration)", "A", "B", "change"
            )
        )
    );
    for s in &diff.stages {
        println!(
            "  {:<26} {:>10} {:>10}  {}",
            output::stage(s.stage),
            optional_duration(s.a_ns),
            optional_duration(s.b_ns),
            change(s.delta_ns, 8)
        );
    }
}

fn steps(diff: &BootDiff) {
    let longer: Vec<&StepChange> = diff.steps.iter().filter(|s| s.delta_ns > 0).collect();
    let shorter: Vec<&StepChange> = diff.steps.iter().filter(|s| s.delta_ns < 0).collect();
    for (heading, steps) in [
        ("Steps that took longer in B", longer),
        ("Steps that took less time in B", shorter),
    ] {
        println!();
        println!("{}", paint(style::HEADING, heading));
        if steps.is_empty() {
            println!("  {}", paint(style::DIM, "none (by 10 ms or more)"));
        }
        for step in steps.iter().take(STEPS) {
            let detail = match (step.a_ns, step.b_ns) {
                (Some(a), Some(b)) => format!(
                    "{} → {}",
                    duration(a.max(0) as u64),
                    duration(b.max(0) as u64)
                ),
                (None, Some(_)) => "only in B".to_owned(),
                (Some(_), None) => "only in A".to_owned(),
                (None, None) => String::new(),
            };
            println!(
                "  {}  {}  {}",
                change(Some(step.delta_ns), 9),
                step.title,
                paint(style::DIM, detail)
            );
        }
        if steps.len() > STEPS {
            println!(
                "  {}",
                paint(
                    style::DIM,
                    format!("and {} more (--json)", steps.len() - STEPS)
                )
            );
        }
    }
}

fn programs(diff: &BootDiff) {
    for (heading, names) in [
        ("Programs run in B only", &diff.new_programs),
        ("Programs run in A only", &diff.gone_programs),
    ] {
        if names.is_empty() {
            continue;
        }
        println!();
        println!(
            "{}",
            paint(style::HEADING, format!("{heading} ({})", names.len()))
        );
        let shown: Vec<&str> = names.iter().take(PROGRAMS).map(String::as_str).collect();
        let more = names.len().saturating_sub(PROGRAMS);
        let mut line = shown.join(", ");
        if more > 0 {
            line.push_str(&format!(", and {more} more"));
        }
        println!("  {line}");
    }
}
