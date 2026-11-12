//! Formatting helpers shared by the commands.

use kokuin_model::{Confidence, MilestoneKind, SourceStatus, Stage};

/// Text styles. `anstream` removes them when the output is not a terminal.
pub mod style {
    use anstyle::{AnsiColor, Style};

    pub const TITLE: Style = Style::new().bold().underline();
    pub const HEADING: Style = Style::new().bold();
    pub const DIM: Style = Style::new().dimmed();
    pub const ID: Style = AnsiColor::Cyan.on_default();
    pub const GOOD: Style = AnsiColor::Green.on_default();
    pub const WARNING: Style = AnsiColor::Yellow.on_default().bold();
    pub const ERROR: Style = AnsiColor::Red.on_default().bold();
}

/// Wraps `text` in `style`.
pub fn paint(style: anstyle::Style, text: impl std::fmt::Display) -> String {
    format!("{}{text}{}", style.render(), style.render_reset())
}

/// A time on the boot clock: `-7.070 s`, `1.423 s`.
pub fn time(t_ns: i64) -> String {
    format!("{:.3} s", t_ns as f64 / 1e9)
}

/// A duration, in the most readable unit: `4.75 s`, `350 ms`, `12 µs`.
pub fn duration(ns: u64) -> String {
    match ns {
        n if n >= 1_000_000_000 => format!("{:.2} s", n as f64 / 1e9),
        n if n >= 10_000_000 => format!("{} ms", n / 1_000_000),
        n if n >= 1_000_000 => format!("{:.1} ms", n as f64 / 1e6),
        n => format!("{} µs", n / 1_000),
    }
}

/// Marker after a time telling how much it can be trusted.
pub fn confidence_mark(confidence: Confidence) -> &'static str {
    match confidence {
        Confidence::Exact => " ",
        Confidence::Approximate => "~",
        Confidence::Inferred => "?",
    }
}

/// The `snake_case` name of a model enumeration value.
pub fn name<T: serde::Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub fn stage(stage: Stage) -> String {
    name(stage)
}

pub fn status_symbol(status: SourceStatus) -> String {
    match status {
        SourceStatus::Ok => paint(style::GOOD, "✓"),
        SourceStatus::Partial => paint(style::WARNING, "◐"),
        _ => paint(style::ERROR, "✗"),
    }
}

/// A short human label for a milestone.
pub fn milestone_label(kind: MilestoneKind) -> &'static str {
    match kind {
        MilestoneKind::FirmwareStart => "Firmware started",
        MilestoneKind::BootloaderLoad => "Bootloader loading",
        MilestoneKind::BootloaderStart => "Bootloader started",
        MilestoneKind::ExitBootServices => "Firmware handed over",
        MilestoneKind::KernelStart => "Kernel started",
        MilestoneKind::InitramfsStart => "Initramfs started",
        MilestoneKind::RootMounted => "Root filesystem mounted",
        MilestoneKind::InitStart => "Init system started",
        MilestoneKind::ServicesStart => "Services starting",
        MilestoneKind::GraphicsReady => "Graphics driver ready",
        MilestoneKind::DisplayManagerStart => "Login screen starting",
        MilestoneKind::SessionStart => "User session started",
        MilestoneKind::CompositorStart => "Compositor started",
        MilestoneKind::SessionReady => "Desktop ready",
        MilestoneKind::RecordingEnd => "Recording ended",
        MilestoneKind::Unknown => "Unknown milestone",
    }
}

/// The legend of the confidence markers.
pub const CONFIDENCE_LEGEND: &str = "~ approximate time   ? no clock, placed by order";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_times_and_durations() {
        assert_eq!(time(-7_070_000_000), "-7.070 s");
        assert_eq!(duration(4_750_000_000), "4.75 s");
        assert_eq!(duration(350_000_000), "350 ms");
        assert_eq!(duration(1_500_000), "1.5 ms");
        assert_eq!(duration(12_000), "12 µs");
    }
}
