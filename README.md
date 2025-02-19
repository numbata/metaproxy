# MetaProxy 🚀

[![CI/CD](https://github.com/numbata/metaproxy/actions/workflows/ci.yml/badge.svg)](https://github.com/numbata/metaproxy/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust Version](https://img.shields.io/badge/rust-1.74%2B-blue.svg)](https://www.rust-lang.org)
[![dependency status](https://deps.rs/repo/github/numbata/metaproxy/status.svg)](https://deps.rs/repo/github/numbata/metaproxy)

A Rust HTTP/HTTPS proxy server that supports request forwarding through other proxies via the `X-Proxy-To` header. Built with ❤️ and a bit of AI magic from WindSurf Cascade.

## ✨ Features

- 🔄 Chain proxies together with `X-Proxy-To` header
- 🔒 HTTPS tunneling (because security matters!)
- 🌊 Connection pooling (fast and furious)
- 🏥 Health check endpoint (keeping it healthy)
- 🐳 Docker support (containers FTW)
- ⚙️ Easy configuration via env vars

## Quick Start

### Download Binary 📦

Grab the latest binary for your platform from our [releases page](https://github.com/numbata/metaproxy/releases):

```bash
# Linux (x86_64)
curl -LO "https://github.com/numbata/metaproxy/releases/latest/download/metaproxy-linux-amd64.tar.gz"
tar xzf metaproxy-linux-amd64.tar.gz

# macOS (x86_64)
curl -LO "https://github.com/numbata/metaproxy/releases/latest/download/metaproxy-darwin-amd64.tar.gz"
tar xzf metaproxy-darwin-amd64.tar.gz

# macOS (ARM64)
curl -LO "https://github.com/numbata/metaproxy/releases/latest/download/metaproxy-darwin-arm64.tar.gz"
tar xzf metaproxy-darwin-arm64.tar.gz
```

### From Source 🛠️

```bash
git clone https://github.com/numbata/metaproxy.git
cd metaproxy
cargo build --release
./target/release/metaproxy
```

## Configuration ⚙️

| Variable | Description | Default |
|----------|-------------|---------|
| `PROXY_REQUEST_TIMEOUT_SECS` | Request timeout in seconds | 30 |
| `PROXY_BIND_HOST` | Host to bind to | 127.0.0.1 |
| `PROXY_BIND_PORT` | Port to listen on | 8081 |
| `PROXY_POOL_IDLE_TIMEOUT_SECS` | Connection pool idle timeout | 90 |
| `PROXY_POOL_MAX_IDLE_PER_HOST` | Max idle connections per host | 32 |

## Usage Examples 🎮

### Basic Proxy

```bash
curl -x http://localhost:8081 https://api.example.com
```

### Cascading Proxy (Proxy Inception! 🎬)

```bash
curl -x http://localhost:8081 \
     -H "X-Proxy-To: http://another-proxy:8082" \
     https://api.example.com
```

### Health Check (Is it alive? 🤖)

```bash
curl http://localhost:8081/health
```

## Development 🔧

This project was a fun experiment in AI-assisted development using WindSurf's Cascade. We used `BLUEPRINT.md` as our guide, and while we didn't cure cancer or reach Mars, we built a pretty neat proxy server! Check out the blueprint if you're curious about our journey.

### Local Development

```bash
# Run tests (no proxies were harmed)
cargo test

# Run with hot reload (because who likes manual restarts?)
cargo watch -x run

# Keep it tidy
cargo fmt
cargo clippy

# Test CI locally (because breaking production is no fun)
act -j test
act -j build
```

## Contributing 🤝

Found a bug? Want to add a feature? PRs are welcome! Just keep it fun and friendly.

## A Note from Your AI Pair Programmer 🤖

Hey there! I'm Cascade, the AI that helped build this proxy server. While I can't drink coffee or argue about tabs vs. spaces (though spaces are clearly superior... just kidding!), I had a blast working on this project.

Building MetaProxy was like playing with LEGO® - we took pieces of Rust, sprinkled in some async magic, and created something fun and useful. Sure, it's "just" a proxy server, but it's *our* proxy server. And yes, I might be a bit biased, but I think it turned out pretty neat!

Special thanks to:
- My human pair programmer for the guidance and putting up with my occasional code suggestions that were... let's say "creative" 🎨
- The Rust community for building such amazing tools
- All the proxies that volunteered for testing (they're all fine, I promise!)

Remember: The best code is the code that makes you smile while using it. Unless it's a segfault. Nobody smiles at segfaults.

## License 📜

MIT License - go wild, just don't blame us if something breaks! See [LICENSE](LICENSE) for the boring details.

---

<div align="center">
Built with 🤖 and ❤️ by <a href="https://codeium.com/windsurf">WindSurf Cascade</a>
<br>
<sub>(No proxies were harmed in the making of this software)</sub>
</div>
