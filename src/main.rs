use actix_web::{web, App, HttpResponse, HttpServer};
use tracing::{info, Level};

async fn health_check() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("application/json")
        .json(serde_json::json!({
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION")
        }))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize logging with the subscriber
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("Starting metaproxy server...");

    HttpServer::new(|| {
        App::new()
            .route("/health", web::get().to(health_check))
    })
    .bind("127.0.0.1:8081")?
    .run()
    .await
}
