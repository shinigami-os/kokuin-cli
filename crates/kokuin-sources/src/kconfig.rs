//! The configuration of the running kernel (`/proc/config.gz` or
//! `/boot/config-<release>`), to tell which data sources it can provide.

use std::{collections::BTreeMap, io::Read};

use crate::Context;

/// State of a kernel option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionState {
    /// Built into the kernel (`=y`).
    BuiltIn,
    /// Built as a module (`=m`).
    Module,
    /// Not enabled.
    Disabled,
}

/// A kernel configuration.
#[derive(Debug, Clone)]
pub struct KernelConfig {
    /// Where it was read from.
    pub origin: String,
    options: BTreeMap<String, String>,
}

impl KernelConfig {
    /// Reads the configuration of the running kernel. The error says where it was
    /// looked for.
    pub fn load(ctx: &Context, release: &str) -> Result<KernelConfig, String> {
        if let Ok(compressed) = ctx.read_bytes("/proc/config.gz") {
            let mut text = String::new();
            if flate2::read::GzDecoder::new(compressed.as_slice())
                .read_to_string(&mut text)
                .is_ok()
            {
                return Ok(Self::parse("/proc/config.gz", &text));
            }
        }
        for path in [
            format!("/boot/config-{release}"),
            format!("/lib/modules/{release}/config"),
        ] {
            if let Ok(text) = ctx.read_text(&path) {
                return Ok(Self::parse(&path, &text));
            }
        }
        Err(format!(
            "the kernel configuration was not found (/proc/config.gz needs CONFIG_IKCONFIG_PROC, \
             /boot/config-{release} is installed by some distributions)"
        ))
    }

    fn parse(origin: &str, text: &str) -> KernelConfig {
        let options = text
            .lines()
            .filter_map(|line| line.split_once('='))
            .filter(|(name, _)| name.starts_with("CONFIG_"))
            .map(|(name, value)| (name.to_owned(), value.trim().to_owned()))
            .collect();
        KernelConfig {
            origin: origin.to_owned(),
            options,
        }
    }

    /// State of the option `name` (`CONFIG_...`).
    pub fn state(&self, name: &str) -> OptionState {
        match self.options.get(name).map(String::as_str) {
            Some("y") => OptionState::BuiltIn,
            Some("m") => OptionState::Module,
            _ => OptionState::Disabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_option_states() {
        let config = KernelConfig::parse("test", "CONFIG_A=y\nCONFIG_B=m\n# CONFIG_C is not set\n");
        assert_eq!(config.state("CONFIG_A"), OptionState::BuiltIn);
        assert_eq!(config.state("CONFIG_B"), OptionState::Module);
        assert_eq!(config.state("CONFIG_C"), OptionState::Disabled);
    }
}
