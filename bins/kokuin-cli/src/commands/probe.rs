//! `probe`: kokuin's EFI probe, which times the firmware for one boot.
//!
//! `install` puts it on the EFI system partition with a boot entry of its own,
//! `arm` makes the next boot (only) go through it, `disarm` and `remove` undo.

use anstream::println;
use kokuin_bootcfg::efi::{Action, Firmware};

use super::enable::may_change;
use crate::{
    Failure,
    output::{paint, style},
};

/// What to do with the probe.
#[derive(Debug, Clone, Copy, clap::Subcommand)]
pub enum ProbeCommand {
    /// Put the probe on the EFI system partition, with a boot entry of its own that
    /// the firmware does not start by itself.
    Install,
    /// Make the next boot, and only it, go through the probe.
    Arm,
    /// Undo `arm` before rebooting.
    Disarm,
    /// Remove the probe and its boot entry.
    Remove,
}

pub fn run(command: ProbeCommand, dry_run: bool) -> Result<(), Failure> {
    let firmware = Firmware::live();
    match command {
        ProbeCommand::Install => {
            let actions = firmware.plan_install().map_err(Failure)?;
            change(
                &actions,
                dry_run,
                "Done: the probe is installed. `sudo kokuin-cli probe arm` makes the next boot \
                 go through it.",
            )
        }
        ProbeCommand::Arm => {
            let actions = firmware.plan_arm().map_err(Failure)?;
            if actions.is_empty() {
                println!("The next boot already goes through the probe.");
                return Ok(());
            }
            change(
                &actions,
                dry_run,
                "Done: the next boot goes through the probe, once. After rebooting, \
                 `kokuin-cli show last` shows the firmware and bootloader stages timed.",
            )
        }
        ProbeCommand::Disarm => {
            let actions = firmware.plan_disarm();
            if actions.is_empty() {
                println!("The probe is not armed.");
                return Ok(());
            }
            change(&actions, dry_run, "Done: the next boot is a normal one.")
        }
        ProbeCommand::Remove => {
            let actions = firmware.plan_remove();
            if actions.is_empty() {
                println!("The probe is not installed.");
                return Ok(());
            }
            change(
                &actions,
                dry_run,
                "Done: the probe and its boot entry are gone.",
            )
        }
    }
}

/// Shows the changes to the firmware's boot entries, then makes them unless
/// `dry_run`. Nothing is said when there are none.
pub fn change(actions: &[Action], dry_run: bool, done: &str) -> Result<(), Failure> {
    if actions.is_empty() {
        return Ok(());
    }
    println!("{}", paint(style::HEADING, "Firmware boot entries"));
    for action in actions {
        println!("  {}", action.describe());
    }
    println!();
    if !may_change(dry_run, "the firmware's boot entries")? {
        return Ok(());
    }
    Firmware::live().apply(actions).map_err(|err| {
        Failure(format!(
            "the firmware's boot entries were not updated: {err}"
        ))
    })?;
    if !done.is_empty() {
        println!("{done}");
    }
    Ok(())
}
