use actix_web::{
    error::ErrorBadRequest,
    http::StatusCode,
    Error, HttpRequest, HttpResponse,
};
use bytes::Bytes;
use futures::{Stream, StreamExt};
use pin_project::pin_project;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{pin::Pin, sync::Arc, task::{Context, Poll}, time::Duration};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{error, info, warn};
use url::Url;

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
    client: Client,
}

impl ProxyClient {
    pub fn new(config: ProxyConfig) -> Result<Self, Error> {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(config.request_timeout)
            .pool_idle_timeout(config.pool_idle_timeout)
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .build()
            .map_err(|e| {
                error!("Failed to create HTTP client: {}", e);
                ErrorBadRequest(ProxyError::ClientError(e))
            })?;

        Ok(Self {
            config: Arc::new(config),
            client,
        })
    }
}

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("Missing X-Proxy-To header")]
    MissingHeader,
    #[error("Invalid proxy URL: {0}")]
    InvalidUrl(String),
    #[error("Failed to parse proxy URL: {0}")]
    UrlParseError(#[from] url::ParseError),
    #[error("Failed to forward request: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("Request timeout after {} seconds", .0.as_secs())]
    Timeout(Duration),
    #[error("Failed to create HTTP client: {0}")]
    ClientError(reqwest::Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyTarget {
    pub url: Url,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(skip)]
    pub timeout: Duration,
}

#[pin_project]
pub struct StreamingBody {
    #[pin]
    rx: mpsc::Receiver<Bytes>,
}

impl Stream for StreamingBody {
    type Item = Result<Bytes, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().rx.poll_recv(cx).map(|opt| opt.map(Ok))
    }
}

impl ProxyTarget {
    pub fn from_header(header_value: Option<&str>, timeout: Duration) -> Result<Self, Error> {
        let header_value = header_value.ok_or_else(|| {
            error!("X-Proxy-To header is missing");
            ErrorBadRequest(ProxyError::MissingHeader)
        })?;

        let url = Url::parse(header_value).map_err(|e| {
            error!("Failed to parse proxy URL: {}", e);
            ErrorBadRequest(ProxyError::UrlParseError(e))
        })?;

        if !url.scheme().starts_with("http") {
            warn!("Invalid URL scheme: {}", url.scheme());
            return Err(ErrorBadRequest(ProxyError::InvalidUrl(
                "URL scheme must be http or https".to_string(),
            )));
        }

        let username = if url.username().is_empty() {
            None
        } else {
            Some(url.username().to_string())
        };

        Ok(ProxyTarget {
            username,
            password: url.password().map(|s| s.to_string()),
            url,
            timeout,
        })
    }

    pub async fn forward_request(
        &self,
        req: HttpRequest,
        body: Vec<u8>,
        proxy_client: &ProxyClient,
    ) -> Result<HttpResponse, Error> {
        let method = req.method().clone();
        let path = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("");
        let query = req.uri().query().unwrap_or("");

        // Construct target URL
        let mut target_url = format!("{}{}", self.url.as_str().trim_end_matches('/'), path);
        if !query.is_empty() {
            target_url = format!("{}?{}", target_url, query);
        }
        info!("Forwarding {} request to: {}", method, target_url);

        // Build request
        let mut proxy_req = proxy_client.client.request(method.clone(), &target_url);

        // Forward headers, excluding hop-by-hop headers
        for (key, value) in req.headers() {
            if !should_skip_header(key.as_str()) {
                proxy_req = proxy_req.header(key.as_str(), value);
            }
        }

        // Add body if present
        if !body.is_empty() {
            proxy_req = proxy_req.body(body);
        }

        // Send request and handle response
        let response = match proxy_req.send().await {
            Ok(resp) => resp,
            Err(e) if e.is_timeout() => {
                error!("Request timed out after {} seconds", self.timeout.as_secs());
                return Err(ErrorBadRequest(ProxyError::Timeout(self.timeout)));
            }
            Err(e) => {
                error!("Failed to forward request: {}", e);
                return Err(ErrorBadRequest(ProxyError::RequestError(e)));
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
            if !should_skip_header(key.as_str()) {
                builder.insert_header((key.as_str(), value));
            }
        }

        Ok(builder.streaming(StreamingBody { rx }))
    }
}

fn should_skip_header(header: &str) -> bool {
    const SKIP_HEADERS: [&str; 7] = [
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailers",
        "transfer-encoding",
    ];

    header.to_lowercase().starts_with("x-proxy-")
        || SKIP_HEADERS.contains(&header.to_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_target_from_valid_header() {
        let header = "http://proxy.example.com:8080";
        let timeout = Duration::from_secs(30);
        let target = ProxyTarget::from_header(Some(header), timeout).unwrap();
        assert_eq!(target.url.as_str(), format!("{}/", header));
        assert_eq!(target.username, None);
        assert_eq!(target.password, None);
        assert_eq!(target.timeout, timeout);
    }

    #[test]
    fn test_proxy_target_with_custom_timeout() {
        let header = "http://proxy.example.com:8080";
        let custom_timeout = Duration::from_secs(60);
        let target = ProxyTarget::from_header(Some(header), custom_timeout).unwrap();
        assert_eq!(target.timeout, custom_timeout);
    }

    #[test]
    fn test_proxy_target_with_auth() {
        let header = "http://user:pass@proxy.example.com:8080";
        let timeout = Duration::from_secs(30);
        let target = ProxyTarget::from_header(Some(header), timeout).unwrap();
        assert_eq!(target.username, Some("user".to_string()));
        assert_eq!(target.password, Some("pass".to_string()));
    }

    #[test]
    fn test_proxy_target_missing_header() {
        let timeout = Duration::from_secs(30);
        assert!(ProxyTarget::from_header(None, timeout).is_err());
    }

    #[test]
    fn test_proxy_target_invalid_url() {
        let timeout = Duration::from_secs(30);
        assert!(ProxyTarget::from_header(Some("not a url"), timeout).is_err());
    }

    #[test]
    fn test_proxy_target_invalid_scheme() {
        let timeout = Duration::from_secs(30);
        assert!(ProxyTarget::from_header(Some("ftp://proxy.example.com"), timeout).is_err());
    }

    #[test]
    fn test_should_skip_header() {
        assert!(should_skip_header("connection"));
        assert!(should_skip_header("Connection"));
        assert!(should_skip_header("x-proxy-to"));
        assert!(should_skip_header("X-Proxy-Custom"));
        assert!(!should_skip_header("content-type"));
        assert!(!should_skip_header("authorization"));
    }
}
