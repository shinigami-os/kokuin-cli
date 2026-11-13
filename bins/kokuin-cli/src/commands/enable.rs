//! `enable`, `disable` and `deep`: record every boot, stop, or record the next boot
//! with kernel tracing.
//!
//! Recording a boot means starting `kokuin-init` instead of the usual init, with the
//! `init=` kernel parameter. These commands add kokuin's parameters to the bootloader
//! configuration, or remove them, after showing exactly what changes.

use std::{fs, path::Path};

use anstream::println;
use kokuin_bootcfg::{KOKUIN_INIT, Plan, PlanError, Section, System};
use kokuin_sources::{
    Context,
    kconfig::{KernelConfig, OptionState},
};

use crate::{
    Failure, boots,
    output::{paint, style},
};

/// The recorder kokuin-init starts (the same path as in kokuin-init).
pub const RECORDER: &str = match option_env!("KOKUIN_CLI") {
    Some(path) => path,
    None => "/usr/bin/kokuin-cli",
};

/// Kernel options the recorder needs, and what happens without them.
const RECORDER_OPTIONS: Requirement = Requirement {
    options: &["CONFIG_CONNECTOR", "CONFIG_PROC_EVENTS"],
    without: "the recorder will poll /proc instead, with less precise times and without \
              the shortest processes",
};
/// Kernel options the tracing of a deep boot needs.
const TRACING_OPTIONS: Requirement = Requirement {
    options: &["CONFIG_TRACEPOINTS", "CONFIG_EVENT_TRACING"],
    without: "the deep boot will time the kernel's init functions, but not name the \
              shortest processes",
};

pub fn enable(dry_run: bool) -> Result<(), Failure> {
    let parameters = kokuin_bootcfg::parameters(current_init().as_deref());
    RECORDER_OPTIONS.warn_if_missing();
    let plan = kokuin_bootcfg::plan_enable(&System::read(), Section::Recording, &parameters);
    change(
        plan,
        dry_run,
        &parameters,
        &Messages {
            nothing_to_do: "Boot recording is already enabled.",
            done: "Done: the next boots will be recorded. After rebooting, see the recording \
                   with `kokuin-cli list` and `kokuin-cli show last`.",
            needs_recorder: true,
        },
    )
}

pub fn disable(dry_run: bool) -> Result<(), Failure> {
    let plan = kokuin_bootcfg::plan_disable(&System::read(), &Section::ALL);
    change(
        plan,
        dry_run,
        &format!("init={KOKUIN_INIT}"),
        &Messages {
            nothing_to_do: "Boot recording is not enabled.",
            done: &format!(
                "Done: the next boots will not be recorded. Recorded boots stay in {}.",
                boots::DIR
            ),
            needs_recorder: false,
        },
    )
}

pub fn deep(dry_run: bool, cancel: bool) -> Result<(), Failure> {
    if cancel {
        let plan = kokuin_bootcfg::plan_disable(&System::read(), &[Section::Deep]);
        return change(
            plan,
            dry_run,
            kokuin_bootcfg::DEEP_TRACING,
            &Messages {
                nothing_to_do: "No deep boot is armed.",
                done: "Done: the next boot will not be traced.",
                needs_recorder: false,
            },
        );
    }
    let parameters = kokuin_bootcfg::deep_parameters(current_init().as_deref());
    RECORDER_OPTIONS.warn_if_missing();
    TRACING_OPTIONS.warn_if_missing();
    let plan = kokuin_bootcfg::plan_enable(&System::read(), Section::Deep, &parameters);
    change(
        plan,
        dry_run,
        &parameters,
        &Messages {
            nothing_to_do: "A deep boot is already armed: it will be the next one.",
            done: "Done: the next boot will be recorded with kernel tracing. It will be a little \
                   slower. Once it is recorded, kokuin removes these settings by itself.",
            needs_recorder: true,
        },
    )
}

/// What to tell around a change.
struct Messages<'a> {
    nothing_to_do: &'a str,
    done: &'a str,
    /// The change makes the next boot start kokuin-init: it must be installed.
    needs_recorder: bool,
}

/// Shows a planned change, then makes it unless `dry_run`. When the bootloader is not
/// supported, tells what to do by hand with `parameters`.
fn change(
    plan: Result<Plan, PlanError>,
    dry_run: bool,
    parameters: &str,
    messages: &Messages,
) -> Result<(), Failure> {
    let plan = match plan {
        Ok(plan) => plan,
        Err(PlanError::Unsupported) => {
            println!("kokuin cannot change this bootloader's configuration (it knows GRUB).");
            println!("{}", kokuin_bootcfg::manual_instructions(parameters));
            return Ok(());
        }
    };
    if plan.is_empty() {
        println!("{}", messages.nothing_to_do);
        return Ok(());
    }
    describe(&plan);
    if messages.needs_recorder {
        let missing: Vec<String> = [(KOKUIN_INIT, "kokuin-init"), (RECORDER, "the recorder")]
            .into_iter()
            .filter(|(path, _)| !Path::new(path).is_file())
            .map(|(path, what)| format!("{what} is not installed at {path}"))
            .collect();
        if !missing.is_empty() {
            return Err(Failure(missing.join("; ")));
        }
    }
    if dry_run {
        println!("{}", paint(style::DIM, "Nothing was changed (--dry-run)."));
        return Ok(());
    }
    if !rustix::process::geteuid().is_root() {
        return Err(Failure(
            "changing the bootloader configuration needs root: run it with sudo".into(),
        ));
    }
    kokuin_bootcfg::apply(&plan).map_err(|err| {
        Failure(format!(
            "the bootloader configuration was not updated: {err}"
        ))
    })?;
    println!("{}", messages.done);
    Ok(())
}

/// The `init=` of the running kernel's command line.
fn current_init() -> Option<String> {
    let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();
    kokuin_bootcfg::init_parameter(&cmdline).map(str::to_owned)
}

/// Shows the changes: files, lines added and removed, then the command run.
fn describe(plan: &Plan) {
    println!(
        "{}",
        paint(style::HEADING, format!("Bootloader: {}", plan.bootloader))
    );
    for edit in &plan.edits {
        let path = edit.path.display();
        match (&edit.before, &edit.after) {
            (None, Some(after)) => {
                println!("  Create {path}:");
                print_lines(after.lines(), "+", style::GOOD);
            }
            (Some(_), None) => println!("  Delete {path}"),
            (Some(before), Some(after)) => {
                println!("  Change {path}:");
                let removed = before.lines().filter(|l| !after.lines().any(|a| a == *l));
                let added = after.lines().filter(|l| !before.lines().any(|b| b == *l));
                print_lines(removed, "-", style::ERROR);
                print_lines(added, "+", style::GOOD);
            }
            (None, None) => {}
        }
    }
    if !plan.regenerate.is_empty() {
        println!("  Then run: {}", plan.regenerate.join(" "));
    }
    println!();
}

fn print_lines<'a>(lines: impl Iterator<Item = &'a str>, mark: &str, style: anstyle::Style) {
    for line in lines {
        println!("    {}", paint(style, format!("{mark} {line}")));
    }
}

/// Kernel options a feature needs.
struct Requirement {
    options: &'static [&'static str],
    /// What happens without them.
    without: &'static str,
}

impl Requirement {
    /// Warns when the running kernel lacks some of the options.
    fn warn_if_missing(&self) {
        let release = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
        let Ok(config) = KernelConfig::load(&Context::live(), release.trim()) else {
            return;
        };
        let missing: Vec<&str> = self
            .options
            .iter()
            .copied()
            .filter(|option| config.state(option) == OptionState::Disabled)
            .collect();
        if !missing.is_empty() {
            println!(
                "{} the kernel lacks {}: {}.\n",
                paint(style::WARNING, "Note:"),
                missing.join(" and "),
                self.without
            );
        }
    }
}
