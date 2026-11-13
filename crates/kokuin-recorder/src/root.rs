//! Following the switch from the initramfs to the real root filesystem.
//!
//! A recorder started inside the initramfs keeps running when the initramfs
//! switches to the real root (`switch_root`, `run-init`, systemd's switch-root): only
//! PID 1 moves into the new root, every other process stays in the old one, whose
//! `/proc` and `/dev` mounts were moved away. The recorder keeps a handle on `/proc`
//! from the start; when PID 1's root is no longer its own, it moves there too, through
//! `/proc/1/root`, the way into PID 1's view of the files.

use std::io;

use rustix::{
    fd::OwnedFd,
    fs::{AtFlags, Mode, OFlags, open, openat, stat, statat},
    process::{chdir, chroot, fchdir},
};

/// Watches whether PID 1 changed root, and follows it.
pub struct RootFollower {
    /// `/proc`, opened before the switch: it stays usable after its mount moved.
    proc: OwnedFd,
    /// Device and inode of this process's root.
    own_root: (u64, u64),
}

impl RootFollower {
    /// Starts watching, from the current root.
    pub fn new() -> io::Result<Self> {
        let proc = open(
            "/proc",
            OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let root = stat("/")?;
        Ok(Self {
            proc,
            own_root: (root.st_dev, root.st_ino),
        })
    }

    /// Whether PID 1 now has another root than this process.
    pub fn switched(&self) -> bool {
        statat(&self.proc, "1/root", AtFlags::empty())
            .is_ok_and(|root| (root.st_dev, root.st_ino) != self.own_root)
    }

    /// Moves this process into PID 1's root.
    pub fn follow(&mut self) -> io::Result<()> {
        let new_root = openat(
            &self.proc,
            "1/root",
            OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        fchdir(&new_root)?;
        chroot(".")?;
        chdir("/")?;
        let root = stat("/")?;
        self.own_root = (root.st_dev, root.st_ino);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_is_in_the_same_root_as_init_here() {
        // Without root, /proc/1/root cannot be read: no switch is seen either way.
        let follower = RootFollower::new().unwrap();
        assert!(!follower.switched());
    }
}
