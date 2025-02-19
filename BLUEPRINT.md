**Blueprint: High-Performance Proxy Server in Rust**

## **Goal**
Develop a high-performance **HTTP/HTTPS proxy server** in **Rust** using **Actix-Web** that dynamically routes requests based on the `X-Proxy-To` header. The proxy should efficiently handle **100,000+ concurrent requests** with low latency and minimal memory overhead.

---

## **Requirements**
### **1. Functional Requirements**
- Accepts **HTTP and HTTPS** requests from clients.
- Extracts the `X-Proxy-To` header to determine the upstream proxy.
- Supports **authentication** in the upstream proxy (e.g., `user:password@proxy:port`).
- Efficiently proxies requests, preserving headers and body.
- Uses **connection pooling** to improve performance.
- Handles **timeouts** and **retries** gracefully.
- Supports **streaming responses** to prevent memory overflows.
- Provides **logging** for request tracking and debugging.
- Can be deployed via **Docker** and scaled horizontally.

### **2. Non-Functional Requirements**
- **High performance**: Optimized for **low latency (<10ms per request)**.
- **Memory efficiency**: Minimal resource footprint compared to traditional proxies.
- **Security**: Properly handles **TLS and request validation**.
- **Scalability**: Supports **horizontal scaling** via load balancing (e.g., NGINX/Kubernetes).

---

## **Incremental Milestones**

### **1. Project Setup**
**Goal:** Establish the foundation of the project with essential dependencies.

**Success Criteria:**
- The Rust project compiles successfully.
- The `/health` endpoint returns a valid response.
- All dependencies are correctly integrated.

**Todo:**
- [x] Initialize Rust project with `cargo new proxy-server`.
- [x] Add dependencies (`actix-web`, `reqwest`, `tokio`, `serde`, `tracing`).
- [x] Set up a basic **Actix-Web** server with a `/health` endpoint.

### **2. Extract and Validate `X-Proxy-To` Header**
**Goal:** Ensure incoming requests contain a valid `X-Proxy-To` header.

**Success Criteria:**
- The proxy rejects requests without the header.
- The header is parsed correctly into host, port, and credentials if provided.
- Invalid values return appropriate error responses.

**Todo:**
- [x] Extract `X-Proxy-To` from incoming requests.
- [x] Validate that the header contains a valid URL or IP.
- [x] Return an error response if validation fails.

### **3. Implement Basic Request Forwarding**
**Goal:** Forward requests to the upstream proxy specified in `X-Proxy-To`.

**Success Criteria:**
- Requests are successfully forwarded to the upstream proxy.
- Responses are correctly returned to the client.
- Basic errors such as unreachable upstream proxies are handled gracefully.

**Todo:**
- [x] Use `reqwest` to send requests to the specified proxy.
- [x] Preserve HTTP methods and headers.
- [x] Return responses from the upstream proxy to the client.

### **4. Handle Request Body and Streaming**
**Goal:** Support large request and response bodies through streaming.

**Success Criteria:**
- Large request bodies are correctly forwarded.
- Response bodies are streamed back to the client.
- Memory usage remains constant regardless of payload size.

**Todo:**
- [x] Implement request body streaming.
- [x] Implement response body streaming.
- [x] Add proper error handling for stream interruptions.

### **5. Optimize Connection Handling and Performance**
**Goal:** Improve efficiency by reducing overhead and maximizing throughput.

**Success Criteria:**
- Connection pooling reduces unnecessary connection creation overhead.
- Streaming responses minimize memory usage.
- DNS resolution and retries improve reliability.

**Todo:**
- [ ] Implement **connection pooling** using `reqwest`.
- [ ] Enable **streaming responses** to prevent memory overuse.
- [ ] Set up **timeouts and retry mechanisms**.
- [ ] Optimize **DNS resolution**.

### **6. Implement Structured Logging**
**Goal:** Provide clear, structured logs for debugging and request tracking.

**Success Criteria:**
- Logs capture key request details: domain, upstream proxy, response size, client ID, status, and response time.
- Logs provide structured, queryable data.

**Todo:**
- [ ] Integrate **tracing** for structured logging.
- [ ] Log request/response metadata.
- [ ] Ensure logs are formatted for readability.

### **7. Automate Testing and CI/CD Integration**
**Goal:** Ensure code reliability with automated testing and CI/CD workflows.

**Success Criteria:**
- Unit and integration tests validate correct behavior.
- GitHub Actions automatically builds and tests code on push and PRs.

**Todo:**
- [ ] Write **unit tests** for core functions.
- [ ] Implement **integration tests** using a mock upstream server.
- [ ] Set up a **GitHub Actions workflow** for automated testing.

### **8. Implement Dockerization**
**Goal:** Package the proxy for easy deployment in containerized environments.

**Success Criteria:**
- The Docker image builds successfully.
- The proxy starts correctly in a containerized environment.

**Todo:**
- [ ] Create a `Dockerfile` for building the proxy.
- [ ] Use **multi-stage builds** for efficiency.
- [ ] Ensure the containerized proxy runs with minimal overhead.

### **9. Final Optimizations for Production Readiness**
**Goal:** Ensure stability, reliability, and efficiency in real-world deployment.

**Success Criteria:**
- CPU and memory usage remain within acceptable limits under load.
- The proxy handles real-world traffic with minimal failures.

**Todo:**
- [ ] Review and refine **memory and CPU usage**.
- [ ] Validate **stability under heavy concurrent load**.
- [ ] Deploy to a **production environment** and verify reliability.

---

## Configuration

The proxy server can be configured using the following environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `PROXY_BIND_HOST` | Host address to bind the server to | `127.0.0.1` |
| `PROXY_BIND_PORT` | Port number to listen on | `8081` |
| `PROXY_REQUEST_TIMEOUT_SECS` | Timeout in seconds for upstream requests | `30` |

### Examples

```bash
# Development: bind to localhost with default timeout
PROXY_BIND_HOST=127.0.0.1 PROXY_BIND_PORT=8081 cargo run

# Testing: use custom port and longer timeout
PROXY_BIND_HOST=127.0.0.1 PROXY_BIND_PORT=3000 PROXY_REQUEST_TIMEOUT_SECS=60 cargo run

# Production: bind to all interfaces
PROXY_BIND_HOST=0.0.0.0 PROXY_BIND_PORT=80 PROXY_REQUEST_TIMEOUT_SECS=30 cargo run
```

---

## **Notes for LLM Code Generators**
- Use **`actix-web`** as the main framework.
- Optimize for **async performance** (`tokio`, `reqwest` with `HttpClient` pooling).
- Follow **Rust best practices** (zero-cost abstractions, efficient memory usage).
- Implement **logging hooks** (e.g., `tracing`).
- Ensure **containerization** for deployment.
- Commit changes to git repo after each step
