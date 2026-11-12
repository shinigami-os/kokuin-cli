//! Detects the milestones of the boot from the items and the processes.

use std::collections::BTreeMap;

use kokuin_kb::{KmsgTag, ProcessRole};
use kokuin_model::{Confidence, EventKind, MilestoneKind};

use crate::{
    item::{Affinity, Item},
    processes::Processes,
};

/// A detected milestone. `item` is the `order` of the item it comes from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Found {
    pub kind: MilestoneKind,
    pub t_ns: i64,
    pub confidence: Confidence,
    pub item: Option<usize>,
}

/// Lower is better.
pub(crate) fn rank(confidence: Confidence) -> u8 {
    match confidence {
        Confidence::Exact => 0,
        Confidence::Approximate => 1,
        Confidence::Inferred => 2,
    }
}

/// Detects the milestones, sorted by time. For each kind, the most trustworthy
/// candidate wins, then the earliest.
///
/// `kmsg_available` tells whether the kernel log could be read: without it, the
/// absence of an initramfs cannot be told from the absence of evidence.
pub(crate) fn detect(items: &[Item], processes: &Processes, kmsg_available: bool) -> Vec<Found> {
    let mut candidates: Vec<Found> = Vec::new();
    let mut push = |kind, item: &Item| {
        if let Some(t_ns) = item.obs.t_ns {
            candidates.push(Found {
                kind,
                t_ns,
                confidence: item.obs.confidence,
                item: Some(item.order),
            });
        }
    };

    for item in items {
        if let Some(kind) = item.obs.milestone {
            push(kind, item);
        }
    }
    let first_tagged = |tag| items.iter().find(|item| item.tag == Some(tag));
    let has_initramfs = first_tagged(KmsgTag::InitramfsUnpack).is_some();
    if let Some(item) = first_tagged(KmsgTag::RunInit) {
        push(
            if has_initramfs {
                MilestoneKind::InitramfsStart
            } else {
                MilestoneKind::InitStart
            },
            item,
        );
    }
    if let Some(item) = first_tagged(KmsgTag::SwitchRoot) {
        push(MilestoneKind::InitStart, item);
    }
    if let Some(item) = first_tagged(KmsgTag::DiskMount) {
        push(MilestoneKind::RootMounted, item);
    }

    // The kernel always starts at 0.
    let kernel_item = items
        .iter()
        .find(|i| i.source == "kmsg" && i.obs.explain_id.as_deref() == Some("kernel.start"));
    candidates.push(Found {
        kind: MilestoneKind::KernelStart,
        t_ns: 0,
        confidence: Confidence::Exact,
        item: kernel_item.map(|i| i.order),
    });

    // Without a direct sign of the switch to the real root, estimate it.
    if !candidates
        .iter()
        .any(|c| c.kind == MilestoneKind::InitStart)
    {
        let fallback = if has_initramfs {
            candidates
                .iter()
                .find(|c| c.kind == MilestoneKind::RootMounted)
                .map(|c| Found {
                    kind: MilestoneKind::InitStart,
                    confidence: Confidence::Approximate,
                    item: None,
                    ..*c
                })
        } else if kmsg_available {
            start_item_of(items, |pid| pid == 1).map(|item| Found {
                kind: MilestoneKind::InitStart,
                t_ns: item.obs.t_ns.unwrap_or_default(),
                confidence: Confidence::Approximate,
                item: Some(item.order),
            })
        } else {
            None
        };
        candidates.extend(fallback);
    }

    if let Some(user) = processes.first_user_process() {
        let item = items
            .iter()
            .find(|i| i.obs.kind == EventKind::ProcessStart && i.process == Some(user.process.id));
        if let (Some(item), Some(t_ns)) = (item, user.process.start_ns) {
            candidates.push(Found {
                kind: MilestoneKind::SessionStart,
                t_ns,
                confidence: item.obs.confidence,
                item: Some(item.order),
            });
        }
    }

    // A compositor that is also the desktop shell is ready when it starts.
    let compositor_shells = processes
        .entries
        .iter()
        .filter(|e| e.role == Some(ProcessRole::CompositorShell) && e.affinity == Affinity::User);
    for entry in compositor_shells {
        let item = items
            .iter()
            .find(|i| i.obs.kind == EventKind::ProcessStart && i.process == Some(entry.process.id));
        if let (Some(item), Some(t_ns)) = (item, entry.process.start_ns) {
            candidates.push(Found {
                kind: MilestoneKind::SessionReady,
                t_ns,
                confidence: Confidence::Approximate,
                item: Some(item.order),
            });
        }
    }

    let mut best: BTreeMap<MilestoneKind, Found> = BTreeMap::new();
    for candidate in candidates {
        let better = best.get(&candidate.kind).is_none_or(|b| {
            (rank(candidate.confidence), candidate.t_ns) < (rank(b.confidence), b.t_ns)
        });
        if better {
            best.insert(candidate.kind, candidate);
        }
    }
    let mut found: Vec<Found> = best.into_values().collect();
    found.sort_by_key(|f| (f.t_ns, f.kind));
    found
}

fn start_item_of(items: &[Item], pid_matches: impl Fn(u32) -> bool) -> Option<&Item> {
    items
        .iter()
        .find(|i| i.obs.kind == EventKind::ProcessStart && i.obs.pid.is_some_and(&pid_matches))
}
