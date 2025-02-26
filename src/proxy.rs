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
use crate::error::{Result, Error};

/// A structure representing a proxy binding on a given port,
/// along with its upstream proxy configuration.
pub struct ProxyBinding {
    pub port: u16,
    // The upstream proxy address wrapped in a Mutex for dynamic updates.
    pub upstream: Arc<Mutex<String>>,
    // Used to signal the listener to shut down.
    pub shutdown_tx: oneshot::Sender<()>,
}

/// Shared type for dynamic proxy bindings.
pub type BindingMap = Arc<Mutex<HashMap<u16, ProxyBinding>>>;

/// Spawns a proxy listener on the given port which routes connections to its configured upstream.
pub async fn spawn_proxy_listener(
    port: u16,
    upstream: Arc<Mutex<String>>,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<()> {
    // Bind to the specified port.
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    println!("Proxy listener started on port {}", port);

    tokio::select! {
        _ = async {
            loop {
                match listener.accept().await {
                    Ok((client_stream, addr)) => {
                        println!("Accepted connection from {}", addr);
                        let upstream_clone = upstream.clone();
                        tokio::spawn(async move {
                            let upstream_addr = upstream_clone.lock().await.clone();
                            if let Err(e) = handle_connection(client_stream, upstream_addr).await {
                                eprintln!("Error handling connection: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("Error accepting connection: {}", e);
                    }
                }
            }
        } => {}
        _ = shutdown_rx => {
            println!("Shutting down proxy listener on port {}", port);
        }
    }

    Ok(())
}

/// Adjusts the request headers for forwarding to the upstream proxy.
/// This includes removing the "Proxy-Connection" header and updating the "Connection" header.
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
/// It inserts the new header before the final blank line.
pub fn inject_proxy_auth(header_bytes: &[u8], encoded: &str) -> Vec<u8> {
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
/// For CONNECT requests, it tunnels data after relaying the upstream's response.
/// For other requests, it adjusts headers before forwarding.
pub async fn handle_connection(mut client_stream: TcpStream, upstream_addr: String) -> Result<()> {
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
            return Err(Error::Custom("Request header too large".to_string()));
        }
    }
    
    // Parse the HTTP request header.
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);
    
    let _ = req.parse(&header_bytes)
        .map_err(|e| Error::Custom(format!("Failed to parse HTTP request: {}", e)))?;
    
    let method = req.method.ok_or("No HTTP method found")?;
    println!("Received {} request", method);

    // Parse the upstream URL to extract host and port.
    let parsed_url = url::Url::parse(&upstream_addr)?;
    let host = parsed_url.host_str().ok_or("Invalid upstream host")?;
    let port = parsed_url.port_or_known_default().ok_or("Invalid upstream port")?;
    let connect_addr = format!("{}:{}", host, port);
    println!("Connecting to upstream proxy at {}", connect_addr);

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
            println!("Injected Proxy-Authorization header");
            println!("Sending CONNECT header:\n{}", String::from_utf8_lossy(&header_to_send));
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
                return Err(Error::Custom("Response header too large".to_string()));
            }
        }
        
        // Forward the upstream's response to the client.
        client_stream.write_all(&response_header).await?;
        client_stream.flush().await?;

        // Tunnel data between client and upstream.
        let _ = copy_bidirectional(&mut client_stream, &mut upstream_stream).await?;
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
                let mut body = vec![0; content_length];
                client_reader.read_exact(&mut body).await?;
                upstream_stream.write_all(&body).await?;
            }
        }
        upstream_stream.flush().await?;

        // Relay the remainder of the connection.
        let mut client_stream = client_reader.into_inner();
        let _ = copy_bidirectional(&mut client_stream, &mut upstream_stream).await?;
    }
    
    Ok(())
}
