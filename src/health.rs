use actix_web::{HttpResponse, web};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Serialize, Deserialize)]
pub struct PoolStats {
    pub active_connections: u64,
    pub idle_connections: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryStats {
    pub allocated_mb: f64,
    pub total_mb: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
    pub pool_stats: PoolStats,
    pub memory_stats: MemoryStats,
    pub requests_total: u64,
    pub requests_last_minute: u64,
}

#[derive(Clone)]
pub struct HealthMetrics {
    start_time: Arc<Instant>,
    requests_total: Arc<AtomicU64>,
    requests_last_minute: Arc<RwLock<Vec<Instant>>>,
}

impl Default for HealthMetrics {
    fn default() -> Self {
        Self {
            start_time: Arc::new(Instant::now()),
            requests_total: Arc::new(AtomicU64::new(0)),
            requests_last_minute: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

impl HealthMetrics {
    pub fn record_request(&self) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        let now = Instant::now();
        tokio::spawn({
            let requests_last_minute = self.requests_last_minute.clone();
            async move {
                let mut requests = requests_last_minute.write().await;
                requests.push(now);

                // Remove requests older than 1 minute
                let one_minute_ago = now - Duration::from_secs(60);
                requests.retain(|&t| t >= one_minute_ago);
            }
        });
    }

    async fn get_requests_last_minute(&self) -> u64 {
        let requests = self.requests_last_minute.read().await;
        let now = Instant::now();
        let one_minute_ago = now - Duration::from_secs(60);
        requests.iter().filter(|&&t| t >= one_minute_ago).count() as u64
    }

    fn get_memory_stats() -> MemoryStats {
        // Note: This is a placeholder. In production, you'd want to use
        // a crate like sysinfo or jemallocator to get actual memory stats
        MemoryStats {
            allocated_mb: 0.0,
            total_mb: 0.0,
        }
    }

    pub async fn get_health_status(&self, pool_stats: PoolStats) -> HealthStatus {
        HealthStatus {
            status: "healthy".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: self.start_time.elapsed().as_secs(),
            pool_stats,
            memory_stats: Self::get_memory_stats(),
            requests_total: self.requests_total.load(Ordering::Relaxed),
            requests_last_minute: self.get_requests_last_minute().await,
        }
    }
}

pub async fn health_check(metrics: web::Data<HealthMetrics>) -> HttpResponse {
    // For now, we're using placeholder pool stats
    let pool_stats = PoolStats {
        active_connections: 0,
        idle_connections: 0,
    };

    let status = metrics.get_health_status(pool_stats).await;
    HttpResponse::Ok().json(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{test, App};

    #[actix_web::test]
    async fn test_health_check() {
        let metrics = web::Data::new(HealthMetrics::default());

        // Record some test requests
        metrics.record_request();
        metrics.record_request();

        // Create test app
        let app = test::init_service(
            App::new()
                .app_data(metrics.clone())
                .route("/health", web::get().to(health_check))
        ).await;

        // Send request
        let req = test::TestRequest::get().uri("/health").to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());

        // Verify response body
        let body = test::read_body(resp).await;
        let status = serde_json::from_str::<HealthStatus>(
            std::str::from_utf8(&body).unwrap()
        ).unwrap();

        assert_eq!(status.status, "healthy");
        assert_eq!(status.requests_total, 2);
    }
}
