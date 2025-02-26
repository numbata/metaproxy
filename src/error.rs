use std::error::Error as StdError;
use std::fmt;
use std::io;
use warp::reject::Reject;

/// Custom error type for the metaproxy application
#[derive(Debug)]
pub enum Error {
    /// IO errors
    Io(io::Error),
    /// HTTP parsing errors
    HttpParse(httparse::Error),
    /// URL parsing errors
    UrlParse(url::ParseError),
    /// JSON serialization/deserialization errors
    Json(serde_json::Error),
    /// Custom error with a message
    Custom(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => write!(f, "IO error: {}", err),
            Error::HttpParse(err) => write!(f, "HTTP parse error: {}", err),
            Error::UrlParse(err) => write!(f, "URL parse error: {}", err),
            Error::Json(err) => write!(f, "JSON error: {}", err),
            Error::Custom(msg) => write!(f, "{}", msg),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Io(err) => Some(err),
            Error::HttpParse(err) => Some(err),
            Error::UrlParse(err) => Some(err),
            Error::Json(err) => Some(err),
            Error::Custom(_) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<httparse::Error> for Error {
    fn from(err: httparse::Error) -> Self {
        Error::HttpParse(err)
    }
}

impl From<url::ParseError> for Error {
    fn from(err: url::ParseError) -> Self {
        Error::UrlParse(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Json(err)
    }
}

impl From<&str> for Error {
    fn from(msg: &str) -> Self {
        Error::Custom(msg.to_string())
    }
}

impl From<String> for Error {
    fn from(msg: String) -> Self {
        Error::Custom(msg)
    }
}

/// Custom rejection type for warp
#[derive(Debug)]
pub struct CustomRejection(pub Error);

impl Reject for CustomRejection {}

/// Result type alias using our custom Error
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error as IoError, ErrorKind};

    #[test]
    fn test_from_io_error() {
        let io_err = IoError::new(ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        
        match err {
            Error::Io(_) => assert!(true),
            _ => panic!("Expected Error::Io variant"),
        }
    }

    #[test]
    fn test_from_str() {
        let err: Error = "test error".into();
        
        match err {
            Error::Custom(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected Error::Custom variant"),
        }
    }

    #[test]
    fn test_from_string() {
        let err: Error = "test error".to_string().into();
        
        match err {
            Error::Custom(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected Error::Custom variant"),
        }
    }

    #[test]
    fn test_display() {
        let err: Error = "test error".into();
        assert_eq!(format!("{}", err), "test error");
        
        let io_err = IoError::new(ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(format!("{}", err).contains("IO error"));
    }
}
