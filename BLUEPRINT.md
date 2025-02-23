**MetaProxy Blueprint**

## Overview
MetaProxy is a high-performance HTTP/HTTPS proxy server written in Rust using Actix-Web. It supports cascading proxy functionality, allowing requests to be forwarded through another proxy specified in the `X-Proxy-To` header.

## Features

### Core Functionality
- [x] HTTP/HTTPS proxy support
- [x] Cascading proxy support via configurable header
- [x] CONNECT method handling for HTTPS tunneling
- [x] Health check endpoint
- [x] Request metrics tracking
- [x] Configurable timeouts
- [x] Support for proxy authentication

### Configuration
The proxy server can be configured using the following environment variables:
- `PROXY_REQUEST_TIMEOUT_SECS` - Request timeout in seconds (default: 30)
- `PROXY_BIND_HOST` - Host to bind to (default: 127.0.0.1)
- `PROXY_BIND_PORT` - Port to bind to (default: 8081)
- `PROXY_UPSTREAM_HEADER` - Header name to lookup upstream proxy URL (default: "X-Proxy-To")
- `PROXY_ALLOW_DIRECT` - Allow direct connections when no upstream proxy header is present (default: true)

### Upstream Proxy Resolution
1. For each request, MetaProxy checks the header specified by `PROXY_UPSTREAM_HEADER` (e.g., "X-Proxy-To")
2. If the header is present, its value is used as the upstream proxy URL for this request
3. If the header is missing:
   - When `PROXY_ALLOW_DIRECT=true`: connects directly to the target
   - When `PROXY_ALLOW_DIRECT=false`: returns 400 Bad Request

## Architecture

### Components
1. **Main Server (`main.rs`)**
   - Initializes the Actix-Web server
   - Sets up configuration from environment variables
   - Configures request handlers and middleware
   - Initializes metrics collection

2. **Proxy Handler (`proxy.rs`)**
   - Implements two distinct request handling paths:
     1. CONNECT Tunnel Handler:
        - Handles HTTPS CONNECT requests
        - Uses Actix's connection upgrade mechanism
        - Establishes direct TCP tunnels
        - Manages bidirectional stream copying
     2. Regular Proxy Handler:
        - Handles standard HTTP/HTTPS requests
        - Manages proxy target resolution
        - Implements request forwarding logic
   - Implements proper error handling and logging

3. **Health Monitoring (`health.rs`)**
   - Provides health check endpoint
   - Tracks request metrics
   - Basic system health information

### Request Flow
1. Client sends request to MetaProxy
2. For CONNECT requests:
   ```rust
   async fn handle_connect(req: HttpRequest, payload: web::Payload) -> Result<HttpResponse, Error> {
       // 1. Extract target from CONNECT request
       let authority = req.uri().authority()?;
       
       // 2. Connect to target server (direct or via upstream)
       let target_stream = establish_tunnel(authority).await?;
       
       // 3. Upgrade connection to raw TCP
       let upgraded = web::Upgraded::new(payload);
       
       // 4. Setup bidirectional tunnel
       tokio::spawn(async move {
           let (client_read, client_write) = tokio::io::split(upgraded);
           let (target_read, target_write) = target_stream.into_split();
           
           tokio::try_join!(
               tokio::io::copy(client_read, target_write),
               tokio::io::copy(target_read, client_write)
           )
       });
       
       // 5. Return upgrade response
       Ok(HttpResponse::Ok().upgrade())
   }
   ```

3. For non-CONNECT requests:
   - Check for and validate upstream proxy header
   - Create client with appropriate configuration
   - Forward request to target or upstream proxy
   - Stream response back to client

### Error Handling
- Proper error types for different failure scenarios
- Detailed error logging with tracing
- Graceful connection cleanup
- Timeout handling for all async operations

## Security
- TLS certificate validation for upstream connections
- Headers sanitization to prevent proxy-related security issues
- Sensitive headers filtering
- Connection upgrade security checks

## Performance Considerations
- Asynchronous I/O with Tokio
- Direct TCP tunneling for CONNECT
- Streaming response bodies
- Configurable timeouts
- Proper resource cleanup

## Testing Strategy
1. Unit Tests:
   - Request parsing and validation
   - Header manipulation
   - Configuration handling
   - Error scenarios

2. Integration Tests:
   - Direct CONNECT tunneling
   - Upstream proxy CONNECT
   - Regular HTTP proxying
   - Error handling
   - Load handling

3. Load Tests:
   - Concurrent CONNECT tunnels
   - Mixed request types
   - Resource cleanup
   - Memory usage patterns

## Future Enhancements
- [ ] TLS termination
- [ ] Request/response logging with sanitization
- [ ] Proxy authentication for CONNECT
- [ ] Rate limiting
- [ ] Multiple upstream proxies
- [ ] Request/response transformation
- [ ] WebSocket support
- [ ] Circuit breaker for upstream proxies
- [ ] Metrics export (Prometheus)
- [ ] Optional connection pooling for improved performance
