//! Kernel log (`/dev/kmsg`).
//!
//! Each record is `prio,seq,timestamp_us,flags[,...];message`, followed by
//! optional ` KEY=value` lines (the device the message is about).

use std::io;

use kokuin_model::{Confidence, EventKind, LogLevel, SourceStatus};

use crate::{Context, Observation, Source, SourceOutput, report};

const PATH: &str = "/dev/kmsg";
/// The whole kernel log of the boot, kept by the boot recorder before the kernel's
/// buffer could wrap (same format as `/dev/kmsg`).
pub const RECORDED_PATH: &str = "/run/kokuin/recorder/kmsg";
const EXPLAIN: &str = "kernel.kmsg";

/// The kernel log.
pub struct Kmsg;

/// One record of the kernel log.
#[derive(Debug, Clone, PartialEq)]
pub struct KmsgRecord {
    /// Syslog facility (0 = kernel).
    pub facility: u8,
    /// Syslog level (0 = emergency ... 7 = debug).
    pub level: u8,
    /// Sequence number, counting from 0 at boot.
    pub seq: u64,
    /// Time since the kernel started, in µs.
    pub timestamp_us: u64,
    /// The message.
    pub message: String,
    /// Device properties (`SUBSYSTEM`, `DEVICE`).
    pub properties: Vec<(String, String)>,
}

impl Source for Kmsg {
    fn id(&self) -> &'static str {
        "kmsg"
    }

    fn explain_id(&self) -> &'static str {
        EXPLAIN
    }

    fn kernel_options(&self) -> &'static [&'static str] {
        &["CONFIG_PRINTK"]
    }

    fn collect(&self, ctx: &Context) -> SourceOutput {
        let recorded = ctx
            .read_bytes(RECORDED_PATH)
            .map(|bytes| parse(&String::from_utf8_lossy(&bytes)))
            .ok();
        let live = ctx
            .read_device(PATH)
            .map(|bytes| parse(&String::from_utf8_lossy(&bytes)));
        let records = match (recorded, live) {
            // The recorder's copy, then what the kernel logged after it stopped.
            (Some(mut records), live) => {
                let last = records.last().map(|r| r.seq);
                let newer = live
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|r| last.is_none_or(|last| r.seq > last));
                records.extend(newer);
                records
            }
            (None, Ok(records)) => records,
            (None, Err(err)) => {
                let mut report = report::io_error(self.id(), EXPLAIN, PATH, &err);
                let restricted = ctx
                    .read_text("/proc/sys/kernel/dmesg_restrict")
                    .is_ok_and(|v| v.trim() == "1");
                if err.kind() == io::ErrorKind::PermissionDenied && restricted {
                    report.reason = Some(
                        "reading the kernel log needs root (kernel.dmesg_restrict = 1)".into(),
                    );
                }
                return SourceOutput::empty(report);
            }
        };
        let report = match records.first().map(|r| r.seq) {
            None => report::with_status(
                self.id(),
                EXPLAIN,
                SourceStatus::Failed,
                Some("the kernel log is empty".into()),
            ),
            Some(0) => report::ok(self.id(), EXPLAIN),
            Some(lost) => report::with_status(
                self.id(),
                EXPLAIN,
                SourceStatus::Partial,
                Some(format!(
                    "the first {lost} messages of the boot were overwritten because the kernel log \
                     buffer is full (enlarge it with the log_buf_len=4M kernel parameter)"
                )),
            ),
        };
        let observations = records.into_iter().map(observation).collect();
        SourceOutput {
            report,
            observations,
            processes: Vec::new(),
        }
    }
}

/// Parses `/dev/kmsg` records.
pub fn parse(text: &str) -> Vec<KmsgRecord> {
    let mut records: Vec<KmsgRecord> = Vec::new();
    for line in text.lines() {
        if let Some(property) = line.strip_prefix(' ') {
            if let (Some(record), Some((key, value))) =
                (records.last_mut(), property.split_once('='))
            {
                record.properties.push((key.to_owned(), unescape(value)));
            }
            continue;
        }
        let Some((header, message)) = line.split_once(';') else {
            continue;
        };
        let mut fields = header.split(',');
        let (Some(prio), Some(seq), Some(timestamp)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let (Ok(prio), Ok(seq), Ok(timestamp_us)) =
            (prio.parse::<u32>(), seq.parse(), timestamp.parse())
        else {
            continue;
        };
        records.push(KmsgRecord {
            facility: (prio >> 3) as u8,
            level: (prio & 7) as u8,
            seq,
            timestamp_us,
            message: unescape(message),
            properties: Vec::new(),
        });
    }
    records
}

/// The kernel escapes non-printable bytes as `\xNN`.
fn unescape(text: &str) -> String {
    if !text.contains("\\x") {
        return text.to_owned();
    }
    let mut bytes = Vec::with_capacity(text.len());
    let raw = text.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'\\' && raw.get(i + 1) == Some(&b'x') {
            if let Some(byte) = text
                .get(i + 2..i + 4)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                bytes.push(byte);
                i += 4;
                continue;
            }
        }
        bytes.push(raw[i]);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn facility_name(facility: u8) -> &'static str {
    match facility {
        0 => "kern",
        1 => "user",
        2 => "mail",
        3 => "daemon",
        4 => "auth",
        5 => "syslog",
        6 => "lpr",
        7 => "news",
        8 => "uucp",
        9 => "cron",
        10 => "authpriv",
        11 => "ftp",
        16..=23 => "local",
        _ => "other",
    }
}

fn level(level: u8) -> LogLevel {
    match level {
        0 => LogLevel::Emerg,
        1 => LogLevel::Alert,
        2 => LogLevel::Crit,
        3 => LogLevel::Err,
        4 => LogLevel::Warning,
        5 => LogLevel::Notice,
        6 => LogLevel::Info,
        _ => LogLevel::Debug,
    }
}

fn observation(record: KmsgRecord) -> Observation {
    let t_ns = i64::try_from(record.timestamp_us).ok().map(|us| us * 1_000);
    let mut observation = Observation::new(
        EventKind::Log,
        record.message.clone(),
        t_ns,
        Confidence::Exact,
    )
    .with("seq", record.seq)
    .with("facility", facility_name(record.facility));
    for (key, value) in record.properties {
        observation = observation.with(&key.to_lowercase(), value);
    }
    observation.message = Some(record.message);
    observation.level = Some(level(record.level));
    observation
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
5,0,0,-;Linux version 6.17.7-shinigami (clang) #1 SMP
6,1,12,-;Command line: root=/dev/vda2 ro quiet
6,512,1180000,-;[drm] Initialized virtio_gpu 0.1.0 for virtio0 on minor 0
 SUBSYSTEM=drm
 DEVICE=c226:0
30,600,1500000,-;udevd[340]: starting eudev-3.2.14\\x0a
";

    #[test]
    fn parses_records_and_properties() {
        let records = parse(SAMPLE);
        assert_eq!(records.len(), 4);
        assert_eq!(
            records[2].properties,
            [
                ("SUBSYSTEM".into(), "drm".into()),
                ("DEVICE".into(), "c226:0".into())
            ]
        );
        assert_eq!(records[3].facility, 3);
        assert_eq!(records[3].message, "udevd[340]: starting eudev-3.2.14\n");
    }

    #[test]
    fn builds_observations() {
        let found: Vec<_> = parse(SAMPLE).into_iter().map(observation).collect();
        assert_eq!(found[2].t_ns, Some(1_180_000_000));
        assert_eq!(found[2].data["subsystem"], "drm");
        assert_eq!(found[3].data["facility"], "daemon");
        assert_eq!(found[0].level, Some(LogLevel::Notice));
    }

    #[test]
    fn prefers_the_recorded_log_and_adds_newer_messages() {
        let root = std::env::temp_dir().join(format!("kokuin-kmsg-rec-{}", std::process::id()));
        std::fs::create_dir_all(root.join("dev")).unwrap();
        std::fs::create_dir_all(root.join("run/kokuin/recorder")).unwrap();
        std::fs::write(
            root.join("run/kokuin/recorder/kmsg"),
            "6,0,0,-;Linux version 6.18\n6,1,10,-;second\n",
        )
        .unwrap();
        std::fs::write(root.join("dev/kmsg"), "6,1,10,-;second\n6,2,90,-;later\n").unwrap();
        let output = Kmsg.collect(&Context::with_root(&root));
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(output.report.status, SourceStatus::Ok);
        let titles: Vec<_> = output
            .observations
            .iter()
            .map(|o| o.title.as_str())
            .collect();
        assert_eq!(titles, ["Linux version 6.18", "second", "later"]);
    }

    #[test]
    fn reports_overwritten_messages() {
        let root = std::env::temp_dir().join(format!("kokuin-kmsg-{}", std::process::id()));
        std::fs::create_dir_all(root.join("dev")).unwrap();
        std::fs::write(root.join("dev/kmsg"), "6,1200,5000,-;late message\n").unwrap();
        let output = Kmsg.collect(&Context::with_root(&root));
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(output.report.status, SourceStatus::Partial);
        assert!(
            output
                .report
                .reason
                .unwrap()
                .contains("first 1200 messages")
        );
    }
}
