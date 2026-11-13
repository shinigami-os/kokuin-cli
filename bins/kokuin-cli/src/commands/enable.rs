//! `enable` and `disable`: record every boot, or stop.
//!
//! Recording a boot means starting `kokuin-init` instead of the usual init, with the
//! `init=` kernel parameter. These commands add it to the bootloader configuration,
//! or remove it, after showing exactly what changes.

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

pub fn enable(dry_run: bool) -> Result<(), Failure> {
    let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_default();
    let parameters = kokuin_bootcfg::parameters(kokuin_bootcfg::init_parameter(&cmdline));

    let mut problems = Vec::new();
    for (path, what) in [(KOKUIN_INIT, "kokuin-init"), (RECORDER, "the recorder")] {
        if !Path::new(path).is_file() {
            problems.push(format!("{what} is not installed at {path}"));
        }
    }
    warn_about_kernel_options();

    let plan = match kokuin_bootcfg::plan_enable(&System::read(), Section::Recording, &parameters) {
        Ok(plan) => plan,
        Err(PlanError::Unsupported) => {
            println!("kokuin cannot change this bootloader's configuration (it knows GRUB).");
            println!("{}", kokuin_bootcfg::manual_instructions(&parameters));
            return Ok(());
        }
    };
    if plan.is_empty() {
        println!("Boot recording is already enabled.");
        return Ok(());
    }
    describe(&plan);
    if !problems.is_empty() {
        return Err(Failure(problems.join("; ")));
    }
    run(&plan, dry_run)?;
    if !dry_run {
        println!(
            "Done: the next boots will be recorded. After rebooting, see the recording with \
             `kokuin-cli list` and `kokuin-cli show last`."
        );
    }
    Ok(())
}

pub fn disable(dry_run: bool) -> Result<(), Failure> {
    let plan = match kokuin_bootcfg::plan_disable(&System::read(), Section::Recording) {
        Ok(plan) => plan,
        Err(PlanError::Unsupported) => {
            println!(
                "kokuin cannot change this bootloader's configuration: remove `init={KOKUIN_INIT}` \
                 from the kernel command line by hand."
            );
            return Ok(());
        }
    };
    if plan.is_empty() {
        println!("Boot recording is not enabled.");
        return Ok(());
    }
    describe(&plan);
    run(&plan, dry_run)?;
    if !dry_run {
        println!(
            "Done: the next boots will not be recorded. Recorded boots stay in {}.",
            boots::DIR
        );
    }
    Ok(())
}

fn run(plan: &Plan, dry_run: bool) -> Result<(), Failure> {
    if dry_run {
        println!("{}", paint(style::DIM, "Nothing was changed (--dry-run)."));
        return Ok(());
    }
    if !rustix::process::geteuid().is_root() {
        return Err(Failure(
            "changing the bootloader configuration needs root: run it with sudo".into(),
        ));
    }
    kokuin_bootcfg::apply(plan).map_err(|err| {
        Failure(format!(
            "the bootloader configuration was not updated: {err}"
        ))
    })
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

/// The recorder works best with the kernel's process events; without them it polls.
fn warn_about_kernel_options() {
    let release = fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
    let Ok(config) = KernelConfig::load(&Context::live(), release.trim()) else {
        return;
    };
    let missing: Vec<&str> = ["CONFIG_CONNECTOR", "CONFIG_PROC_EVENTS"]
        .into_iter()
        .filter(|option| config.state(option) == OptionState::Disabled)
        .collect();
    if !missing.is_empty() {
        println!(
            "{} the kernel lacks {}: the recorder will poll /proc instead, with less precise \
             times and without the shortest processes.\n",
            paint(style::WARNING, "Note:"),
            missing.join(" and ")
        );
    }
}
