//! The kernel log's clock and the boot clock.
//!
//! The kernel stamps its log with the scheduler clock. Process events, `/proc` and
//! init systems use `CLOCK_MONOTONIC`. Both start near 0 when the kernel starts and
//! usually agree within a millisecond, but not always. In a virtual machine, the
//! monotonic clock can lag early in the boot. Time daemons (chrony, ntpd) also speed
//! it up or slow it down to correct the time, which the scheduler clock ignores.
//!
//! The gap builds up early: until the kernel switches to its final clock source
//! (`clocksource: Switched to clocksource ...`, well before the first second), the
//! monotonic clock counts timer ticks, and some get lost. After that, it only moves
//! when a time daemon starts correcting the time.
//!
//! While it runs, the boot recorder measures the gap: it reads each kernel message
//! as soon as it is written and notes the monotonic time
//! (`/run/kokuin/recorder/clock.json`). The kernel log source then places its
//! messages on the boot clock with [`KmsgClock`]. Without a recording, the kernel
//! log's own times are used as they are.

use serde::{Deserialize, Serialize};

use crate::Context;

/// The gaps measured by the boot recorder.
pub const PATH: &str = "/run/kokuin/recorder/clock.json";

/// The content of `clock.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClockFile {
    /// Measured gaps, in time order.
    pub samples: Vec<ClockSample>,
}

/// One measured gap: at `kmsg_ns` on the kernel log's clock, the boot clock read
/// `kmsg_ns + offset_ns`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockSample {
    /// Time on the kernel log's clock, in ns.
    pub kmsg_ns: i64,
    /// Boot clock minus kernel log clock, in ns.
    pub offset_ns: i64,
}

/// Converts kernel log times to the boot clock.
#[derive(Debug, Clone, PartialEq)]
pub struct KmsgClock {
    /// Sorted, starting at the kernel start where both clocks read 0.
    points: Vec<ClockSample>,
}

impl KmsgClock {
    /// The conversion measured by the boot recorder, if the boot was recorded.
    /// `settled_kmsg_ns` is when the kernel switched to its final clock source.
    pub fn load(ctx: &Context, settled_kmsg_ns: Option<i64>) -> Option<Self> {
        let text = ctx.read_text(PATH).ok()?;
        let file: ClockFile = serde_json::from_str(&text).ok()?;
        (!file.samples.is_empty()).then(|| Self::from_samples(file.samples, settled_kmsg_ns))
    }

    /// The conversion from measured gaps. The gap grows from 0 at the kernel start to
    /// the first measure when the clock source settles (`settled_kmsg_ns`; at the
    /// first measure if unknown), and follows the measures afterwards.
    pub fn from_samples(mut samples: Vec<ClockSample>, settled_kmsg_ns: Option<i64>) -> Self {
        samples.sort_by_key(|s| s.kmsg_ns);
        samples.retain(|s| s.kmsg_ns > 0);
        let mut points = vec![ClockSample {
            kmsg_ns: 0,
            offset_ns: 0,
        }];
        if let (Some(settled), Some(first)) = (settled_kmsg_ns, samples.first()) {
            if settled > 0 && settled < first.kmsg_ns {
                points.push(ClockSample {
                    kmsg_ns: settled,
                    offset_ns: first.offset_ns,
                });
            }
        }
        points.extend(samples);
        Self { points }
    }

    /// Whether a kernel message tells that the kernel switched clock source.
    pub fn is_clock_source_switch(message: &str) -> bool {
        message.starts_with("clocksource: Switched to clocksource")
    }

    /// The boot clock time of a kernel log time: the gap is interpolated between
    /// the measures, and stays at the last one after it.
    pub fn to_boot_ns(&self, kmsg_ns: i64) -> i64 {
        let after = self.points.partition_point(|p| p.kmsg_ns <= kmsg_ns);
        let offset = match (
            after.checked_sub(1).map(|i| self.points[i]),
            self.points.get(after),
        ) {
            (Some(before), Some(next)) => {
                let span = (next.kmsg_ns - before.kmsg_ns) as i128;
                let progress = (kmsg_ns - before.kmsg_ns) as i128;
                let change = (next.offset_ns - before.offset_ns) as i128;
                before.offset_ns + (change * progress / span.max(1)) as i64
            }
            (Some(last), None) => last.offset_ns,
            (None, _) => 0,
        };
        kmsg_ns + offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: i64 = 1_000_000;

    #[test]
    fn the_gap_is_reached_when_the_clock_source_settles() {
        let clock = KmsgClock::from_samples(
            vec![ClockSample {
                kmsg_ns: 2_500 * MS,
                offset_ns: -260 * MS,
            }],
            Some(700 * MS),
        );
        assert_eq!(clock.to_boot_ns(350 * MS), 220 * MS);
        assert_eq!(clock.to_boot_ns(700 * MS), 440 * MS);
        assert_eq!(clock.to_boot_ns(2_200 * MS), 1_940 * MS);
    }

    #[test]
    fn interpolates_between_measures() {
        let clock = KmsgClock::from_samples(
            vec![
                ClockSample {
                    kmsg_ns: 4_000 * MS,
                    offset_ns: -400 * MS,
                },
                ClockSample {
                    kmsg_ns: 2_000 * MS,
                    offset_ns: -300 * MS,
                },
            ],
            None,
        );
        // From 0 at the kernel start to the first measure.
        assert_eq!(clock.to_boot_ns(1_000 * MS), 850 * MS);
        assert_eq!(clock.to_boot_ns(2_000 * MS), 1_700 * MS);
        assert_eq!(clock.to_boot_ns(3_000 * MS), 2_650 * MS);
        // After the last measure, its gap holds.
        assert_eq!(clock.to_boot_ns(10_000 * MS), 9_600 * MS);
    }

    #[test]
    fn the_file_survives_json() {
        let file = ClockFile {
            samples: vec![ClockSample {
                kmsg_ns: 2_000 * MS,
                offset_ns: -270 * MS,
            }],
        };
        let json = serde_json::to_string(&file).unwrap();
        assert_eq!(serde_json::from_str::<ClockFile>(&json).unwrap(), file);
    }
}
