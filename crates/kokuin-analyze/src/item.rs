use kokuin_kb::KmsgTag;
use kokuin_model::{Event, EventId, ProcessId, Stage};
use kokuin_sources::Observation;

/// Which part of the system something belongs to. It refines the stage given by the
/// time when stages overlap (services keep starting during the graphical stage).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Affinity {
    /// Nothing known: the time decides.
    None,
    /// System services and daemons.
    System,
    /// The login screen and the graphics stack.
    Graphical,
    /// The user session.
    User,
}

/// An observation on its way to becoming an event.
#[derive(Debug, Clone)]
pub(crate) struct Item {
    pub obs: Observation,
    /// Id of the source that produced it.
    pub source: String,
    /// Fact recognized by a kernel log rule.
    pub tag: Option<KmsgTag>,
    /// The process it is about.
    pub process: Option<ProcessId>,
    pub affinity: Affinity,
    /// Position before sorting: keeps the order of the source for equal times, and
    /// identifies the item once sorted.
    pub order: usize,
}

impl Item {
    pub(crate) fn new(obs: Observation, source: &str) -> Self {
        Self {
            obs,
            source: source.to_owned(),
            tag: None,
            process: None,
            affinity: Affinity::None,
            order: 0,
        }
    }

    pub(crate) fn stage(&self) -> Stage {
        self.obs.stage.unwrap_or(Stage::Unknown)
    }

    pub(crate) fn into_event(self, id: EventId) -> Event {
        let obs = self.obs;
        let mut event = Event::new(
            id,
            obs.stage.unwrap_or(Stage::Unknown),
            self.source,
            obs.kind,
            obs.title,
            obs.t_ns,
            obs.confidence,
        );
        event.dur_ns = obs.dur_ns;
        event.message = obs.message.filter(|message| *message != event.title);
        event.level = obs.level;
        event.process = self.process;
        event.explain_id = obs.explain_id;
        event.data = obs.data;
        event
    }
}
