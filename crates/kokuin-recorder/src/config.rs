//! `/etc/kokuin/config.toml`: every setting is optional, and so is the file.
//!
//! ```toml
//! [recorder]
//! settle_secs = 30   # keep recording this long after the desktop is ready
//! timeout_secs = 180 # stop at the latest this long after the kernel started
//! keep_boots = 10    # recorded boots kept in /var/lib/kokuin/boots
//! ```

use std::{fs, io, path::Path};

use serde::Deserialize;

/// Where the configuration is read from.
pub const PATH: &str = "/etc/kokuin/config.toml";

/// The whole configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The boot recorder.
    pub recorder: RecorderConfig,
}

/// Settings of the boot recorder.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecorderConfig {
    /// Seconds to keep recording after the desktop is ready.
    pub settle_secs: u64,
    /// Seconds after the kernel start when the recording stops in any case.
    pub timeout_secs: u64,
    /// How many recorded boots to keep.
    pub keep_boots: usize,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            settle_secs: 30,
            timeout_secs: 180,
            keep_boots: 10,
        }
    }
}

impl Config {
    /// Reads the configuration file; the defaults when there is none.
    pub fn load(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map_err(|err| format!("{}: {err}", path.display())),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(format!("cannot read {}: {err}", path.display())),
        }
    }

    /// Parses the content of a configuration file.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_settings_keep_their_default() {
        let config = Config::parse("[recorder]\nsettle_secs = 5\n").unwrap();
        assert_eq!(config.recorder.settle_secs, 5);
        assert_eq!(config.recorder.timeout_secs, 180);
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn a_misspelled_setting_is_an_error() {
        assert!(Config::parse("[recorder]\nsettle = 5\n").is_err());
    }

    #[test]
    fn no_file_means_the_defaults() {
        assert_eq!(
            Config::load(Path::new("/nonexistent/config.toml")),
            Ok(Config::default())
        );
    }
}
