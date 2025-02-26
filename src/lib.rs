/*!
 * # Metaproxy 🚀
 *
 * A modular HTTP proxy server with dynamic binding configuration via a REST API.
 *
 * ## Features ✨
 *
 * - **Dynamic Proxy Bindings** 🔄: Create, update, and delete proxy bindings at runtime via REST API
 * - **HTTP Proxy** 🌐: Support for standard HTTP proxying with header adjustment
 * - **CONNECT Tunneling** 🔒: Support for HTTPS tunneling via the CONNECT method
 * - **Modular Architecture** 🧩: Clean separation of concerns for better maintainability and testability
 * - **Async I/O** ⚡: Built on Tokio for high-performance asynchronous I/O
 * - **Request Timeouts** ⏱️: Configurable timeouts for upstream requests
 *
 * ## Modules 📦
 *
 * - `api`: API routes and handlers for managing proxy bindings
 * - `config`: Configuration handling and command line argument parsing
 * - `error`: Error types and handling
 * - `proxy`: Core proxy functionality including request handling and connection management
 *
 * ## Quick Start 🚀
 *
 * ```rust
 * use metaproxy::config::Config;
 * use metaproxy::run;
 *
 * #[tokio::main]
 * async fn main() -> Result<(), Box<dyn std::error::Error>> {
 *     // Create a default configuration
 *     let config = Config::default();
 *
 *     // Run the proxy server
 *     run(config).await?;
 *
 *     Ok(())
 * }
 * ```
 *
 * ## API Usage Examples 📝
 *
 * ### Creating a Proxy Binding
 *
 * ```bash
 * curl -X POST http://127.0.0.1:8000/proxy \
 *   -H "Content-Type: application/json" \
 *   -d '{"port": 9000, "upstream": "http://127.0.0.1:8080"}'
 * ```
 *
 * ### Using the Proxy
 *
 * ```bash
 * # HTTP request through the proxy
 * curl -x http://127.0.0.1:9000 http://example.com
 *
 * # HTTPS request through the proxy
 * curl -x http://127.0.0.1:9000 https://example.com
 * ```
 *
 * ## Architecture 🏗️
 *
 * Metaproxy uses a modular architecture with the following components:
 *
 * 1. **API Server**: Handles REST API requests for managing proxy bindings
 * 2. **Proxy Manager**: Manages the lifecycle of proxy bindings
 * 3. **Connection Handler**: Processes incoming client connections
 * 4. **Request Processor**: Handles HTTP and HTTPS requests
 *
 * The proxy server uses Tokio for asynchronous I/O and Warp for the REST API.
 */

/// API module for managing proxy bindings via REST endpoints
pub mod api;
/// Configuration module for handling command line arguments and settings
pub mod config;
/// Error handling module with custom error types
pub mod error;
/// Core proxy functionality module for handling connections and data transfer
pub mod proxy;

use log::{info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::api::create_routes;
use crate::config::Config;
use crate::error::Result;
use crate::proxy::BindingMap;

/// Run the metaproxy server with the given configuration
///
/// This function initializes the proxy server with the provided configuration,
/// sets up the API routes for managing proxy bindings, and starts the server.
///
/// # Arguments
///
/// * `config` - The server configuration containing bind address and other settings
///
/// # Returns
///
/// A `Result` indicating success or an error if the server fails to start
///
/// # Example
///
/// ```no_run
/// use metaproxy::config::Config;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let config = Config::from_args();
///     metaproxy::run(config).await?;
///     Ok(())
/// }
/// ```
pub async fn run(config: Config) -> Result<()> {
    info!("Starting proxy server on {}", config.bind);

    // Log the timeout configuration
    if let Some(timeout) = config.get_request_timeout() {
        info!("Request timeout set to {} seconds", timeout.as_secs());
    } else {
        info!("No request timeout configured");
    }

    // Shared state to store active proxy bindings.
    let bindings: BindingMap = Arc::new(Mutex::new(HashMap::new()));
    info!("Initialized empty binding map");

    // Store the timeout configuration for use in proxy handlers
    let timeout = config.get_request_timeout();

    // Create API routes
    let routes = create_routes(bindings.clone(), timeout);
    info!("Created API routes");

    // Start the API server on the specified bind address.
    let bind_addr = config.get_bind_addr()?;
    info!("Binding to address: {}", bind_addr);

    let (_, server) = warp::serve(routes).bind_with_graceful_shutdown(bind_addr, async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C signal handler");
    });

    // Run the server
    info!("Server started, waiting for connections");
    server.await;
    warn!("Received shutdown signal, stopping server");
    info!("Server shutdown complete");
    Ok(())
}
