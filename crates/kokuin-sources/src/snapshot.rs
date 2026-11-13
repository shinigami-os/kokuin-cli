//! Snapshots: everything the sources read on a system, saved to a directory so the
//! boot can be analyzed again later, elsewhere, or with newer rules.
//!
//! A snapshot directory holds:
//!
//! - `root/`: the files that were read, at their system path (`root/proc/1/stat`);
//! - `snapshot.json`: everything else ([`Snapshot`]): the clocks, the commands run
//!   and their output, directory listings, link targets and times, read errors.
//!
//! A [`Context`](crate::Context) saves one while it reads the live system
//! ([`Context::recording`](crate::Context::recording)), and reads one back
//! ([`Context::from_snapshot`](crate::Context::from_snapshot)).

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};

/// Version of the `snapshot.json` layout.
pub const FORMAT: u32 = 1;

/// Name of the description file in a snapshot directory.
pub const DESCRIPTION_FILE: &str = "snapshot.json";

/// Name of the directory holding the files that were read.
pub const ROOT_DIR: &str = "root";

/// What a snapshot remembers besides file contents. Paths are absolute system paths.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    /// Version of this layout ([`FORMAT`]).
    pub format: u32,
    /// Program and version that saved the snapshot.
    pub created_by: String,
    /// When the snapshot was saved, in ns since the Unix epoch.
    pub captured_at_unix_ns: i64,
    /// PID of the program that saved the snapshot (it is not part of the boot).
    pub pid: u32,
    /// Wall-clock time of the kernel start, in ns since the Unix epoch.
    pub boot_epoch_unix_ns: Option<i64>,
    /// Clock ticks per second, the unit of process times in `/proc`.
    pub clock_ticks_per_second: u64,
    /// The CPU's time counter read with the boot clock, if it was asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tsc: Option<TscSample>,
    /// Whether each tested path existed.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub exists: BTreeMap<String, bool>,
    /// Entries of each listed directory.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub listings: BTreeMap<String, Vec<String>>,
    /// Target of each symbolic link read.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub links: BTreeMap<String, String>,
    /// Modification time of each file or link asked for, in ns since the Unix epoch.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub mtimes: BTreeMap<String, i64>,
    /// Paths that could not be read, other than missing ones.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub errors: BTreeMap<String, RecordedError>,
    /// Commands run, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<RecordedCommand>,
}

/// The CPU's time counter (TSC) and the boot clock, read at the same moment, with
/// the counter's frequency: enough to place any counter value on the boot clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TscSample {
    /// The counter.
    pub tsc: u64,
    /// The boot clock (`CLOCK_MONOTONIC`) at the same moment, in ns.
    pub mono_ns: i64,
    /// Counter ticks per second.
    pub hz: u64,
}

impl TscSample {
    /// The boot clock time, in ns, when the counter read `tsc`.
    pub fn boot_ns(&self, tsc: u64) -> i64 {
        let ticks_before = i128::from(self.tsc) - i128::from(tsc);
        let ns_before = ticks_before * 1_000_000_000 / i128::from(self.hz.max(1));
        i64::try_from(i128::from(self.mono_ns) - ns_before).unwrap_or(i64::MIN)
    }
}

/// An error met while reading a path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedError {
    /// What kind of error.
    pub kind: ErrorKind,
    /// The system's error message.
    pub message: String,
}

/// Kinds of read errors worth telling apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// Not allowed (usually: not root).
    PermissionDenied,
    /// Anything else.
    Other,
}

/// A command and what it gave.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedCommand {
    /// Program name.
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Standard output, when the command succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    /// Why it failed, otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Snapshot {
    /// Reads the description of the snapshot in `dir`.
    pub fn load(dir: &Path) -> io::Result<Self> {
        let path = dir.join(DESCRIPTION_FILE);
        let text = fs::read_to_string(&path).map_err(|err| {
            io::Error::new(err.kind(), format!("cannot read {}: {err}", path.display()))
        })?;
        let snapshot: Snapshot = serde_json::from_str(&text).map_err(|err| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not a kokuin snapshot: {err}", path.display()),
            )
        })?;
        if snapshot.format != FORMAT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "{} uses snapshot format {}, this version reads format {FORMAT}",
                    path.display(),
                    snapshot.format
                ),
            ));
        }
        Ok(snapshot)
    }

    /// The error recorded for a path, as it was met.
    pub(crate) fn error(&self, path: &Path) -> Option<io::Error> {
        let recorded = self.errors.get(&key(path))?;
        let kind = match recorded.kind {
            ErrorKind::PermissionDenied => io::ErrorKind::PermissionDenied,
            ErrorKind::Other => io::ErrorKind::Other,
        };
        Some(io::Error::new(kind, recorded.message.clone()))
    }

    /// What a command gave, if it was run.
    pub(crate) fn command(&self, program: &str, args: &[&str]) -> Option<io::Result<String>> {
        let command = self
            .commands
            .iter()
            .find(|c| c.program == program && c.args.iter().eq(args.iter()))?;
        Some(match (&command.stdout, &command.error) {
            (Some(stdout), _) => Ok(stdout.clone()),
            (None, error) => Err(io::Error::other(
                error
                    .clone()
                    .unwrap_or_else(|| format!("`{program}` failed")),
            )),
        })
    }
}

/// Saves what a context reads into a snapshot directory.
#[derive(Debug)]
pub(crate) struct Recorder {
    dir: PathBuf,
    state: Mutex<RecorderState>,
}

#[derive(Debug)]
struct RecorderState {
    snapshot: Snapshot,
    /// The first error met while saving, reported by [`Recorder::save`].
    failure: Option<io::Error>,
}

impl Recorder {
    /// Prepares an empty snapshot directory (created if needed).
    pub(crate) fn create(dir: PathBuf, snapshot: Snapshot) -> io::Result<Self> {
        let not_empty = fs::read_dir(&dir).is_ok_and(|mut entries| entries.next().is_some());
        if not_empty {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "the directory is not empty",
            ));
        }
        fs::create_dir_all(dir.join(ROOT_DIR))?;
        Ok(Self {
            dir,
            state: Mutex::new(RecorderState {
                snapshot,
                failure: None,
            }),
        })
    }

    /// When the snapshot was started, in ns since the Unix epoch.
    pub(crate) fn captured_at_unix_ns(&self) -> i64 {
        self.lock().snapshot.captured_at_unix_ns
    }

    /// Changes the description.
    pub(crate) fn update(&self, change: impl FnOnce(&mut Snapshot)) {
        change(&mut self.lock().snapshot);
    }

    /// Saves the content of a file that was read.
    pub(crate) fn file(&self, path: &Path, bytes: &[u8]) {
        let target = self
            .dir
            .join(ROOT_DIR)
            .join(path.strip_prefix("/").unwrap_or(path));
        let written = target
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .and_then(|()| fs::write(&target, bytes));
        if let Err(err) = written {
            self.lock().failure.get_or_insert(err);
        }
    }

    /// Notes that a path could not be read. Missing paths need no note: the file is
    /// simply absent from the snapshot.
    pub(crate) fn error(&self, path: &Path, err: &io::Error) {
        let kind = match err.kind() {
            io::ErrorKind::NotFound => return,
            io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            _ => ErrorKind::Other,
        };
        let message = err.to_string();
        self.update(|s| {
            s.errors.insert(key(path), RecordedError { kind, message });
        });
    }

    /// Writes the description. Fails if any file could not be saved.
    pub(crate) fn save(&self) -> io::Result<()> {
        let mut state = self.lock();
        if let Some(err) = state.failure.take() {
            return Err(err);
        }
        let json = serde_json::to_string_pretty(&state.snapshot).map_err(io::Error::other)?;
        fs::write(self.dir.join(DESCRIPTION_FILE), json + "\n")
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, RecorderState> {
        // A panic while holding the lock leaves the data usable: keep going.
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The key of a path in the description.
pub(crate) fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
