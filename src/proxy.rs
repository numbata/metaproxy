use actix_web::{
    error::ErrorBadRequest,
    http::{header::HeaderMap, StatusCode},
    web, Error, HttpRequest, HttpResponse,
};
use bytes::Bytes;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info, warn};
use url::Url;

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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyTarget {
    pub url: Url,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl ProxyTarget {
    pub fn from_header(header_value: Option<&str>) -> Result<Self, Error> {
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
        })
    }

    pub async fn forward_request(
        &self,
        req: HttpRequest,
        body: web::Bytes,
    ) -> Result<HttpResponse, Error> {
        let client = reqwest::Client::new();
        let method = req.method().clone();
        let path = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("");

        // Construct target URL
        let target_url = format!("{}{}", self.url.as_str().trim_end_matches('/'), path);
        info!("Forwarding request to: {}", target_url);

        // Build request
        let mut proxy_req = client.request(method.clone(), &target_url);

        // Forward headers, excluding hop-by-hop headers
        for (key, value) in req.headers() {
            if !should_skip_header(key.as_str()) {
                proxy_req = proxy_req.header(key.as_str(), value);
            }
        }

        // Add body if present
        if !body.is_empty() {
            proxy_req = proxy_req.body(body.to_vec());
        }

        // Send request
        let response = proxy_req.send().await.map_err(|e| {
            error!("Failed to forward request: {}", e);
            ErrorBadRequest(ProxyError::RequestError(e))
        })?;

        // Build response
        let mut builder = HttpResponse::build(StatusCode::from_u16(response.status().as_u16()).unwrap());

        // Forward response headers
        for (key, value) in response.headers() {
            if !should_skip_header(key.as_str()) {
                builder.insert_header((key.as_str(), value));
            }
        }

        // Stream response body
        Ok(builder.streaming(response.bytes_stream().map(|result| {
            result.map_err(|e| {
                error!("Error streaming response: {}", e);
                actix_web::error::ErrorInternalServerError(e)
            })
        })))
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
        let target = ProxyTarget::from_header(Some(header)).unwrap();
        assert_eq!(target.url.as_str(), format!("{}/", header));
        assert_eq!(target.username, None);
        assert_eq!(target.password, None);
    }

    #[test]
    fn test_proxy_target_with_auth() {
        let header = "http://user:pass@proxy.example.com:8080";
        let target = ProxyTarget::from_header(Some(header)).unwrap();
        assert_eq!(target.username, Some("user".to_string()));
        assert_eq!(target.password, Some("pass".to_string()));
    }

    #[test]
    fn test_proxy_target_missing_header() {
        assert!(ProxyTarget::from_header(None).is_err());
    }

    #[test]
    fn test_proxy_target_invalid_url() {
        assert!(ProxyTarget::from_header(Some("not a url")).is_err());
    }

    #[test]
    fn test_proxy_target_invalid_scheme() {
        assert!(ProxyTarget::from_header(Some("ftp://proxy.example.com")).is_err());
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
