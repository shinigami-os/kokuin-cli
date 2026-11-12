//! Keeps the whole kernel log while recording, before the kernel's ring buffer can
//! overwrite its first messages, and measures the gap between the kernel log's clock
//! and the boot clock (see `kokuin_sources::clock`).

use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex, PoisonError},
    thread,
};

use kokuin_sources::{clock::ClockSample, kmsg};
use rustix::{
    event::{PollFd, PollFlags, poll},
    fs::{Mode, OFlags, open},
    io::{Errno, read},
};

use crate::now_ns;

/// Width of the windows in which the best measure of the clock gap is kept.
const SAMPLE_WINDOW_NS: i64 = 1_000_000_000;

#[derive(Default)]
struct State {
    records: Vec<u8>,
    /// Best measure of each window, by window number.
    samples: BTreeMap<i64, ClockSample>,
}

/// Reads `/dev/kmsg` in the background: every record since the start of the boot,
/// then each new one as soon as it is written.
pub struct KmsgFollower {
    state: Arc<Mutex<State>>,
}

impl KmsgFollower {
    /// Opens `/dev/kmsg` and starts reading it.
    pub fn start() -> io::Result<Self> {
        let device = open(
            "/dev/kmsg",
            OFlags::RDONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let state = Arc::new(Mutex::new(State::default()));
        let shared = Arc::clone(&state);
        thread::Builder::new().name("kmsg".into()).spawn(move || {
            // Each read returns one record. Records already in the buffer come
            // first; once they are read, each new one is read right after the
            // kernel writes it, which measures the gap between the clocks.
            let mut buffer = vec![0u8; 16 * 1024];
            let mut caught_up = false;
            loop {
                match read(&device, &mut buffer[..]) {
                    Ok(0) => break,
                    Ok(n) => {
                        let read_at = now_ns();
                        let record = &buffer[..n];
                        let mut state = shared.lock().unwrap_or_else(PoisonError::into_inner);
                        state.records.extend_from_slice(record);
                        if caught_up {
                            if let Some(sample) = sample(record, read_at) {
                                keep_best(&mut state.samples, sample);
                            }
                        }
                    }
                    Err(Errno::AGAIN) => {
                        caught_up = true;
                        let mut fds = [PollFd::new(&device, PollFlags::IN)];
                        if poll(&mut fds, None).is_err() {
                            break;
                        }
                    }
                    // Records were overwritten before being read: go on with the
                    // oldest one left.
                    Err(Errno::PIPE | Errno::INTR) => {}
                    Err(_) => break,
                }
            }
        })?;
        Ok(Self { state })
    }

    /// The records read so far, in the `/dev/kmsg` format.
    pub fn records(&self) -> Vec<u8> {
        self.lock().records.clone()
    }

    /// The measured gaps between the clocks, in time order.
    pub fn clock_samples(&self) -> Vec<ClockSample> {
        self.lock().samples.values().copied().collect()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The gap measured by reading a record at `read_at_ns` on the boot clock.
fn sample(record: &[u8], read_at_ns: i64) -> Option<ClockSample> {
    let parsed = kmsg::parse(&String::from_utf8_lossy(record));
    let kmsg_ns = i64::try_from(parsed.first()?.timestamp_us).ok()? * 1_000;
    Some(ClockSample {
        kmsg_ns,
        offset_ns: read_at_ns - kmsg_ns,
    })
}

/// Reading happens after writing: the smallest gap of a window is the closest to the
/// truth.
fn keep_best(samples: &mut BTreeMap<i64, ClockSample>, sample: ClockSample) {
    let window = sample.kmsg_ns / SAMPLE_WINDOW_NS;
    samples
        .entry(window)
        .and_modify(|best| {
            if sample.offset_ns < best.offset_ns {
                *best = sample;
            }
        })
        .or_insert(sample);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_the_gap_of_a_record() {
        let sample = sample(b"6,120,2000000,-;usb 1-1: new device\n", 1_730_000_000).unwrap();
        assert_eq!(sample.kmsg_ns, 2_000_000_000);
        assert_eq!(sample.offset_ns, -270_000_000);
    }

    #[test]
    fn keeps_the_smallest_gap_of_each_window() {
        let mut samples = BTreeMap::new();
        let at = |kmsg_ns, offset_ns| ClockSample { kmsg_ns, offset_ns };
        keep_best(&mut samples, at(2_100_000_000, -260_000_000));
        keep_best(&mut samples, at(2_500_000_000, -270_000_000));
        keep_best(&mut samples, at(2_900_000_000, -250_000_000));
        keep_best(&mut samples, at(3_100_000_000, -280_000_000));
        let kept: Vec<_> = samples.values().map(|s| s.offset_ns).collect();
        assert_eq!(kept, [-270_000_000, -280_000_000]);
    }
}
