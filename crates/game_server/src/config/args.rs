//! Command-line argument parsing
//! 
//! This module defines the command-line interface for the Distributed Games Server
//! using the clap crate for argument parsing.

use clap::Parser;
use std::path::PathBuf;

/// Command-line arguments for the Distributed Games Server
/// 
/// These arguments allow users to override configuration file settings
/// and control server behavior from the command line.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Configuration file path
    /// 
    /// Specifies the path to the TOML configuration file.
    /// If the file doesn't exist, a default configuration will be created.
    #[arg(short, long, default_value = "config.toml")]
    pub config: PathBuf,
    
    /// Server listen address
    /// 
    /// Override the listen address from the configuration file.
    /// Format: "IP:PORT" (e.g., "127.0.0.1:8080" or "0.0.0.0:3000")
    #[arg(short, long)]
    pub listen: Option<String>,
    
    /// Plugin directory
    /// 
    /// Override the plugin directory path from the configuration file.
    /// This directory will be searched for plugin files to load.
    #[arg(short, long)]
    pub plugins: Option<PathBuf>,
    
    /// Enable debug logging
    /// 
    /// When enabled, sets the logging level to debug, providing more
    /// detailed output for troubleshooting.
    #[arg(short, long)]
    pub debug: bool,
    
    /// Maximum number of players
    /// 
    /// Override the maximum player count from the configuration file.
    /// This sets the upper limit for concurrent player connections.
    #[arg(long)]
    pub max_players: Option<usize>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            config: PathBuf::from("config.toml"),
            listen: None,
            plugins: None,
            debug: false,
            max_players: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_default() {
        let args = Args::default();
        assert_eq!(args.config, PathBuf::from("config.toml"));
        assert!(!args.debug);
        assert!(args.listen.is_none());
        assert!(args.plugins.is_none());
        assert!(args.max_players.is_none());
    }
}