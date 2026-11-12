//! Processes seen by the boot recorder (`kokuin-cli record --boot`).
//!
//! While the system boots, the recorder writes one JSON object per line ([`Line`]) to
//! `/run/kokuin/recorder/processes.jsonl`: every process created, every program run,
//! every exit, with the time on the boot clock. Unlike `/proc`, this still knows the
//! processes that are gone.

use std::collections::BTreeMap;

use kokuin_model::{Confidence, EndReason, EventKind, MilestoneKind, SourceStatus};
use serde::{Deserialize, Serialize};

use crate::{Context, Observation, ProcessInfo, Source, SourceOutput, report};

/// Where the recorder publishes what it saw.
pub const RECORDER_DIR: &str = "/run/kokuin/recorder";
/// The process events written by the recorder.
pub const PATH: &str = "/run/kokuin/recorder/processes.jsonl";
const EXPLAIN: &str = "init.proc_events";

/// One line of `processes.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Line {
    /// The recorder started watching. The processes already running follow, as
    /// [`Line::Running`].
    Started {
        /// Boot clock, in ns.
        t_ns: i64,
        /// How processes are watched.
        method: WatchMethod,
        /// When kokuin-init handed over to the real init: it starts the recorder
        /// right before, so this is the recorder's own start (10 ms precision).
        /// Missing when the recorder was not started by kokuin-init.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        handover_ns: Option<i64>,
    },
    /// A process that was already running when the recorder started.
    Running {
        /// Its start time, from `/proc` (10 ms precision).
        start_ns: Option<i64>,
        /// PID.
        pid: u32,
        /// Parent PID.
        ppid: Option<u32>,
        /// What it runs.
        #[serde(flatten)]
        details: ProcessDetails,
    },
    /// A process was created (it runs the same program as its parent until it
    /// executes another).
    Fork {
        /// Boot clock, in ns.
        t_ns: i64,
        /// PID of the new process.
        pid: u32,
        /// PID of its parent.
        ppid: u32,
    },
    /// A process started another program.
    Exec {
        /// Boot clock, in ns.
        t_ns: i64,
        /// PID.
        pid: u32,
        /// The program, read while the process was alive.
        #[serde(flatten)]
        details: ProcessDetails,
    },
    /// A process ended.
    Exit {
        /// Boot clock, in ns.
        t_ns: i64,
        /// PID.
        pid: u32,
        /// Exit code, when it exited by itself.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        /// Signal that killed it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_signal: Option<i32>,
    },
    /// The kernel dropped events because the recorder did not read them fast enough.
    Lost {
        /// Boot clock, in ns.
        t_ns: i64,
    },
    /// The recorder stopped.
    Stopped {
        /// Boot clock, in ns.
        t_ns: i64,
        /// Why.
        reason: EndReason,
    },
}

/// What a process runs. Every field may be missing: a process can exit before the
/// recorder reads it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProcessDetails {
    /// Short command name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comm: Option<String>,
    /// Command line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cmdline: Vec<String>,
    /// Executable path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
    /// User id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uid: Option<u32>,
    /// Control group path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cgroup: Option<String>,
}

impl ProcessDetails {
    /// The details of a process read from `/proc`.
    pub fn of(info: &ProcessInfo) -> Self {
        Self {
            comm: Some(info.comm.clone()),
            cmdline: info.cmdline.clone(),
            exe: info.exe.clone(),
            uid: info.uid,
            cgroup: info.cgroup.clone(),
        }
    }
}

/// How the recorder watches processes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchMethod {
    /// The kernel reports each event, with its exact time (`CONFIG_PROC_EVENTS`).
    ProcEvents,
    /// The recorder compares `/proc` listings at short intervals: times are
    /// approximate and processes shorter than the interval are missed.
    Polling,
}

/// Processes seen by the boot recorder.
pub struct ProcEvents;

impl Source for ProcEvents {
    fn id(&self) -> &'static str {
        "proc_events"
    }

    fn explain_id(&self) -> &'static str {
        EXPLAIN
    }

    fn kernel_options(&self) -> &'static [&'static str] {
        &["CONFIG_CONNECTOR", "CONFIG_PROC_EVENTS"]
    }

    fn collect(&self, ctx: &Context) -> SourceOutput {
        let text = match ctx.read_text(PATH) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return SourceOutput::empty(report::with_status(
                    self.id(),
                    EXPLAIN,
                    SourceStatus::Unavailable,
                    Some(
                        "this boot was not recorded (enable the recorder with `kokuin-cli enable`)"
                            .into(),
                    ),
                ));
            }
            Err(err) => {
                return SourceOutput::empty(report::io_error(self.id(), EXPLAIN, PATH, &err));
            }
        };
        let mut lines = Vec::new();
        let mut unreadable = 0;
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            match serde_json::from_str::<Line>(line) {
                Ok(line) => lines.push(line),
                Err(_) => unreadable += 1,
            }
        }
        let replay = replay(&lines);

        let mut problems = Vec::new();
        if replay
            .started
            .is_some_and(|(_, method)| method == WatchMethod::Polling)
        {
            problems.push(
                "processes were polled (the kernel lacks CONFIG_PROC_EVENTS): times are \
                 approximate and very short processes are missing"
                    .to_owned(),
            );
        }
        if replay.lost {
            problems.push("the kernel dropped some process events".to_owned());
        }
        if unreadable > 0 {
            problems.push(format!("{unreadable} lines could not be read"));
        }
        let report = if problems.is_empty() {
            report::ok(self.id(), EXPLAIN)
        } else {
            report::with_status(
                self.id(),
                EXPLAIN,
                SourceStatus::Partial,
                Some(problems.join("; ")),
            )
        };
        let observations = recording_observations(&replay);
        SourceOutput {
            report,
            observations,
            processes: replay.processes,
        }
    }
}

/// The recorder starting and stopping, on the timeline.
fn recording_observations(replay: &Replay) -> Vec<Observation> {
    let mut observations = Vec::new();
    if let Some(t_ns) = replay.handover_ns {
        observations.push(
            Observation::new(
                EventKind::Recording,
                "kokuin-init started the recorder and handed over to the init system",
                Some(t_ns),
                Confidence::Approximate,
            )
            .explained_by("init.pid1")
            .marks(MilestoneKind::InitStart),
        );
    }
    if let Some((t_ns, method)) = replay.started {
        let how = match method {
            WatchMethod::ProcEvents => "with the kernel's process events",
            WatchMethod::Polling => "by polling /proc",
        };
        observations.push(
            Observation::new(
                EventKind::Recording,
                format!("kokuin's recorder started watching processes {how}"),
                Some(t_ns),
                Confidence::Exact,
            )
            .explained_by(EXPLAIN),
        );
    }
    if let Some((t_ns, reason)) = replay.stopped {
        let title = match reason {
            EndReason::SessionReady => {
                "kokuin's recorder stopped: the desktop was ready and settled"
            }
            EndReason::Timeout => "kokuin's recorder stopped at its time limit",
            _ => "kokuin's recorder stopped",
        };
        let reason = serde_json::to_value(reason).unwrap_or_default();
        observations.push(
            Observation::new(EventKind::Recording, title, Some(t_ns), Confidence::Exact)
                .explained_by(EXPLAIN)
                .marks(MilestoneKind::RecordingEnd)
                .with("reason", reason),
        );
    }
    observations
}

/// Why the recorder stopped, from the event marking the end of a recording.
pub fn end_reason(event: &kokuin_model::Event) -> Option<EndReason> {
    serde_json::from_value(event.data.get("reason")?.clone()).ok()
}

/// What the lines tell, once replayed in order.
#[derive(Debug, Default)]
pub struct Replay {
    /// One process per PID lifetime, with the last program it ran.
    pub processes: Vec<ProcessInfo>,
    /// When the recorder started, and how it watched processes.
    pub started: Option<(i64, WatchMethod)>,
    /// When kokuin-init handed over to the real init.
    pub handover_ns: Option<i64>,
    /// Whether events were lost.
    pub lost: bool,
    /// When and why the recorder stopped.
    pub stopped: Option<(i64, EndReason)>,
}

/// Rebuilds the processes from the lines.
pub fn replay(lines: &[Line]) -> Replay {
    let mut replay = Replay::default();
    let mut confidence = Confidence::Exact;
    // Index in `processes` of the live process with each PID.
    let mut alive: BTreeMap<u32, usize> = BTreeMap::new();
    let mut processes: Vec<ProcessInfo> = Vec::new();

    for line in lines {
        match line {
            Line::Started {
                t_ns,
                method,
                handover_ns,
            } => {
                replay.started = Some((*t_ns, *method));
                replay.handover_ns = *handover_ns;
                if *method == WatchMethod::Polling {
                    confidence = Confidence::Approximate;
                }
            }
            Line::Running {
                start_ns,
                pid,
                ppid,
                details,
            } => {
                let mut info = ProcessInfo {
                    pid: *pid,
                    ppid: *ppid,
                    start_ns: *start_ns,
                    confidence: Some(Confidence::Approximate),
                    ..ProcessInfo::default()
                };
                apply(&mut info, details);
                alive.insert(*pid, processes.len());
                processes.push(info);
            }
            Line::Fork { t_ns, pid, ppid } => {
                // Until it runs another program, the child runs its parent's.
                let parent = alive.get(ppid).map(|&i| &processes[i]);
                let info = ProcessInfo {
                    pid: *pid,
                    ppid: Some(*ppid),
                    uid: parent.and_then(|p| p.uid),
                    comm: parent.map(|p| p.comm.clone()).unwrap_or_default(),
                    cmdline: parent.map(|p| p.cmdline.clone()).unwrap_or_default(),
                    exe: parent.and_then(|p| p.exe.clone()),
                    cgroup: parent.and_then(|p| p.cgroup.clone()),
                    start_ns: Some(*t_ns),
                    confidence: Some(confidence),
                    ..ProcessInfo::default()
                };
                alive.insert(*pid, processes.len());
                processes.push(info);
            }
            Line::Exec { t_ns, pid, details } => {
                let index = *alive.entry(*pid).or_insert_with(|| {
                    // Created before the recorder saw it: it starts here at the latest.
                    processes.push(ProcessInfo {
                        pid: *pid,
                        start_ns: Some(*t_ns),
                        confidence: Some(Confidence::Approximate),
                        ..ProcessInfo::default()
                    });
                    processes.len() - 1
                });
                let info = &mut processes[index];
                // A new program: what was inherited from the parent no longer holds.
                info.comm.clear();
                info.cmdline.clear();
                info.exe = None;
                apply(info, details);
            }
            Line::Exit {
                t_ns,
                pid,
                exit_code,
                exit_signal,
            } => {
                if let Some(index) = alive.remove(pid) {
                    let info = &mut processes[index];
                    info.end_ns = Some(*t_ns);
                    info.exit_code = *exit_code;
                    info.exit_signal = *exit_signal;
                }
            }
            Line::Lost { .. } => replay.lost = true,
            Line::Stopped { t_ns, reason } => replay.stopped = Some((*t_ns, *reason)),
        }
    }
    replay.processes = processes;
    replay
}

fn apply(info: &mut ProcessInfo, details: &ProcessDetails) {
    if let Some(comm) = &details.comm {
        info.comm.clone_from(comm);
    }
    if !details.cmdline.is_empty() {
        info.cmdline.clone_from(&details.cmdline);
    }
    if details.exe.is_some() {
        info.exe.clone_from(&details.exe);
    }
    if details.uid.is_some() {
        info.uid = details.uid;
    }
    if details.cgroup.is_some() {
        info.cgroup.clone_from(&details.cgroup);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
{"event":"started","t_ns":1500000000,"method":"proc_events","handover_ns":1490000000}
{"event":"running","start_ns":0,"pid":1,"comm":"init","cmdline":["/sbin/init"]}
{"event":"fork","t_ns":1600000000,"pid":200,"ppid":1}
{"event":"exec","t_ns":1600500000,"pid":200,"comm":"udevd","cmdline":["/sbin/udevd","--daemon"],"exe":"/sbin/udevd","uid":0}
{"event":"fork","t_ns":1700000000,"pid":201,"ppid":200}
{"event":"exit","t_ns":1750000000,"pid":201,"exit_code":0}
{"event":"fork","t_ns":1800000000,"pid":201,"ppid":1}
{"event":"exec","t_ns":1800100000,"pid":201}
{"event":"exit","t_ns":1900000000,"pid":201,"exit_signal":9}
{"event":"stopped","t_ns":20000000000,"reason":"session_ready"}
"#;

    fn sample() -> Vec<Line> {
        SAMPLE
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    #[test]
    fn rebuilds_one_process_per_pid_lifetime() {
        let replay = replay(&sample());
        assert_eq!(
            replay.started,
            Some((1_500_000_000, WatchMethod::ProcEvents))
        );
        assert_eq!(replay.handover_ns, Some(1_490_000_000));
        assert_eq!(
            replay.stopped,
            Some((20_000_000_000, EndReason::SessionReady))
        );
        let processes = &replay.processes;
        assert_eq!(processes.len(), 4);

        let udevd = &processes[1];
        assert_eq!((udevd.pid, udevd.comm.as_str()), (200, "udevd"));
        assert_eq!(udevd.start_ns, Some(1_600_000_000));
        assert_eq!(udevd.exe.as_deref(), Some("/sbin/udevd"));
        assert_eq!(udevd.confidence, Some(Confidence::Exact));

        // A child that never ran another program runs its parent's.
        let worker = &processes[2];
        assert_eq!((worker.pid, worker.comm.as_str()), (201, "udevd"));
        assert_eq!(worker.end_ns, Some(1_750_000_000));
        assert_eq!(worker.exit_code, Some(0));

        // The PID was reused; the program could not be read before it exited.
        let reused = &processes[3];
        assert_eq!((reused.pid, reused.ppid), (201, Some(1)));
        assert_eq!(reused.comm, "");
        assert!(reused.cmdline.is_empty());
        assert_eq!(reused.exit_signal, Some(9));
    }

    #[test]
    fn lines_survive_json() {
        for line in sample() {
            let json = serde_json::to_string(&line).unwrap();
            assert_eq!(serde_json::from_str::<Line>(&json).unwrap(), line, "{json}");
        }
    }

    #[test]
    fn a_boot_that_was_not_recorded_is_unavailable() {
        let output = ProcEvents.collect(&Context::with_root("/nonexistent"));
        assert_eq!(output.report.status, SourceStatus::Unavailable);
    }
}
