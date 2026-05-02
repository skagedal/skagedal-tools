use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::constants;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub workweek: WorkWeekConfig,
    #[serde(default)]
    pub experimental_features: FeaturesConfig,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkWeekConfig {
    #[serde(default = "default_days_per_week")]
    pub days_per_week: u32,
    #[serde(default = "default_hours_per_day")]
    pub hours_per_day: u32,
}

impl Default for WorkWeekConfig {
    fn default() -> Self {
        WorkWeekConfig {
            days_per_week: default_days_per_week(),
            hours_per_day: default_hours_per_day(),
        }
    }
}

fn default_days_per_week() -> u32 {
    constants::DEFAULT_WORK_DAYS_PER_WEEK
}

fn default_hours_per_day() -> u32 {
    constants::DEFAULT_WORK_HOURS_PER_DAY
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct FeaturesConfig {
    #[serde(default)]
    pub auto_transfer_balance: bool,
}

#[derive(Debug)]
pub enum ConfigError {
    OpenFile(PathBuf, std::io::Error),
    InvalidFile(PathBuf, toml::de::Error),
}

pub fn read_config_from_str(str: &str) -> Result<Config, toml::de::Error> {
    toml::from_str(str)
}

pub fn read_config_from_path(path: &Path) -> Result<Config, ConfigError> {
    let contents =
        std::fs::read_to_string(path).map_err(|e| ConfigError::OpenFile(path.to_path_buf(), e))?;
    read_config_from_str(&contents).map_err(|e| ConfigError::InvalidFile(path.to_path_buf(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_workweek() {
        let config = read_config_from_str(
            r"
            [workweek]
            days_per_week = 4
            hours_per_day = 5
            ",
        )
        .unwrap();

        assert_eq!(config.workweek.days_per_week, 4);
        assert_eq!(config.workweek.hours_per_day, 5);
    }

    #[test]
    fn uses_defaults() {
        let config = read_config_from_str(
            r"
            [workweek]
            ",
        )
        .unwrap();

        assert_eq!(config.workweek.days_per_week, 5);
        assert_eq!(config.workweek.hours_per_day, 8);
    }

    #[test]
    fn uses_defaults_for_full_section() {
        let config = read_config_from_str("").unwrap();

        assert_eq!(config.workweek.days_per_week, 5);
        assert_eq!(config.workweek.hours_per_day, 8);
    }
}
