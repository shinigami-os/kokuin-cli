//! The init system: which one runs the machine, and adapters reading its own view of
//! the boot (services and their times).

mod openrc;
mod runit;
mod systemd;

use kokuin_model::ComponentInfo;

pub use openrc::OpenRc;
pub use runit::Runit;
pub use systemd::Systemd;

use crate::{Context, Source};

/// Identifies the init system running as PID 1.
pub fn detect(ctx: &Context) -> Option<ComponentInfo> {
    let comm = ctx.read_text("/proc/1/comm").ok()?;
    let id = match comm.trim() {
        "systemd" => "systemd",
        "runit" | "runit-init" => "runit",
        "openrc-init" => "openrc",
        "s6-svscan" => "s6",
        "dinit" => "dinit",
        // A generic `init` (sysvinit or BusyBox) often runs OpenRC or runit.
        _ if ctx.exists("/run/openrc/softlevel") => "openrc",
        _ if ctx.exists("/run/systemd/system") => "systemd",
        _ if ctx.exists("/run/runit") => "runit",
        _ => match ctx.read_link("/proc/1/exe") {
            Ok(exe) if exe.ends_with("busybox") => "busybox",
            _ => "sysvinit",
        },
    };
    Some(ComponentInfo {
        id: id.to_owned(),
        name: Some(display_name(id).to_owned()),
        version: version(ctx, id),
        explain_id: None,
    })
}

/// The adapter reading the init system `id`, if there is one.
pub fn adapter(id: &str) -> Option<Box<dyn Source>> {
    match id {
        "runit" => Some(Box::new(Runit)),
        "systemd" => Some(Box::new(Systemd)),
        "openrc" => Some(Box::new(OpenRc)),
        _ => None,
    }
}

fn display_name(id: &str) -> &str {
    match id {
        "systemd" => "systemd",
        "runit" => "runit",
        "openrc" => "OpenRC",
        "s6" => "s6",
        "dinit" => "Dinit",
        "busybox" => "BusyBox init",
        _ => "SysV init",
    }
}

fn version(ctx: &Context, id: &str) -> Option<String> {
    let (program, args): (&str, &[&str]) = match id {
        "systemd" => ("systemctl", &["--version"]),
        "openrc" => ("openrc", &["--version"]),
        _ => return None,
    };
    let output = ctx.run(program, args).ok()?;
    parse_version(output.lines().next()?)
}

/// `systemd 255 (255.4-1ubuntu8)` → `255.4-1ubuntu8`; `openrc (OpenRC) 0.54` → `0.54`.
fn parse_version(line: &str) -> Option<String> {
    if let Some((_, rest)) = line.split_once('(') {
        let inside = rest.split(')').next()?;
        if inside.starts_with(|c: char| c.is_ascii_digit()) {
            return Some(inside.to_owned());
        }
    }
    line.split_whitespace()
        .find(|word| word.starts_with(|c: char| c.is_ascii_digit()))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions() {
        assert_eq!(
            parse_version("systemd 255 (255.4-1ubuntu8)").as_deref(),
            Some("255.4-1ubuntu8")
        );
        assert_eq!(
            parse_version("openrc (OpenRC) 0.54").as_deref(),
            Some("0.54")
        );
    }

    #[test]
    fn detects_the_live_init_system() {
        assert!(detect(&Context::live()).is_some());
    }
}
