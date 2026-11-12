//! `kokuin-cli`: records and explains the boot of a Linux system.

mod boots;
mod commands;
mod output;
mod records;

use std::{path::PathBuf, process::ExitCode};

use clap::{Parser, Subcommand, ValueEnum};
use kokuin_model::{EventKind, Stage};

/// Boot flow recorder and explainer: from the power button to a usable desktop.
///
/// Most data sources need root: run with sudo to see everything.
#[derive(Parser)]
#[command(version, about, long_about)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// When to use colors.
    #[arg(long, global = true, value_enum, default_value_t = ColorWhen::Auto)]
    color: ColorWhen,
}

#[derive(Subcommand)]
enum Command {
    /// Show what kokuin can read on this system, and why some data is missing.
    Status {
        /// Print JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Rebuild a record of the current boot and write it as JSON.
    Capture {
        /// Write to this file instead of the standard output.
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Write compact JSON on one line.
        #[arg(long)]
        compact: bool,
        /// Also save everything that was read (files, command outputs, clocks) in this
        /// new folder, to analyze the boot again later or on another machine.
        #[arg(long, value_name = "DIR")]
        raw: Option<PathBuf>,
    },
    /// Summarize a boot: system, stages, milestones, slowest steps.
    Show {
        /// Record file to read (JSON, `-` for standard input), or folder saved by
        /// `capture --raw`. Default: the current boot.
        record: Option<PathBuf>,
        /// Print the whole record as JSON instead of the summary.
        #[arg(long)]
        json: bool,
    },
    /// List everything that happened during a boot, in order.
    Timeline {
        /// Record file to read (JSON, `-` for standard input), or folder saved by
        /// `capture --raw`. Default: the current boot.
        record: Option<PathBuf>,
        /// Only these stages (repeatable).
        #[arg(long, value_parser = parse_enum::<Stage>)]
        stage: Vec<Stage>,
        /// Only events from these sources (repeatable), for example `kmsg`.
        #[arg(long)]
        source: Vec<String>,
        /// Only these kinds of events (repeatable), for example `initcall`.
        #[arg(long, value_parser = parse_enum::<EventKind>)]
        kind: Vec<EventKind>,
        /// Only events whose title or message matches this regular expression.
        #[arg(long)]
        grep: Option<String>,
        /// Also show the original message and the explanation of each event.
        #[arg(short, long)]
        details: bool,
        /// Print the selected events as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Explain a step of the boot. Without an id, list the explanations.
    Explain {
        /// Identifier of the explanation, as shown by the other commands (for example
        /// `firmware.fpdt`).
        id: Option<String>,
        /// Print JSON (title, summary, Markdown body, related, references).
        #[arg(long)]
        json: bool,
    },
    /// Print the JSON Schema of boot records.
    Schema,
    /// Record the current boot. Started by kokuin-init at the beginning of a boot.
    #[command(hide = true)]
    Record,
}

#[derive(Clone, Copy, ValueEnum)]
enum ColorWhen {
    Auto,
    Always,
    Never,
}

/// Parses a `snake_case` value of a model enumeration, refusing `unknown`.
fn parse_enum<T: serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    text: &str,
) -> Result<T, String> {
    let value: T = serde_json::from_value(serde_json::Value::String(text.to_owned()))
        .map_err(|_| format!("unknown value `{text}`"))?;
    let unknown: Option<T> =
        serde_json::from_value(serde_json::Value::String("unknown".into())).ok();
    if Some(&value) == unknown.as_ref() {
        return Err(format!("unknown value `{text}`"));
    }
    Ok(value)
}

/// An error to report to the user.
#[derive(Debug)]
pub struct Failure(pub String);

impl<E: std::error::Error> From<E> for Failure {
    fn from(err: E) -> Self {
        Failure(err.to_string())
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    anstream::ColorChoice::write_global(match cli.color {
        ColorWhen::Auto => anstream::ColorChoice::Auto,
        ColorWhen::Always => anstream::ColorChoice::Always,
        ColorWhen::Never => anstream::ColorChoice::Never,
    });

    let result = match cli.command {
        Command::Status { json } => commands::status::run(json),
        Command::Capture {
            output,
            compact,
            raw,
        } => commands::capture::run(output, compact, raw),
        Command::Show { record, json } => commands::show::run(record, json),
        Command::Timeline {
            record,
            stage,
            source,
            kind,
            grep,
            details,
            json,
        } => {
            let filter = commands::timeline::Filter {
                stages: stage,
                sources: source,
                kinds: kind,
                grep,
            };
            commands::timeline::run(record, filter, details, json)
        }
        Command::Explain { id, json } => commands::explain::run(id, json),
        Command::Schema => commands::schema::run(),
        Command::Record => commands::record::run(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure(message)) => {
            anstream::eprintln!(
                "{}error:{} {message}",
                output::style::ERROR.render(),
                output::style::ERROR.render_reset()
            );
            ExitCode::FAILURE
        }
    }
}
