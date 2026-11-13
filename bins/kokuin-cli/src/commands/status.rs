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
    Failure, boots,
    commands::enable,
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

    let recorder = RecorderState::read();
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
            "recorder": {
                "installed": recorder.installed,
                "enabled": recorder.enabled,
                "deep_armed": recorder.deep_armed,
                "probe_entry": recorder.probe_entry,
                "probe_armed": recorder.probe_armed,
                "initramfs": recorder.initramfs.map(|(generator, hook_installed)| json!({
                    "generator": generator,
                    "hook_installed": hook_installed,
                })),
                "current_boot": recorder.current_boot,
                "boots_recorded": recorder.boots_recorded,
            },
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
    recorder.print();
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

/// Whether boots are recorded here.
struct RecorderState {
    /// kokuin-init and the recorder are installed.
    installed: bool,
    /// kokuin-init is in the bootloader configuration; `None` when kokuin cannot
    /// read that configuration.
    enabled: Option<bool>,
    /// The next boot is a deep one.
    deep_armed: bool,
    /// The EFI probe's boot entry, if the probe is installed.
    probe_entry: Option<u16>,
    /// The next boot goes through the EFI probe.
    probe_armed: bool,
    /// The system booted through UEFI (the probe needs it).
    uefi: bool,
    /// Secure Boot is on (the unsigned probe cannot run).
    secure_boot: bool,
    /// The initramfs generator kokuin knows here, and whether its hook is installed.
    initramfs: Option<(&'static str, bool)>,
    /// Where the recording of this boot is, if it was recorded.
    current_boot: Option<String>,
    boots_recorded: usize,
}

impl RecorderState {
    fn read() -> Self {
        let system = kokuin_bootcfg::System::read();
        let firmware = kokuin_bootcfg::efi::Firmware::live();
        Self {
            installed: [kokuin_bootcfg::KOKUIN_INIT, enable::RECORDER]
                .iter()
                .all(|path| std::path::Path::new(path).is_file()),
            enabled: system
                .grub
                .is_some()
                .then(|| kokuin_bootcfg::is_enabled(&system, kokuin_bootcfg::Section::Recording)),
            deep_armed: kokuin_bootcfg::is_enabled(&system, kokuin_bootcfg::Section::Deep),
            probe_entry: firmware.probe_entry().map(|entry| entry.number),
            probe_armed: firmware.is_armed(),
            uefi: firmware.is_uefi(),
            secure_boot: firmware.secure_boot(),
            initramfs: kokuin_bootcfg::initramfs::Initramfs::read()
                .map(|initramfs| (initramfs.generator.name(), initramfs.is_enabled())),
            current_boot: boots::current().map(|b| b.dir.display().to_string()),
            boots_recorded: boots::list().len(),
        }
    }

    fn print(&self) {
        let mark = |ok: bool| {
            status_symbol(if ok {
                SourceStatus::Ok
            } else {
                SourceStatus::Unavailable
            })
        };
        println!();
        println!("{}", paint(style::HEADING, "Boot recorder"));
        println!(
            "  {} {}",
            mark(self.installed),
            if self.installed {
                format!("installed ({})", kokuin_bootcfg::KOKUIN_INIT)
            } else {
                format!("not installed: {} is missing", kokuin_bootcfg::KOKUIN_INIT)
            }
        );
        match self.enabled {
            Some(true) => println!("  {} enabled: the next boots are recorded", mark(true)),
            Some(false) => println!(
                "  {} not enabled: record the next boots with `sudo kokuin-cli enable`",
                mark(false)
            ),
            None => println!(
                "  {} enabled or not: kokuin cannot read this bootloader's configuration",
                paint(style::DIM, "?")
            ),
        }
        if self.deep_armed {
            println!(
                "  {} a deep boot is armed: the next boot is recorded with kernel tracing",
                mark(true)
            );
        }
        match self.initramfs {
            Some((generator, true)) => println!(
                "  {} recording starts in the initramfs ({generator})",
                mark(true)
            ),
            Some((generator, false)) => println!(
                "  {} the initramfs hook ({generator}) is not installed: `sudo kokuin-cli enable` installs it",
                paint(style::DIM, "·")
            ),
            None => println!(
                "  {} recording starts after the initramfs: kokuin knows no initramfs generator here",
                paint(style::DIM, "·")
            ),
        }
        if self.uefi {
            match (self.probe_entry, self.probe_armed) {
                (Some(entry), true) => println!(
                    "  {} EFI probe installed (Boot{entry:04X}), armed: the next boot goes through it",
                    mark(true)
                ),
                (Some(entry), false) => println!(
                    "  {} EFI probe installed (Boot{entry:04X}); `sudo kokuin-cli probe arm` times the next boot's firmware",
                    mark(true)
                ),
                (None, _) if self.secure_boot => println!(
                    "  {} EFI probe not available: Secure Boot is on, and the probe is not signed",
                    paint(style::DIM, "·")
                ),
                (None, _) => println!(
                    "  {} EFI probe not installed: `sudo kokuin-cli probe install` to time the firmware",
                    paint(style::DIM, "·")
                ),
            }
        }
        match &self.current_boot {
            Some(dir) => println!("  {} this boot was recorded, in {dir}", mark(true)),
            None => println!("  {} this boot was not recorded", mark(false)),
        }
        println!(
            "  {}",
            paint(
                style::DIM,
                format!(
                    "{} boot(s) recorded: `kokuin-cli list`",
                    self.boots_recorded
                )
            )
        );
    }
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
