//! `status`: what can be read on this system, and why some data is missing.

use anstream::println;
use kokuin_kb::Kb;
use kokuin_model::{SourceReport, SourceStatus};
use kokuin_sources::{
    Context, Source,
    kconfig::{KernelConfig, OptionState},
};
use serde_json::json;

use crate::{
    Failure,
    output::{paint, status_symbol, style},
};

struct SourceState {
    report: SourceReport,
    title: Option<String>,
    kernel_options: Vec<(&'static str, Option<OptionState>)>,
}

pub fn run(json: bool) -> Result<(), Failure> {
    let ctx = Context::live();
    let kb = Kb::embedded();
    let system = kokuin_sources::system::collect(&ctx);
    let init = kokuin_sources::init::detect(&ctx);
    let config = KernelConfig::load(&ctx, &system.boot.kernel.release);
    let root = rustix::process::geteuid().is_root();

    let title = |report: &SourceReport| {
        report
            .explain_id
            .as_deref()
            .and_then(|id| kb.article(id))
            .map(|a| a.title.clone())
    };
    let sources = kokuin_sources::sources(init.as_ref().map(|i| i.id.as_str()));
    let mut states: Vec<SourceState> = sources
        .iter()
        .map(|source| {
            let report = source.collect(&ctx).report;
            SourceState {
                title: title(&report),
                kernel_options: options(source.as_ref(), config.as_ref().ok()),
                report,
            }
        })
        .collect();
    let dmi_position = states
        .iter()
        .position(|s| s.report.id == "kmsg")
        .unwrap_or(states.len());
    states.insert(
        dmi_position,
        SourceState {
            title: title(&system.dmi_report),
            kernel_options: Vec::new(),
            report: system.dmi_report,
        },
    );

    if json {
        let sources: Vec<_> = states
            .iter()
            .map(|state| {
                json!({
                    "report": state.report,
                    "title": state.title,
                    "kernel_options": state.kernel_options.iter().map(|(name, option)| json!({
                        "name": name,
                        "state": option.map(option_state_name),
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();
        let status = json!({
            "root": root,
            "boot": system.boot,
            "init": init,
            "kernel_config": config.as_ref().ok().map(|c| c.origin.clone()),
            "sources": sources,
        });
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    let os = system
        .boot
        .os
        .as_ref()
        .and_then(|os| os.pretty_name.clone().or(os.name.clone()));
    println!(
        "{}",
        paint(style::TITLE, "What kokuin can read on this system")
    );
    println!();
    println!(
        "  System   {} · Linux {} · {}",
        os.as_deref().unwrap_or("unknown distribution"),
        system.boot.kernel.release,
        system.boot.machine.arch
    );
    println!(
        "  Firmware {}",
        crate::output::name(system.boot.machine.firmware).to_uppercase()
    );
    if let Some(init) = &init {
        println!("  Init     {}", init.name.as_deref().unwrap_or(&init.id));
    }
    match &config {
        Ok(config) => println!("  Kernel configuration read from {}", config.origin),
        Err(reason) => println!(
            "  {} {reason}",
            paint(style::WARNING, "Kernel configuration unknown:")
        ),
    }
    if !root {
        println!(
            "  {} several sources need root: run `sudo kokuin-cli status` to check them",
            paint(style::WARNING, "Not running as root:")
        );
    }
    println!();
    println!("{}", paint(style::HEADING, "Data sources"));
    for state in &states {
        let report = &state.report;
        println!(
            "  {} {:<13} {}",
            status_symbol(report.status),
            report.id,
            state.title.as_deref().unwrap_or("")
        );
        if let Some(reason) = &report.reason {
            let style = if report.status == SourceStatus::Partial {
                style::DIM
            } else {
                style::WARNING
            };
            println!("      {}", paint(style, reason));
        }
        let missing: Vec<_> = state
            .kernel_options
            .iter()
            .filter(|(_, option)| *option == Some(OptionState::Disabled))
            .map(|(name, _)| *name)
            .collect();
        if !missing.is_empty() {
            println!(
                "      {} {}",
                paint(style::ERROR, "kernel option not enabled:"),
                missing.join(", ")
            );
        }
    }
    println!();
    println!(
        "{}",
        paint(
            style::DIM,
            "Learn about a source with `kokuin-cli explain <id>`, for example `kokuin-cli explain firmware.fpdt`."
        )
    );
    Ok(())
}

fn options(
    source: &dyn Source,
    config: Option<&KernelConfig>,
) -> Vec<(&'static str, Option<OptionState>)> {
    source
        .kernel_options()
        .iter()
        .map(|&name| (name, config.map(|c| c.state(name))))
        .collect()
}

fn option_state_name(state: OptionState) -> &'static str {
    match state {
        OptionState::BuiltIn => "built_in",
        OptionState::Module => "module",
        OptionState::Disabled => "disabled",
    }
}
