use std::{fs, path::PathBuf};

use kokuin_sources::Context;

use crate::{Failure, records};

pub fn run(output: Option<PathBuf>, compact: bool, raw: Option<PathBuf>) -> Result<(), Failure> {
    let ctx = match &raw {
        Some(dir) => Context::recording(dir).map_err(|err| {
            Failure(format!(
                "cannot save the raw data in {}: {err}",
                dir.display()
            ))
        })?,
        None => Context::live(),
    };
    let record = records::build(&ctx);
    if let Some(dir) = &raw {
        ctx.save_snapshot().map_err(|err| {
            Failure(format!(
                "cannot save the raw data in {}: {err}",
                dir.display()
            ))
        })?;
    }
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
