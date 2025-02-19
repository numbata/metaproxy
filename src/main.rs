use actix_web::{
    error::{Error as ActixError, ErrorBadRequest},
    web, App, HttpRequest, HttpResponse, HttpServer,
};
use std::{env, time::Duration};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod health;
mod proxy;

use crate::{
    health::{health_check, HealthMetrics},
    proxy::{ProxyClient, ProxyConfig, ProxyTarget},
};

async fn handle_request(
    req: HttpRequest,
    body: web::Bytes,
    proxy_client: web::Data<ProxyClient>,
) -> Result<HttpResponse, ActixError> {
    // For CONNECT requests, we don't need the X-Proxy-To header
    let method = req.method();
    if method == "CONNECT" {
        let target = ProxyTarget::from_connect(&req)?;
        return target
            .forward_request(req, body.to_vec(), &proxy_client)
            .await;
    }

    // For non-CONNECT requests, we need the X-Proxy-To header
    let proxy_to = req
        .headers()
        .get("X-Proxy-To")
        .ok_or_else(|| ErrorBadRequest("Missing X-Proxy-To header"))?
        .to_str()
        .map_err(|_| ErrorBadRequest("Invalid X-Proxy-To header"))?;

    let target = ProxyTarget::from_header(Some(proxy_to), Duration::from_secs(30))?;
    target
        .forward_request(req, body.to_vec(), &proxy_client)
        .await
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize tracing
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .with_ansi(true)
        .pretty()
        .init();

    // Initialize logging
    tracing_subscriber::fmt::init();

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

    info!("Starting metaproxy server:");
    info!(" - Bind address: {}:{}", config.bind_host, config.bind_port);
    info!(" - Request timeout: {}s", config.request_timeout.as_secs());
    info!(
        " - Pool idle timeout: {}s",
        config.pool_idle_timeout.as_secs()
    );
    info!(
        " - Max idle connections per host: {}",
        config.pool_max_idle_per_host
    );

    let metrics = web::Data::new(HealthMetrics::default());
    let proxy_client = web::Data::new(
        ProxyClient::new(config.clone(), metrics.clone())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?,
    );

    let bind_addr = (config.bind_host.as_str(), config.bind_port);
    HttpServer::new(move || {
        App::new()
            .app_data(proxy_client.clone())
            .app_data(metrics.clone())
            .route("/health", web::get().to(health_check))
            .default_service(web::to(handle_request))
    })
    .bind(bind_addr)?
    .run()
    .await
}

fn get_env_var_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
