//! runit: service states from the `supervise/status` files of `runsv`.

use kokuin_model::{Confidence, EventKind, SourceStatus};

use crate::{Context, Observation, Source, SourceOutput, report};

const EXPLAIN: &str = "init.runit";
/// Directories `runsvdir` is usually given, when the running one cannot be found.
const KNOWN_SERVICE_DIRS: [&str; 5] = [
    "/run/runit/service",
    "/var/service",
    "/etc/service",
    "/service",
    "/etc/sv",
];
/// TAI64 label of the Unix epoch, as used by runit (2^62 + 10 s).
const TAI64_UNIX_EPOCH: u64 = 4_611_686_018_427_387_914;

/// runit's view of the services.
pub struct Runit;

impl Source for Runit {
    fn id(&self) -> &'static str {
        "runit"
    }

    fn explain_id(&self) -> &'static str {
        EXPLAIN
    }

    fn collect(&self, ctx: &Context) -> SourceOutput {
        let Some(dir) = service_dir(ctx) else {
            return SourceOutput::empty(report::with_status(
                self.id(),
                EXPLAIN,
                SourceStatus::Unavailable,
                Some("no runsvdir service directory found".into()),
            ));
        };
        let services = ctx.list_dir(&dir).unwrap_or_default();
        let mut observations = Vec::new();
        let mut unreadable = 0;
        for name in &services {
            match ctx.read_bytes(format!("{dir}/{name}/supervise/status")) {
                Ok(bytes) => {
                    if let Some(status) = parse_status(&bytes) {
                        observations.push(observation(ctx, name, &status));
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => unreadable += 1,
                Err(_) => {}
            }
        }
        let report = if unreadable > 0 {
            report::with_status(
                self.id(),
                EXPLAIN,
                SourceStatus::Partial,
                Some(format!(
                    "reading the state of {unreadable} services in {dir} needs root"
                )),
            )
        } else {
            report::ok(self.id(), EXPLAIN)
        };
        SourceOutput {
            report,
            observations,
            processes: Vec::new(),
        }
    }
}

/// The directory the running `runsvdir` supervises, from its command line.
fn service_dir(ctx: &Context) -> Option<String> {
    let from_process = ctx.list_dir("/proc").ok()?.into_iter().find_map(|pid| {
        let comm = ctx.read_text(format!("/proc/{pid}/comm")).ok()?;
        if comm.trim() != "runsvdir" {
            return None;
        }
        let cmdline = ctx.read_bytes(format!("/proc/{pid}/cmdline")).ok()?;
        cmdline
            .split(|&b| b == 0)
            .skip(1)
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .find(|arg| !arg.is_empty() && !arg.starts_with('-'))
    });
    from_process.or_else(|| {
        KNOWN_SERVICE_DIRS
            .iter()
            .find(|dir| ctx.exists(dir))
            .map(|dir| (*dir).to_owned())
    })
}

/// Contents of a `supervise/status` file.
#[derive(Debug, PartialEq)]
struct Status {
    /// When the service entered its current state, in ns since the Unix epoch.
    since_unix_ns: i64,
    pid: u32,
    state: &'static str,
    want_up: bool,
}

/// Parses the 20-byte status: TAI64N timestamp (12 bytes, big-endian), pid (4 bytes,
/// little-endian), paused, want (`u`/`d`), terminated, state (0 down, 1 run, 2 finish).
fn parse_status(bytes: &[u8]) -> Option<Status> {
    let bytes: &[u8; 20] = bytes.get(..20)?.try_into().ok()?;
    let seconds = u64::from_be_bytes(bytes[0..8].try_into().ok()?).checked_sub(TAI64_UNIX_EPOCH)?;
    let nanoseconds = u32::from_be_bytes(bytes[8..12].try_into().ok()?);
    Some(Status {
        since_unix_ns: i64::try_from(seconds).ok()? * 1_000_000_000 + i64::from(nanoseconds),
        pid: u32::from_le_bytes(bytes[12..16].try_into().ok()?),
        state: match bytes[19] {
            1 => "run",
            2 => "finish",
            _ => "down",
        },
        want_up: bytes[17] == b'u',
    })
}

fn observation(ctx: &Context, name: &str, status: &Status) -> Observation {
    let title = match status.state {
        "run" => format!("Service {name} started"),
        "finish" => format!("Service {name} is finishing"),
        _ => format!("Service {name} stopped"),
    };
    let t_ns = ctx.wall_to_boot_ns(status.since_unix_ns);
    let confidence = if t_ns.is_some() {
        Confidence::Approximate
    } else {
        Confidence::Inferred
    };
    let mut observation = Observation::new(EventKind::Service, title, t_ns, confidence)
        .explained_by(EXPLAIN)
        .with("service", name)
        .with("state", status.state)
        .with("want", if status.want_up { "up" } else { "down" });
    if status.pid != 0 {
        observation.pid = Some(status.pid);
        observation = observation.with("pid", status.pid);
    }
    observation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_status_file() {
        let mut bytes = (TAI64_UNIX_EPOCH + 1_700_000_000).to_be_bytes().to_vec();
        bytes.extend(500_000_000u32.to_be_bytes());
        bytes.extend(455u32.to_le_bytes());
        bytes.extend([0, b'u', 0, 1]);
        let status = parse_status(&bytes).unwrap();
        assert_eq!(
            status,
            Status {
                since_unix_ns: 1_700_000_000_500_000_000,
                pid: 455,
                state: "run",
                want_up: true
            }
        );
    }
}
