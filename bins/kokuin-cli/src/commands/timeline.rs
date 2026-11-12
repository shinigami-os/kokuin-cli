//! `timeline`: everything that happened during a boot, in order.

use anstream::println;
use kokuin_model::{Event, EventKind, Stage};
use regex::Regex;

use crate::{
    Failure,
    output::{self, CONFIDENCE_LEGEND, confidence_mark, duration, paint, style, time},
    records,
};

/// Which events to show. Empty lists mean "all".
pub struct Filter {
    pub stages: Vec<Stage>,
    pub sources: Vec<String>,
    pub kinds: Vec<EventKind>,
    pub grep: Option<String>,
}

impl Filter {
    fn compile(&self) -> Result<impl Fn(&Event) -> bool + '_, Failure> {
        let pattern = self
            .grep
            .as_deref()
            .map(Regex::new)
            .transpose()
            .map_err(|err| Failure(format!("invalid --grep pattern: {err}")))?;
        Ok(move |event: &Event| {
            (self.stages.is_empty() || self.stages.contains(&event.stage))
                && (self.sources.is_empty() || self.sources.contains(&event.source))
                && (self.kinds.is_empty() || self.kinds.contains(&event.kind))
                && pattern.as_ref().is_none_or(|re| {
                    re.is_match(&event.title)
                        || event.message.as_deref().is_some_and(|m| re.is_match(m))
                })
        })
    }
}

pub fn run(boot: Option<String>, filter: Filter, details: bool, json: bool) -> Result<(), Failure> {
    let record = records::load(boot.as_deref())?;
    let keep = filter.compile()?;
    let events: Vec<&Event> = record.events.iter().filter(|e| keep(e)).collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&events)?);
        return Ok(());
    }
    println!(
        "{}",
        paint(
            style::HEADING,
            format!(
                "{:>12}  {:<10} {:<13} {}",
                "TIME", "STAGE", "SOURCE", "EVENT"
            )
        )
    );
    for event in &events {
        let when = match event.t_ns {
            Some(t) => format!("{:>11}{}", time(t), confidence_mark(event.confidence)),
            None => format!("{:>11}{}", "·", confidence_mark(event.confidence)),
        };
        let length = event
            .dur_ns
            .map(|d| paint(style::DIM, format!(" ({})", duration(d))))
            .unwrap_or_default();
        println!(
            "{when}  {:<10} {:<13} {}{length}",
            output::stage(event.stage),
            event.source,
            event.title
        );
        if details {
            if let Some(message) = &event.message {
                println!("{:>39}{}", "", paint(style::DIM, message));
            }
            if let Some(id) = &event.explain_id {
                println!(
                    "{:>39}{} {}",
                    "",
                    paint(style::DIM, "explain:"),
                    paint(style::ID, id)
                );
            }
        }
    }
    println!();
    println!(
        "{}",
        paint(
            style::DIM,
            format!(
                "{} of {} events. {CONFIDENCE_LEGEND}",
                events.len(),
                record.events.len()
            )
        )
    );
    Ok(())
}
