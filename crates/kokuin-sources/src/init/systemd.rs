//! systemd: unit activation times, read with `systemctl show`.

use std::collections::BTreeMap;

use kokuin_model::{Confidence, EventKind, MilestoneKind, SourceStatus, Stage};

use crate::{Context, Observation, Source, SourceOutput, report};

const EXPLAIN: &str = "init.systemd";
/// Unit types worth showing; device, slice and scope units are too many and too
/// technical.
const UNIT_TYPES: [&str; 8] = [
    "service",
    "socket",
    "target",
    "mount",
    "swap",
    "timer",
    "path",
    "automount",
];
const UNIT_PROPERTIES: &str =
    "Id,Description,InactiveExitTimestampMonotonic,ActiveEnterTimestampMonotonic,ExecMainPID,Where";
const MANAGER_PROPERTIES: &str =
    "InitRDTimestampMonotonic,UserspaceTimestampMonotonic,FinishTimestampMonotonic";

/// systemd's view of the boot.
pub struct Systemd;

impl Source for Systemd {
    fn id(&self) -> &'static str {
        "systemd"
    }

    fn explain_id(&self) -> &'static str {
        EXPLAIN
    }

    fn collect(&self, ctx: &Context) -> SourceOutput {
        match read(ctx) {
            Ok((manager, units)) => {
                let observations = manager_observations(&manager)
                    .into_iter()
                    .chain(unit_observations(&units))
                    .collect();
                SourceOutput {
                    report: report::ok(self.id(), EXPLAIN),
                    observations,
                    processes: Vec::new(),
                }
            }
            Err(err) => SourceOutput::empty(report::with_status(
                self.id(),
                EXPLAIN,
                SourceStatus::Unavailable,
                Some(format!("cannot query systemd: {err}")),
            )),
        }
    }
}

type Properties = BTreeMap<String, String>;

fn read(ctx: &Context) -> std::io::Result<(Properties, Vec<Properties>)> {
    let manager = parse_show(&ctx.run("systemctl", &["show", "--property", MANAGER_PROPERTIES])?)
        .into_iter()
        .next()
        .unwrap_or_default();
    let list = ctx.run(
        "systemctl",
        &[
            "list-units",
            "--all",
            "--plain",
            "--no-legend",
            "--no-pager",
        ],
    )?;
    let names: Vec<&str> = list
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| {
            name.rsplit_once('.')
                .is_some_and(|(_, kind)| UNIT_TYPES.contains(&kind))
        })
        .collect();
    let mut units = Vec::new();
    for chunk in names.chunks(200) {
        let mut args = vec!["show", "--property", UNIT_PROPERTIES, "--"];
        args.extend(chunk);
        units.extend(parse_show(&ctx.run("systemctl", &args)?));
    }
    Ok((manager, units))
}

/// Parses `systemctl show` output: `Key=value` lines, one block per unit, blocks
/// separated by an empty line.
fn parse_show(text: &str) -> Vec<Properties> {
    text.split("\n\n")
        .map(|block| {
            block
                .lines()
                .filter_map(|line| line.split_once('='))
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect::<Properties>()
        })
        .filter(|block| !block.is_empty())
        .collect()
}

/// A monotonic timestamp in µs, as ns; `None` when 0 (not reached).
fn monotonic_ns(properties: &Properties, key: &str) -> Option<i64> {
    properties
        .get(key)?
        .parse::<i64>()
        .ok()
        .filter(|&us| us > 0)
        .map(|us| us * 1_000)
}

fn manager_observations(manager: &Properties) -> Vec<Observation> {
    let mut found = Vec::new();
    if let Some(t) = monotonic_ns(manager, "InitRDTimestampMonotonic") {
        found.push(
            Observation::new(
                EventKind::Log,
                "systemd started in the initramfs",
                Some(t),
                Confidence::Exact,
            )
            .in_stage(Stage::Initramfs)
            .explained_by("stage.initramfs")
            .marks(MilestoneKind::InitramfsStart),
        );
    }
    if let Some(t) = monotonic_ns(manager, "UserspaceTimestampMonotonic") {
        found.push(
            Observation::new(
                EventKind::Log,
                "systemd started on the root filesystem",
                Some(t),
                Confidence::Exact,
            )
            .in_stage(Stage::Init)
            .explained_by(EXPLAIN)
            .marks(MilestoneKind::InitStart),
        );
    }
    if let Some(t) = monotonic_ns(manager, "FinishTimestampMonotonic") {
        found.push(
            Observation::new(
                EventKind::Log,
                "systemd finished starting the boot services",
                Some(t),
                Confidence::Exact,
            )
            .explained_by(EXPLAIN),
        );
    }
    found
}

fn unit_observations(units: &[Properties]) -> Vec<Observation> {
    units.iter().filter_map(unit_observation).collect()
}

fn unit_observation(unit: &Properties) -> Option<Observation> {
    let id = unit.get("Id")?;
    let description = unit
        .get("Description")
        .filter(|d| !d.is_empty() && *d != id);
    let started = monotonic_ns(unit, "InactiveExitTimestampMonotonic");
    let active = monotonic_ns(unit, "ActiveEnterTimestampMonotonic");
    let t_ns = started.or(active)?;
    let kind = id.rsplit_once('.').map_or("", |(_, kind)| kind);
    let label = description.map_or_else(|| id.clone(), |d| format!("{id} ({d})"));

    let (event_kind, title) = match kind {
        "target" => (EventKind::Service, format!("Target {label} reached")),
        "mount" | "swap" | "automount" => {
            let what = unit.get("Where").filter(|w| !w.is_empty()).unwrap_or(id);
            (EventKind::Mount, format!("{what} mounted ({id})"))
        }
        _ => (EventKind::Service, format!("{label} started")),
    };
    let mut observation = Observation::new(event_kind, title, Some(t_ns), Confidence::Exact)
        .explained_by(EXPLAIN)
        .with("unit", id.as_str())
        .with(
            "state",
            if active.is_some() {
                "active"
            } else {
                "activating"
            },
        );
    if let (Some(start), Some(end)) = (started, active) {
        observation.dur_ns = u64::try_from(end - start).ok().filter(|&ns| ns > 0);
    }
    if let Some(pid) = unit
        .get("ExecMainPID")
        .and_then(|p| p.parse::<u32>().ok())
        .filter(|&p| p != 0)
    {
        observation.pid = Some(pid);
    }
    if id == "sysinit.target" {
        observation.t_ns = active.or(started);
        observation.dur_ns = None;
        observation.milestone = Some(MilestoneKind::ServicesStart);
    }
    Some(observation)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOW: &str = "\
Id=sshd.service
Description=OpenBSD Secure Shell server
InactiveExitTimestampMonotonic=3100000
ActiveEnterTimestampMonotonic=3350000
ExecMainPID=812
Where=

Id=sysinit.target
Description=System Initialization
InactiveExitTimestampMonotonic=2000000
ActiveEnterTimestampMonotonic=2950000
ExecMainPID=0

Id=boot.mount
Description=/boot
InactiveExitTimestampMonotonic=0
ActiveEnterTimestampMonotonic=1900000
Where=/boot
";

    #[test]
    fn turns_units_into_observations() {
        let found = unit_observations(&parse_show(SHOW));
        assert_eq!(
            found[0].title,
            "sshd.service (OpenBSD Secure Shell server) started"
        );
        assert_eq!(
            (found[0].t_ns, found[0].dur_ns, found[0].pid),
            (Some(3_100_000_000), Some(250_000_000), Some(812))
        );
        assert_eq!(found[1].milestone, Some(MilestoneKind::ServicesStart));
        assert_eq!(found[1].t_ns, Some(2_950_000_000));
        assert_eq!(found[2].title, "/boot mounted (boot.mount)");
        assert_eq!(found[2].kind, EventKind::Mount);
    }
}
