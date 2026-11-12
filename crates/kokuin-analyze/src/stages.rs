//! Stage spans, and the attribution of events and processes to stages.

use kokuin_kb::Kb;
use kokuin_model::{Confidence, MilestoneKind as M, Stage, StageSpan};

use crate::{
    item::{Affinity, Item},
    milestones::{Found, rank},
};

/// How a stage is delimited by milestones (the first one found is used).
struct Rule {
    stage: Stage,
    starts: &'static [M],
    ends: &'static [M],
    /// The stage happens on every boot, even when its start is not measured.
    always: bool,
}

const RULES: [Rule; 9] = [
    Rule {
        stage: Stage::Firmware,
        starts: &[M::FirmwareStart],
        ends: &[M::BootloaderLoad, M::BootloaderStart],
        always: true,
    },
    Rule {
        stage: Stage::Bootloader,
        starts: &[M::BootloaderLoad, M::BootloaderStart],
        ends: &[M::ExitBootServices, M::KernelStart],
        always: true,
    },
    Rule {
        stage: Stage::Kernel,
        starts: &[M::KernelStart],
        ends: &[M::InitramfsStart, M::InitStart],
        always: true,
    },
    Rule {
        stage: Stage::Initramfs,
        starts: &[M::InitramfsStart],
        ends: &[M::InitStart],
        always: false,
    },
    Rule {
        stage: Stage::Init,
        starts: &[M::InitStart],
        ends: &[M::ServicesStart],
        always: false,
    },
    Rule {
        stage: Stage::Services,
        starts: &[M::ServicesStart],
        ends: &[],
        always: false,
    },
    Rule {
        stage: Stage::Graphical,
        starts: &[M::DisplayManagerStart],
        ends: &[M::SessionStart],
        always: false,
    },
    Rule {
        stage: Stage::Session,
        starts: &[M::SessionStart, M::CompositorStart],
        ends: &[M::SessionReady],
        always: false,
    },
    Rule {
        stage: Stage::Runtime,
        starts: &[M::SessionReady],
        ends: &[M::RecordingEnd],
        always: false,
    },
];

/// The time span of one stage, before conversion to the model.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Span {
    pub stage: Stage,
    pub start: Option<(i64, Confidence)>,
    pub end: Option<(i64, Confidence)>,
}

impl Span {
    pub(crate) fn into_model(self, kb: &Kb) -> StageSpan {
        let confidences = [self.start, self.end].into_iter().flatten().map(|(_, c)| c);
        let confidence = confidences
            .max_by_key(|&c| rank(c))
            .unwrap_or(Confidence::Inferred);
        let explain = format!("stage.{}", stage_name(self.stage));
        StageSpan {
            stage: self.stage,
            start_ns: self.start.map(|(t, _)| t),
            end_ns: self.end.map(|(t, _)| t),
            confidence,
            explain_id: kb.article(&explain).map(|_| explain),
        }
    }
}

pub(crate) fn stage_name(stage: Stage) -> String {
    serde_json::to_value(stage)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Spans of the stages that happened, in boot order.
pub(crate) fn spans(found: &[Found]) -> Vec<Span> {
    let first = |kinds: &[M]| {
        kinds
            .iter()
            .find_map(|kind| found.iter().find(|f| f.kind == *kind))
            .map(|f| (f.t_ns, f.confidence))
    };
    RULES
        .iter()
        .filter_map(|rule| {
            let start = first(rule.starts);
            (start.is_some() || rule.always).then(|| Span {
                stage: rule.stage,
                start,
                end: first(rule.ends),
            })
        })
        .collect()
}

/// The stage running at `t_ns`: the last one started at or before it.
pub(crate) fn stage_at(spans: &[Span], t_ns: Option<i64>) -> Stage {
    let Some(t) = t_ns else { return Stage::Unknown };
    spans
        .iter()
        .rfind(|span| span.start.is_some_and(|(start, _)| start <= t))
        .or_else(|| spans.first())
        .map_or(Stage::Unknown, |span| span.stage)
}

/// Adjusts a time-based stage where stages overlap: system services starting while
/// the login screen is up still belong to the services stage, and so on.
pub(crate) fn refine(stage: Stage, affinity: Affinity) -> Stage {
    match (affinity, stage) {
        (Affinity::System, Stage::Graphical | Stage::Session) => Stage::Services,
        (Affinity::Graphical, Stage::Services | Stage::Session) => Stage::Graphical,
        (Affinity::User, Stage::Services | Stage::Graphical) => Stage::Session,
        _ => stage,
    }
}

/// Gives each item without a stage the stage of its time, refined by its affinity.
/// Items without a time take the stage of the previous item of the same source.
pub(crate) fn attribute(items: &mut [Item], spans: &[Span]) {
    let has = |stage| spans.iter().any(|s| s.stage == stage);
    let mut previous: std::collections::BTreeMap<String, Stage> = Default::default();
    for item in items.iter_mut() {
        if item.obs.stage.is_none() {
            let by_time = stage_at(spans, item.obs.t_ns);
            let stage = match by_time {
                Stage::Unknown => previous.get(&item.source).copied().unwrap_or(Stage::Kernel),
                _ => refine(by_time, item.affinity),
            };
            item.obs.stage = Some(if has(stage) { stage } else { by_time });
        }
        previous.insert(item.source.clone(), item.stage());
    }
}

/// Sort key putting every item on one timeline. Items without a time are placed at
/// the start of their stage, after the timed items of that instant, in source order.
pub(crate) fn sort_key(item: &Item, spans: &[Span]) -> (i64, bool, usize, usize) {
    let stage_rank = Stage::ALL
        .iter()
        .position(|&s| s == item.stage())
        .unwrap_or(Stage::ALL.len());
    match item.obs.t_ns {
        Some(t) => (t, false, stage_rank, item.order),
        None => (
            stage_start_key(item.stage(), spans),
            true,
            stage_rank,
            item.order,
        ),
    }
}

/// Start of a stage for sorting: its measured start, else the key of the previous
/// stage, else the beginning of time.
fn stage_start_key(stage: Stage, spans: &[Span]) -> i64 {
    let mut key = i64::MIN / 2;
    for candidate in Stage::ALL {
        if let Some((start, _)) = spans
            .iter()
            .find(|s| s.stage == candidate)
            .and_then(|s| s.start)
        {
            key = start;
        }
        if candidate == stage {
            break;
        }
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(kind: M, t_ns: i64) -> Found {
        Found {
            kind,
            t_ns,
            confidence: Confidence::Exact,
            item: None,
        }
    }

    #[test]
    fn derives_spans_from_milestones() {
        let spans = spans(&[
            found(M::KernelStart, 0),
            found(M::InitStart, 1_400),
            found(M::ServicesStart, 2_900),
            found(M::DisplayManagerStart, 3_100),
        ]);
        let summary: Vec<_> = spans
            .iter()
            .map(|s| (s.stage, s.start.map(|(t, _)| t), s.end.map(|(t, _)| t)))
            .collect();
        assert_eq!(
            summary,
            [
                (Stage::Firmware, None, None),
                (Stage::Bootloader, None, Some(0)),
                (Stage::Kernel, Some(0), Some(1_400)),
                (Stage::Init, Some(1_400), Some(2_900)),
                (Stage::Services, Some(2_900), None),
                (Stage::Graphical, Some(3_100), None),
            ]
        );
        assert_eq!(stage_at(&spans, Some(3_500)), Stage::Graphical);
        assert_eq!(
            refine(stage_at(&spans, Some(3_500)), Affinity::System),
            Stage::Services
        );
        assert_eq!(stage_at(&spans, Some(-5)), Stage::Firmware);
    }
}
