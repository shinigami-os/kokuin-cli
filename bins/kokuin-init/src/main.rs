//! `kokuin-init`: the first program of a recorded boot.
//!
//! `kokuin-cli enable` adds `init=/usr/lib/kokuin/kokuin-init` to the kernel command
//! line, so the kernel (or the initramfs, once the root filesystem is mounted) starts
//! this program as PID 1 instead of the usual init. It does two things:
//!
//! 1. starts the boot recorder (`kokuin-cli record`) in the background;
//! 2. becomes the real init: it replaces itself with it (`exec`), so the real init
//!    keeps PID 1 and never knows the difference.
//!
//! It must never stop the system from booting: if the recorder cannot start, the boot
//! goes on without it. When an initramfs hook already started the recorder, it only
//! hands over to the real init. The real init is the program named by the `kokuin_init`
//! kernel parameter (the kernel hands unknown parameters without a dot to init as
//! environment variables), else the first of the usual inits that exists.

use std::{
    env,
    ffi::{OsStr, OsString},
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
};

/// The boot recorder. Set `KOKUIN_CLI` when building to install it elsewhere.
const RECORDER: &str = match option_env!("KOKUIN_CLI") {
    Some(path) => path,
    None => "/usr/bin/kokuin-cli",
};
/// The kernel parameter naming the real init.
const REAL_INIT_PARAMETER: &str = "kokuin_init";
/// Where inits usually are, in the order the kernel itself tries them.
const USUAL_INITS: [&str; 4] = ["/sbin/init", "/etc/init", "/bin/init", "/bin/sh"];

fn main() {
    let arguments: Vec<OsString> = env::args_os().skip(1).collect();
    let requested = env::var_os(REAL_INIT_PARAMETER);

    // Started from the initramfs, a recorder may already be running.
    if !recorder_running()
        && let Err(err) = start_recorder()
    {
        eprintln!("kokuin-init: the boot is not recorded: cannot start {RECORDER}: {err}");
    }
    for init in inits(requested, |path| path.is_file()) {
        // `exec` only returns if the program could not be started.
        let err = Command::new(&init)
            .arg0(&init)
            .args(&arguments)
            .env_remove(REAL_INIT_PARAMETER)
            .exec();
        eprintln!(
            "kokuin-init: cannot start {}: {err}",
            init.to_string_lossy()
        );
    }
    // PID 1 exiting makes the kernel panic: at least the reason is on the console.
    eprintln!("kokuin-init: no init program could be started");
}

/// The abstract socket a running recorder holds (`kokuin_recorder::instance::NAME`).
const RECORDER_NAME: &[u8] = b"kokuin-recorder";

/// Whether a recorder is running: then it holds its socket name.
fn recorder_running() -> bool {
    use std::os::{
        linux::net::SocketAddrExt,
        unix::net::{SocketAddr, UnixListener},
    };
    let Ok(address) = SocketAddr::from_abstract_name(RECORDER_NAME) else {
        return false;
    };
    // Taking the name and dropping it at once leaves it free for the recorder.
    matches!(
        UnixListener::bind_addr(&address),
        Err(err) if err.kind() == std::io::ErrorKind::AddrInUse
    )
}

/// Starts the recorder, without waiting for it. The real init adopts it.
fn start_recorder() -> std::io::Result<()> {
    Command::new(RECORDER)
        .arg("record")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
}

/// The programs to try as the real init, in order: the requested one, then the
/// usual ones; only those that exist, and never kokuin-init itself.
fn inits(requested: Option<OsString>, exists: impl Fn(&Path) -> bool) -> Vec<OsString> {
    let mut found: Vec<OsString> = Vec::new();
    let candidates = requested
        .into_iter()
        .filter(|path| !path.is_empty())
        .chain(USUAL_INITS.iter().map(OsString::from));
    for candidate in candidates {
        let path = Path::new(&candidate);
        let is_self = path.file_name() == Some(OsStr::new("kokuin-init"));
        if !is_self && exists(path) && !found.contains(&candidate) {
            found.push(candidate);
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inits_with(requested: Option<&str>, existing: &[&str]) -> Vec<OsString> {
        inits(requested.map(OsString::from), |path| {
            existing.iter().any(|e| Path::new(e) == path)
        })
    }

    #[test]
    fn the_usual_init_by_default() {
        assert_eq!(
            inits_with(None, &["/sbin/init", "/bin/sh"]),
            ["/sbin/init", "/bin/sh"]
        );
    }

    #[test]
    fn the_requested_init_comes_first() {
        assert_eq!(
            inits_with(
                Some("/sbin/openrc-init"),
                &["/sbin/openrc-init", "/sbin/init"]
            ),
            ["/sbin/openrc-init", "/sbin/init"]
        );
    }

    #[test]
    fn a_requested_init_that_does_not_exist_is_skipped() {
        assert_eq!(
            inits_with(Some("/sbin/missing"), &["/sbin/init"]),
            ["/sbin/init"]
        );
        assert_eq!(inits_with(Some(""), &["/sbin/init"]), ["/sbin/init"]);
    }

    #[test]
    fn never_itself() {
        let own = "/usr/lib/kokuin/kokuin-init";
        assert_eq!(inits_with(Some(own), &[own, "/bin/sh"]), ["/bin/sh"]);
    }

    #[test]
    fn nothing_found() {
        assert!(inits_with(None, &[]).is_empty());
    }
}
