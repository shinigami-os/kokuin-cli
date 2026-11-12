use std::{fs, path::PathBuf};

use crate::{Failure, records};

pub fn run(output: Option<PathBuf>, compact: bool) -> Result<(), Failure> {
    let record = records::capture_current_boot();
    records::warn_about_missing_sources(&record);
    let json = if compact {
        serde_json::to_string(&record)?
    } else {
        serde_json::to_string_pretty(&record)?
    };
    match output {
        Some(path) => fs::write(&path, json + "\n")
            .map_err(|err| Failure(format!("cannot write {}: {err}", path.display())))?,
        None => println!("{json}"),
    }
    Ok(())
}
