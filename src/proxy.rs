use actix_web::{error::ErrorBadRequest, Error};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, warn};
use url::Url;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("Missing X-Proxy-To header")]
    MissingHeader,
    #[error("Invalid proxy URL: {0}")]
    InvalidUrl(String),
    #[error("Failed to parse proxy URL: {0}")]
    UrlParseError(#[from] url::ParseError),
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
}
