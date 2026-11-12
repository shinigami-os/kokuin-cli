//! Recognizes kernel log messages with the knowledge base rules.

use kokuin_kb::Kb;

use crate::item::Item;

/// Applies the first matching rule to each kernel log message: title, kind,
/// explanation, milestone, tag, duration, and the captured values as data.
pub(crate) fn apply_kmsg_rules(items: &mut [Item], kb: &Kb) {
    for item in items.iter_mut().filter(|item| item.source == "kmsg") {
        let Some(message) = item.obs.message.as_deref() else {
            continue;
        };
        let Some(found) = kb.match_kmsg(message) else {
            continue;
        };
        let rule = found.rule;

        if let Some(title) = found.title.clone() {
            item.obs.title = title;
        }
        if let Some(kind) = rule.kind {
            item.obs.kind = kind;
        }
        if let Some(explain) = &rule.explain {
            item.obs.explain_id = Some(explain.clone());
        }
        if item.obs.milestone.is_none() {
            item.obs.milestone = rule.milestone;
        }
        item.tag = rule.tag;
        if let Some(duration) = found.duration_ns() {
            // The message is printed when the work ends.
            item.obs.dur_ns = Some(duration);
            item.obs.t_ns = item.obs.t_ns.map(|end| end - duration as i64);
        }
        for (name, value) in found.captures {
            item.obs.data.entry(name).or_insert(value.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use kokuin_model::{Confidence, EventKind, MilestoneKind};
    use kokuin_sources::Observation;

    use super::*;

    fn kmsg(message: &str, t_ns: i64) -> Item {
        let mut obs = Observation::new(EventKind::Log, message, Some(t_ns), Confidence::Exact);
        obs.message = Some(message.to_owned());
        Item::new(obs, "kmsg")
    }

    #[test]
    fn recognizes_messages() {
        let mut items = vec![
            kmsg(
                "[drm] Initialized i915 1.6.0 for 0000:00:02.0 on minor 0",
                1_000_000_000,
            ),
            kmsg(
                "initcall pci_init+0x0/0x40 returned 0 after 1500 usecs",
                2_000_000_000,
            ),
            kmsg("something unknown", 3_000_000_000),
        ];
        apply_kmsg_rules(&mut items, Kb::embedded());

        assert_eq!(items[0].obs.title, "Graphics driver i915 is ready");
        assert_eq!(items[0].obs.milestone, Some(MilestoneKind::GraphicsReady));
        assert_eq!(items[0].obs.data["driver"], "i915");
        assert_eq!(items[1].obs.kind, EventKind::Initcall);
        assert_eq!(
            (items[1].obs.t_ns, items[1].obs.dur_ns),
            (Some(1_998_500_000), Some(1_500_000))
        );
        assert_eq!(items[2].obs.title, "something unknown");
    }
}
