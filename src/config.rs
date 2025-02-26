/*!
 * # Configuration Module
 *
 * This module handles the configuration for the metaproxy server,
 * including command line argument parsing and validation.
 */

use clap::Parser;
use std::net::SocketAddr;
use crate::error::Result;

/// Proxy server configuration
///
/// This struct represents the configuration for the metaproxy server.
/// It is populated from command line arguments using the `clap` crate.
///
/// # Example
///
/// ```no_run
/// use metaproxy::config::Config;
///
/// let config = Config::from_args();
/// println!("Binding to: {}", config.bind);
/// ```
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Config {
    /// Address to bind the proxy server to
    ///
    /// This should be in the format of `host:port`, e.g., `127.0.0.1:8000`.
    /// The server will listen for incoming connections on this address.
    #[arg(long, default_value = "127.0.0.1:8000")]
    pub bind: String,
}

impl Config {
    /// Parse command line arguments into Config
    ///
    /// This function uses the `clap` crate to parse command line arguments
    /// and populate a `Config` struct.
    ///
    /// # Returns
    ///
    /// A `Config` struct populated with values from command line arguments
    pub fn from_args() -> Self {
        Config::parse()
    }

    /// Get the socket address to bind to
    ///
    /// This function parses the `bind` string into a `SocketAddr`.
    ///
    /// # Returns
    ///
    /// A `Result` containing either the parsed `SocketAddr` or an error
    /// if the string could not be parsed.
    pub fn get_bind_addr(&self) -> Result<SocketAddr> {
        self.bind.parse()
            .map_err(|e| format!("Invalid bind address format ({}): {}", self.bind, e).into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config {
            bind: "127.0.0.1:8000".to_string(),
        };
        assert_eq!(config.bind, "127.0.0.1:8000");
    }

    #[test]
    fn test_valid_bind_addr() {
        let config = Config {
            bind: "127.0.0.1:8000".to_string(),
        };
        let addr = config.get_bind_addr().unwrap();
        assert_eq!(addr.to_string(), "127.0.0.1:8000");
    }

    #[test]
    fn test_invalid_bind_addr() {
        let config = Config {
            bind: "invalid:address".to_string(),
        };
        assert!(config.get_bind_addr().is_err());
    }
}
