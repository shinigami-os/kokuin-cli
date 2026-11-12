//! Builds kokuin boot records.
//!
//! The sources (`kokuin-sources`) report raw observations. This crate turns them into
//! a [`BootRecord`]:
//!
//! 1. recognize kernel log messages with the knowledge base rules (`enrich`);
//! 2. rebuild the process tree, recognize programs, attribute them to services
//!    (`processes`);
//! 3. detect the milestones (`milestones`) and derive the stage spans (`stages`);
//! 4. attribute every event to a stage, order everything on one timeline, assign ids;
//! 5. identify the bootloader and complete the system description (`components`).

mod components;
mod enrich;
mod item;
mod milestones;
mod processes;
mod stages;

use kokuin_kb::Kb;
use kokuin_model::{BootRecord, CaptureInfo, ComponentInfo, EventId, Milestone, SourceStatus};
use kokuin_sources::{Context, SourceOutput, system::SystemInfo};

use crate::item::Item;

/// Everything collected for one boot.
#[derive(Debug, Clone)]
pub struct Collected {
    /// Identity of the boot and of the system.
    pub system: SystemInfo,
    /// The init system, if it was identified.
    pub init: Option<ComponentInfo>,
    /// Output of each source, in boot order.
    pub outputs: Vec<SourceOutput>,
}

/// Reads every data source of the system described by `ctx`.
pub fn collect(ctx: &Context) -> Collected {
    let system = kokuin_sources::system::collect(ctx);
    let init = kokuin_sources::init::detect(ctx);
    let outputs = kokuin_sources::sources(init.as_ref().map(|i| i.id.as_str()))
        .iter()
        .map(|source| source.collect(ctx))
        .collect();
    Collected {
        system,
        init,
        outputs,
    }
}

/// Builds the boot record from what was collected.
pub fn build_record(collected: Collected, capture: CaptureInfo, kb: &Kb) -> BootRecord {
    let Collected {
        system,
        init,
        outputs,
    } = collected;
    let mut record = BootRecord::new(system.boot, capture);

    // Reports in boot order: DMI goes with the firmware sources, before the kernel log.
    record.sources = outputs.iter().map(|o| o.report.clone()).collect();
    let dmi_position = record
        .sources
        .iter()
        .position(|r| r.id == "kmsg")
        .unwrap_or(record.sources.len());
    record.sources.insert(dmi_position, system.dmi_report);
    let kmsg_available = record
        .sources
        .iter()
        .any(|r| r.id == "kmsg" && matches!(r.status, SourceStatus::Ok | SourceStatus::Partial));

    // 1. Observations, recognized.
    let mut items: Vec<Item> = Vec::new();
    let mut process_infos = Vec::new();
    for output in outputs {
        let source = output.report.id;
        items.extend(
            output
                .observations
                .into_iter()
                .map(|obs| Item::new(obs, &source)),
        );
        process_infos.extend(
            output
                .processes
                .into_iter()
                .map(|info| (source.clone(), info)),
        );
    }
    enrich::apply_kmsg_rules(&mut items, kb);

    // 2. Processes, and their start events.
    let mut processes = processes::build(process_infos, kb);
    items.append(&mut processes.start_items);
    for (order, item) in items.iter_mut().enumerate() {
        item.order = order;
        if item.process.is_none() {
            item.process = item
                .obs
                .pid
                .and_then(|pid| processes.find(pid, item.obs.t_ns));
        }
    }

    // 3. Milestones and stages.
    let found = milestones::detect(&items, &processes, kmsg_available);
    let spans = stages::spans(&found);

    // 4. Stages of events and processes, one timeline, ids in timeline order.
    stages::attribute(&mut items, &spans);
    for entry in &mut processes.entries {
        let by_time = stages::stage_at(&spans, entry.process.start_ns);
        entry.process.stage = stages::refine(by_time, entry.affinity);
    }
    items.sort_by_key(|item| stages::sort_key(item, &spans));
    let mut position_of = vec![0; items.len()];
    for (position, item) in items.iter().enumerate() {
        position_of[item.order] = position;
    }
    let event_id = |position: usize| EventId(position as u64 + 1);

    record.milestones = found
        .iter()
        .map(|m| {
            let item = m.item.map(|order| &items[position_of[order]]);
            Milestone {
                kind: m.kind,
                t_ns: m.t_ns,
                confidence: m.confidence,
                event: m.item.map(|order| event_id(position_of[order])),
                explain_id: item.and_then(|i| i.obs.explain_id.clone()),
            }
        })
        .collect();
    record.stages = spans.into_iter().map(|span| span.into_model(kb)).collect();

    // 5. System description.
    components::complete(&mut record, init, &items, kb);

    record.processes = processes
        .entries
        .into_iter()
        .map(|entry| entry.process)
        .collect();
    record.events = items
        .into_iter()
        .enumerate()
        .map(|(position, item)| item.into_event(event_id(position)))
        .collect();
    record
}

#[cfg(test)]
mod tests;
