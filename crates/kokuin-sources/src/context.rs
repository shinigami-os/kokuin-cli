use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    time::UNIX_EPOCH,
};

use rustix::time::{ClockId, clock_gettime};

/// Where the sources read from: the running system, or a directory laid out like one.
#[derive(Debug, Clone)]
pub struct Context {
    root: PathBuf,
    live: bool,
}

impl Context {
    /// The running system.
    pub fn live() -> Self {
        Self {
            root: PathBuf::from("/"),
            live: true,
        }
    }

    /// A directory laid out like a system (`<root>/proc`, `<root>/sys`...), for tests
    /// and for analyzing data copied from another machine. Commands are not run and
    /// clocks are not read in this mode.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            live: false,
        }
    }

    /// Whether this is the running system.
    pub fn is_live(&self) -> bool {
        self.live
    }

    /// Resolves an absolute system path (`/proc/cmdline`) inside the root.
    pub fn path(&self, absolute: impl AsRef<Path>) -> PathBuf {
        let absolute = absolute.as_ref();
        self.root
            .join(absolute.strip_prefix("/").unwrap_or(absolute))
    }

    /// Whether a system path exists.
    pub fn exists(&self, absolute: impl AsRef<Path>) -> bool {
        self.path(absolute).exists()
    }

    /// Reads a text file, without its trailing newline.
    pub fn read_text(&self, absolute: impl AsRef<Path>) -> io::Result<String> {
        let mut text = fs::read_to_string(self.path(absolute))?;
        let trimmed_len = text.trim_end_matches(['\n', '\0']).len();
        text.truncate(trimmed_len);
        Ok(text)
    }

    /// Reads a binary file.
    pub fn read_bytes(&self, absolute: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        fs::read(self.path(absolute))
    }

    /// Reads the target of a symbolic link.
    pub fn read_link(&self, absolute: impl AsRef<Path>) -> io::Result<PathBuf> {
        fs::read_link(self.path(absolute))
    }

    /// Lists the names of the entries of a directory, sorted.
    pub fn list_dir(&self, absolute: impl AsRef<Path>) -> io::Result<Vec<String>> {
        let mut names: Vec<String> = fs::read_dir(self.path(absolute))?
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect();
        names.sort();
        Ok(names)
    }

    /// Modification time of a file or link itself (not its target), in ns since the
    /// Unix epoch.
    pub fn link_mtime_unix_ns(&self, absolute: impl AsRef<Path>) -> io::Result<i64> {
        let modified = fs::symlink_metadata(self.path(absolute))?.modified()?;
        let since_epoch = modified
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?;
        i64::try_from(since_epoch.as_nanos()).map_err(io::Error::other)
    }

    /// Runs a command and returns its standard output. Only on the live system.
    pub fn run(&self, program: &str, args: &[&str]) -> io::Result<String> {
        if !self.live {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("`{program}` can only be run on the live system"),
            ));
        }
        let output = Command::new(program).args(args).output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(io::Error::other(format!(
                "`{program}` failed: {}",
                stderr.trim()
            )));
        }
        String::from_utf8(output.stdout).map_err(io::Error::other)
    }

    /// Clock ticks per second, the unit of process start times in `/proc`.
    pub fn clock_ticks_per_second(&self) -> u64 {
        if self.live {
            rustix::param::clock_ticks_per_second()
        } else {
            100
        }
    }

    /// Wall-clock time of the boot clock origin (kernel start), in ns since the Unix
    /// epoch.
    ///
    /// Live, it is computed from the clocks with nanosecond precision. Otherwise it
    /// comes from `btime` in `/proc/stat`, with a one-second precision.
    pub fn boot_epoch_unix_ns(&self) -> Option<i64> {
        if self.live {
            let realtime = clock_gettime(ClockId::Realtime);
            let boottime = clock_gettime(ClockId::Boottime);
            let ns = |t: rustix::time::Timespec| t.tv_sec * 1_000_000_000 + t.tv_nsec;
            return Some(ns(realtime) - ns(boottime));
        }
        let stat = self.read_text("/proc/stat").ok()?;
        let btime = stat.lines().find_map(|line| line.strip_prefix("btime "))?;
        btime.trim().parse::<i64>().ok().map(|s| s * 1_000_000_000)
    }

    /// Converts a wall-clock time (ns since the Unix epoch) to the boot clock.
    pub fn wall_to_boot_ns(&self, unix_ns: i64) -> Option<i64> {
        Some(unix_ns - self.boot_epoch_unix_ns()?)
    }
}

/// Formats a wall-clock time (ns since the Unix epoch) as RFC 3339 in the local
/// time zone, with milliseconds: `2026-11-12T08:44:58.120+01:00`.
pub fn format_wall_clock(unix_ns: i64) -> String {
    match jiff::Timestamp::from_nanosecond(i128::from(unix_ns)) {
        Ok(timestamp) => timestamp
            .to_zoned(jiff::tz::TimeZone::system())
            .strftime("%Y-%m-%dT%H:%M:%S%.3f%:z")
            .to_string(),
        Err(_) => unix_ns.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_absolute_paths_inside_the_root() {
        let ctx = Context::with_root("/tmp/fixture");
        assert_eq!(
            ctx.path("/proc/cmdline"),
            PathBuf::from("/tmp/fixture/proc/cmdline")
        );
    }

    #[test]
    fn live_boot_epoch_is_in_the_past() {
        let now = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap();
        let boot = Context::live().boot_epoch_unix_ns().unwrap();
        assert!(boot > 0 && i128::from(boot) < now.as_nanos() as i128);
    }
}
