# Metaproxy

A modular HTTP proxy server with dynamic binding configuration via a REST API.

## Features

- **Dynamic Proxy Bindings**: Create, update, and delete proxy bindings at runtime via REST API
- **HTTP Proxy**: Support for standard HTTP proxying with header adjustment
- **CONNECT Tunneling**: Support for HTTPS tunneling via the CONNECT method
- **Modular Architecture**: Clean separation of concerns for better maintainability and testability
- **Async I/O**: Built on Tokio for high-performance asynchronous I/O

## Installation

### Prerequisites

- Rust 1.56.0 or later
- Cargo

### Building from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/metaproxy.git
cd metaproxy

# Build the project
cargo build --release

# Run the binary
./target/release/metaproxy
```

## Usage

### Command Line Options

```bash
# Start the proxy server on the default address (127.0.0.1:8000)
./target/release/metaproxy

# Start the proxy server on a custom address
./target/release/metaproxy --bind 0.0.0.0:8080
```

### API Endpoints

The proxy server exposes the following REST API endpoints:

#### Health Check

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

#### Create Proxy Binding

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

#### Update Proxy Binding

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

#### Delete Proxy Binding

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

## Example Usage

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

## Development

### Project Structure

- `src/main.rs` - Entry point for the application
- `src/lib.rs` - Library interface and module exports
- `src/config.rs` - Configuration handling
- `src/error.rs` - Error types and handling
- `src/api.rs` - API routes and handlers
- `src/proxy.rs` - Proxy functionality

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
