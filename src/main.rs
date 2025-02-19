use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use tracing::{info, Level};

mod proxy;
use proxy::ProxyTarget;

async fn health_check() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .json(serde_json::json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION")
        }))
}

async fn handle_request(req: HttpRequest) -> Result<HttpResponse, actix_web::Error> {
    const PROXY_HEADER: &str = "x-proxy-to";

    // Extract and validate the X-Proxy-To header
    let proxy_target = ProxyTarget::from_header(
        req.headers()
            .get(PROXY_HEADER)
            .and_then(|h| h.to_str().ok()),
    )?;

    // For now, return the parsed proxy information
    Ok(HttpResponse::Ok().json(proxy_target))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging with the subscriber
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("Starting metaproxy server...");

    HttpServer::new(|| {
        App::new()
            .route("/health", web::get().to(health_check))
            .default_service(web::to(handle_request))
    })
    .bind("127.0.0.1:8081")?
    .run()
    .await
}
