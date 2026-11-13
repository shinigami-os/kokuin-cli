//! Compares two boots: what moved, what got slower or faster, what is new.
//!
//! Steps are matched between the boots by a key that stays the same from one boot
//! to the next: the service name, the kernel init function, or else the source and
//! title of the event.

use std::collections::{BTreeMap, BTreeSet};

use kokuin_model::{
    BootDiff, BootRecord, DiffSide, Event, EventKind, MilestoneChange, Stage, StageChange,
    StepChange,
};

use crate::processes::display_name;

/// Changes smaller than this are noise.
const MIN_CHANGE_NS: i64 = 10_000_000;

/// Compares boot A (before) with boot B (after).
pub fn diff(a: &BootRecord, b: &BootRecord) -> BootDiff {
    let programs_a = programs(a);
    let programs_b = programs(b);
    // Shown with the longest name seen: the full one where it could be read.
    let only_in = |these: &BTreeMap<String, String>, others: &BTreeMap<String, String>| {
        these
            .iter()
            .filter(|(comm, _)| !others.contains_key(*comm))
            .map(|(_, name)| name.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    };
    BootDiff {
        schema_version: kokuin_model::SCHEMA_VERSION.to_owned(),
        a: side(a),
        b: side(b),
        milestones: milestones(a, b),
        stages: stages(a, b),
        steps: steps(a, b),
        new_programs: only_in(&programs_b, &programs_a),
        gone_programs: only_in(&programs_a, &programs_b),
    }
}

fn side(record: &BootRecord) -> DiffSide {
    DiffSide {
        boot_id: record.boot.boot_id.clone(),
        started_at: record.boot.started_at.clone(),
        kernel_release: record.boot.kernel.release.clone(),
        method: record.capture.method,
    }
}

fn delta(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    Some(b? - a?)
}

fn milestones(a: &BootRecord, b: &BootRecord) -> Vec<MilestoneChange> {
    let time = |record: &BootRecord, kind| {
        record
            .milestones
            .iter()
            .find(|m| m.kind == kind)
            .map(|m| m.t_ns)
    };
    let kinds: BTreeSet<_> = a
        .milestones
        .iter()
        .chain(&b.milestones)
        .map(|m| m.kind)
        .collect();
    let mut changes: Vec<MilestoneChange> = kinds
        .into_iter()
        .map(|kind| {
            let (a_ns, b_ns) = (time(a, kind), time(b, kind));
            MilestoneChange {
                kind,
                a_ns,
                b_ns,
                delta_ns: delta(a_ns, b_ns),
            }
        })
        .collect();
    // Stable: milestones at the same time keep their boot order.
    changes.sort_by_key(|m| m.b_ns.or(m.a_ns));
    changes
}

fn stages(a: &BootRecord, b: &BootRecord) -> Vec<StageChange> {
    let duration = |record: &BootRecord, stage: Stage| {
        let span = record.stages.iter().find(|s| s.stage == stage)?;
        Some(span.end_ns? - span.start_ns?)
    };
    let stages: BTreeSet<Stage> = a.stages.iter().chain(&b.stages).map(|s| s.stage).collect();
    stages
        .into_iter()
        .map(|stage| {
            let (a_ns, b_ns) = (duration(a, stage), duration(b, stage));
            StageChange {
                stage,
                a_ns,
                b_ns,
                delta_ns: delta(a_ns, b_ns),
            }
        })
        .collect()
}

/// What matches a step between two boots.
pub fn step_key(event: &Event) -> String {
    let data = |key: &str| event.data.get(key).and_then(|v| v.as_str());
    match event.kind {
        EventKind::Service => match data("unit").or_else(|| data("service")) {
            Some(name) => format!("service:{name}"),
            None => format!("{}:{}", event.source, event.title),
        },
        EventKind::Initcall => match data("function") {
            Some(function) => format!("initcall:{function}"),
            None => format!("{}:{}", event.source, event.title),
        },
        _ => format!("{}:{}", event.source, event.title),
    }
}

/// The timed steps of a boot by key: title and duration. A step seen several times
/// counts once, the first time.
fn timed_steps(record: &BootRecord) -> BTreeMap<String, (&str, i64)> {
    let mut steps = BTreeMap::new();
    for event in &record.events {
        let Some(duration) = event.dur_ns.filter(|&d| d > 0) else {
            continue;
        };
        steps.entry(step_key(event)).or_insert((
            event.title.as_str(),
            i64::try_from(duration).unwrap_or(i64::MAX),
        ));
    }
    steps
}

fn steps(a: &BootRecord, b: &BootRecord) -> Vec<StepChange> {
    let steps_a = timed_steps(a);
    let steps_b = timed_steps(b);
    let keys: BTreeSet<&String> = steps_a.keys().chain(steps_b.keys()).collect();
    let mut changes: Vec<StepChange> = keys
        .into_iter()
        .filter_map(|key| {
            let (in_a, in_b) = (steps_a.get(key), steps_b.get(key));
            let (a_ns, b_ns) = (in_a.map(|s| s.1), in_b.map(|s| s.1));
            let delta_ns = b_ns.unwrap_or(0) - a_ns.unwrap_or(0);
            (delta_ns.abs() >= MIN_CHANGE_NS).then(|| StepChange {
                key: key.clone(),
                title: in_b.or(in_a).map(|s| s.0.to_owned()).unwrap_or_default(),
                a_ns,
                b_ns,
                delta_ns,
            })
        })
        .collect();
    changes.sort_by(|x, y| {
        y.delta_ns
            .abs()
            .cmp(&x.delta_ns.abs())
            .then_with(|| x.key.cmp(&y.key))
    });
    changes
}

/// The programs a boot ran, by the kernel's name (`comm`, cut to 15 bytes, the same
/// from one boot to the next), with the longest name known for each. Unnamed
/// processes are left out.
fn programs(record: &BootRecord) -> BTreeMap<String, String> {
    let mut programs: BTreeMap<String, String> = BTreeMap::new();
    for process in record.processes.iter().filter(|p| !p.comm.is_empty()) {
        let name = display_name(process);
        let known = programs.entry(process.comm.clone()).or_default();
        if name.len() > known.len() {
            *known = name.into_owned();
        }
    }
    programs
}

#[cfg(test)]
mod tests {
    use kokuin_model::{Confidence, EventId, Milestone, MilestoneKind, ProcessId, StageSpan};
    use serde_json::json;

    use super::*;

    /// The example record of the format documentation, reduced to what a test sets.
    fn record(ready_ns: i64, services: &[(&str, i64)], programs: &[&str]) -> BootRecord {
        let example = include_str!("../../../docs/examples/boot-record.example.json");
        let mut record: BootRecord = serde_json::from_str(example).unwrap();
        record.milestones = vec![Milestone {
            kind: MilestoneKind::SessionReady,
            t_ns: ready_ns,
            confidence: Confidence::Exact,
            event: None,
            explain_id: None,
        }];
        record.stages = vec![StageSpan {
            stage: Stage::Kernel,
            start_ns: Some(0),
            end_ns: Some(ready_ns / 10),
            confidence: Confidence::Exact,
            explain_id: None,
        }];
        record.events = services
            .iter()
            .enumerate()
            .map(|(i, (unit, dur_ms))| {
                let mut event = Event::new(
                    EventId(i as u64 + 1),
                    Stage::Services,
                    "systemd",
                    EventKind::Service,
                    format!("{unit} started"),
                    Some(0),
                    Confidence::Exact,
                );
                event.dur_ns = Some(*dur_ms as u64 * 1_000_000);
                event.data.insert("unit".into(), json!(unit));
                event
            })
            .collect();
        let template = record.processes[0].clone();
        record.processes = programs
            .iter()
            .enumerate()
            .map(|(i, name)| kokuin_model::Process {
                id: ProcessId(i as u64 + 1),
                comm: (*name).into(),
                exe: None,
                cmdline: Vec::new(),
                ..template.clone()
            })
            .collect();
        record
    }

    #[test]
    fn a_boot_compared_with_itself_has_no_changes() {
        let boot = record(12_000_000_000, &[("a.service", 500)], &["sway"]);
        let diff = diff(&boot, &boot);
        assert!(diff.steps.is_empty());
        assert!(diff.new_programs.is_empty() && diff.gone_programs.is_empty());
        assert_eq!(diff.milestones[0].delta_ns, Some(0));
    }

    #[test]
    fn finds_what_moved_and_what_is_new() {
        let a = record(
            12_000_000_000,
            &[
                ("slow.service", 500),
                ("same.service", 300),
                ("gone.service", 50),
            ],
            &["sway", "old-tool"],
        );
        let b = record(
            13_500_000_000,
            &[
                ("slow.service", 1_700),
                ("same.service", 305),
                ("new.service", 40),
            ],
            &["sway", "new-tool"],
        );
        let diff = diff(&a, &b);
        assert_eq!(diff.milestones[0].delta_ns, Some(1_500_000_000));
        assert_eq!(diff.stages[0].delta_ns, Some(150_000_000));
        let keys: Vec<_> = diff.steps.iter().map(|s| s.key.as_str()).collect();
        // 5 ms on same.service is noise; the others are sorted by size.
        assert_eq!(
            keys,
            [
                "service:slow.service",
                "service:gone.service",
                "service:new.service"
            ]
        );
        assert_eq!(diff.steps[0].delta_ns, 1_200_000_000);
        assert_eq!(diff.steps[1].b_ns, None);
        assert_eq!(diff.new_programs, ["new-tool"]);
        assert_eq!(diff.gone_programs, ["old-tool"]);
    }

    #[test]
    fn a_program_named_in_full_or_cut_is_the_same() {
        let a = record(12_000_000_000, &[], &["systemd-run-gen"]);
        let mut b = record(12_000_000_000, &[], &["systemd-run-gen"]);
        b.processes[0].exe = Some("/usr/lib/systemd/systemd-run-generator".into());
        let diff = diff(&a, &b);
        assert!(diff.new_programs.is_empty() && diff.gone_programs.is_empty());
    }
}
