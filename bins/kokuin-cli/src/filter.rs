//! Which events to show: the filters of `timeline` and of the TUI.

use kokuin_model::{Event, EventKind, Stage};
use regex::Regex;

use crate::Failure;

/// Which events to show. Empty lists mean "all".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// Only these stages.
    pub stages: Vec<Stage>,
    /// Only events from these sources.
    pub sources: Vec<String>,
    /// Only these kinds of events.
    pub kinds: Vec<EventKind>,
    /// Only events whose title or message matches this regular expression.
    pub grep: Option<String>,
    /// Only events that have an explanation or a duration: leaves out the raw
    /// kernel log lines nothing explains.
    pub explained_only: bool,
}

impl Filter {
    /// The test an event must pass. Fails on an invalid `grep` pattern.
    pub fn matcher(&self) -> Result<impl Fn(&Event) -> bool + '_, Failure> {
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
                && (!self.explained_only || event.explain_id.is_some() || event.dur_ns.is_some())
                && pattern.as_ref().is_none_or(|re| {
                    re.is_match(&event.title)
                        || event.message.as_deref().is_some_and(|m| re.is_match(m))
                })
        })
    }
}
