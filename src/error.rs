/*!
 * # Error Handling Module
 *
 * This module defines custom error types and handling for the metaproxy application.
 * It provides a unified error type that can be used throughout the application,
 * with conversions from common error types.
 */

use std::error::Error as StdError;
use std::fmt;
use std::io;
use warp::reject::Reject;

/// Custom error type for the metaproxy application
///
/// This enum represents all possible errors that can occur in the metaproxy application.
/// It implements the standard Error trait and provides conversions from common error types.
#[derive(Debug)]
pub enum Error {
    /// IO errors from the standard library
    Io(io::Error),
    /// HTTP parsing errors from the httparse crate
    HttpParse(httparse::Error),
    /// URL parsing errors from the url crate
    UrlParse(url::ParseError),
    /// JSON serialization/deserialization errors from serde_json
    Json(serde_json::Error),
    /// Custom error with a message string
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

/// Convert from io::Error to our custom Error type
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}

/// Convert from httparse::Error to our custom Error type
impl From<httparse::Error> for Error {
    fn from(err: httparse::Error) -> Self {
        Error::HttpParse(err)
    }
}

/// Convert from url::ParseError to our custom Error type
impl From<url::ParseError> for Error {
    fn from(err: url::ParseError) -> Self {
        Error::UrlParse(err)
    }
}

/// Convert from serde_json::Error to our custom Error type
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Json(err)
    }
}

/// Convert from &str to our custom Error type
impl From<&str> for Error {
    fn from(msg: &str) -> Self {
        Error::Custom(msg.to_string())
    }
}

/// Convert from String to our custom Error type
impl From<String> for Error {
    fn from(msg: String) -> Self {
        Error::Custom(msg)
    }
}

/// Custom rejection type for warp
///
/// This type is used to convert our custom Error type into a warp::Rejection,
/// which can be used in warp filters.
#[derive(Debug)]
pub struct CustomRejection(pub Error);

impl Reject for CustomRejection {}

/// Result type alias using our custom Error
///
/// This type alias makes it easier to use our custom Error type throughout the application.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error as IoError, ErrorKind};

    #[test]
    fn test_from_io_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::Other, "test");
        let error = Error::from(io_error);
        match error {
            Error::Io(_) => {} // Just check that it's the right variant
            _ => panic!("Expected Io error"),
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
