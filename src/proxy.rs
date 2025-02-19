use actix_web::{
    error::{Error as ActixError, ErrorBadRequest, ResponseError},
    http::StatusCode,
    web,
    HttpRequest, HttpResponse,
};
use bytes::Bytes;
use futures::{StreamExt, Stream};
use pin_project::pin_project;
use reqwest::{
    header::HeaderName,
    Client, Proxy,
};
use serde::{Deserialize, Serialize};
use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{error, info};
use url::Url;
use crate::health::{HealthMetrics, PoolStats};

#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub request_timeout: Duration,
    pub bind_host: String,
    pub bind_port: u16,
    pub pool_idle_timeout: Duration,
    pub pool_max_idle_per_host: usize,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(30),
            bind_host: "127.0.0.1".to_string(),
            bind_port: 8081,
            pool_idle_timeout: Duration::from_secs(90),
            pool_max_idle_per_host: 32,
        }
    }
}

#[derive(Clone)]
pub struct ProxyClient {
    pub config: Arc<ProxyConfig>,
    metrics: web::Data<HealthMetrics>,
}

impl ProxyClient {
    pub fn new(config: ProxyConfig, metrics: web::Data<HealthMetrics>) -> Result<Self, ActixError> {
        Ok(Self {
            config: Arc::new(config),
            metrics,
        })
    }

    pub fn get_pool_stats(&self) -> PoolStats {
        PoolStats {
            active_connections: 0,
            idle_connections: 0,
        }
    }

    fn create_client_with_proxy(&self, proxy_url: &str) -> Result<Client, ActixError> {
        let proxy = Proxy::all(proxy_url)
            .map_err(|e| {
                error!("Failed to create proxy: {}", e);
                ErrorBadRequest(e.to_string())
            })?;

        Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(self.config.request_timeout)
            .pool_idle_timeout(self.config.pool_idle_timeout)
            .pool_max_idle_per_host(self.config.pool_max_idle_per_host)
            .proxy(proxy)
            .build()
            .map_err(|e| {
                error!("Failed to create HTTP client with proxy: {}", e);
                ErrorBadRequest(e.to_string())
            })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyTarget {
    pub url: Url,
    pub timeout: Duration,
}

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Gateway error: {0}")]
    Gateway(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl ResponseError for ProxyError {
    fn status_code(&self) -> StatusCode {
        match self {
            ProxyError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ProxyError::Gateway(_) => StatusCode::BAD_GATEWAY,
            ProxyError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code()).json(serde_json::json!({
            "error": self.to_string(),
            "code": self.status_code().as_u16()
        }))
    }
}

#[pin_project]
pub struct StreamingBody {
    #[pin]
    rx: mpsc::Receiver<Bytes>,
}

impl Stream for StreamingBody {
    type Item = Result<Bytes, ActixError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        this.rx.poll_recv(cx).map(|opt| opt.map(Ok))
    }
}

impl ProxyTarget {
    pub fn from_connect(_req: &HttpRequest) -> Result<Self, ActixError> {
        Ok(Self {
            url: Url::parse("http://localhost").unwrap(), // Dummy URL for CONNECT phase
            timeout: Duration::from_secs(30),
        })
    }

    pub fn from_header(header_value: Option<&str>, timeout: Duration) -> Result<Self, ActixError> {
        let header_value = header_value.ok_or_else(|| {
            error!("X-Proxy-To header is missing");
            ErrorBadRequest("Missing X-Proxy-To header")
        })?;

        let url = Url::parse(header_value).map_err(|e| {
            error!("Failed to parse proxy URL: {}", e);
            ErrorBadRequest(format!("Invalid proxy URL: {}", e))
        })?;

        if !url.scheme().starts_with("http") {
            error!("Invalid URL scheme: {}", url.scheme());
            return Err(ErrorBadRequest("URL scheme must be http or https"));
        }

        Ok(Self { url, timeout })
    }

    pub async fn forward_request(
        &self,
        req: HttpRequest,
        body: Vec<u8>,
        proxy_client: &ProxyClient,
    ) -> Result<HttpResponse, ActixError> {
        // Record the request in metrics
        proxy_client.metrics.record_request();

        let method = req.method().clone();

        // Handle CONNECT method differently
        if method == reqwest::Method::CONNECT {
            let uri = req.uri();
            let authority = uri.authority()
                .ok_or_else(|| ErrorBadRequest("No authority in CONNECT request"))?;

            // For CONNECT requests, we just return a 200 OK to establish the tunnel
            info!("Establishing CONNECT tunnel to: {}", authority);
            
            let mut builder = HttpResponse::Ok();
            builder.insert_header(("Proxy-Connection", "Keep-Alive"));
            return Ok(builder.finish());
        }

        // For non-CONNECT requests, we need the X-Proxy-To header
        // Create a new client with the proxy configuration
        let client = proxy_client.create_client_with_proxy(self.url.as_str())?;

        // For non-CONNECT requests
        let uri = req.uri().to_string();
        let mut client_req = client.request(method.clone(), uri);

        // Forward headers
        for (key, value) in req.headers() {
            if !should_skip_header(key) {
                client_req = client_req.header(key, value);
            }
        }

        // Add the request body
        client_req = client_req.body(body);

        // Send the request
        let response = match client_req.send().await {
            Ok(resp) => resp,
            Err(e) if e.is_timeout() => {
                error!("Request timed out after {} seconds", self.timeout.as_secs());
                return Err(ErrorBadRequest(format!("Request timeout after {:?}", self.timeout)));
            }
            Err(e) => {
                error!("Failed to forward request: {}", e);
                return Err(ErrorBadRequest(format!("Failed to forward request: {}", e)));
            }
        };

        let status = response.status();
        let headers = response.headers().clone();
        info!("Received response with status: {}", status);

        // Create a channel for streaming the response body
        let (tx, rx) = mpsc::channel(2);

        // Spawn a task to stream the response body
        tokio::spawn(async move {
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                match chunk {
                    Ok(bytes) => {
                        if tx.send(bytes).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Error streaming response: {}", e);
                        break;
                    }
                }
            }
        });

        // Build response with streaming body
        let mut builder = HttpResponse::build(StatusCode::from_u16(status.as_u16()).unwrap());

        // Forward response headers
        for (key, value) in headers.iter() {
            if !should_skip_header(key) {
                builder.insert_header((key.as_str(), value));
            }
        }

        Ok(builder.streaming(StreamingBody { rx }))
    }
}

fn should_skip_header(header_name: &HeaderName) -> bool {
    const SKIP_HEADERS: [&str; 6] = [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailers",
    ];

    SKIP_HEADERS
        .iter()
        .any(|&h| h.eq_ignore_ascii_case(header_name.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_proxy_target_from_valid_header() {
        let header = "http://proxy.example.com:8080";
        let timeout = Duration::from_secs(30);
        let target = ProxyTarget::from_header(Some(header), timeout).unwrap();
        assert_eq!(target.url.as_str(), format!("{}/", header));
        assert_eq!(target.timeout, timeout);
    }

    #[test]
    fn test_proxy_target_invalid_url() {
        let header = "not a url";
        let timeout = Duration::from_secs(30);
        assert!(ProxyTarget::from_header(Some(header), timeout).is_err());
    }

    #[test]
    fn test_proxy_target_missing_header() {
        let timeout = Duration::from_secs(30);
        assert!(ProxyTarget::from_header(None, timeout).is_err());
    }

    #[test]
    fn test_proxy_target_invalid_scheme() {
        let header = "ftp://proxy.example.com:8080";
        let timeout = Duration::from_secs(30);
        assert!(ProxyTarget::from_header(Some(header), timeout).is_err());
    }

    #[test]
    fn test_proxy_target_with_custom_timeout() {
        let header = "http://proxy.example.com:8080";
        let timeout = Duration::from_secs(60);
        let target = ProxyTarget::from_header(Some(header), timeout).unwrap();
        assert_eq!(target.timeout, timeout);
    }

    #[test]
    fn test_should_skip_header() {
        assert!(should_skip_header(&HeaderName::from_static("connection")));
        assert!(should_skip_header(&HeaderName::from_static(
            "proxy-authenticate"
        )));
        assert!(!should_skip_header(
            &HeaderName::from_str("content-type").unwrap()
        ));
        assert!(!should_skip_header(
            &HeaderName::from_str("authorization").unwrap()
        ));
    }
}
