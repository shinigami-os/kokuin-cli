use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use rustix::{
    fs::{Mode, OFlags},
    time::{ClockId, clock_gettime},
};

use crate::snapshot::{self, Recorder, Snapshot};

/// Where the sources read from: the running system, a directory laid out like one, or
/// a snapshot saved earlier.
#[derive(Debug, Clone)]
pub struct Context {
    root: PathBuf,
    origin: Origin,
}

#[derive(Debug, Clone)]
enum Origin {
    /// The running system, and maybe a snapshot being saved of what is read.
    Live(Option<Arc<Recorder>>),
    /// A directory laid out like a system.
    Directory,
    /// A snapshot: its files are in the root, the rest is in its description.
    Snapshot(Arc<Snapshot>),
}

impl Context {
    /// The running system.
    pub fn live() -> Self {
        Self {
            root: PathBuf::from("/"),
            origin: Origin::Live(None),
        }
    }

    /// The running system, saving everything that is read into the directory `dir`
    /// (created if needed, must be empty). Call [`Context::save_snapshot`] once done.
    pub fn recording(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let snapshot = Snapshot {
            format: snapshot::FORMAT,
            created_by: format!("kokuin-sources {}", env!("CARGO_PKG_VERSION")),
            captured_at_unix_ns: live_now_unix_ns(),
            pid: std::process::id(),
            boot_epoch_unix_ns: Some(live_boot_epoch_unix_ns()),
            clock_ticks_per_second: rustix::param::clock_ticks_per_second(),
            ..Snapshot::default()
        };
        let recorder = Recorder::create(dir.into(), snapshot)?;
        Ok(Self {
            root: PathBuf::from("/"),
            origin: Origin::Live(Some(Arc::new(recorder))),
        })
    }

    /// A snapshot saved by a recording context.
    pub fn from_snapshot(dir: impl AsRef<Path>) -> io::Result<Self> {
        let dir = dir.as_ref();
        let snapshot = Snapshot::load(dir)?;
        Ok(Self {
            root: dir.join(snapshot::ROOT_DIR),
            origin: Origin::Snapshot(Arc::new(snapshot)),
        })
    }

    /// A directory laid out like a system (`<root>/proc`, `<root>/sys`...), for tests.
    /// Commands are not run and clocks are not read in this mode.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            origin: Origin::Directory,
        }
    }

    /// Finishes the snapshot being saved, if any.
    pub fn save_snapshot(&self) -> io::Result<()> {
        match &self.origin {
            Origin::Live(Some(recorder)) => recorder.save(),
            _ => Ok(()),
        }
    }

    /// PID of the program reading the system: it is not part of the boot.
    pub fn own_pid(&self) -> Option<u32> {
        match &self.origin {
            Origin::Live(_) => Some(std::process::id()),
            Origin::Snapshot(snapshot) => Some(snapshot.pid),
            Origin::Directory => None,
        }
    }

    /// When the system is observed (now, or when the snapshot was saved), in ns since
    /// the Unix epoch.
    pub fn now_unix_ns(&self) -> Option<i64> {
        match &self.origin {
            Origin::Live(None) => Some(live_now_unix_ns()),
            Origin::Live(Some(recorder)) => Some(recorder.captured_at_unix_ns()),
            Origin::Snapshot(snapshot) => Some(snapshot.captured_at_unix_ns),
            Origin::Directory => None,
        }
    }

    /// Resolves an absolute system path (`/proc/cmdline`) inside the root.
    pub fn path(&self, absolute: impl AsRef<Path>) -> PathBuf {
        let absolute = absolute.as_ref();
        self.root
            .join(absolute.strip_prefix("/").unwrap_or(absolute))
    }

    /// Whether a system path exists.
    pub fn exists(&self, absolute: impl AsRef<Path>) -> bool {
        let absolute = absolute.as_ref();
        if let Some(&exists) = self.recorded(|s| s.exists.get(&snapshot::key(absolute))) {
            return exists;
        }
        let exists = self.path(absolute).exists();
        self.record(|recorder| {
            recorder.update(|s| {
                s.exists.insert(snapshot::key(absolute), exists);
            });
        });
        exists
    }

    /// Reads a text file, without its trailing newline.
    pub fn read_text(&self, absolute: impl AsRef<Path>) -> io::Result<String> {
        let bytes = self.read_bytes(absolute)?;
        let mut text = String::from_utf8(bytes)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        let trimmed_len = text.trim_end_matches(['\n', '\0']).len();
        text.truncate(trimmed_len);
        Ok(text)
    }

    /// Reads a binary file.
    pub fn read_bytes(&self, absolute: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        let absolute = absolute.as_ref();
        self.access(
            absolute,
            |_| None,
            || fs::read(self.path(absolute)),
            |recorder, bytes| recorder.file(absolute, bytes),
        )
    }

    /// Reads what a device hands out right now, without waiting for more: the records
    /// of `/dev/kmsg`, for example. Works on plain files too.
    pub fn read_device(&self, absolute: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        let absolute = absolute.as_ref();
        self.access(
            absolute,
            |_| None,
            || read_available(&self.path(absolute)),
            |recorder, bytes| recorder.file(absolute, bytes),
        )
    }

    /// Reads the target of a symbolic link.
    pub fn read_link(&self, absolute: impl AsRef<Path>) -> io::Result<PathBuf> {
        let absolute = absolute.as_ref();
        self.access(
            absolute,
            |s| s.links.get(&snapshot::key(absolute)).map(PathBuf::from),
            || fs::read_link(self.path(absolute)),
            |recorder, target| {
                recorder.update(|s| {
                    s.links
                        .insert(snapshot::key(absolute), snapshot::key(target));
                });
            },
        )
    }

    /// Lists the names of the entries of a directory, sorted.
    pub fn list_dir(&self, absolute: impl AsRef<Path>) -> io::Result<Vec<String>> {
        let absolute = absolute.as_ref();
        self.access(
            absolute,
            |s| s.listings.get(&snapshot::key(absolute)).cloned(),
            || {
                let mut names: Vec<String> = fs::read_dir(self.path(absolute))?
                    .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
                    .collect();
                names.sort();
                Ok(names)
            },
            |recorder, names| {
                recorder.update(|s| {
                    s.listings.insert(snapshot::key(absolute), names.clone());
                });
            },
        )
    }

    /// Modification time of a file or link itself (not its target), in ns since the
    /// Unix epoch.
    pub fn link_mtime_unix_ns(&self, absolute: impl AsRef<Path>) -> io::Result<i64> {
        let absolute = absolute.as_ref();
        self.access(
            absolute,
            |s| s.mtimes.get(&snapshot::key(absolute)).copied(),
            || {
                let modified = fs::symlink_metadata(self.path(absolute))?.modified()?;
                let since_epoch = modified
                    .duration_since(UNIX_EPOCH)
                    .map_err(io::Error::other)?;
                i64::try_from(since_epoch.as_nanos()).map_err(io::Error::other)
            },
            |recorder, &mtime| {
                recorder.update(|s| {
                    s.mtimes.insert(snapshot::key(absolute), mtime);
                });
            },
        )
    }

    /// Runs a command and returns its standard output. Only on the live system, or
    /// from a snapshot where the same command was run.
    pub fn run(&self, program: &str, args: &[&str]) -> io::Result<String> {
        match &self.origin {
            Origin::Live(recorder) => {
                let result = run_command(program, args);
                if let Some(recorder) = recorder {
                    recorder.update(|s| {
                        s.commands.push(snapshot::RecordedCommand {
                            program: program.to_owned(),
                            args: args.iter().map(|&a| a.to_owned()).collect(),
                            stdout: result.as_ref().ok().cloned(),
                            error: result.as_ref().err().map(ToString::to_string),
                        });
                    });
                }
                result
            }
            Origin::Snapshot(snapshot) => snapshot.command(program, args).unwrap_or_else(|| {
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("`{program}` was not run when the snapshot was saved"),
                ))
            }),
            Origin::Directory => Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("`{program}` can only be run on the live system"),
            )),
        }
    }

    /// Clock ticks per second, the unit of process start times in `/proc`.
    pub fn clock_ticks_per_second(&self) -> u64 {
        match &self.origin {
            Origin::Live(_) => rustix::param::clock_ticks_per_second(),
            Origin::Snapshot(snapshot) => snapshot.clock_ticks_per_second,
            Origin::Directory => 100,
        }
    }

    /// Wall-clock time of the boot clock origin (kernel start), in ns since the Unix
    /// epoch.
    ///
    /// Live, it is computed from the clocks with nanosecond precision, and a snapshot
    /// remembers it. Otherwise it comes from `btime` in `/proc/stat`, with a one-second
    /// precision.
    pub fn boot_epoch_unix_ns(&self) -> Option<i64> {
        match &self.origin {
            Origin::Live(_) => return Some(live_boot_epoch_unix_ns()),
            Origin::Snapshot(snapshot) if snapshot.boot_epoch_unix_ns.is_some() => {
                return snapshot.boot_epoch_unix_ns;
            }
            _ => {}
        }
        let stat = self.read_text("/proc/stat").ok()?;
        let btime = stat.lines().find_map(|line| line.strip_prefix("btime "))?;
        btime.trim().parse::<i64>().ok().map(|s| s * 1_000_000_000)
    }

    /// Converts a wall-clock time (ns since the Unix epoch) to the boot clock.
    pub fn wall_to_boot_ns(&self, unix_ns: i64) -> Option<i64> {
        Some(unix_ns - self.boot_epoch_unix_ns()?)
    }

    /// One access to a path: from the snapshot description when it has the answer
    /// (`replay`), else from the files (`read`), saved by the recorder if any (`save`).
    fn access<T>(
        &self,
        absolute: &Path,
        replay: impl FnOnce(&Snapshot) -> Option<T>,
        read: impl FnOnce() -> io::Result<T>,
        save: impl FnOnce(&Recorder, &T),
    ) -> io::Result<T> {
        if let Origin::Snapshot(snapshot) = &self.origin {
            if let Some(err) = snapshot.error(absolute) {
                return Err(err);
            }
            if let Some(value) = replay(snapshot) {
                return Ok(value);
            }
        }
        let result = read();
        self.record(|recorder| match &result {
            Ok(value) => save(recorder, value),
            Err(err) => recorder.error(absolute, err),
        });
        result
    }

    fn recorded<'a, T>(&'a self, get: impl FnOnce(&'a Snapshot) -> Option<T>) -> Option<T> {
        match &self.origin {
            Origin::Snapshot(snapshot) => get(snapshot),
            _ => None,
        }
    }

    fn record(&self, save: impl FnOnce(&Recorder)) {
        if let Origin::Live(Some(recorder)) = &self.origin {
            save(recorder);
        }
    }
}

fn run_command(program: &str, args: &[&str]) -> io::Result<String> {
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

/// Reads until the end of a file, or until a device has nothing more to give.
fn read_available(path: &Path) -> io::Result<Vec<u8>> {
    let fd = rustix::fs::open(
        path,
        OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let mut file = fs::File::from(fd);
    let mut bytes = Vec::new();
    let mut buffer = vec![0u8; 16 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&buffer[..n]),
            Err(err) if err.kind() == io::ErrorKind::WouldBlock => break,
            // EPIPE on /dev/kmsg: records were overwritten while reading; the next
            // read continues with the oldest one left.
            Err(err) if err.kind() == io::ErrorKind::BrokenPipe => continue,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        }
    }
    Ok(bytes)
}

fn live_now_unix_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
}

fn live_boot_epoch_unix_ns() -> i64 {
    let ns = |t: rustix::time::Timespec| t.tv_sec * 1_000_000_000 + t.tv_nsec;
    ns(clock_gettime(ClockId::Realtime)) - ns(clock_gettime(ClockId::Boottime))
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
        let now = live_now_unix_ns();
        let boot = Context::live().boot_epoch_unix_ns().unwrap();
        assert!(boot > 0 && boot < now);
    }

    #[test]
    fn a_snapshot_replays_what_was_read() {
        let dir = std::env::temp_dir().join(format!("kokuin-snapshot-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);

        let live = Context::recording(&dir).unwrap();
        let ostype = live.read_text("/proc/sys/kernel/ostype").unwrap();
        let listing = live.list_dir("/proc/sys/kernel/random").unwrap();
        let exe = live.read_link("/proc/self/exe").unwrap();
        let mtime = live.link_mtime_unix_ns("/proc/self/exe").unwrap();
        let uname = live.run("uname", &["-s"]).unwrap();
        let has_proc = live.exists("/proc/self");
        assert!(live.read_text("/does/not/exist").is_err());
        live.save_snapshot().unwrap();

        let replay = Context::from_snapshot(&dir).unwrap();
        assert_eq!(replay.read_text("/proc/sys/kernel/ostype").unwrap(), ostype);
        assert_eq!(replay.list_dir("/proc/sys/kernel/random").unwrap(), listing);
        assert_eq!(replay.read_link("/proc/self/exe").unwrap(), exe);
        assert_eq!(replay.link_mtime_unix_ns("/proc/self/exe").unwrap(), mtime);
        assert_eq!(replay.run("uname", &["-s"]).unwrap(), uname);
        assert_eq!(replay.exists("/proc/self"), has_proc);
        assert_eq!(replay.own_pid(), Some(std::process::id()));
        assert!(replay.read_text("/does/not/exist").is_err());
        assert!(replay.run("uname", &["-r"]).is_err());
        assert_eq!(
            replay.boot_epoch_unix_ns(),
            Snapshot::load(&dir).unwrap().boot_epoch_unix_ns
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_snapshot_needs_an_empty_directory() {
        let dir = std::env::temp_dir().join(format!("kokuin-snapshot-full-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("something"), "").unwrap();
        let err = Context::recording(&dir).unwrap_err();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }
}
