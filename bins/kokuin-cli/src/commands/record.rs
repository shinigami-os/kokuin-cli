//! `record`: records the current boot. `kokuin-init` starts it at the very beginning
//! of a boot; it is not meant to be run by hand.
//!
//! Nobody reads its output during a boot, so it reports in the kernel log
//! (`kokuin: ...`), where the boot record of that same boot shows it.

use std::{
    fs,
    io::Write,
    path::Path,
    thread,
    time::{Duration, Instant},
};

use kokuin_kb::Kb;
use kokuin_recorder::{
    Recording,
    config::{self, Config},
};
use kokuin_sources::{
    Context,
    clock::{self, ClockFile},
    kmsg, proc_events,
};

use crate::{Failure, boots, records};

/// How long to wait for `/proc` (an initramfs moves it to the root filesystem before
/// starting init; without one, the init system mounts it early).
const PROC_WAIT: Duration = Duration::from_secs(30);

pub fn run() -> Result<(), Failure> {
    let result = record();
    match &result {
        Ok(dir) => log(5, &format!("boot recorded in {dir}")),
        Err(Failure(message)) => log(4, &format!("recording failed: {message}")),
    }
    result.map(|_| ())
}

fn record() -> Result<String, Failure> {
    // Leave the session of init: its signals are not for the recorder.
    let _ = rustix::process::setsid();
    // The kernel starts init with no file mode mask: without one, everything saved
    // would be writable by everyone.
    rustix::process::umask(rustix::fs::Mode::from_raw_mode(0o022));
    wait_for(Path::new("/proc/self/stat"), PROC_WAIT)?;
    let config = Config::load(Path::new(config::PATH)).unwrap_or_else(|err| {
        log(4, &format!("{err}; using the default settings"));
        Config::default()
    });

    let recording = kokuin_recorder::record(&config.recorder, Kb::embedded());
    publish(&recording)?;

    let boot_id = boots::current_boot_id()?;
    let dir = boots::prepare(&boot_id)?;
    let ctx = Context::recording(dir.join(boots::RAW_DIR))?;
    let record = records::build(&ctx);
    ctx.save_snapshot()?;
    let json = serde_json::to_string_pretty(&record)? + "\n";
    fs::write(dir.join(boots::RECORD_FILE), json)?;

    if let Err(err) = boots::prune(config.recorder.keep_boots) {
        log(4, &format!("cannot delete old recorded boots: {err}"));
    }
    Ok(dir.display().to_string())
}

/// Puts what the recorder saw where the sources read it.
fn publish(recording: &Recording) -> Result<(), Failure> {
    fs::create_dir_all(proc_events::RECORDER_DIR)?;
    let mut lines = String::new();
    for line in &recording.lines {
        lines.push_str(&serde_json::to_string(line)?);
        lines.push('\n');
    }
    fs::write(proc_events::PATH, lines)?;
    if !recording.kmsg.is_empty() {
        fs::write(kmsg::RECORDED_PATH, &recording.kmsg)?;
    }
    if !recording.clock.is_empty() {
        let file = ClockFile {
            samples: recording.clock.clone(),
        };
        fs::write(clock::PATH, serde_json::to_string(&file)? + "\n")?;
    }
    Ok(())
}

fn wait_for(path: &Path, limit: Duration) -> Result<(), Failure> {
    let start = Instant::now();
    while !path.exists() {
        if start.elapsed() > limit {
            return Err(Failure(format!("{} never appeared", path.display())));
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

/// Writes a message to the kernel log, with a syslog level (4 warning, 5 notice).
fn log(level: u8, message: &str) {
    // One write is one message: build the line first.
    let line = format!("<{level}>kokuin: {message}\n");
    if let Ok(mut device) = fs::OpenOptions::new().write(true).open("/dev/kmsg") {
        let _ = device.write_all(line.as_bytes());
    }
}
