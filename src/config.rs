/*!
 * # Configuration Module
 *
 * This module handles the configuration for the metaproxy server,
 * including command line argument parsing and validation.
 */

use crate::error::Result;
use clap::Parser;
use std::net::SocketAddr;
use std::time::Duration;

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

    /// Request timeout in seconds
    ///
    /// If a request to the upstream server doesn't complete within this time,
    /// it will be canceled and an error will be returned to the client.
    /// Set to 0 for no timeout.
    #[arg(long, default_value = "30")]
    pub request_timeout: u64,
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
    /// A `Result` containing the parsed `SocketAddr` or an error if parsing fails
    pub fn get_bind_addr(&self) -> Result<SocketAddr> {
        self.bind
            .parse()
            .map_err(|e| format!("Invalid bind address: {}", e).into())
    }

    /// Get the request timeout as a Duration
    ///
    /// This function converts the request_timeout value to a Duration.
    /// If the timeout is 0, it returns None (no timeout).
    ///
    /// # Returns
    ///
    /// An Option containing the timeout Duration, or None if no timeout is set
    pub fn get_request_timeout(&self) -> Option<Duration> {
        if self.request_timeout == 0 {
            None
        } else {
            Some(Duration::from_secs(self.request_timeout))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config {
            bind: "127.0.0.1:8000".to_string(),
            request_timeout: 30,
        };
        assert_eq!(config.bind, "127.0.0.1:8000");
        assert_eq!(config.request_timeout, 30);
    }

    #[test]
    fn test_valid_bind_addr() {
        let config = Config {
            bind: "127.0.0.1:8000".to_string(),
            request_timeout: 30,
        };
        let addr = config.get_bind_addr().unwrap();
        assert_eq!(addr.to_string(), "127.0.0.1:8000");
    }

    #[test]
    fn test_invalid_bind_addr() {
        let config = Config {
            bind: "invalid:address".to_string(),
            request_timeout: 30,
        };
        assert!(config.get_bind_addr().is_err());
    }

    #[test]
    fn test_request_timeout() {
        let config = Config {
            bind: "127.0.0.1:8000".to_string(),
            request_timeout: 30,
        };
        let timeout = config.get_request_timeout().unwrap();
        assert_eq!(timeout.as_secs(), 30);
    }

    #[test]
    fn test_no_request_timeout() {
        let config = Config {
            bind: "127.0.0.1:8000".to_string(),
            request_timeout: 0,
        };
        assert!(config.get_request_timeout().is_none());
    }
}
