//! Recorded boots: one directory per boot, `/var/lib/kokuin/boots/<boot_id>/`, holding
//! `record.json` and the raw snapshot it was built from (`raw/`).
//!
//! If `/var` could not be written when the recording ended (a read-only root
//! filesystem), the boot is kept in `/run/kokuin/boots/` instead, until the next
//! reboot.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use kokuin_model::BootRecord;

use crate::{Failure, records};

/// Where recorded boots are kept.
pub const DIR: &str = "/var/lib/kokuin/boots";
/// Where they are kept when `/var` cannot be written.
pub const FALLBACK_DIR: &str = "/run/kokuin/boots";
/// The record, in each boot directory.
pub const RECORD_FILE: &str = "record.json";
/// The raw snapshot the record was built from, in each boot directory.
pub const RAW_DIR: &str = "raw";

/// A recorded boot.
pub struct StoredBoot {
    /// Its directory.
    pub dir: PathBuf,
    /// Its record.
    pub record: BootRecord,
}

/// The id of the current boot.
pub fn current_boot_id() -> io::Result<String> {
    Ok(fs::read_to_string("/proc/sys/kernel/random/boot_id")?
        .trim()
        .to_owned())
}

/// Creates the empty directory of a boot (replacing an earlier recording of it), in
/// `/var/lib/kokuin/boots`, or in `/run/kokuin/boots` if that fails.
pub fn prepare(boot_id: &str) -> Result<PathBuf, Failure> {
    let mut errors = Vec::new();
    for base in [DIR, FALLBACK_DIR] {
        let dir = Path::new(base).join(boot_id);
        let created = match fs::remove_dir_all(&dir) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
        .and_then(|()| fs::create_dir_all(&dir));
        match created {
            Ok(()) => return Ok(dir),
            Err(err) => errors.push(format!("{}: {err}", dir.display())),
        }
    }
    Err(Failure(format!(
        "cannot create the boot directory ({})",
        errors.join("; ")
    )))
}

/// The recorded boots, oldest first.
pub fn list() -> Vec<StoredBoot> {
    let mut boots: Vec<StoredBoot> = [DIR, FALLBACK_DIR]
        .iter()
        .flat_map(|base| fs::read_dir(base).into_iter().flatten().flatten())
        .filter_map(|entry| load(&entry.path()).ok())
        .collect();
    boots.sort_by(|a, b| {
        let key = |boot: &StoredBoot| boot.record.boot.started_at.clone();
        key(a).cmp(&key(b))
    });
    boots
}

/// Reads the record of a boot directory.
pub fn load(dir: &Path) -> Result<StoredBoot, Failure> {
    Ok(StoredBoot {
        dir: dir.to_owned(),
        record: records::read_file(&dir.join(RECORD_FILE))?,
    })
}

/// Finds a recorded boot: `last` (or `-1`), `-2` for the one before, and so on, or
/// the start of a boot id.
pub fn find(selector: &str) -> Result<StoredBoot, Failure> {
    let mut boots = list();
    let from_end = match selector {
        "last" => Some(1),
        _ => selector
            .strip_prefix('-')
            .and_then(|n| n.parse::<usize>().ok()),
    };
    if let Some(n) = from_end {
        let count = boots.len();
        return count
            .checked_sub(n)
            .filter(|_| n > 0)
            .map(|index| boots.swap_remove(index))
            .ok_or_else(|| match count {
                0 => Failure("no boot recorded yet: see `kokuin-cli enable`".into()),
                _ => Failure(format!(
                    "only {count} boot(s) recorded: see `kokuin-cli list`"
                )),
            });
    }
    let mut matching: Vec<StoredBoot> = boots
        .into_iter()
        .filter(|b| b.record.boot.boot_id.starts_with(selector))
        .collect();
    match matching.len() {
        1 => Ok(matching.remove(0)),
        0 => Err(Failure(format!(
            "`{selector}` is not a file, a folder or a recorded boot: see `kokuin-cli list`"
        ))),
        n => Err(Failure(format!(
            "{n} recorded boots start with `{selector}`: give more of the id"
        ))),
    }
}

/// The recording of the current boot, if there is one.
pub fn current() -> Option<StoredBoot> {
    let boot_id = current_boot_id().ok()?;
    [DIR, FALLBACK_DIR]
        .iter()
        .find_map(|base| load(&Path::new(base).join(&boot_id)).ok())
}

/// Deletes the oldest recorded boots, keeping `keep`.
pub fn prune(keep: usize) -> io::Result<()> {
    let boots = list();
    let excess = boots.len().saturating_sub(keep);
    for boot in boots.into_iter().take(excess) {
        fs::remove_dir_all(&boot.dir)?;
    }
    Ok(())
}
