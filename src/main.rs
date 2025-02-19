use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use futures::StreamExt;
use std::env;
use std::time::Duration;
use tracing::{info, Level};

mod proxy;
use proxy::{ProxyConfig, ProxyTarget};

async fn health_check() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .json(serde_json::json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION")
        }))
}

async fn handle_request(
    req: HttpRequest,
    mut payload: web::Payload,
    config: web::Data<ProxyConfig>,
) -> Result<HttpResponse, actix_web::Error> {
    const PROXY_HEADER: &str = "x-proxy-to";
    
    // Extract and validate the X-Proxy-To header
    let proxy_target = ProxyTarget::from_header(
        req.headers()
            .get(PROXY_HEADER)
            .and_then(|h| h.to_str().ok()),
        config.request_timeout,
    )?;

    // Collect the request body
    let mut body = Vec::new();
    while let Some(chunk) = payload.next().await {
        body.extend_from_slice(&chunk?);
    }

    // Forward the request to the target
    proxy_target.forward_request(req, body).await
}

fn get_env_var_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging with the subscriber
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    // Read configuration from environment
    let config = ProxyConfig {
        request_timeout: Duration::from_secs(
            get_env_var_or("PROXY_REQUEST_TIMEOUT_SECS", 30)
        ),
        bind_host: env::var("PROXY_BIND_HOST")
            .unwrap_or_else(|_| "127.0.0.1".to_string()),
        bind_port: get_env_var_or("PROXY_BIND_PORT", 8081),
    };

    info!("Starting metaproxy server:");
    info!(" - Bind address: {}:{}", config.bind_host, config.bind_port);
    info!(" - Request timeout: {}s", config.request_timeout.as_secs());

    let config = web::Data::new(config);

    HttpServer::new(move || {
        App::new()
            .app_data(config.clone())
            .route("/health", web::get().to(health_check))
            .default_service(web::to(handle_request))
    })
    .bind(format!("{}:{}", config.bind_host, config.bind_port))?
    .run()
    .await
}
