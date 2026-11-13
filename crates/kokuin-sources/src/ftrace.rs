//! The kernel's trace of a deep boot.
//!
//! On a deep boot, the kernel traces every program started (the
//! `sched:sched_process_exec` event, asked for on the kernel command line) on the
//! monotonic clock, from the very beginning. The recorder saves that trace in
//! `/run/kokuin/recorder/trace`. It names even the processes that exited before the
//! recorder could read their `/proc` entry.
//!
//! A line of the trace, as tracefs prints it:
//!
//! ```text
//!   sh-1352  [001] .....  2.034567: sched_process_exec: filename=/sbin/openrc pid=1352 old_pid=1352
//! ```

use crate::ProcessInfo;

/// The trace saved by the recorder on a deep boot.
pub const PATH: &str = "/run/kokuin/recorder/trace";

/// A program started, as traced by the kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exec {
    /// Boot clock, in ns.
    pub t_ns: i64,
    /// The process.
    pub pid: u32,
    /// The program file, as given to `exec`.
    pub filename: String,
}

/// The programs started, from the text of a trace. Other lines are skipped.
pub fn parse_execs(trace: &str) -> Vec<Exec> {
    trace.lines().filter_map(parse_exec).collect()
}

fn parse_exec(line: &str) -> Option<Exec> {
    let (before, fields) = line.split_once(": sched_process_exec: ")?;
    let timestamp = before.split_whitespace().last()?.trim_end_matches(':');
    let rest = fields.strip_prefix("filename=")?;
    // The file name may hold spaces: the fields after it are the last two.
    let (filename, numbers) = rest.rsplit_once(" pid=")?;
    let pid = numbers.split_whitespace().next()?.parse().ok()?;
    Some(Exec {
        t_ns: seconds_to_ns(timestamp)?,
        pid,
        filename: filename.to_owned(),
    })
}

/// `2.034567` (seconds) in ns, without the rounding of floating point.
fn seconds_to_ns(text: &str) -> Option<i64> {
    let (seconds, fraction) = text.split_once('.').unwrap_or((text, ""));
    let mut digits = fraction.to_owned();
    if digits.len() > 9 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    while digits.len() < 9 {
        digits.push('0');
    }
    Some(seconds.parse::<i64>().ok()? * 1_000_000_000 + digits.parse::<i64>().ok()?)
}

/// Names the processes that have no name yet (they exited before the recorder read
/// them) with the last program the trace shows them starting during their life.
/// Returns how many were named.
pub fn name_processes(processes: &mut [ProcessInfo], execs: &[Exec]) -> usize {
    /// Process events and the trace are on the same clock, but not stamped at the
    /// very same instant.
    const SLACK_NS: i64 = 1_000_000;
    let mut named = 0;
    for process in processes.iter_mut().filter(|p| p.comm.is_empty()) {
        let during_its_life = |exec: &&Exec| {
            exec.pid == process.pid
                && process
                    .start_ns
                    .is_none_or(|start| exec.t_ns >= start - SLACK_NS)
                && process.end_ns.is_none_or(|end| exec.t_ns <= end + SLACK_NS)
        };
        if let Some(exec) = execs.iter().rfind(during_its_life) {
            let name = exec.filename.rsplit('/').next().unwrap_or(&exec.filename);
            // The kernel names a process after its program file, cut to 15 bytes.
            process.comm = name.chars().take(15).collect();
            if exec.filename.starts_with('/') {
                process.exe = Some(exec.filename.clone());
            }
            named += 1;
        }
    }
    named
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACE: &str = "\
# tracer: nop
#
#           TASK-PID     CPU#  |||||  TIMESTAMP  FUNCTION
#              | |         |   |||||     |         |
            init-1       [000] .....     2.030112: sched_process_exec: filename=/sbin/init pid=1 old_pid=1
              sh-1352    [001] .....     2.034567: sched_process_exec: filename=/usr/libexec/my tool pid=1352 old_pid=1352
          <idle>-0       [000] d.h..     2.040000: some_other_event: x=1
";

    #[test]
    fn reads_the_programs_started() {
        let execs = parse_execs(TRACE);
        assert_eq!(
            execs,
            [
                Exec {
                    t_ns: 2_030_112_000,
                    pid: 1,
                    filename: "/sbin/init".into()
                },
                Exec {
                    t_ns: 2_034_567_000,
                    pid: 1352,
                    filename: "/usr/libexec/my tool".into()
                },
            ]
        );
    }

    #[test]
    fn names_only_unnamed_processes_of_the_same_life() {
        let unnamed = |pid, start_ns, end_ns| ProcessInfo {
            pid,
            start_ns: Some(start_ns),
            end_ns: Some(end_ns),
            ..ProcessInfo::default()
        };
        let mut processes = vec![
            unnamed(1352, 2_034_000_000, 2_036_000_000),
            // The same PID, later: the trace says nothing about this one.
            unnamed(1352, 5_000_000_000, 5_100_000_000),
            ProcessInfo {
                pid: 1,
                comm: "init".into(),
                ..ProcessInfo::default()
            },
        ];
        let named = name_processes(&mut processes, &parse_execs(TRACE));
        assert_eq!(named, 1);
        assert_eq!(processes[0].comm, "my tool");
        assert_eq!(processes[0].exe.as_deref(), Some("/usr/libexec/my tool"));
        assert_eq!(processes[1].comm, "");
    }

    #[test]
    fn converts_seconds_exactly() {
        assert_eq!(seconds_to_ns("2.034567"), Some(2_034_567_000));
        assert_eq!(seconds_to_ns("12"), Some(12_000_000_000));
        assert_eq!(seconds_to_ns("1.2x"), None);
    }
}
