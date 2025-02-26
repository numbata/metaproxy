/*!
 * # Proxy Module
 *
 * This module provides the core proxy functionality for the application.
 * It handles TCP connections, HTTP/HTTPS proxying, and manages proxy bindings.
 *
 * ## Features
 *
 * - TCP connection handling
 * - HTTP/HTTPS proxying
 * - Dynamic proxy binding management
 * - Request timeouts for upstream connections
 */

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;
use log::{info, warn, debug};
use crate::error::{Result, Error};
use url::Url;
use base64::Engine;

/// A map of port numbers to proxy bindings
pub type BindingMap = Arc<Mutex<HashMap<u16, ProxyBinding>>>;

/// A proxy binding that maps a port to an upstream server
pub struct ProxyBinding {
    /// The port number for this binding
    pub port: u16,
    /// The upstream server address
    pub upstream: Arc<Mutex<String>>,
    /// A channel to signal shutdown of this binding
    pub shutdown_tx: oneshot::Sender<()>,
}

/// Spawn a proxy listener on the given port
///
/// This function creates a TCP listener on the specified port and handles
/// incoming connections by forwarding them to the configured upstream server.
///
/// # Arguments
///
/// * `port` - The port number to listen on
/// * `upstream` - The upstream server address
/// * `shutdown_rx` - A channel to signal shutdown of this listener
/// * `request_timeout` - Optional timeout for upstream connections
///
/// # Returns
///
/// A result indicating success or failure
pub async fn spawn_proxy_listener(port: u16, upstream: Arc<Mutex<String>>, shutdown_rx: oneshot::Receiver<()>, request_timeout: Option<Duration>) -> Result<()> {
    // Create a TCP listener on the specified port
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    info!("Proxy listener started on {}", addr);

    tokio::select! {
        result = handle_connections(listener, upstream, request_timeout) => {
            result
        }
        _ = shutdown_rx => {
            info!("Shutting down proxy listener on port {}", port);
            Ok(())
        }
    }
}

/// Handle incoming connections on a TCP listener
///
/// This function accepts connections on the given listener and spawns
/// a task to handle each connection.
///
/// # Arguments
///
/// * `listener` - The TCP listener to accept connections from
/// * `upstream` - The upstream server address
/// * `request_timeout` - Optional timeout for upstream connections
///
/// # Returns
///
/// A result indicating success or failure
async fn handle_connections(listener: TcpListener, upstream: Arc<Mutex<String>>, request_timeout: Option<Duration>) -> Result<()> {
    loop {
        // Accept a new connection
        let (client_stream, client_addr) = listener.accept().await?;
        debug!("Accepted connection from {}", client_addr);

        // Get the current upstream address
        let upstream_addr = {
            let upstream_lock = upstream.lock().await;
            (*upstream_lock).clone()
        };

        // Spawn a task to handle the connection
        let timeout_clone = request_timeout.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(client_stream, upstream_addr, timeout_clone).await {
                warn!("Error handling connection: {}", e);
            }
        });
    }
}

/// Handle a client connection
///
/// This function determines whether the connection is a CONNECT request
/// (for HTTPS tunneling) or a standard HTTP request, and handles it accordingly.
///
/// # Arguments
///
/// * `client_stream` - The client TCP stream
/// * `upstream_addr` - The upstream server address
/// * `request_timeout` - Optional timeout for upstream connections
///
/// # Returns
///
/// A result indicating success or failure
async fn handle_connection(
    mut client_stream: TcpStream,
    upstream_addr: String,
    request_timeout: Option<Duration>,
) -> Result<()> {
    // Buffer to read the initial request
    let mut buf = [0u8; 4096];
    
    // Read the initial data from the client
    let n = match client_stream.read(&mut buf).await {
        Ok(n) if n == 0 => return Err(Error::Custom("Client closed connection".to_string())),
        Ok(n) => n,
        Err(e) => return Err(Error::from(e)),
    };
    
    // Check if this is a CONNECT request (HTTPS)
    if n >= 7 && &buf[..7] == b"CONNECT" {
        debug!("CONNECT request detected, handling as HTTPS");
        // Extract the target from the CONNECT request
        let request_str = std::str::from_utf8(&buf[..n])
            .map_err(|_| Error::Custom("Invalid UTF-8 in CONNECT request".to_string()))?;
        
        // Parse the CONNECT request to extract the target host:port
        let lines: Vec<&str> = request_str.split("\r\n").collect();
        if lines.is_empty() {
            return Err(Error::Custom("Empty CONNECT request".to_string()));
        }
        
        let connect_line = lines[0];
        let parts: Vec<&str> = connect_line.split_whitespace().collect();
        if parts.len() < 2 {
            return Err(Error::Custom("Invalid CONNECT request format".to_string()));
        }
        
        let target = parts[1];
        debug!("CONNECT request for {}", target);
        
        // Send 200 Connection Established to the client
        let response = "HTTP/1.1 200 Connection Established\r\n\r\n";
        client_stream.write_all(response.as_bytes()).await?;
        
        // Handle the CONNECT tunnel
        handle_connect(client_stream, &upstream_addr, request_timeout).await
    } else {
        debug!("HTTP request detected");
        // This is a regular HTTP request
        // Create a buffer with the data we've already read
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&buf[..n]);
        
        // Handle the HTTP request
        handle_http_request(client_stream, &upstream_addr, request_timeout).await
    }
}

/// Handle a CONNECT request for HTTPS tunneling
///
/// This function processes a CONNECT request, establishes a tunnel to the
/// target server, and proxies data between the client and the target.
///
/// # Arguments
///
/// * `client_stream` - The client TCP stream
/// * `upstream_addr` - The upstream server address
/// * `request_timeout` - Optional timeout for upstream connections
///
/// # Returns
///
/// A result indicating success or failure
async fn handle_connect(
    mut client_stream: TcpStream,
    upstream_addr: &str,
    request_timeout: Option<Duration>,
) -> Result<()> {
    // Read the CONNECT request line
    let mut buf = Vec::with_capacity(4096);
    let mut temp_buf = [0u8; 1024];
    
    loop {
        let n = client_stream.read(&mut temp_buf).await?;
        if n == 0 {
            return Err(Error::Custom("Client closed connection before sending complete request".to_string()));
        }
        
        buf.extend_from_slice(&temp_buf[..n]);
        
        // Check if we've reached the end of the headers (double CRLF)
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            break;
        }
        
        // Prevent buffer overflow from malformed requests
        if buf.len() > 8192 {
            return Err(Error::Custom("Request header too large".to_string()));
        }
    }
    
    // Parse the request
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);
    req.parse(&buf)?;
    
    // Extract the target host and port from the request
    let target = req.path.ok_or_else(|| Error::Custom("Missing target in CONNECT request".to_string()))?;
    debug!("CONNECT request for {}", target);
    
    // Parse the upstream URL to extract credentials and host:port
    let upstream_url = url::Url::parse(upstream_addr)
        .map_err(|_| Error::Custom(format!("Invalid upstream URL: {}", upstream_addr)))?;
    
    let host = upstream_url.host_str()
        .ok_or_else(|| Error::Custom(format!("Missing host in upstream URL: {}", upstream_addr)))?;
    
    let port = upstream_url.port().unwrap_or_else(|| {
        if upstream_url.scheme() == "https" { 443 } else { 80 }
    });
    
    let upstream_host_port = format!("{}:{}", host, port);
    debug!("Connecting to upstream proxy: {}", upstream_host_port);
    
    // Connect to the upstream proxy
    let mut upstream_stream = if let Some(timeout_duration) = request_timeout {
        match timeout(timeout_duration, TcpStream::connect(&upstream_host_port)).await {
            Ok(result) => result?,
            Err(_) => {
                warn!("Connection to upstream proxy timed out after {:?}: {}", timeout_duration, upstream_host_port);
                // Send an error response to the client
                let response = format!(
                    "HTTP/1.1 504 Gateway Timeout\r\n\
                     Connection: close\r\n\
                     Content-Length: 27\r\n\
                     \r\n\
                     Connection timeout occurred."
                );
                client_stream.write_all(response.as_bytes()).await?;
                return Err(Error::Custom(format!("Connection to upstream proxy timed out after {:?}", timeout_duration)));
            }
        }
    } else {
        TcpStream::connect(&upstream_host_port).await?
    };
    
    // If the upstream proxy requires authentication, add the Proxy-Authorization header
    let username = upstream_url.username();
    if !username.is_empty() {
        let password = upstream_url.password().unwrap_or("");
        let auth = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", username, password));
        let connect_request = format!("CONNECT {} HTTP/1.1\r\nHost: {}\r\nProxy-Authorization: Basic {}\r\n\r\n", target, target, auth);
        upstream_stream.write_all(connect_request.as_bytes()).await?;
    } else {
        let connect_request = format!("CONNECT {} HTTP/1.1\r\nHost: {}\r\n\r\n", target, target);
        upstream_stream.write_all(connect_request.as_bytes()).await?;
    }
    
    // Read the response from the upstream proxy
    let mut response_buf = [0u8; 1024];
    let mut response = Vec::new();
    let mut headers_complete = false;
    
    while !headers_complete {
        let n = upstream_stream.read(&mut response_buf).await?;
        if n == 0 {
            return Err(Error::Custom("Upstream proxy closed connection before sending complete response".to_string()));
        }
        
        response.extend_from_slice(&response_buf[..n]);
        
        // Check if we've reached the end of the headers (double CRLF)
        if response.len() >= 4 {
            for i in 0..response.len() - 3 {
                if &response[i..i+4] == b"\r\n\r\n" {
                    headers_complete = true;
                    break;
                }
            }
        }
        
        // Prevent buffer overflow from malformed responses
        if response.len() > 8192 {
            return Err(Error::Custom("Response header too large".to_string()));
        }
    }
    
    // Check if the response is 200 OK
    let response_str = String::from_utf8_lossy(&response);
    if !response_str.starts_with("HTTP/1.1 200") && !response_str.starts_with("HTTP/1.0 200") {
        let error_msg = format!("Upstream proxy returned error: {}", response_str.lines().next().unwrap_or("Unknown error"));
        client_stream.write_all(response.as_slice()).await?;
        return Err(Error::Custom(error_msg));
    }
    
    // Send 200 OK to the client
    client_stream.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
    
    // Copy data in both directions
    match tokio::io::copy_bidirectional(&mut client_stream, &mut upstream_stream).await {
        Ok((from_client, from_upstream)) => {
            debug!("CONNECT tunnel closed. Bytes: client->upstream: {}, upstream->client: {}", from_client, from_upstream);
        }
        Err(e) => {
            warn!("Error in CONNECT tunnel: {}", e);
        }
    }
    
    Ok(())
}

/// Handle a standard HTTP request
///
/// This function processes a standard HTTP request, forwards it to the
/// upstream server, and returns the response to the client.
///
/// # Arguments
///
/// * `client_stream` - The client TCP stream
/// * `upstream_addr` - The upstream server address
/// * `request_timeout` - Optional timeout for upstream connections
///
/// # Returns
///
/// A result indicating success or failure
async fn handle_http_request(
    mut client_stream: TcpStream,
    upstream_addr: &str,
    request_timeout: Option<Duration>,
) -> Result<()> {
    // Read the HTTP request from the client
    let mut buf = Vec::with_capacity(4096);
    let mut temp_buf = [0u8; 1024];
    
    loop {
        let n = client_stream.read(&mut temp_buf).await?;
        if n == 0 {
            return Err(Error::Custom("Client closed connection before sending complete request".to_string()));
        }
        
        buf.extend_from_slice(&temp_buf[..n]);
        
        // Check if we've reached the end of the headers (double CRLF)
        if buf.len() >= 4 && &buf[buf.len() - 4..] == b"\r\n\r\n" {
            break;
        }
        
        // Prevent buffer overflow from malformed requests
        if buf.len() > 8192 {
            return Err(Error::Custom("Request header too large".to_string()));
        }
    }
    
    // Parse the request
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);
    req.parse(&buf)?;
    
    // Extract request details
    let method = req.method.ok_or_else(|| Error::Custom("Missing method in HTTP request".to_string()))?;
    let path = req.path.ok_or_else(|| Error::Custom("Missing path in HTTP request".to_string()))?;
    let version = req.version.ok_or_else(|| Error::Custom("Missing version in HTTP request".to_string()))?;
    
    debug!("{} {} HTTP/1.{}", method, path, version);
    
    // Parse the upstream URL to extract credentials and host:port
    let upstream_url = Url::parse(upstream_addr)
        .map_err(|_| Error::Custom(format!("Invalid upstream URL: {}", upstream_addr)))?;
    
    let host = upstream_url.host_str()
        .ok_or_else(|| Error::Custom(format!("Missing host in upstream URL: {}", upstream_addr)))?;
    
    let port = upstream_url.port().unwrap_or_else(|| {
        if upstream_url.scheme() == "https" { 443 } else { 80 }
    });
    
    let upstream_host_port = format!("{}:{}", host, port);
    debug!("Connecting to upstream proxy: {}", upstream_host_port);
    
    // Connect to the upstream proxy
    let mut upstream_stream = if let Some(timeout_duration) = request_timeout {
        match timeout(timeout_duration, TcpStream::connect(&upstream_host_port)).await {
            Ok(result) => result?,
            Err(_) => {
                warn!("Connection to upstream proxy timed out after {:?}: {}", timeout_duration, upstream_host_port);
                // Send an error response to the client
                let response = format!(
                    "HTTP/1.1 504 Gateway Timeout\r\n\
                     Connection: close\r\n\
                     Content-Length: 27\r\n\
                     \r\n\
                     Connection timeout occurred."
                );
                client_stream.write_all(response.as_bytes()).await?;
                return Err(Error::Custom(format!("Connection to upstream proxy timed out after {:?}", timeout_duration)));
            }
        }
    } else {
        TcpStream::connect(&upstream_host_port).await?
    };
    
    // Modify the request to use absolute URLs and add proxy authentication if needed
    let mut modified_request = Vec::new();
    
    // Find the end of the request line
    let mut request_line_end = 0;
    for i in 0..buf.len() - 1 {
        if buf[i] == b'\r' && buf[i + 1] == b'\n' {
            request_line_end = i + 2;
            break;
        }
    }
    
    if request_line_end == 0 {
        return Err(Error::Custom("Invalid HTTP request format".to_string()));
    }
    
    // Extract the request line
    let request_line = String::from_utf8_lossy(&buf[0..request_line_end - 2]);
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    
    if parts.len() != 3 {
        return Err(Error::Custom("Invalid HTTP request line".to_string()));
    }
    
    // Extract the host header
    let mut host_header = None;
    for i in 0..req.headers.len() {
        if req.headers[i].name.to_lowercase() == "host" {
            host_header = Some(String::from_utf8_lossy(req.headers[i].value).to_string());
            break;
        }
    }
    
    let host_value = host_header.ok_or_else(|| Error::Custom("Missing Host header in HTTP request".to_string()))?;
    
    // Construct an absolute URL for the proxy request
    let absolute_url = if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        format!("http://{}{}", host_value, path)
    };
    
    // Create a new request line with the absolute URL
    let new_request_line = format!("{} {} HTTP/1.{}\r\n", method, absolute_url, version);
    modified_request.extend_from_slice(new_request_line.as_bytes());
    
    // Copy all headers except Proxy-Connection
    let mut headers_end = 0;
    let mut i = request_line_end;
    let mut skip_header = false;
    let mut header_start = i;
    
    while i < buf.len() - 1 {
        if buf[i] == b'\r' && buf[i + 1] == b'\n' {
            if skip_header {
                skip_header = false;
            } else {
                modified_request.extend_from_slice(&buf[header_start..i + 2]);
            }
            
            // Check if we've reached the end of headers
            if i + 3 < buf.len() && buf[i + 2] == b'\r' && buf[i + 3] == b'\n' {
                headers_end = i + 4;
                break;
            }
            
            header_start = i + 2;
            
            // Check if the next header is Proxy-Connection
            if header_start + 16 < buf.len() {
                let header_prefix = &buf[header_start..header_start + 16];
                if header_prefix.to_ascii_lowercase().starts_with(b"proxy-connection") {
                    skip_header = true;
                }
            }
        }
        i += 1;
    }
    
    // Add Proxy-Authorization header if credentials are provided
    let username = upstream_url.username();
    if !username.is_empty() {
        let password = upstream_url.password().unwrap_or("");
        let auth = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", username, password));
        let auth_header = format!("Proxy-Authorization: Basic {}\r\n", auth);
        modified_request.extend_from_slice(auth_header.as_bytes());
    }
    
    // Add the final CRLF to complete the headers
    modified_request.extend_from_slice(b"\r\n");
    
    // Add the request body if present
    if headers_end > 0 && headers_end < buf.len() {
        modified_request.extend_from_slice(&buf[headers_end..]);
    }
    
    // Send the modified request to the upstream proxy
    upstream_stream.write_all(&modified_request).await?;
    
    // Copy data in both directions
    match tokio::io::copy_bidirectional(&mut client_stream, &mut upstream_stream).await {
        Ok((from_client, from_upstream)) => {
            debug!("HTTP request completed. Bytes: client->upstream: {}, upstream->client: {}", from_client, from_upstream);
        }
        Err(e) => {
            warn!("Error in HTTP request: {}", e);
        }
    }
    
    Ok(())
}
