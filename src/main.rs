use actix_web::{web, App, HttpServer};
use std::{env, time::Duration};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use crate::{
    health::{health_check, HealthMetrics},
    proxy::{ProxyClient, ProxyConfig},
};

mod health;
mod proxy;

fn get_env_var_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing
    FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .with_ansi(true)
        .pretty()
        .init();

    // Read configuration from environment
    let config = ProxyConfig {
        request_timeout: Duration::from_secs(get_env_var_or("PROXY_REQUEST_TIMEOUT_SECS", 30)),
        bind_host: env::var("PROXY_BIND_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
        bind_port: get_env_var_or("PROXY_BIND_PORT", 8081),
        pool_idle_timeout: Duration::from_secs(get_env_var_or("PROXY_POOL_IDLE_TIMEOUT_SECS", 90)),
        pool_max_idle_per_host: get_env_var_or("PROXY_POOL_MAX_IDLE_PER_HOST", 32),
    };

    info!(
        host = %config.bind_host,
        port = config.bind_port,
        timeout_secs = ?config.request_timeout.as_secs(),
        pool_idle_timeout_secs = ?config.pool_idle_timeout.as_secs(),
        pool_max_idle_per_host = config.pool_max_idle_per_host,
        "Starting MetaProxy server..."
    );

    let metrics = web::Data::new(HealthMetrics::default());
    let proxy_client = web::Data::new(
        ProxyClient::new(config.clone(), metrics.clone())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?,
    );

    let bind_addr = format!("http://{}:{}", config.bind_host, config.bind_port);
    let config_data = web::Data::new(config.clone());

    let server = HttpServer::new(move || {
        App::new()
            .app_data(proxy_client.clone())
            .app_data(metrics.clone())
            .app_data(config_data.clone())
            .route("/health", web::get().to(health_check))
            .default_service(web::to(proxy::handle_request))
    })
    .bind((config.bind_host, config.bind_port))?
    .run();

    info!(address = %bind_addr, "🚀 MetaProxy is ready to accept connections");

    server.await
}
