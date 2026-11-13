use std::collections::BTreeMap;

use kokuin_model::{EventKind, MilestoneKind};
use regex::Regex;
use serde::Deserialize;

use crate::KbError;

/// What a kernel log rule tells the analysis beyond a title and an explanation:
/// facts that need the context of the whole boot to be turned into milestones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KmsgTag {
    /// The kernel found UEFI firmware (captures `spec` and `vendor`).
    EfiInfo,
    /// The kernel unpacked an initramfs.
    InitramfsUnpack,
    /// The kernel started the first process (captures `path`). It is the initramfs
    /// `/init` when an initramfs was unpacked, the real init otherwise.
    RunInit,
    /// A disk filesystem was mounted (captures `device`). The first one is
    /// usually the root filesystem.
    DiskMount,
    /// The init system switched from the initramfs to the real root filesystem.
    SwitchRoot,
}

/// A rule recognizing kernel log messages.
#[derive(Debug)]
pub struct KmsgRule {
    pattern: Regex,
    title: Option<String>,
    /// Kind of event the message describes (`log` when absent).
    pub kind: Option<EventKind>,
    /// Explanation of the message.
    pub explain: Option<String>,
    /// Milestone the first matching message marks.
    pub milestone: Option<MilestoneKind>,
    /// Fact for the analysis.
    pub tag: Option<KmsgTag>,
    /// Name of the capture holding a duration in microseconds, for messages that
    /// report how long something took. The event then starts that long before the
    /// message.
    pub duration_us: Option<String>,
    /// The message tells nothing another message does not: it gives no event (it
    /// stays in the raw kernel log).
    pub hidden: bool,
}

/// The result of a [`KmsgRule`] matching a message.
#[derive(Debug)]
pub struct KmsgMatch<'a> {
    /// The rule that matched.
    pub rule: &'a KmsgRule,
    /// Title rendered from the rule template, if the rule has one.
    pub title: Option<String>,
    /// Named captures of the pattern.
    pub captures: BTreeMap<String, String>,
}

impl KmsgMatch<'_> {
    /// Duration extracted by the rule, in ns.
    pub fn duration_ns(&self) -> Option<u64> {
        let name = self.rule.duration_us.as_deref()?;
        self.captures
            .get(name)?
            .parse::<u64>()
            .ok()
            .map(|us| us * 1_000)
    }
}

impl KmsgRule {
    /// The pattern of this rule.
    pub fn pattern(&self) -> &str {
        self.pattern.as_str()
    }

    /// Matches `message` against this rule.
    pub fn apply(&self, message: &str) -> Option<KmsgMatch<'_>> {
        let found = self.pattern.captures(message)?;
        let captures: BTreeMap<String, String> = self
            .pattern
            .capture_names()
            .flatten()
            .filter_map(|name| Some((name.to_owned(), found.name(name)?.as_str().to_owned())))
            .collect();
        let title = self
            .title
            .as_deref()
            .map(|template| render(template, &captures));
        Some(KmsgMatch {
            rule: self,
            title,
            captures,
        })
    }
}

/// Replaces each `{name}` of `template` by the capture `name`.
fn render(template: &str, captures: &BTreeMap<String, String>) -> String {
    captures
        .iter()
        .fold(template.to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
}

/// Role of a program in the boot, which tells which milestone it marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessRole {
    /// Starts and watches services (runit `runsvdir`...): marks `services_start`.
    ServiceSupervisor,
    /// Creates device files and loads drivers for hardware (udev).
    DeviceManager,
    /// Text login on a terminal.
    Login,
    /// Graphical login screen: marks `display_manager_start`.
    DisplayManager,
    /// Wayland compositor or X server: marks `compositor_start` in a user session.
    Compositor,
    /// Panel, bar or desktop shell: marks `session_ready` in a user session.
    SessionShell,
    /// Compositor that is also the desktop shell (GNOME Shell): marks both
    /// `compositor_start` and `session_ready` in a user session.
    CompositorShell,
    /// Recognized only to explain it.
    Other,
}

/// A rule recognizing a program by its name.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessRule {
    /// Program names: command name (`comm`) or executable file name.
    pub names: Vec<String>,
    /// Role of the program.
    pub role: ProcessRole,
    /// Explanation of the program.
    pub explain: Option<String>,
}

impl ProcessRule {
    /// Whether `name` (a command name or an executable path) is one of this rule's
    /// programs.
    pub fn matches(&self, name: &str) -> bool {
        let file_name = name.rsplit('/').next().unwrap_or(name);
        self.names.iter().any(|known| known == file_name)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KmsgRuleFile {
    rule: Vec<RawKmsgRule>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKmsgRule {
    #[serde(rename = "match")]
    pattern: String,
    title: Option<String>,
    kind: Option<EventKind>,
    explain: Option<String>,
    milestone: Option<MilestoneKind>,
    tag: Option<KmsgTag>,
    duration_us: Option<String>,
    #[serde(default)]
    hidden: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessRuleFile {
    process: Vec<ProcessRule>,
}

pub(crate) fn parse_kmsg_rules(source: &str) -> Result<Vec<KmsgRule>, KbError> {
    let file: KmsgRuleFile = toml::from_str(source).map_err(|err| KbError {
        location: "rules/kmsg.toml".into(),
        message: err.to_string(),
    })?;
    file.rule
        .into_iter()
        .map(|raw| {
            let location = format!("rules/kmsg.toml, rule `{}`", raw.pattern);
            let pattern = Regex::new(&raw.pattern).map_err(|err| KbError {
                location: location.clone(),
                message: err.to_string(),
            })?;
            let names: Vec<&str> = pattern.capture_names().flatten().collect();
            if let Some(capture) = raw.duration_us.as_deref().filter(|c| !names.contains(c)) {
                return Err(KbError {
                    location,
                    message: format!("`duration_us` names an unknown capture `{capture}`"),
                });
            }
            Ok(KmsgRule {
                title: raw.title,
                kind: raw.kind,
                explain: raw.explain,
                milestone: raw.milestone,
                tag: raw.tag,
                duration_us: raw.duration_us,
                hidden: raw.hidden,
                pattern,
            })
        })
        .collect()
}

pub(crate) fn parse_process_rules(source: &str) -> Result<Vec<ProcessRule>, KbError> {
    let file: ProcessRuleFile = toml::from_str(source).map_err(|err| KbError {
        location: "rules/processes.toml".into(),
        message: err.to_string(),
    })?;
    Ok(file.process)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_title_and_duration() {
        let rules = parse_kmsg_rules(
            r#"
            [[rule]]
            match = '^initcall (?P<function>\S+) returned \S+ after (?P<usecs>\d+) usecs'
            title = "{function} took {usecs} µs"
            kind = "initcall"
            duration_us = "usecs"
            "#,
        )
        .unwrap();
        let found = rules[0]
            .apply("initcall pci_init+0x0/0x40 returned 0 after 1500 usecs")
            .unwrap();
        assert_eq!(
            found.title.as_deref(),
            Some("pci_init+0x0/0x40 took 1500 µs")
        );
        assert_eq!(found.duration_ns(), Some(1_500_000));
    }

    #[test]
    fn rejects_unknown_duration_capture() {
        let source = "[[rule]]\nmatch = 'x'\nduration_us = 'nope'\n";
        assert!(parse_kmsg_rules(source).is_err());
    }

    #[test]
    fn process_rule_matches_paths_and_names() {
        let rule = ProcessRule {
            names: vec!["sway".into()],
            role: ProcessRole::Compositor,
            explain: None,
        };
        assert!(rule.matches("/usr/bin/sway"));
        assert!(rule.matches("sway"));
        assert!(!rule.matches("swaybg"));
    }
}
