//! Processes still running, from `/proc`. Used to rebuild a boot after the fact.

use kokuin_model::{Confidence, SourceStatus};

use crate::{Context, ProcessInfo, Source, SourceOutput, report};

const EXPLAIN: &str = "init.procfs";
/// `PF_KTHREAD` in the flags of `/proc/<pid>/stat`.
const KERNEL_THREAD: u64 = 0x0020_0000;

/// Running processes.
pub struct Procfs;

impl Source for Procfs {
    fn id(&self) -> &'static str {
        "procfs"
    }

    fn explain_id(&self) -> &'static str {
        EXPLAIN
    }

    fn kernel_options(&self) -> &'static [&'static str] {
        &["CONFIG_PROC_FS"]
    }

    fn collect(&self, ctx: &Context) -> SourceOutput {
        let pids = match ctx.list_dir("/proc") {
            Ok(entries) => entries
                .into_iter()
                .filter_map(|name| name.parse::<u32>().ok()),
            Err(err) => {
                return SourceOutput::empty(report::io_error(
                    self.id(),
                    EXPLAIN,
                    &ctx.path("/proc"),
                    &err,
                ));
            }
        };
        let own_pid = ctx.is_live().then(std::process::id);
        let ticks = ctx.clock_ticks_per_second();
        let mut processes: Vec<ProcessInfo> = pids
            .filter(|&pid| Some(pid) != own_pid)
            .filter_map(|pid| read_process(ctx, pid, ticks))
            .collect();
        processes.sort_by_key(|p| (p.start_ns, p.pid));

        let reason = format!(
            "only the {} processes still running are visible: those that already exited left no trace \
             (record a boot to see them all)",
            processes.len()
        );
        SourceOutput {
            report: report::with_status(self.id(), EXPLAIN, SourceStatus::Partial, Some(reason)),
            observations: Vec::new(),
            processes,
        }
    }
}

/// Reads one process; `None` for kernel threads and processes that vanished.
fn read_process(ctx: &Context, pid: u32, ticks_per_second: u64) -> Option<ProcessInfo> {
    let stat = parse_stat(&ctx.read_text(format!("/proc/{pid}/stat")).ok()?)?;
    if stat.flags & KERNEL_THREAD != 0 {
        return None;
    }
    let cmdline: Vec<String> = ctx
        .read_bytes(format!("/proc/{pid}/cmdline"))
        .map(|bytes| {
            bytes
                .split(|&b| b == 0)
                .filter(|arg| !arg.is_empty())
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect()
        })
        .unwrap_or_default();
    let uid = ctx
        .read_text(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("Uid:"))?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        });
    let cgroup = ctx
        .read_text(format!("/proc/{pid}/cgroup"))
        .ok()
        .and_then(|text| {
            // cgroup v2: "0::/system.slice/sshd.service"
            text.lines()
                .find_map(|line| line.strip_prefix("0::"))
                .map(str::to_owned)
        });
    let start_ns = i64::try_from(stat.start_ticks * 1_000_000_000 / ticks_per_second.max(1)).ok();
    Some(ProcessInfo {
        pid,
        ppid: Some(stat.ppid).filter(|&ppid| ppid != 0),
        uid,
        comm: stat.comm,
        cmdline,
        exe: ctx
            .read_link(format!("/proc/{pid}/exe"))
            .ok()
            .map(|p| p.display().to_string()),
        start_ns,
        cgroup: cgroup.filter(|c| c != "/"),
        confidence: Some(Confidence::Approximate),
        ..ProcessInfo::default()
    })
}

struct Stat {
    comm: String,
    ppid: u32,
    flags: u64,
    start_ticks: u64,
}

/// Parses `/proc/<pid>/stat`. The command name is between parentheses and may itself
/// contain spaces and parentheses, so fields are counted from the last `)`.
fn parse_stat(text: &str) -> Option<Stat> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let comm = text.get(open + 1..close)?.to_owned();
    // After the name: state (field 3), ppid (4), ... flags (9), ... starttime (22).
    let fields: Vec<&str> = text.get(close + 1..)?.split_whitespace().collect();
    Some(Stat {
        comm,
        ppid: fields.get(1)?.parse().ok()?,
        flags: fields.get(6)?.parse().ok()?,
        start_ticks: fields.get(19)?.parse().ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stat_with_a_tricky_name() {
        let text =
            "612 (my (odd) prog) S 455 612 612 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 580 1000 50";
        let stat = parse_stat(text).unwrap();
        assert_eq!(stat.comm, "my (odd) prog");
        assert_eq!(stat.ppid, 455);
        assert_eq!(stat.flags, 4194560);
        assert_eq!(stat.start_ticks, 580);
    }

    #[test]
    fn reads_live_processes() {
        let output = Procfs.collect(&Context::live());
        assert!(output.processes.iter().any(|p| p.pid == 1));
        assert!(output.processes.iter().all(|p| p.start_ns.is_some()));
    }
}
