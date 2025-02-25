use std::{
    collections::HashMap,
    error::Error,
    str,
    sync::Arc,
};
use tokio::{
    io::{copy_bidirectional, AsyncReadExt, AsyncWriteExt, BufReader, split},
    net::{TcpListener, TcpStream},
    io::AsyncBufReadExt,
    sync::{Mutex, oneshot},
};
use warp::Filter;
use serde_json::json;
use httparse;

/// A structure representing a proxy binding on a given port,
/// along with its upstream proxy configuration.
#[derive(Clone)]
struct ProxyBinding {
    port: u16,
    // The upstream proxy address (e.g., "127.1.0.1:8080") wrapped in a Mutex for dynamic updates.
    upstream: Arc<Mutex<String>>,
    // Used to signal the listener to shut down.
    shutdown_tx: oneshot::Sender<()>,
}

/// Shared type for dynamic proxy bindings.
type BindingMap = Arc<Mutex<HashMap<u16, ProxyBinding>>>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Shared state to store active proxy bindings.
    let bindings: BindingMap = Arc::new(Mutex::new(HashMap::new()));

    // Clone bindings for use in API routes.
    let api_bindings = bindings.clone();

    // Define the API routes.
    // - POST /proxy: Create a new binding.
    // - PUT /proxy/{port}: Update the upstream for a binding.
    // - DELETE /proxy/{port}: Delete a binding.
    // - GET /health: Return the current bindings and upstream addresses.
    let proxy_routes = warp::path("proxy")
        .and(warp::path::param::<u16>().or(warp::any().map(|| 0)).unify())
        .and(warp::method())
        .and(warp::body::json())
        .and_then(move |port: u16, method: warp::http::Method, body: serde_json::Value| {
            let bindings = api_bindings.clone();
            async move {
                match method {
                    warp::http::Method::POST => {
                        // For creation, read the "port" and "upstream" from the request body.
                        let new_port = body.get("port")
                            .and_then(|v| v.as_u64())
                            .ok_or_else(|| warp::reject::custom("Missing port"))? as u16;
                        let upstream = body.get("upstream")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| warp::reject::custom("Missing upstream"))?
                            .to_string();

                        let mut map = bindings.lock().await;
                        if map.contains_key(&new_port) {
                            return Err(warp::reject::custom("Port already in use"));
                        }
                        // Create a shutdown channel for the new binding.
                        let (shutdown_tx, shutdown_rx) = oneshot::channel();
                        // Spawn the proxy listener task.
                        spawn_proxy(new_port, upstream.clone(), shutdown_rx, bindings.clone());
                        // Save the binding.
                        map.insert(new_port, ProxyBinding {
                            port: new_port,
                            upstream: Arc::new(Mutex::new(upstream)),
                            shutdown_tx,
                        });
                        Ok::<_, warp::Rejection>(warp::reply::json(&json!({
                            "status": "created",
                            "port": new_port
                        })))
                    },
                    warp::http::Method::PUT => {
                        // Update the upstream for an existing binding.
                        let mut map = bindings.lock().await;
                        if let Some(binding) = map.get(&port) {
                            let new_upstream = body.get("upstream")
                                .and_then(|v| v.as_str())
                                .ok_or_else(|| warp::reject::custom("Missing upstream"))?
                                .to_string();
                            *binding.upstream.lock().await = new_upstream;
                            Ok(warp::reply::json(&json!({
                                "status": "updated",
                                "port": port
                            })))
                        } else {
                            Err(warp::reject::custom("Binding not found"))
                        }
                    },
                    warp::http::Method::DELETE => {
                        // Delete the binding on the specified port.
                        let mut map = bindings.lock().await;
                        if let Some(binding) = map.remove(&port) {
                            let _ = binding.shutdown_tx.send(());
                            Ok(warp::reply::json(&json!({
                                "status": "deleted",
                                "port": port
                            })))
                        } else {
                            Err(warp::reject::custom("Binding not found"))
                        }
                    },
                    _ => Err(warp::reject::custom("Unsupported method")),
                }
            }
        });

    let health_route = warp::path("health").map({
        let bindings = bindings.clone();
        move || {
            let bindings = bindings.clone();
            async move {
                let map = bindings.lock().await;
                let info: Vec<_> = map.iter().map(|(&port, binding)| {
                    let upstream = binding.upstream.blocking_lock().clone();
                    json!({ "port": port, "upstream": upstream })
                }).collect();
                warp::reply::json(&json!({ "bindings": info }))
            }
        }
    });

    // Combine API routes.
    let routes = proxy_routes.or(health_route);

    // Start the API server on port 8000.
    tokio::spawn(warp::serve(routes).run(([127, 0, 0, 1], 8000)));
    println!("API server running on http://127.0.0.1:8000");

    // Keep the main task alive indefinitely.
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
    }
}

/// Spawns a proxy listener on the given port which routes connections to its configured upstream.
/// It listens for a shutdown signal (via the oneshot channel) to stop the listener.
fn spawn_proxy(port: u16, _initial_upstream: String, mut shutdown_rx: oneshot::Receiver<()>, bindings: BindingMap) {
    tokio::spawn(async move {
        let addr = format!("127.0.0.1:{}", port);
        let listener = TcpListener::bind(&addr).await.expect("Failed to bind proxy port");
        println!("Started proxy on {}", addr);
        loop {
            tokio::select! {
                Ok((client, client_addr)) = listener.accept() => {
                    println!("Accepted connection from {} on port {}", client_addr, port);
                    // Retrieve the current upstream for this binding.
                    let upstream;
                    {
                        let map = bindings.lock().await;
                        if let Some(binding) = map.get(&port) {
                            upstream = binding.upstream.lock().await.clone();
                        } else {
                            println!("Binding removed for port {}", port);
                            break;
                        }
                    }
                    // Handle the client connection using our extended HTTP/HTTPS proxy logic.
                    let upstream_clone = upstream.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(client, upstream_clone).await {
                            eprintln!("Error handling connection on port {}: {:?}", port, e);
                        }
                    });
                },
                _ = &mut shutdown_rx => {
                    println!("Shutting down proxy on port {}", port);
                    break;
                }
            }
        }
    });
}

/// Adjusts HTTP request headers by removing headers that should not be forwarded to the upstream proxy,
/// such as "Proxy-Connection" and "Connection".
fn adjust_request_headers(header_bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let header_str = str::from_utf8(header_bytes)?;
    let lines: Vec<&str> = header_str.split("\r\n").collect();
    let mut new_lines = Vec::with_capacity(lines.len());

    // Preserve the request line.
    if let Some(request_line) = lines.get(0) {
        new_lines.push(*request_line);
    }

    // Process remaining header lines.
    for line in lines.iter().skip(1) {
        if line.is_empty() {
            continue;
        }
        let lower = line.to_lowercase();
        if lower.starts_with("proxy-connection:") || lower.starts_with("connection:") {
            continue;
        }
        new_lines.push(*line);
    }
    // Mark the end of headers.
    new_lines.push("");
    let adjusted_header = new_lines.join("\r\n");
    Ok(adjusted_header.into_bytes())
}

/// Handles a client connection, determining whether it is a CONNECT (HTTPS) request or a standard HTTP request.
/// For CONNECT requests, it tunnels data after relaying the upstream's response.
/// For other requests, it adjusts headers before forwarding.
pub async fn handle_connection(mut client_stream: TcpStream, upstream_addr: String) -> Result<(), Box<dyn Error>> {
    // Wrap the client stream in a BufReader for header inspection.
    let mut client_reader = BufReader::new(&mut client_stream);
    let mut header_bytes = Vec::new();

    // Read until the end of HTTP headers.
    loop {
        let mut line = Vec::new();
        let n = client_reader.read_until(b'\n', &mut line).await?;
        if n == 0 {
            return Err("Client closed connection before sending header".into());
        }
        header_bytes.extend_from_slice(&line);
        if header_bytes.ends_with(b"\r\n\r\n") {
            break;
        }
        if header_bytes.len() > 8192 {
            return Err("Header too large".into());
        }
    }

    // Parse the HTTP header.
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut req = httparse::Request::new(&mut headers);
    let parse_status = req.parse(&header_bytes)?;
    if !parse_status.is_complete() {
        return Err("Incomplete HTTP header".into());
    }
    let method = req.method.ok_or("No HTTP method found")?;
    println!("Received {} request", method);

    // Connect to the configured upstream proxy.
    let mut upstream_stream = TcpStream::connect(&upstream_addr).await?;
    println!("Connected to upstream proxy at {}", upstream_addr);

    if method.eq_ignore_ascii_case("CONNECT") {
        // For HTTPS CONNECT requests, forward the header as-is.
        upstream_stream.write_all(&header_bytes).await?;
        upstream_stream.flush().await?;

        // Read upstream's response header.
        let mut upstream_reader = BufReader::new(&mut upstream_stream);
        let mut resp_bytes = Vec::new();
        loop {
            let mut line = Vec::new();
            let n = upstream_reader.read_until(b'\n', &mut line).await?;
            if n == 0 {
                return Err("Upstream closed connection while waiting for CONNECT response".into());
            }
            resp_bytes.extend_from_slice(&line);
            if resp_bytes.ends_with(b"\r\n\r\n") {
                break;
            }
            if resp_bytes.len() > 8192 {
                return Err("Upstream response header too large".into());
            }
        }
        println!("Upstream CONNECT response: {}", String::from_utf8_lossy(&resp_bytes));

        // Relay the response back to the client.
        client_stream.write_all(&resp_bytes).await?;
        client_stream.flush().await?;

        // Tunnel data between client and upstream.
        let _ = copy_bidirectional(&mut client_stream, &mut upstream_stream).await?;

    } else {
        // For regular HTTP requests, adjust the headers.
        let adjusted_header = adjust_request_headers(&header_bytes)?;
        upstream_stream.write_all(&adjusted_header).await?;

        // Forward any additional buffered data.
        let buffered = client_reader.buffer();
        if !buffered.is_empty() {
            upstream_stream.write_all(buffered).await?;
            let amt = buffered.len();
            client_reader.consume(amt);
        }
        upstream_stream.flush().await?;

        // Relay the remainder of the connection.
        let client_stream = client_reader.into_inner();
        let _ = copy_bidirectional(&mut client_stream, &mut upstream_stream).await?;
    }

    Ok(())
}
