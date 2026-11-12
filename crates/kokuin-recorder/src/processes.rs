//! Turns what the kernel reports into the lines of `processes.jsonl`: kernel events
//! when the kernel sends them, `/proc` listings compared over time otherwise.

use std::collections::{HashMap, HashSet};

use kokuin_sources::{
    Context, ProcessInfo,
    proc_events::{Line, ProcessDetails},
    procfs,
};

use crate::cn_proc::{Event, exit_status};

/// PID of `kthreadd`, the parent of every kernel thread.
const KTHREADD: u32 = 2;

/// A line, and what is known of the program it is about (to recognize it).
pub type Seen = (Line, Option<ProcessInfo>);

/// The processes running now, except kernel threads and the recorder itself.
pub fn running(ctx: &Context) -> Vec<ProcessInfo> {
    let ticks = ctx.clock_ticks_per_second();
    let own = ctx.own_pid();
    let mut processes: Vec<ProcessInfo> = ctx
        .list_dir("/proc")
        .unwrap_or_default()
        .iter()
        .filter_map(|name| name.parse::<u32>().ok())
        .filter(|&pid| Some(pid) != own)
        .filter_map(|pid| procfs::read_process(ctx, pid, ticks))
        .collect();
    processes.sort_by_key(|p| (p.start_ns, p.pid));
    processes
}

/// The line for a process already running when the recording starts.
pub fn running_line(info: &ProcessInfo) -> Line {
    Line::Running {
        start_ns: info.start_ns,
        pid: info.pid,
        ppid: info.ppid,
        details: ProcessDetails::of(info),
    }
}

/// Follows the kernel's process events.
pub struct Tracker {
    ctx: Context,
    ticks: u64,
    kernel_threads: HashSet<u32>,
    /// Processes listed as running, and when the listing was made: events about
    /// their creation that arrive later are not new.
    listed: HashMap<u32, i64>,
}

impl Tracker {
    /// `listed` are the processes listed as running at `listed_at_ns`.
    pub fn new(ctx: Context, listed: &[ProcessInfo], listed_at_ns: i64) -> Self {
        Self {
            ticks: ctx.clock_ticks_per_second(),
            ctx,
            kernel_threads: HashSet::new(),
            listed: listed.iter().map(|p| (p.pid, listed_at_ns)).collect(),
        }
    }

    /// The line for one kernel event, if it is about a process (not a thread, not a
    /// kernel thread).
    pub fn line(&mut self, event: Event) -> Option<Seen> {
        match event {
            Event::Fork {
                t_ns,
                parent_tgid,
                child_pid,
                child_tgid,
            } => {
                if child_pid != child_tgid {
                    return None; // a new thread
                }
                if parent_tgid == KTHREADD || child_tgid == KTHREADD {
                    self.kernel_threads.insert(child_tgid);
                    return None;
                }
                if self.listed.get(&child_tgid).is_some_and(|&at| t_ns <= at) {
                    return None;
                }
                let line = Line::Fork {
                    t_ns,
                    pid: child_tgid,
                    ppid: parent_tgid,
                };
                Some((line, None))
            }
            Event::Exec { t_ns, tgid } => {
                // Read it now, while it runs: it may exit at any moment.
                let info = procfs::read_process(&self.ctx, tgid, self.ticks);
                let details = info.as_ref().map(ProcessDetails::of).unwrap_or_default();
                let line = Line::Exec {
                    t_ns,
                    pid: tgid,
                    details,
                };
                Some((line, info))
            }
            Event::Exit {
                t_ns,
                pid,
                tgid,
                status,
            } => {
                if pid != tgid || self.kernel_threads.remove(&tgid) {
                    return None;
                }
                let (exit_code, exit_signal) = exit_status(status);
                let line = Line::Exit {
                    t_ns,
                    pid: tgid,
                    exit_code,
                    exit_signal,
                };
                Some((line, None))
            }
            Event::Ack { .. } | Event::Other => None,
        }
    }
}

/// Compares `/proc` listings, for kernels without process events.
pub struct Poller {
    ctx: Context,
    /// Start time of each known process, to tell a reused PID from the same process.
    known: HashMap<u32, Option<i64>>,
}

impl Poller {
    /// `listed` are the processes listed as running when the recording started.
    pub fn new(ctx: Context, listed: &[ProcessInfo]) -> Self {
        Self {
            ctx,
            known: listed.iter().map(|p| (p.pid, p.start_ns)).collect(),
        }
    }

    /// What changed since the last listing, which was before `now_ns`.
    pub fn poll(&mut self, now_ns: i64) -> Vec<Seen> {
        let current = running(&self.ctx);
        let mut seen = Vec::new();
        let alive: HashSet<u32> = current.iter().map(|p| p.pid).collect();
        let mut ended: Vec<u32> = self
            .known
            .keys()
            .copied()
            .filter(|pid| !alive.contains(pid))
            .collect();
        ended.sort_unstable();
        for pid in ended {
            self.known.remove(&pid);
            let line = Line::Exit {
                t_ns: now_ns,
                pid,
                exit_code: None,
                exit_signal: None,
            };
            seen.push((line, None));
        }
        for info in current {
            if self.known.get(&info.pid) == Some(&info.start_ns) {
                continue;
            }
            if self.known.contains_key(&info.pid) {
                // The PID was reused: the old process ended.
                let line = Line::Exit {
                    t_ns: now_ns,
                    pid: info.pid,
                    exit_code: None,
                    exit_signal: None,
                };
                seen.push((line, None));
            }
            self.known.insert(info.pid, info.start_ns);
            let t_ns = info.start_ns.unwrap_or(now_ns);
            let fork = Line::Fork {
                t_ns,
                pid: info.pid,
                ppid: info.ppid.unwrap_or(0),
            };
            let exec = Line::Exec {
                t_ns,
                pid: info.pid,
                details: ProcessDetails::of(&info),
            };
            seen.push((fork, None));
            seen.push((exec, Some(info)));
        }
        seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> Tracker {
        Tracker::new(Context::with_root("/nonexistent"), &[], 0)
    }

    #[test]
    fn threads_and_kernel_threads_are_not_processes() {
        let mut tracker = tracker();
        let thread = Event::Fork {
            t_ns: 10,
            parent_tgid: 500,
            child_pid: 501,
            child_tgid: 500,
        };
        assert!(tracker.line(thread).is_none());
        let kworker = Event::Fork {
            t_ns: 10,
            parent_tgid: KTHREADD,
            child_pid: 90,
            child_tgid: 90,
        };
        assert!(tracker.line(kworker).is_none());
        let kworker_exit = Event::Exit {
            t_ns: 20,
            pid: 90,
            tgid: 90,
            status: 0,
        };
        assert!(tracker.line(kworker_exit).is_none());
    }

    #[test]
    fn a_process_is_created_runs_and_exits() {
        let mut tracker = tracker();
        let fork = Event::Fork {
            t_ns: 10,
            parent_tgid: 1,
            child_pid: 600,
            child_tgid: 600,
        };
        assert_eq!(
            tracker.line(fork).unwrap().0,
            Line::Fork {
                t_ns: 10,
                pid: 600,
                ppid: 1
            }
        );
        // The process is already gone: its program cannot be read.
        let (exec, info) = tracker
            .line(Event::Exec {
                t_ns: 11,
                tgid: 600,
            })
            .unwrap();
        assert!(info.is_none());
        assert_eq!(
            exec,
            Line::Exec {
                t_ns: 11,
                pid: 600,
                details: ProcessDetails::default()
            }
        );
        let exit = Event::Exit {
            t_ns: 12,
            pid: 600,
            tgid: 600,
            status: 1 << 8,
        };
        assert_eq!(
            tracker.line(exit).unwrap().0,
            Line::Exit {
                t_ns: 12,
                pid: 600,
                exit_code: Some(1),
                exit_signal: None
            }
        );
    }

    #[test]
    fn a_listed_process_is_not_created_again() {
        let listed = ProcessInfo {
            pid: 700,
            ..ProcessInfo::default()
        };
        let mut tracker = Tracker::new(Context::with_root("/nonexistent"), &[listed], 50);
        let fork = Event::Fork {
            t_ns: 40,
            parent_tgid: 1,
            child_pid: 700,
            child_tgid: 700,
        };
        assert!(tracker.line(fork).is_none());
    }

    #[test]
    fn polling_sees_a_process_start_and_end() {
        let ctx = Context::live();
        let mut poller = Poller::new(ctx.clone(), &running(&ctx));
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let pid = child.id();
        let started = poller.poll(1);
        child.kill().unwrap();
        child.wait().unwrap();
        let ended = poller.poll(2);

        let exec = started.iter().find_map(|(line, _)| match line {
            Line::Exec {
                pid: p, details, ..
            } if *p == pid => Some(details),
            _ => None,
        });
        assert_eq!(exec.and_then(|d| d.comm.as_deref()), Some("sleep"));
        assert!(ended.iter().any(|(line, _)| matches!(
            line,
            Line::Exit { pid: p, t_ns: 2, .. } if *p == pid
        )));
    }

    #[test]
    fn running_skips_the_recorder_itself() {
        let processes = running(&Context::live());
        assert!(!processes.iter().any(|p| p.pid == std::process::id()));
        assert!(processes.iter().any(|p| p.pid == 1));
    }
}
