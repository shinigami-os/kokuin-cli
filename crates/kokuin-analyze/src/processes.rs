//! Rebuilds the process tree, recognizes programs, attributes processes to services
//! and to the system or the user session.

use std::{borrow::Cow, collections::HashMap};

use kokuin_kb::{Kb, ProcessRole};
use kokuin_model::{Confidence, EventKind, MilestoneKind, Process, ProcessId, Stage};
use kokuin_sources::{Observation, ProcessInfo, is_normal_user};

use crate::item::{Affinity, Item};

/// Two sources seeing a process with the same PID started this close see the same
/// process (`/proc` start times are rounded to 10 ms).
const SAME_PROCESS_NS: i64 = 50_000_000;

pub(crate) struct ProcessEntry {
    pub process: Process,
    pub role: Option<ProcessRole>,
    pub affinity: Affinity,
}

pub(crate) struct Processes {
    /// Sorted by start time; `entries[i].process.id` is `i + 1`.
    pub entries: Vec<ProcessEntry>,
    /// One `process_start` item per process with a known start time.
    pub start_items: Vec<Item>,
}

impl Processes {
    /// The process with this PID that was alive at `t_ns`. PIDs are reused, so when
    /// several processes had it, the last one started before `t_ns` is chosen.
    pub(crate) fn find(&self, pid: u32, t_ns: Option<i64>) -> Option<ProcessId> {
        let mut candidates = self.entries.iter().filter(|e| e.process.pid == pid);
        match t_ns {
            Some(t) => candidates
                .rfind(|e| e.process.start_ns.is_none_or(|start| start <= t))
                .or_else(|| self.entries.iter().find(|e| e.process.pid == pid)),
            None => candidates.next(),
        }
        .map(|e| e.process.id)
    }

    /// The first process of a normal user.
    pub(crate) fn first_user_process(&self) -> Option<&ProcessEntry> {
        self.entries
            .iter()
            .filter(|e| e.affinity == Affinity::User && e.process.start_ns.is_some())
            .min_by_key(|e| e.process.start_ns)
    }
}

pub(crate) fn build(infos: Vec<(String, ProcessInfo)>, kb: &Kb) -> Processes {
    let mut infos = merge_duplicates(infos);
    infos.sort_by_key(|(_, info)| (info.start_ns.is_none(), info.start_ns, info.pid));

    let mut entries: Vec<ProcessEntry> = infos
        .iter()
        .enumerate()
        .map(|(index, (_, info))| {
            let rule = kb.match_process(info.names());
            ProcessEntry {
                process: Process {
                    id: ProcessId(index as u64 + 1),
                    pid: info.pid,
                    parent: None,
                    ppid: info.ppid,
                    uid: info.uid,
                    comm: info.comm.clone(),
                    cmdline: info.cmdline.clone(),
                    exe: info.exe.clone(),
                    start_ns: info.start_ns,
                    end_ns: info.end_ns,
                    exit_code: info.exit_code,
                    exit_signal: info.exit_signal,
                    service: info.cgroup.as_deref().and_then(service_from_cgroup),
                    stage: Stage::Unknown,
                    confidence: info.confidence.unwrap_or(Confidence::Approximate),
                    explain_id: rule.and_then(|r| r.explain.clone()),
                },
                role: rule.map(|r| r.role),
                affinity: Affinity::None,
            }
        })
        .collect();

    link_parents(&mut entries);
    attribute_runit_services(&mut entries);
    for index in 0..entries.len() {
        entries[index].affinity = affinity(&entries, index);
    }

    let start_items = entries
        .iter()
        .zip(&infos)
        .filter_map(|(entry, (source, _))| start_item(entry, source))
        .collect();
    Processes {
        entries,
        start_items,
    }
}

/// Keeps one entry per process when several sources saw it (the recorder and `/proc`
/// both see the processes still running). The first source wins; the others fill in
/// what it did not know.
fn merge_duplicates(infos: Vec<(String, ProcessInfo)>) -> Vec<(String, ProcessInfo)> {
    let mut merged: Vec<(String, ProcessInfo)> = Vec::with_capacity(infos.len());
    let mut by_pid: HashMap<u32, Vec<usize>> = HashMap::new();
    for (source, info) in infos {
        let same = by_pid.get(&info.pid).and_then(|indices| {
            indices.iter().copied().find(|&i| {
                let (kept_source, kept) = &merged[i];
                *kept_source != source
                    && match (kept.start_ns, info.start_ns) {
                        (Some(a), Some(b)) => (a - b).abs() <= SAME_PROCESS_NS,
                        _ => false,
                    }
            })
        });
        match same {
            Some(index) => fill_missing(&mut merged[index].1, info),
            None => {
                by_pid.entry(info.pid).or_default().push(merged.len());
                merged.push((source, info));
            }
        }
    }
    merged
}

fn fill_missing(kept: &mut ProcessInfo, other: ProcessInfo) {
    if kept.comm.is_empty() {
        kept.comm = other.comm;
    }
    if kept.cmdline.is_empty() {
        kept.cmdline = other.cmdline;
    }
    kept.ppid = kept.ppid.or(other.ppid);
    kept.uid = kept.uid.or(other.uid);
    kept.exe = kept.exe.take().or(other.exe);
    kept.cgroup = kept.cgroup.take().or(other.cgroup);
}

/// Affinity of a service event (`data.service` or `data.unit`): graphical for
/// display managers, system otherwise.
pub(crate) fn service_affinity(item: &Item, kb: &Kb) -> Affinity {
    let name = ["service", "unit"]
        .iter()
        .find_map(|key| item.obs.data.get(*key)?.as_str());
    let program = name.map(|n| n.rsplit_once('.').map_or(n, |(base, _)| base));
    match program
        .and_then(|p| kb.match_process([p]))
        .map(|rule| rule.role)
    {
        Some(ProcessRole::DisplayManager) => Affinity::Graphical,
        _ => Affinity::System,
    }
}

/// The parent is the process with the parent PID that started last before the child.
fn link_parents(entries: &mut [ProcessEntry]) {
    for index in 0..entries.len() {
        let Some(ppid) = entries[index].process.ppid else {
            continue;
        };
        let start = entries[index].process.start_ns;
        let parent = entries[..index]
            .iter()
            .rev()
            .find(|candidate| candidate.process.pid == ppid && candidate.process.start_ns <= start)
            .map(|candidate| candidate.process.id);
        entries[index].process.parent = parent;
    }
}

fn parent_of(entries: &[ProcessEntry], index: usize) -> Option<usize> {
    let parent = entries[index].process.parent?;
    Some(parent.0 as usize - 1)
}

/// Iterates over the ancestors of a process, closest first.
fn ancestors(entries: &[ProcessEntry], index: usize) -> impl Iterator<Item = usize> + '_ {
    std::iter::successors(parent_of(entries, index), move |&i| parent_of(entries, i)).take(64)
}

/// The service a control group belongs to: `/system.slice/sshd.service` (systemd),
/// `/openrc.sshd` (OpenRC).
fn service_from_cgroup(cgroup: &str) -> Option<String> {
    let last = cgroup.rsplit('/').next()?;
    if last.ends_with(".service") && cgroup.contains("system.slice") {
        return Some(last.to_owned());
    }
    cgroup
        .split('/')
        .find_map(|part| part.strip_prefix("openrc."))
        .map(str::to_owned)
}

/// runit runs each service under `runsv <name>`.
fn attribute_runit_services(entries: &mut [ProcessEntry]) {
    for index in 0..entries.len() {
        if entries[index].process.service.is_some() {
            continue;
        }
        let service = ancestors(entries, index).find_map(|ancestor| {
            let process = &entries[ancestor].process;
            (process.comm == "runsv")
                .then(|| process.cmdline.get(1).cloned())
                .flatten()
        });
        entries[index].process.service = service;
    }
}

fn affinity(entries: &[ProcessEntry], index: usize) -> Affinity {
    let entry = &entries[index];
    if is_normal_user(entry.process.uid) {
        return Affinity::User;
    }
    let graphical_role = |e: &ProcessEntry| {
        matches!(
            e.role,
            Some(
                ProcessRole::DisplayManager
                    | ProcessRole::Compositor
                    | ProcessRole::SessionShell
                    | ProcessRole::CompositorShell
            )
        )
    };
    let under_display_manager =
        ancestors(entries, index).any(|a| entries[a].role == Some(ProcessRole::DisplayManager));
    if graphical_role(entry) || under_display_manager {
        Affinity::Graphical
    } else {
        Affinity::System
    }
}

/// Name to show: the command name, or the full program name when the kernel
/// truncated it (15 characters), taken from the executable or the command line.
fn display_name(process: &Process) -> Cow<'_, str> {
    if process.comm.is_empty() {
        // The recorder saw it, but it exited before its program could be read.
        return Cow::Owned(format!("unnamed process {}", process.pid));
    }
    if process.comm.len() < 15 {
        return Cow::Borrowed(&process.comm);
    }
    process
        .exe
        .iter()
        .chain(process.cmdline.first())
        // Programs can rewrite their command line: `gdm-session-worker [pam/gdm]`.
        .filter_map(|path| path.split_whitespace().next()?.rsplit('/').next())
        .find(|name| name.starts_with(&process.comm))
        .map_or(Cow::Borrowed(&process.comm), Cow::Borrowed)
}

fn start_item(entry: &ProcessEntry, source: &str) -> Option<Item> {
    let process = &entry.process;
    let start = process.start_ns?;
    let name = display_name(process);
    let title = if process.pid == 1 {
        format!("{name} runs as PID 1 (the init system)")
    } else {
        format!("{name} started")
    };
    let user = entry.affinity == Affinity::User;
    let milestone = match entry.role {
        Some(ProcessRole::ServiceSupervisor) => Some(MilestoneKind::ServicesStart),
        Some(ProcessRole::DisplayManager) => Some(MilestoneKind::DisplayManagerStart),
        // A compositor-shell also marks `session_ready`, see `milestones::detect`.
        Some(ProcessRole::Compositor | ProcessRole::CompositorShell) if user => {
            Some(MilestoneKind::CompositorStart)
        }
        Some(ProcessRole::SessionShell) if user => Some(MilestoneKind::SessionReady),
        _ => None,
    };

    let mut obs = Observation::new(
        EventKind::ProcessStart,
        title,
        Some(start),
        process.confidence,
    )
    .with("pid", process.pid);
    obs.explain_id = process
        .explain_id
        .clone()
        .or_else(|| (process.pid == 1).then(|| "init.pid1".to_owned()));
    obs.milestone = milestone;
    obs.pid = Some(process.pid);
    if !process.cmdline.is_empty() {
        obs.message = Some(process.cmdline.join(" "));
    }
    if let Some(exe) = &process.exe {
        obs = obs.with("exe", exe.as_str());
    }
    if let Some(service) = &process.service {
        obs = obs.with("service", service.as_str());
    }
    if let Some(uid) = process.uid {
        obs = obs.with("uid", uid);
    }
    let mut item = Item::new(obs, source);
    item.process = Some(process.id);
    item.affinity = entry.affinity;
    Some(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(pid: u32, ppid: u32, uid: u32, comm: &str, start_ms: i64) -> (String, ProcessInfo) {
        let info = ProcessInfo {
            pid,
            ppid: Some(ppid),
            uid: Some(uid),
            comm: comm.into(),
            cmdline: vec![comm.into()],
            start_ns: Some(start_ms * 1_000_000),
            ..ProcessInfo::default()
        };
        ("procfs".into(), info)
    }

    #[test]
    fn builds_tree_roles_and_affinities() {
        let mut runsv = info(450, 401, 0, "runsv", 3_050);
        runsv.1.cmdline = vec!["runsv".into(), "greetd".into()];
        let processes = build(
            vec![
                info(612, 455, 1000, "sway", 5_800),
                info(1, 0, 0, "runit", 1_420),
                info(401, 1, 0, "runsvdir", 2_950),
                runsv,
                info(455, 450, 0, "greetd", 3_100),
                info(470, 455, 102, "cage", 3_200),
            ],
            Kb::embedded(),
        );
        let by_pid = |pid| {
            processes
                .entries
                .iter()
                .find(|e| e.process.pid == pid)
                .unwrap()
        };

        assert_eq!(by_pid(612).process.parent, Some(by_pid(455).process.id));
        assert_eq!(by_pid(455).process.service.as_deref(), Some("greetd"));
        assert_eq!(by_pid(612).affinity, Affinity::User);
        assert_eq!(by_pid(470).affinity, Affinity::Graphical);
        assert_eq!(by_pid(401).affinity, Affinity::System);
        assert_eq!(processes.first_user_process().unwrap().process.pid, 612);

        let milestones: Vec<_> = processes
            .start_items
            .iter()
            .filter_map(|i| i.obs.milestone)
            .collect();
        assert_eq!(
            milestones,
            [
                MilestoneKind::ServicesStart,
                MilestoneKind::DisplayManagerStart,
                MilestoneKind::CompositorStart
            ]
        );
        assert_eq!(
            processes.find(455, Some(4_000_000_000)),
            Some(by_pid(455).process.id)
        );
    }

    #[test]
    fn shows_full_names_of_truncated_commands() {
        let (_, mut worker) = info(700, 455, 0, "gdm-session-wor", 18_620);
        worker.cmdline = vec!["gdm-session-worker [pam/gdm-password]".into()];
        let processes = build(vec![("procfs".into(), worker)], Kb::embedded());
        assert_eq!(
            processes.start_items[0].obs.title,
            "gdm-session-worker started"
        );
    }

    #[test]
    fn merges_a_process_seen_by_two_sources() {
        let recorded = ProcessInfo {
            pid: 40,
            comm: "sway".into(),
            start_ns: Some(13_002_345_678),
            end_ns: None,
            ..ProcessInfo::default()
        };
        let running = ProcessInfo {
            pid: 40,
            comm: "sway".into(),
            cmdline: vec!["sway".into()],
            start_ns: Some(13_000_000_000),
            ..ProcessInfo::default()
        };
        let reused = ProcessInfo {
            pid: 40,
            comm: "sh".into(),
            start_ns: Some(60_000_000_000),
            ..ProcessInfo::default()
        };
        let merged = merge_duplicates(vec![
            ("proc_events".into(), recorded),
            ("procfs".into(), running),
            ("procfs".into(), reused),
        ]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].1.start_ns, Some(13_002_345_678));
        assert_eq!(merged[0].1.cmdline, ["sway"]);
        assert_eq!(merged[1].1.comm, "sh");
    }

    #[test]
    fn reads_services_from_cgroups() {
        assert_eq!(
            service_from_cgroup("/system.slice/sshd.service").as_deref(),
            Some("sshd.service")
        );
        assert_eq!(service_from_cgroup("/openrc.sshd").as_deref(), Some("sshd"));
        assert_eq!(
            service_from_cgroup("/user.slice/user-1000.slice/session-2.scope"),
            None
        );
    }
}
