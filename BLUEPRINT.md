**MetaProxy Blueprint**

## Overview
MetaProxy is a high-performance HTTP/HTTPS proxy server written in Rust using Actix-Web. It supports cascading proxy functionality, allowing requests to be forwarded through another proxy specified in the `X-Proxy-To` header.

## Features

### Core Functionality
- [x] HTTP/HTTPS proxy support
- [x] Cascading proxy support via `X-Proxy-To` header
- [x] CONNECT method handling for HTTPS tunneling
- [x] Connection pooling
- [x] Health check endpoint
- [x] Request metrics tracking
- [x] Configurable timeouts and connection limits
- [x] Support for proxy authentication

### Configuration
The proxy server can be configured using the following environment variables:
- `PROXY_REQUEST_TIMEOUT_SECS` - Request timeout in seconds (default: 30)
- `PROXY_BIND_HOST` - Host to bind to (default: 127.0.0.1)
- `PROXY_BIND_PORT` - Port to bind to (default: 8081)
- `PROXY_POOL_IDLE_TIMEOUT_SECS` - Connection pool idle timeout in seconds (default: 90)
- `PROXY_POOL_MAX_IDLE_PER_HOST` - Maximum idle connections per host (default: 32)

## Architecture

### Components
1. **Main Server (`main.rs`)**
   - Initializes the Actix-Web server
   - Sets up configuration from environment variables
   - Configures request handlers and middleware

2. **Proxy Handler (`proxy.rs`)**
   - Handles incoming HTTP/HTTPS requests
   - Manages proxy target resolution
   - Implements request forwarding logic
   - Handles CONNECT method for HTTPS tunneling
   - Manages connection pooling

3. **Health Monitoring (`health.rs`)**
   - Provides health check endpoint
   - Tracks request metrics
   - Monitors connection pool status

### Request Flow
1. Client sends request to MetaProxy
2. For CONNECT requests:
   - Establish tunnel without requiring X-Proxy-To header
   - Return 200 OK to establish connection
3. For non-CONNECT requests:
   - Extract X-Proxy-To header
   - Create client with proxy configuration
   - Forward request to target proxy
   - Stream response back to client

## Security
- TLS certificate validation can be disabled for development/testing
- Headers are sanitized to prevent proxy-related security issues
- Sensitive headers are not forwarded

## Performance Considerations
- Asynchronous I/O with Tokio
- Connection pooling to reduce overhead
- Streaming response bodies to minimize memory usage
- Configurable timeouts and connection limits

## Future Enhancements
- [ ] TLS termination
- [ ] Implement request/response logging
- [ ] Add support for proxy authentication in CONNECT phase
- [ ] Add rate limiting
- [ ] Support for multiple upstream proxies
- [ ] Request/response transformation
