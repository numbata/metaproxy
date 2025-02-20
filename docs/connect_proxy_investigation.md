# CONNECT Proxy Investigation Report

## Overview
**Date**: 2025-02-20
**Issue**: HTTPS CONNECT tunneling through upstream proxy fails
**Status**: Unresolved
**Error Message**: "Proxy CONNECT aborted"

## Technical Context

### Expected Behavior
1. Client sends CONNECT request to our proxy (port 8083)
2. Our proxy forwards CONNECT to upstream proxy (127.0.0.1:12345)
3. After successful CONNECT, a bidirectional tunnel is established
4. TLS handshake occurs through the tunnel
5. HTTPS traffic flows through the tunnel

### Actual Behavior
1. Client CONNECT request received successfully
2. Our proxy establishes TCP connection to upstream
3. CONNECT request sent to upstream
4. Connection drops before TLS handshake
5. Client receives "Proxy CONNECT aborted"

### Reproduction Steps
```bash
# Start our proxy
RUST_LOG=debug cargo run

# In another terminal
curl -v --proxy http://127.0.0.1:8083 https://www.google.com
```

Debug output shows:
```
*   Trying 127.0.0.1:8083...
* Connected to 127.0.0.1 (127.0.0.1) port 8083
* CONNECT tunnel: HTTP/1.1 negotiated
* Establish HTTP proxy tunnel to www.google.com:443
> CONNECT www.google.com:443 HTTP/1.1
> Host: www.google.com:443
> User-Agent: curl/8.7.1
> Proxy-Connection: Keep-Alive
> 
* Proxy CONNECT aborted
* Closing connection
```

## Code Analysis

### Current Implementation
```rust
// In src/proxy.rs
if method == reqwest::Method::CONNECT {
    // 1. Parse authority
    let authority = uri.authority()
        .ok_or_else(|| ErrorBadRequest("No authority in CONNECT request"))?;

    // 2. Connect to upstream
    let mut upstream = TcpStream::connect((proxy_host, proxy_port)).await?;

    // 3. Send CONNECT request
    let connect_req = format!(
        "CONNECT {} HTTP/1.1\r\nHost: {}\r\nConnection: Keep-Alive\r\n\r\n",
        authority, authority
    );
    upstream.write_all(connect_req.as_bytes()).await?;

    // 4. Read response
    // This part works - we get 200 OK from upstream

    // 5. Setup bidirectional tunnel
    let (mut upstream_read, mut upstream_write) = io::split(upstream);
    
    // 6. This is where things go wrong - the streaming setup
    actix_web::rt::spawn(async move {
        let mut buf = [0u8; 8192];
        loop {
            match upstream_read.read(&mut buf).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let chunk = Bytes::copy_from_slice(&buf[..n]);
                    if let Err(e) = tx.send(chunk).await {
                        error!("Failed to send to client: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to read from upstream: {}", e);
                    break;
                }
            }
        }
    });
}
```

### Key Findings

1. **Connection Timing**
   - Connection drops consistently after CONNECT response
   - No error logs from upstream_read/write tasks
   - Happens before TLS handshake begins

2. **Data Flow Issues**
   - No data reaches the upstream proxy after CONNECT
   - Client-to-upstream direction seems blocked
   - No error messages from tokio tasks

3. **Protocol Analysis**
   ```
   # What we send to upstream:
   CONNECT www.google.com:443 HTTP/1.1
   Host: www.google.com:443
   Connection: Keep-Alive

   # What we should send (maybe):
   CONNECT www.google.com:443 HTTP/1.1
   Host: www.google.com:443
   Proxy-Connection: Keep-Alive
   Connection: keep-alive
   ```

4. **Resource Management**
   - TCP streams might be dropped too early
   - Tokio tasks may not live long enough
   - Channel capacity (1024) might be insufficient

## Failed Attempts

### Attempt 1: reqwest Client
```rust
let client = proxy_client.create_client_with_proxy(self.url.as_str())?;
let connect_resp = client
    .request(reqwest::Method::CONNECT, format!("http://{}", authority))
    .send()
    .await?;
```
**Why it failed**: reqwest doesn't expose TCP stream for tunneling

### Attempt 2: Manual Stream Copying
```rust
tokio::io::copy_bidirectional(&mut client_stream, &mut upstream).await?;
```
**Why it failed**: Incompatible with actix-web's streaming response model

### Attempt 3: Custom Stream Implementation
```rust
#[pin_project]
struct TunnelStream {
    #[pin]
    upstream: TcpStream,
    buffer: BytesMut,
}
```
**Why it failed**: Complex lifetime issues with actix-web

## Hypotheses

1. **Task Lifetime**
   ```rust
   // Current
   actix_web::rt::spawn(async move { ... });
   
   // Maybe needed
   let handle = actix_web::rt::spawn(async move { ... });
   response.on_disconnect(move || {
       handle.abort();
   });
   ```

2. **Connection Headers**
   - Maybe upstream expects different headers
   - Try adding more connection-related headers

3. **Buffer Management**
   - Current fixed buffer might be too small
   - Need to implement proper backpressure

## Next Steps

### Immediate Actions
1. Add tracing to track task lifecycles:
   ```rust
   tracing::trace_span!("tunnel_task").in_scope(|| {
       // streaming code
   });
   ```

2. Implement proper connection cleanup:
   ```rust
   impl Drop for TunnelStream {
       fn drop(&mut self) {
           tracing::debug!("TunnelStream dropped");
       }
   }
   ```

3. Test with different upstream proxies to isolate the issue

### Investigation Tools
1. **Wireshark Filter**:
   ```
   tcp.port == 8083 || tcp.port == 12345
   ```

2. **Debug Script**:
   ```bash
   #!/bin/bash
   RUST_LOG=trace cargo run &
   sleep 1
   curl -v --proxy http://127.0.0.1:8083 https://www.google.com
   ```

## Questions to Answer

1. **Task Lifecycle**
   - When exactly do the tokio tasks terminate?
   - Are we properly handling task cancellation?

2. **Connection State**
   - Is the upstream proxy closing the connection?
   - Are we properly handling TCP keepalive?

3. **Memory Management**
   - Are we leaking resources?
   - Is there proper backpressure?

## References

1. [RFC 7231 - CONNECT](https://tools.ietf.org/html/rfc7231#section-4.3.6)
   - Key quote: "A proxy MUST send an appropriate Via header field in the CONNECT request"

2. [Tokio TcpStream docs](https://docs.rs/tokio/latest/tokio/net/struct.TcpStream.html)
   - Important: "Splitting a TcpStream into readable and writable halves is an atomic operation"

3. [Actix-web Streaming Response](https://actix.rs/docs/response/#streaming-response)
   - Note about keeping tasks alive

4. Similar Issues:
   - [Tokio Issue #3998](https://github.com/tokio-rs/tokio/issues/3998)
   - [Actix-web Issue #1760](https://github.com/actix/actix-web/issues/1760)

## Potential Solutions

1. **Custom Transport Layer**
   ```rust
   struct ProxyTransport {
       stream: TcpStream,
       state: Arc<Mutex<ConnectionState>>,
   }
   ```

2. **Connection Pool**
   ```rust
   type PoolKey = (String, u16);
   type Pool = Arc<Mutex<HashMap<PoolKey, Vec<TcpStream>>>>;
   ```

3. **Proper Error Propagation**
   ```rust
   #[derive(Error, Debug)]
   enum TunnelError {
       #[error("tunnel setup failed: {0}")]
       Setup(#[from] io::Error),
       #[error("tunnel dropped: {0}")]
       Drop(String),
   }
   ```
