use clap::Parser;
use std::net::SocketAddr;
use crate::error::Result;

/// Proxy server configuration
#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Config {
    /// Address to bind the proxy server to
    #[arg(long, default_value = "127.0.0.1:8000")]
    pub bind: String,
}

impl Config {
    /// Parse command line arguments into Config
    pub fn from_args() -> Self {
        Config::parse()
    }

    /// Get the socket address to bind to
    pub fn get_bind_addr(&self) -> Result<SocketAddr> {
        self.bind.parse()
            .map_err(|e| format!("Invalid bind address format ({}): {}", self.bind, e).into())
    }
}
