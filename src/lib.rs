pub mod api;
pub mod config;
pub mod error;
pub mod proxy;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::api::create_routes;
use crate::config::Config;
use crate::error::Result;
use crate::proxy::BindingMap;

/// Run the metaproxy server with the given configuration
pub async fn run(config: Config) -> Result<()> {
    println!("Starting proxy server on {}", config.bind);

    // Shared state to store active proxy bindings.
    let bindings: BindingMap = Arc::new(Mutex::new(HashMap::new()));

    // Create API routes
    let routes = create_routes(bindings.clone());

    // Start the API server on the specified bind address.
    let bind_addr = config.get_bind_addr()?;
    let (_, server) = warp::serve(routes)
        .bind_with_graceful_shutdown(bind_addr, async {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to install CTRL+C signal handler");
        });

    // Run the server
    server.await;
    
    Ok(())
}
