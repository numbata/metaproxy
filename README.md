<div align="center">

# 🚀 Metaproxy

[![Rust](https://img.shields.io/badge/rust-1.56%2B-orange.svg)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)]()
[![Code Style](https://img.shields.io/badge/code%20style-rustfmt-blue.svg)](https://github.com/rust-lang/rustfmt)

A modular HTTP proxy server with dynamic binding configuration via a REST API.

</div>

> **Note**: This codebase was primarily generated with the assistance of an AI coding assistant (Cascade by Codeium).

## ✨ Features

- 🔄 **Dynamic Proxy Bindings**: Create, update, and delete proxy bindings at runtime via REST API
- 🌐 **HTTP Proxy**: Support for standard HTTP proxying with header adjustment
- 🔒 **CONNECT Tunneling**: Support for HTTPS tunneling via the CONNECT method
- 🧩 **Modular Architecture**: Clean separation of concerns for better maintainability and testability
- ⚡ **Async I/O**: Built on Tokio for high-performance asynchronous I/O
- ⏱️ **Request Timeouts**: Configurable timeouts for upstream connections to prevent hanging requests

## 📦 Installation

### Prerequisites

- 🦀 Rust 1.56.0 or later
- 📦 Cargo

### Building from Source

```bash
# Clone the repository
git clone https://github.com/numbata/metaproxy.git
cd metaproxy

# Build the project
cargo build --release

# Run the binary
./target/release/metaproxy
```

## 🚀 Usage

```bash
# Start the proxy server with default settings
cargo run

# Start the proxy server with a custom bind address
cargo run -- --bind 0.0.0.0:8000

# Start the proxy server with a custom request timeout (in seconds)
cargo run -- --request-timeout 10

# Disable request timeout (wait indefinitely)
cargo run -- --request-timeout 0
```

### 🎮 Command Line Options

| Option | Description | Default |
|--------|-------------|---------|
| `--bind` | Address to bind the proxy server to | `127.0.0.1:8000` |
| `--request-timeout` | Timeout for upstream requests in seconds (0 for no timeout) | `30` |

### 🔌 API Endpoints

The proxy server exposes the following REST API endpoints:

#### 💓 Health Check

```
GET /health
```

Returns the status of the proxy server and a list of active bindings.

Example response:
```json
{
  "status": "ok",
  "bindings": [
    {
      "port": 9000,
      "upstream": "http://127.0.0.1:8080"
    }
  ]
}
```

#### 🆕 Create Proxy Binding

```
POST /proxy
```

Creates a new proxy binding.

Request body:
```json
{
  "port": 9000,
  "upstream": "http://127.0.0.1:8080"
}
```

Example response:
```json
{
  "status": "created",
  "port": 9000,
  "upstream": "http://127.0.0.1:8080"
}
```

#### 🔄 Update Proxy Binding

```
PUT /proxy/{port}
```

Updates an existing proxy binding.

Request body:
```json
{
  "upstream": "http://127.0.0.1:9090"
}
```

Example response:
```json
{
  "status": "updated",
  "port": 9000,
  "upstream": "http://127.0.0.1:9090"
}
```

#### 🗑️ Delete Proxy Binding

```
DELETE /proxy/{port}
```

Deletes an existing proxy binding.

Example response:
```json
{
  "status": "deleted",
  "port": 9000
}
```

## 📝 Example Usage

### Creating a Proxy Binding

```bash
# Create a proxy binding on port 9000 that forwards to 127.0.0.1:8080
curl -X POST http://127.0.0.1:8000/proxy \
  -H "Content-Type: application/json" \
  -d '{"port": 9000, "upstream": "http://127.0.0.1:8080"}'
```

### Using the Proxy

```bash
# Use the proxy for HTTP requests
curl -x http://127.0.0.1:9000 http://example.com

# Use the proxy for HTTPS requests
curl -x http://127.0.0.1:9000 https://example.com
```

## ⏱️ Request Timeouts

Metaproxy includes configurable request timeouts for upstream connections. This helps prevent hanging connections and improves reliability when upstream servers are unresponsive.

- ⏰ **Global Timeout**: Set a global timeout for all proxy bindings using the `--request-timeout` command line option
- 🛑 **Automatic Cancellation**: Requests that exceed the timeout are automatically canceled with an appropriate error message
- 🔧 **Configurable**: Timeout can be set in seconds, or disabled completely by setting it to 0

Example:
```bash
# Set a 5-second timeout for all requests
cargo run -- --request-timeout 5
```

When a timeout occurs, the connection is terminated and an error is logged:
```
[2025-02-26T01:15:22Z WARN metaproxy::proxy] Connection to upstream timed out after 5 seconds: example.com:80
```

## 💻 Development

### 📁 Project Structure

- `src/main.rs` - Entry point for the application
- `src/lib.rs` - Library interface and module exports
- `src/config.rs` - Configuration handling
- `src/error.rs` - Error types and handling
- `src/api.rs` - API routes and handlers
- `src/proxy.rs` - Proxy functionality

### 🧪 Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture
```

## 📊 Logging

Metaproxy uses the `log` crate with `env_logger` for structured logging. You can control the log level by setting the `RUST_LOG` environment variable when running the application.

### 📋 Log Levels

- 🔴 **error**: Logs critical errors that prevent the application from functioning properly
- 🟠 **warn**: Logs potentially harmful situations that don't prevent the application from running
- 🟢 **info**: Logs general information about the application's operation (default)
- 🔵 **debug**: Logs detailed information useful for debugging
- 🟣 **trace**: Logs very detailed information, including internal operations of libraries

### Examples

```bash
# Show only errors and warnings
RUST_LOG=warn cargo run

# Show info level and above (recommended for normal use)
RUST_LOG=info cargo run

# Show all logs including debug information
RUST_LOG=debug cargo run

# Show extremely verbose logging (including from dependencies)
RUST_LOG=trace cargo run

# Target specific modules
RUST_LOG=metaproxy::proxy=debug,metaproxy::api=info cargo run
```

### 📋 Log Format

Each log entry includes:
- ⏰ Timestamp
- 🏷️ Log level
- 📍 Module path
- 📝 Message

Example:
```
[2025-02-26T01:06:22Z INFO metaproxy::api] Creating new proxy binding on port 8080 with upstream http://example.com
```

## 💭 AI Insights and Future Directions

As the AI assistant that helped generate this codebase, I'd like to share some thoughts on the architecture and potential future improvements:

### 🤔 Personal Reflections

Working on Metaproxy has highlighted the elegant match between Rust's ownership model and network programming challenges. The async/await pattern in Tokio makes concurrent code both efficient and readable, while Rust's type system helps prevent common networking bugs at compile time. What excites me most about this project isn't just the code itself, but the foundation it provides for numerous applications - from API gateways to testing environments, security tools to performance monitoring systems. We've deliberately kept Metaproxy focused on core functionality while establishing clean interfaces for future expansion. This balance between immediate utility and future extensibility embodies what I consider the essence of good software design.

### 🏗️ Architecture Decisions

The modular architecture of Metaproxy was designed with several key principles in mind:

1. **Separation of Concerns**: Each module has a clear, focused responsibility:
   - `api.rs` handles the REST API interface
   - `proxy.rs` manages the core proxy functionality
   - `config.rs` handles configuration
   - `error.rs` provides unified error handling

2. **Concurrency Model**: The use of Tokio's asynchronous runtime and Mutex-protected shared state allows for:
   - Efficient handling of multiple concurrent connections
   - Thread-safe updates to proxy bindings at runtime
   - Non-blocking I/O operations

3. **Error Handling**: The custom error type with conversions from common error types provides:
   - Consistent error reporting across the application
   - Detailed error messages for debugging
   - Type safety through the Result type alias

### 🔮 Future Improvements

If I were to continue developing this project, here are some enhancements I would consider:

1. **Performance Optimizations**:
   - 🚄 Implement connection pooling for upstream connections
   - 💾 Add caching for frequently accessed resources
   - ⚡ Optimize buffer sizes for different types of traffic

2. **Security Enhancements**:
   - 🔒 Add TLS support for secure client connections
   - 🔑 Implement authentication for the API endpoints
   - 🛡️ Add request validation and rate limiting

3. **Observability**:
   - 📊 Implement structured logging with different log levels
   - 📈 Add metrics collection for monitoring performance
   - 🔍 Create tracing for request paths through the system

4. **Advanced Features**:
   - 🔌 Support for WebSockets and HTTP/2
   - 🔄 Content transformation and filtering
   - ⚖️ Load balancing across multiple upstream servers
   - 🔌 Circuit breaking for failing upstream servers

The current implementation provides a solid foundation that can be extended in many directions based on specific use cases and requirements.

## 📜 License

This project is licensed under the MIT License - see the LICENSE file for details.

## 👥 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
