use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use futures::StreamExt;
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

async fn handle_request(
    req: HttpRequest,
    mut payload: web::Payload,
) -> Result<HttpResponse, actix_web::Error> {
    const PROXY_HEADER: &str = "x-proxy-to";

    // Extract and validate the X-Proxy-To header
    let proxy_target = ProxyTarget::from_header(
        req.headers()
            .get(PROXY_HEADER)
            .and_then(|h| h.to_str().ok()),
    )?;

    // Collect the request body
    let mut body = Vec::new();
    while let Some(chunk) = payload.next().await {
        body.extend_from_slice(&chunk?);
    }

    // Forward the request to the target
    proxy_target.forward_request(req, body).await
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
