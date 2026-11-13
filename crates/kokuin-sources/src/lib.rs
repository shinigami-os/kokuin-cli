//! Readers for the data sources of a kokuin boot record.
//!
//! Each data source (a firmware table, the kernel log, the init system...) is a
//! [`Source`]. Collecting it never fails: when the data is missing or unreadable, the
//! [`SourceReport`] of its output says why, in words meant for the user.
//!
//! Sources only report what they read, as [`Observation`]s and [`ProcessInfo`]s. The
//! analysis (`kokuin-analyze`) merges them into a boot record: it recognizes messages,
//! detects milestones, places everything on one timeline.
//!
//! Every file access goes through a [`Context`], so the sources can read the running
//! system, a fixture directory laid out like one, or a [`snapshot`] saved earlier.

mod binary;
pub mod clock;
mod context;
pub mod efivars;
pub mod fpdt;
pub mod ftrace;
pub mod init;
pub mod kconfig;
pub mod kmsg;
pub mod proc_events;
pub mod procfs;
mod report;
pub mod snapshot;
pub mod system;
pub mod tpm;
mod uefi;

use kokuin_model::{
    Confidence, EventKind, LogLevel, MilestoneKind, SourceReport, SourceStatus, Stage,
};
use serde_json::{Map, Value};

pub use context::{Context, format_wall_clock};

/// A data source.
pub trait Source {
    /// Identifier of the source, used as `source` in events (for example `kmsg`).
    fn id(&self) -> &'static str;

    /// Identifier of the article explaining this source.
    fn explain_id(&self) -> &'static str;

    /// Kernel options (`CONFIG_...`) this source needs.
    fn kernel_options(&self) -> &'static [&'static str] {
        &[]
    }

    /// Reads the source. Never fails: problems are described in the report.
    fn collect(&self, ctx: &Context) -> SourceOutput;
}

/// What a [`Source`] gave.
#[derive(Debug, Clone)]
pub struct SourceOutput {
    /// Whether the source could be read, and why not.
    pub report: SourceReport,
    /// Things the source saw.
    pub observations: Vec<Observation>,
    /// Processes the source saw.
    pub processes: Vec<ProcessInfo>,
}

impl SourceOutput {
    /// An output with this report and no data.
    pub fn empty(report: SourceReport) -> Self {
        Self {
            report,
            observations: Vec::new(),
            processes: Vec::new(),
        }
    }

    /// Whether the source gave nothing because it is not readable here.
    pub fn is_unavailable(&self) -> bool {
        matches!(
            self.report.status,
            SourceStatus::Unavailable | SourceStatus::Failed
        )
    }
}

/// One thing a source saw. It becomes an event of the record.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    /// When it happened on the boot clock, in ns.
    pub t_ns: Option<i64>,
    /// How long it lasted, in ns.
    pub dur_ns: Option<u64>,
    /// How much the times can be trusted.
    pub confidence: Confidence,
    /// The stage, when the source knows it. Otherwise the analysis deduces it from
    /// the time.
    pub stage: Option<Stage>,
    /// What it describes.
    pub kind: EventKind,
    /// Short human-readable summary.
    pub title: String,
    /// Original message, when there is one.
    pub message: Option<String>,
    /// Severity, for log messages.
    pub level: Option<LogLevel>,
    /// PID of the process it is about.
    pub pid: Option<u32>,
    /// Article explaining it.
    pub explain_id: Option<String>,
    /// Milestone it marks, when the source knows it.
    pub milestone: Option<MilestoneKind>,
    /// Source-specific details.
    pub data: Map<String, Value>,
}

impl Observation {
    /// An observation with the required fields; the others are empty.
    pub fn new(
        kind: EventKind,
        title: impl Into<String>,
        t_ns: Option<i64>,
        confidence: Confidence,
    ) -> Self {
        Self {
            t_ns,
            dur_ns: None,
            confidence,
            stage: None,
            kind,
            title: title.into(),
            message: None,
            level: None,
            pid: None,
            explain_id: None,
            milestone: None,
            data: Map::new(),
        }
    }

    /// Sets the stage.
    pub fn in_stage(mut self, stage: Stage) -> Self {
        self.stage = Some(stage);
        self
    }

    /// Sets the explanation.
    pub fn explained_by(mut self, explain_id: &str) -> Self {
        self.explain_id = Some(explain_id.to_owned());
        self
    }

    /// Sets the milestone this observation marks.
    pub fn marks(mut self, milestone: MilestoneKind) -> Self {
        self.milestone = Some(milestone);
        self
    }

    /// Adds a `data` entry.
    pub fn with(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.data.insert(key.to_owned(), value.into());
        self
    }
}

/// A process a source saw.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcessInfo {
    /// PID.
    pub pid: u32,
    /// PID of the parent.
    pub ppid: Option<u32>,
    /// User id.
    pub uid: Option<u32>,
    /// Short command name.
    pub comm: String,
    /// Command line.
    pub cmdline: Vec<String>,
    /// Executable path.
    pub exe: Option<String>,
    /// Creation time on the boot clock, in ns.
    pub start_ns: Option<i64>,
    /// Exit time on the boot clock, in ns.
    pub end_ns: Option<i64>,
    /// Exit code.
    pub exit_code: Option<i32>,
    /// Signal that killed it.
    pub exit_signal: Option<i32>,
    /// Control group path (`/system.slice/sshd.service`), which often tells the
    /// service the process belongs to.
    pub cgroup: Option<String>,
    /// How much the times can be trusted.
    pub confidence: Option<Confidence>,
}

/// First user id of normal (human) users on most distributions.
const FIRST_NORMAL_UID: u32 = 1000;
/// `nobody`.
const NOBODY_UID: u32 = 65534;

impl ProcessInfo {
    /// Names a knowledge base rule can recognize the program by: command name,
    /// executable and first argument.
    pub fn names(&self) -> Vec<&str> {
        let mut names = vec![self.comm.as_str()];
        names.extend(self.exe.as_deref());
        names.extend(self.cmdline.first().map(String::as_str));
        names
    }
}

/// Whether a user id belongs to a person (not root, a system account or `nobody`).
pub fn is_normal_user(uid: Option<u32>) -> bool {
    uid.is_some_and(|uid| uid >= FIRST_NORMAL_UID && uid != NOBODY_UID)
}

/// The sources to read on this system, in the order of the boot. `init` is the
/// detected init system (see [`init::detect`]): its adapter is added when there
/// is one.
pub fn sources(init: Option<&str>) -> Vec<Box<dyn Source>> {
    let mut sources: Vec<Box<dyn Source>> = vec![
        Box::new(fpdt::Fpdt),
        Box::new(tpm::TpmEventLog),
        Box::new(efivars::EfiVars),
        Box::new(kmsg::Kmsg),
        Box::new(proc_events::ProcEvents),
        Box::new(procfs::Procfs),
    ];
    if let Some(adapter) = init.and_then(init::adapter) {
        sources.push(adapter);
    }
    sources
}
