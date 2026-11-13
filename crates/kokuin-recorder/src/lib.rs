//! Records a boot while it happens.
//!
//! Started very early by `kokuin-init` (as `kokuin-cli record --boot`), the recorder
//! notes every process created, every program run and every exit, keeps the whole
//! kernel log, and stops once the desktop is ready and the system had time to settle
//! ([`Watch`]). What it saw becomes the `proc_events` and `kmsg` sources of the boot
//! record.
//!
//! Processes are followed with the kernel's process events ([`cn_proc`]), or by
//! comparing `/proc` listings when the kernel lacks them.

pub mod cn_proc;
pub mod config;
pub mod instance;
pub mod kmsg;
pub mod processes;
pub mod root;
pub mod watch;

use std::{thread, time::Duration};

use kokuin_kb::Kb;
use kokuin_model::EndReason;
use kokuin_sources::{
    Context, ProcessInfo,
    clock::ClockSample,
    proc_events::{Line, StartedFrom, WatchMethod},
    procfs,
};
use rustix::time::{ClockId, clock_gettime};

pub use crate::watch::Watch;
use crate::{
    cn_proc::{Listener, Received},
    config::RecorderConfig,
    kmsg::KmsgFollower,
    processes::{Poller, Seen, Tracker},
    root::RootFollower,
};

/// How long to wait for the kernel to confirm the subscription to process events.
const SUBSCRIPTION_WAIT: Duration = Duration::from_millis(500);
/// How long to wait for process events before checking whether to stop.
const EVENT_WAIT: Duration = Duration::from_millis(100);
/// Interval between two `/proc` listings without process events.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// What a recording saw.
#[derive(Debug, Clone)]
pub struct Recording {
    /// The lines of `processes.jsonl`.
    pub lines: Vec<Line>,
    /// The kernel log, in the `/dev/kmsg` format. Empty if it could not be read.
    pub kmsg: Vec<u8>,
    /// The measured gaps between the kernel log's clock and the boot clock.
    pub clock: Vec<ClockSample>,
    /// Why it stopped.
    pub end_reason: EndReason,
}

/// The boot clock: ns since the kernel started (`CLOCK_MONOTONIC`, the clock of
/// process events and of the kernel log).
pub fn now_ns() -> i64 {
    let t = clock_gettime(ClockId::Monotonic);
    t.tv_sec * 1_000_000_000 + t.tv_nsec
}

/// Records until the desktop is ready and settled, or until the timeout. Started
/// from the initramfs, it follows the switch to the real root filesystem.
pub fn record(config: &RecorderConfig, kb: &Kb, from: StartedFrom) -> Recording {
    let ctx = Context::live();
    let mut follower = match from {
        StartedFrom::Initramfs => RootFollower::new().ok(),
        StartedFrom::Init => None,
    };
    let kmsg = KmsgFollower::start().ok();
    let seconds = |s: u64| i64::try_from(s.saturating_mul(1_000_000_000)).unwrap_or(i64::MAX);
    let mut watch = Watch::new(seconds(config.settle_secs), seconds(config.timeout_secs));

    // Subscribe before listing what runs, so nothing falls in between.
    let listener = Listener::subscribe(SUBSCRIPTION_WAIT).ok();
    let method = if listener.is_some() {
        WatchMethod::ProcEvents
    } else {
        WatchMethod::Polling
    };
    let mut lines = vec![Line::Started {
        t_ns: now_ns(),
        method,
        handover_ns: match from {
            StartedFrom::Init => handover_ns(&ctx),
            StartedFrom::Initramfs => None,
        },
        from,
    }];
    let running = processes::running(&ctx);
    let listed_at = now_ns();
    for info in &running {
        observe(&mut watch, kb, info.start_ns.unwrap_or(listed_at), info);
        lines.push(processes::running_line(info));
    }
    let mut watcher = match listener {
        Some(mut listener) => {
            let _ = listener.set_wait(EVENT_WAIT);
            Watcher::Events(listener, Tracker::new(ctx.clone(), &running, listed_at))
        }
        None => Watcher::Polling(Poller::new(ctx.clone(), &running)),
    };

    // The last time PID 1 started a program: the initramfs's switch to the real
    // root ends with PID 1 starting the real init.
    let mut init_exec_ns = None;
    let end_reason = loop {
        // Before anything else: once the switch happened, the end of the recording
        // must be written in the real root.
        if let Some(root) = follower.as_mut().filter(|root| root.switched()) {
            let t_ns = init_exec_ns.unwrap_or_else(now_ns);
            if root.follow().is_ok() {
                lines.push(Line::RootSwitched { t_ns });
            }
            // Followed or not, there is only one switch to watch for.
            follower = None;
        }
        if let Some(reason) = watch.end_reason(now_ns()) {
            break reason;
        }
        for (line, info) in watcher.next() {
            if let Line::Exec { t_ns, pid: 1, .. } = &line {
                init_exec_ns = Some(*t_ns);
            }
            if let (Some(info), Line::Exec { t_ns, .. }) = (&info, &line) {
                observe(&mut watch, kb, *t_ns, info);
            }
            lines.push(line);
        }
    };
    lines.push(Line::Stopped {
        t_ns: now_ns(),
        reason: end_reason,
    });
    Recording {
        lines,
        kmsg: kmsg.as_ref().map(KmsgFollower::records).unwrap_or_default(),
        clock: kmsg
            .as_ref()
            .map(KmsgFollower::clock_samples)
            .unwrap_or_default(),
        end_reason,
    }
}

/// When kokuin-init handed over to the real init, if it started this recorder: it
/// does so right after starting it, so the recorder's own start time tells.
fn handover_ns(ctx: &Context) -> Option<i64> {
    if !rustix::process::getppid().is_some_and(|parent| parent.is_init()) {
        return None;
    }
    let own = ctx.own_pid()?;
    procfs::read_process(ctx, own, ctx.clock_ticks_per_second())?.start_ns
}

/// Tells the watch about a program, with its role in the knowledge base.
fn observe(watch: &mut Watch, kb: &Kb, t_ns: i64, info: &ProcessInfo) {
    let role = kb.match_process(info.names()).map(|rule| rule.role);
    watch.observe(t_ns, role, info.uid);
}

/// Where process changes come from.
enum Watcher {
    Events(Listener, Tracker),
    Polling(Poller),
}

impl Watcher {
    /// The next changes. Returns after a short wait even if nothing happened.
    fn next(&mut self) -> Vec<Seen> {
        match self {
            Watcher::Events(listener, tracker) => match listener.receive() {
                Ok(Received::Events(events)) => {
                    events.into_iter().filter_map(|e| tracker.line(e)).collect()
                }
                Ok(Received::Lost) => vec![(Line::Lost { t_ns: now_ns() }, None)],
                Ok(Received::Nothing) => Vec::new(),
                // The socket broke: wait before stopping at the deadline.
                Err(_) => {
                    thread::sleep(EVENT_WAIT);
                    Vec::new()
                }
            },
            Watcher::Polling(poller) => {
                thread::sleep(POLL_INTERVAL);
                poller.poll(now_ns())
            }
        }
    }
}
