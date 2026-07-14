use std::fs;
use serde::Deserialize;
use std::path::PathBuf;
use crate::config::error::ConfigError;

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WhitelistFailurePolicy {
    /// If the remote whitelist endpoint cannot be reached (network error,
    /// timeout, unexpected status code), reject the connecting client. This
    /// is the safer default for a whitelist that exists to keep unknown
    /// apps off the relay.
    #[default]
    FailClosed,
    /// If the remote whitelist endpoint cannot be reached, fall back to the
    /// local `whitelist` config value. Only use this if availability matters
    /// more than strict enforcement of the remote list.
    FailOpenToLocal,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    #[serde(default = "defaults::udp_bind_address")]
    pub udp_bind_address: String,

    #[serde(default = "defaults::whitelist")]
    pub whitelist: Vec<String>,

    #[serde(default = "defaults::allowed_versions")]
    pub allowed_versions: Vec<String>,

    #[serde(default = "defaults::empty_string")]
    pub remote_whitelist_endpoint: String,

    #[serde(default = "defaults::empty_string")]
    pub remote_whitelist_token: String,

    #[serde(default)]
    pub whitelist_failure_policy: WhitelistFailurePolicy,

    #[serde(default = "defaults::empty_string")]
    pub relay_id: String,
}

/// Loads config from `config.toml` if present, otherwise from environment
/// variables (optionally via a `.env` file).
///
/// Unlike a previous version of this function, a malformed/partial
/// environment configuration is a hard error rather than being silently
/// replaced with hardcoded defaults. Silently falling back on parse
/// failure is dangerous for a relay: an operator who mistypes
/// `WHITELIST` or `ALLOWED_VERSIONS` would otherwise get a relay that
/// silently starts with an empty whitelist (i.e. allows every app) instead
/// of failing to start.
pub fn load_config(path: &str) -> Result<Config, ConfigError> {
    let config_path = PathBuf::from(path);

    if config_path.exists() {
        let config_str = fs::read_to_string(path)?;
        return Ok(toml::from_str(&config_str)?);
    }

    Ok(envy::from_env::<Config>()?)
}

mod defaults {
    pub fn udp_bind_address() -> String { "0.0.0.0:8080".to_string() }
    pub fn whitelist() -> Vec<String> { vec![] }
    pub fn allowed_versions() -> Vec<String> { vec![] }
    pub fn empty_string() -> String { "".to_string() }
}