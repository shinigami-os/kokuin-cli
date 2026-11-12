//! `list`: the recorded boots.

use anstream::println;
use kokuin_model::{EndReason, MilestoneKind};
use serde_json::json;

use crate::{
    Failure,
    boots::{self, StoredBoot},
    output::{duration, paint, style},
};

pub fn run(json: bool) -> Result<(), Failure> {
    let boots = boots::list();
    let current = boots::current_boot_id().ok();
    let is_current =
        |boot: &StoredBoot| current.as_deref() == Some(boot.record.boot.boot_id.as_str());

    if json {
        let list: Vec<_> = boots
            .iter()
            .map(|boot| {
                json!({
                    "boot_id": boot.record.boot.boot_id,
                    "current": is_current(boot),
                    "started_at": boot.record.boot.started_at,
                    "dir": boot.dir,
                    "method": boot.record.capture.method,
                    "end_reason": boot.record.capture.end_reason,
                    "session_ready_ns": session_ready_ns(boot),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&list)?);
        return Ok(());
    }

    if boots.is_empty() {
        println!(
            "No boot recorded yet. Record the next ones with `sudo kokuin-cli enable`, then reboot."
        );
        return Ok(());
    }
    println!(
        "{}",
        paint(
            style::HEADING,
            format!(
                "  {:<10} {:<31} {:>13}  {}",
                "BOOT", "STARTED", "DESKTOP READY", "RECORDING"
            )
        )
    );
    for boot in &boots {
        let mark = if is_current(boot) { "*" } else { " " };
        let id: String = boot.record.boot.boot_id.chars().take(8).collect();
        let ready = session_ready_ns(boot)
            .map(|t| duration(t.max(0) as u64))
            .unwrap_or_else(|| "—".into());
        let ended = match boot.record.capture.end_reason {
            Some(EndReason::SessionReady) => "until the desktop settled",
            Some(EndReason::Timeout) => "stopped at its time limit",
            _ => "",
        };
        println!(
            "{mark} {id:<10} {:<31} {ready:>13}  {}",
            boot.record.boot.started_at.as_deref().unwrap_or("?"),
            paint(style::DIM, ended)
        );
    }
    println!();
    println!(
        "{}",
        paint(
            style::DIM,
            "* this boot. Show one with `kokuin-cli show <boot>`: `last`, `-2`, or the start of its id."
        )
    );
    Ok(())
}

fn session_ready_ns(boot: &StoredBoot) -> Option<i64> {
    boot.record
        .milestones
        .iter()
        .find(|m| m.kind == MilestoneKind::SessionReady)
        .map(|m| m.t_ns)
}
