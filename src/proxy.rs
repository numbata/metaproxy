/*!
 * # Proxy Module
 *
 * This module provides the core proxy functionality for the metaproxy application.
 * It handles TCP connections, HTTP/HTTPS proxying, and connection tunneling.
 *
 * The proxy module supports:
 * - Standard HTTP proxying with header adjustment
 * - HTTPS tunneling via the CONNECT method
 * - Dynamic upstream configuration
 * - Graceful shutdown of proxy listeners
 */

use std::{
    collections::HashMap,
    sync::Arc,
};
use tokio::{
    io::{copy_bidirectional, AsyncWriteExt, BufReader, AsyncReadExt},
    net::{TcpListener, TcpStream},
    sync::{Mutex, oneshot},
};
use base64::Engine;
use log::{info, warn, error, debug, trace};
use crate::error::{Result, Error};

/// A structure representing a proxy binding on a given port,
/// along with its upstream proxy configuration.
///
/// This struct contains all the information needed to manage a proxy binding,
/// including the port it's bound to, the upstream server address, and a
/// channel for signaling shutdown.
pub struct ProxyBinding {
    /// The port number this proxy binding is listening on
    pub port: u16,
    /// The upstream proxy address wrapped in a Mutex for dynamic updates
    pub upstream: Arc<Mutex<String>>,
    /// Used to signal the listener to shut down
    pub shutdown_tx: oneshot::Sender<()>,
}

/// Shared type for dynamic proxy bindings.
///
/// This type alias represents a thread-safe map of port numbers to proxy bindings,
/// allowing multiple threads to safely access and modify the proxy bindings.
pub type BindingMap = Arc<Mutex<HashMap<u16, ProxyBinding>>>;

/// Spawns a proxy listener on the given port which routes connections to its configured upstream.
///
/// This function creates a TCP listener on the specified port and handles incoming connections
/// by forwarding them to the configured upstream server. It continues to accept connections
/// until a shutdown signal is received.
///
/// # Arguments
///
/// * `port` - The port number to bind the proxy listener to
/// * `upstream` - The upstream server address, wrapped in a Mutex for dynamic updates
/// * `shutdown_rx` - A oneshot channel receiver for signaling shutdown
///
/// # Returns
///
/// A `Result` indicating success or an error if the listener fails to bind or accept connections
pub async fn spawn_proxy_listener(
    port: u16,
    upstream: Arc<Mutex<String>>,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<()> {
    // Bind to the specified port.
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    info!("Proxy listener started on port {}", port);

    tokio::select! {
        _ = async {
            loop {
                match listener.accept().await {
                    Ok((client_stream, addr)) => {
                        debug!("Accepted connection from {}", addr);
                        let upstream_clone = upstream.clone();
                        tokio::spawn(async move {
                            let upstream_addr = upstream_clone.lock().await.clone();
                            if let Err(e) = handle_connection(client_stream, upstream_addr).await {
                                error!("Error handling connection: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Error accepting connection: {}", e);
                    }
                }
            }
        } => {}
        _ = shutdown_rx => {
            info!("Shutting down proxy listener on port {}", port);
        }
    }

    Ok(())
}

/// Adjusts the request headers for forwarding to the upstream proxy.
///
/// This function modifies HTTP request headers to make them suitable for forwarding
/// to an upstream proxy. It removes the "Proxy-Connection" header and updates
/// the "Connection" header to maintain proper HTTP semantics.
///
/// # Arguments
///
/// * `header_bytes` - The original HTTP request headers as bytes
///
/// # Returns
///
/// A `Result` containing the modified headers as bytes, or an error if parsing fails
fn adjust_request_headers(header_bytes: &[u8]) -> Result<Vec<u8>> {
    // Convert header bytes to a string (assuming valid UTF-8).
    let header_str = std::str::from_utf8(header_bytes)
        .map_err(|_| Error::Custom("Invalid UTF-8 in request headers".to_string()))?;
    
    // Split the header into lines.
    let lines: Vec<String> = header_str.split("\r\n").map(String::from).collect();
    
    // Remove "Proxy-Connection" header and update "Connection" header.
    let mut adjusted_lines = Vec::new();
    for line in lines {
        if line.to_lowercase().starts_with("proxy-connection:") {
            // Skip this header.
        } else if line.to_lowercase().starts_with("connection:") {
            // Replace with "Connection: close".
            adjusted_lines.push("Connection: close".to_string());
        } else {
            adjusted_lines.push(line);
        }
    }
    
    // Reassemble the header with CRLF line endings.
    let adjusted_header = adjusted_lines.join("\r\n");
    
    Ok(adjusted_header.into_bytes())
}

/// Injects a "Proxy-Authorization: Basic ..." header into the CONNECT header.
///
/// This function adds a Proxy-Authorization header to the HTTP request headers
/// for authenticating with an upstream proxy. It inserts the new header before
/// the final blank line of the headers.
///
/// # Arguments
///
/// * `header_bytes` - The original HTTP request headers as bytes
/// * `encoded` - The Base64-encoded credentials for proxy authentication
///
/// # Returns
///
/// The modified headers as bytes with the added Proxy-Authorization header
fn inject_proxy_auth(header_bytes: &[u8], encoded: &str) -> Vec<u8> {
    // Convert header bytes to a string (assuming valid UTF-8).
    let header_str = std::str::from_utf8(header_bytes).unwrap();
    // Split the header into lines.
    let mut lines: Vec<String> = header_str.split("\r\n").map(String::from).collect();
    // Remove trailing empty line if present.
    while let Some(last) = lines.last() {
        if last.is_empty() {
            lines.pop();
        } else {
            break;
        }
    }
    // Insert the proxy auth header.
    lines.push(format!("Proxy-Authorization: Basic {}", encoded));
    // Reassemble with CRLF and add the final CRLF.
    let new_header = lines.join("\r\n") + "\r\n\r\n";
    new_header.into_bytes()
}

/// Handles a client connection, determining whether it is a CONNECT (HTTPS) request or a standard HTTP request.
///
/// This function processes an incoming TCP connection and determines the appropriate
/// handling based on the request type. For CONNECT requests (typically HTTPS), it
/// establishes a tunnel between the client and the upstream server. For standard
/// HTTP requests, it adjusts the headers and forwards the request to the upstream.
///
/// # Arguments
///
/// * `client_stream` - The TCP stream from the client connection
/// * `upstream_addr` - The address of the upstream server to forward the request to
///
/// # Returns
///
/// A `Result` indicating success or an error if handling the connection fails
async fn handle_connection(mut client_stream: TcpStream, upstream_addr: String) -> Result<()> {
    // Read the initial request header.
    let mut client_reader = BufReader::new(&mut client_stream);
    let mut header_bytes = Vec::new();
    
    // Read until we encounter the end of the HTTP header (double CRLF).
    let mut prev_was_cr = false;
    let mut empty_line_count = 0;
    
    loop {
        let byte = client_reader.read_u8().await?;
        header_bytes.push(byte);
        
        if byte == b'\r' {
            prev_was_cr = true;
        } else if byte == b'\n' && prev_was_cr {
            empty_line_count += 1;
            if empty_line_count == 2 {
                break;
            }
            prev_was_cr = false;
        } else {
            empty_line_count = 0;
            prev_was_cr = false;
        }
        
        // Prevent buffer overflow from malformed requests.
        if header_bytes.len() > 8192 {
            warn!("Request header too large (> 8KB), rejecting");
            return Err(Error::Custom("Request header too large".to_string()));
        }
    }
    
    // Parse the HTTP request header.
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);
    
    let _ = req.parse(&header_bytes)
        .map_err(|e| Error::Custom(format!("Failed to parse HTTP request: {}", e)))?;
    
    let method = req.method.ok_or("No HTTP method found")?;
    debug!("Received {} request", method);

    // Parse the upstream URL to extract host and port.
    let parsed_url = url::Url::parse(&upstream_addr)?;
    let host = parsed_url.host_str().ok_or("Invalid upstream host")?;
    let port = parsed_url.port_or_known_default().ok_or("Invalid upstream port")?;
    let connect_addr = format!("{}:{}", host, port);
    debug!("Connecting to upstream proxy at {}", connect_addr);

    let mut upstream_stream = TcpStream::connect(&connect_addr).await?;

    if method.eq_ignore_ascii_case("CONNECT") {
        // For CONNECT requests, check if the upstream URL contains credentials.
        let mut header_to_send = header_bytes.clone();
        if !parsed_url.username().is_empty() {
            let user = parsed_url.username();
            let pass = parsed_url.password().unwrap_or("");
            let credentials = format!("{}:{}", user, pass);
            let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
            header_to_send = inject_proxy_auth(&header_bytes, &encoded);
            debug!("Injected Proxy-Authorization header");
            trace!("Sending CONNECT header:\n{}", String::from_utf8_lossy(&header_to_send));
        }
        // Forward the (possibly modified) CONNECT header.
        upstream_stream.write_all(&header_to_send).await?;
        upstream_stream.flush().await?;

        // Read upstream's response header.
        let mut upstream_reader = BufReader::new(&mut upstream_stream);
        let mut response_header = Vec::new();
        
        // Read until we encounter the end of the HTTP header (double CRLF).
        let mut prev_was_cr = false;
        let mut empty_line_count = 0;
        
        loop {
            let byte = upstream_reader.read_u8().await?;
            response_header.push(byte);
            
            if byte == b'\r' {
                prev_was_cr = true;
            } else if byte == b'\n' && prev_was_cr {
                empty_line_count += 1;
                if empty_line_count == 2 {
                    break;
                }
                prev_was_cr = false;
            } else {
                empty_line_count = 0;
                prev_was_cr = false;
            }
            
            // Prevent buffer overflow from malformed responses.
            if response_header.len() > 8192 {
                warn!("Response header too large (> 8KB), terminating connection");
                return Err(Error::Custom("Response header too large".to_string()));
            }
        }
        
        // Forward the upstream's response to the client.
        client_stream.write_all(&response_header).await?;
        client_stream.flush().await?;

        debug!("Established CONNECT tunnel, starting bidirectional copy");
        // Tunnel data between client and upstream.
        let (client_bytes, upstream_bytes) = copy_bidirectional(&mut client_stream, &mut upstream_stream).await?;
        debug!("CONNECT tunnel closed. Transferred {} bytes from client and {} bytes from upstream", client_bytes, upstream_bytes);
    } else {
        // For regular HTTP requests, adjust headers.
        let adjusted_header = adjust_request_headers(&header_bytes)?;
        upstream_stream.write_all(&adjusted_header).await?;

        // If there's a Content-Length header, read and forward the request body.
        for header in headers.iter() {
            if header.name.eq_ignore_ascii_case("content-length") {
                let content_length = std::str::from_utf8(header.value)
                    .map_err(|_| Error::Custom("Invalid Content-Length header".to_string()))?
                    .parse::<usize>()
                    .map_err(|_| Error::Custom("Invalid Content-Length value".to_string()))?;
                debug!("Forwarding request body of {} bytes", content_length);
                let mut body = vec![0; content_length];
                client_reader.read_exact(&mut body).await?;
                upstream_stream.write_all(&body).await?;
            }
        }
        upstream_stream.flush().await?;

        debug!("Starting bidirectional copy for HTTP request");
        // Relay the remainder of the connection.
        let mut client_stream = client_reader.into_inner();
        let (client_bytes, upstream_bytes) = copy_bidirectional(&mut client_stream, &mut upstream_stream).await?;
        debug!("HTTP connection closed. Transferred {} bytes from client and {} bytes from upstream", client_bytes, upstream_bytes);
    }
    
    Ok(())
}
