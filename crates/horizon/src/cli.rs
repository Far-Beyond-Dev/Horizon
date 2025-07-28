//! Command-line interface handling for the Horizon game server.
//!
//! This module provides command-line argument parsing and CLI interface management
//! using the `clap` crate for robust argument handling.

use clap::{Arg, Command};
use std::path::PathBuf;
use plugin_system::PluginSafetyConfig;

/// Command line arguments parsed from user input.
/// 
/// This structure holds all the command-line options that can be used to
/// override configuration file settings or provide runtime parameters.
#[derive(Debug, Clone)]
pub struct CliArgs {
    /// Path to the configuration file
    pub config_path: PathBuf,
    /// Optional override for plugin directory
    pub plugin_dir: Option<PathBuf>,
    /// Optional override for bind address
    pub bind_address: Option<String>,
    /// Optional override for log level
    pub log_level: Option<String>,
    /// Whether to force JSON log output
    pub json_logs: bool,
    /// Whether to allow plugins with different Rust compiler versions (DANGEROUS)
    pub danger_allow_unsafe_plugins: bool,
    /// Whether to allow plugins with different ABI versions (DANGEROUS)
    pub danger_allow_abi_mismatch: bool,
    /// Whether to require exact version matching including patch digits
    pub strict_versioning: bool,
}

impl CliArgs {
    /// Parses command line arguments using clap.
    /// 
    /// Sets up the command-line interface with all available options and
    /// returns a structured representation of the parsed arguments.
    /// 
    /// # Returns
    /// 
    /// A `CliArgs` instance containing all parsed command-line options.
    /// 
    /// # Panics
    /// 
    /// This function will panic if required arguments are missing, though
    /// all arguments have sensible defaults defined in the clap configuration.
    pub fn parse() -> Self {
        let matches = Command::new("Horizon Game Server")
            .version("1.0.0")
            .author("Horizon Team <team@horizon.dev>")
            .about("High-performance game server with clean plugin architecture")
            .arg(
                Arg::new("config")
                    .short('c')
                    .long("config")
                    .value_name("FILE")
                    .help("Configuration file path")
                    .default_value("config.toml"),
            )
            .arg(
                Arg::new("plugins")
                    .short('p')
                    .long("plugins")
                    .value_name("DIR")
                    .help("Plugin directory path"),
            )
            .arg(
                Arg::new("bind")
                    .short('b')
                    .long("bind")
                    .value_name("ADDRESS")
                    .help("Bind address (e.g., 127.0.0.1:8080)"),
            )
            .arg(
                Arg::new("log-level")
                    .short('l')
                    .long("log-level")
                    .value_name("LEVEL")
                    .help("Log level (trace, debug, info, warn, error)"),
            )
            .arg(
                Arg::new("json-logs")
                    .long("json-logs")
                    .help("Output logs in JSON format")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                Arg::new("danger-allow-unsafe-plugins")
                    .long("danger-allow-unsafe-plugins")
                    .help("Allow loading plugins compiled with different Rust compiler versions (MAY CAUSE CRASHES)")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                Arg::new("danger-allow-abi-mismatch")
                    .long("danger-allow-abi-mismatch")
                    .help("Allow loading plugins with different ABI versions (MAY CAUSE CRASHES OR UNDEFINED BEHAVIOR)")
                    .action(clap::ArgAction::SetTrue),
            )
            .arg(
                Arg::new("strict-versioning")
                    .long("strict-versioning")
                    .help("Require exact version matching including patch digits (default: only major.minor must match)")
                    .action(clap::ArgAction::SetTrue),
            )
            .get_matches();

        Self {
            config_path: PathBuf::from(
                matches
                    .get_one::<String>("config")
                    .expect("Default config path should always be set")
            ),
            plugin_dir: matches.get_one::<String>("plugins").map(PathBuf::from),
            bind_address: matches.get_one::<String>("bind").cloned(),
            log_level: matches.get_one::<String>("log-level").cloned(),
            json_logs: matches.get_flag("json-logs"),
            danger_allow_unsafe_plugins: matches.get_flag("danger-allow-unsafe-plugins"),
            danger_allow_abi_mismatch: matches.get_flag("danger-allow-abi-mismatch"),
            strict_versioning: matches.get_flag("strict-versioning"),
        }
    }

    /// Converts CLI arguments to plugin safety configuration.
    /// 
    /// This extracts the safety-related flags and creates a `PluginSafetyConfig`
    /// that can be used by the plugin manager.
    /// 
    /// # Returns
    /// 
    /// A `PluginSafetyConfig` with flags set based on command-line arguments.
    pub fn to_plugin_safety_config(&self) -> PluginSafetyConfig {
        PluginSafetyConfig {
            allow_unsafe_plugins: self.danger_allow_unsafe_plugins,
            allow_abi_mismatch: self.danger_allow_abi_mismatch,
            strict_versioning: self.strict_versioning,
        }
    }
}