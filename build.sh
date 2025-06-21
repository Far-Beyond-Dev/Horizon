#!/bin/bash
# build.sh - Production build script for Distributed Games Server

set -e

echo "Building Distributed Games Server for Production..."

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    print_error "Cargo.toml not found. Please run this script from the project root."
    exit 1
fi

# Clean previous builds
print_status "Cleaning previous builds..."
cargo clean

# Build shared types library first
print_status "Building shared types library..."
cargo build --release --package shared-types

# Build server core
print_status "Building server core..."
cargo build --release --package server-core

# Build horizon plugin as dynamic library
print_status "Building Horizon plugin..."
cargo build --release --package horizon-plugin

# Build main server executable
print_status "Building main server executable..."
cargo build --release --package game-server

# Create deployment directory
DEPLOY_DIR="deploy"
print_status "Creating deployment directory: $DEPLOY_DIR"
rm -rf $DEPLOY_DIR
mkdir -p $DEPLOY_DIR/{bin,plugins,config}

# Copy binaries
print_status "Copying binaries..."
cp target/release/game-server $DEPLOY_DIR/bin/

# Copy plugin libraries
print_status "Copying plugin libraries..."
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    cp target/release/libhorizon_plugin.so $DEPLOY_DIR/plugins/libhorizon.so
elif [[ "$OSTYPE" == "darwin"* ]]; then
    cp target/release/libhorizon_plugin.dylib $DEPLOY_DIR/plugins/libhorizon.dylib
elif [[ "$OSTYPE" == "msys" || "$OSTYPE" == "win32" ]]; then
    cp target/release/horizon_plugin.dll $DEPLOY_DIR/plugins/horizon.dll
else
    print_warning "Unknown OS type: $OSTYPE"
    print_warning "Please manually copy the plugin library to $DEPLOY_DIR/plugins/"
fi

# Create default configuration
print_status "Creating default configuration..."
cat > $DEPLOY_DIR/config/config.toml << 'EOF'
# Distributed Games Server Configuration

[server]
listen_addr = "0.0.0.0:8080"
max_players = 1000
tick_rate = 50  # milliseconds between ticks

[region]
# Define the 3D region bounds this server instance manages
min_x = -1000.0
max_x = 1000.0
min_y = -1000.0
max_y = 1000.0
min_z = -100.0
max_z = 100.0

[plugins]
directory = "plugins"
auto_load = ["horizon"]  # Plugins to load automatically on startup

[logging]
level = "info"  # trace, debug, info, warn, error
json_format = false
EOF

# Create startup script
print_status "Creating startup script..."
cat > $DEPLOY_DIR/start_server.sh << 'EOF'
#!/bin/bash
# Startup script for Distributed Games Server

cd "$(dirname "$0")"

# Set environment variables
export RUST_LOG=${RUST_LOG:-info}
export RUST_BACKTRACE=${RUST_BACKTRACE:-1}

# Check if config exists
if [ ! -f "config/config.toml" ]; then
    echo "Configuration file not found. Creating default..."
    mkdir -p config
fi

# Start the server
echo "Starting Distributed Games Server..."
exec ./bin/game-server --config config/config.toml "$@"
EOF

chmod +x $DEPLOY_DIR/start_server.sh

# Create systemd service file
print_status "Creating systemd service file..."
cat > $DEPLOY_DIR/config/game-server.service << 'EOF'
[Unit]
Description=Distributed Games Server
After=network.target
Wants=network.target

[Service]
Type=simple
User=gameserver
Group=gameserver
WorkingDirectory=/opt/game-server
ExecStart=/opt/game-server/start_server.sh
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=game-server
KillMode=mixed
KillSignal=SIGTERM
TimeoutStopSec=30

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/game-server
CapabilityBoundingSet=CAP_NET_BIND_SERVICE

# Resource limits
LimitNOFILE=65536
LimitNPROC=4096

[Install]
WantedBy=multi-user.target
EOF

# Create Docker support
print_status "Creating Dockerfile..."
cat > $DEPLOY_DIR/Dockerfile << 'EOF'
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create user
RUN useradd -m -u 1000 gameserver

# Copy application
COPY bin/ /app/bin/
COPY plugins/ /app/plugins/
COPY config/ /app/config/
COPY start_server.sh /app/

# Set permissions
RUN chown -R gameserver:gameserver /app
RUN chmod +x /app/start_server.sh

# Switch to non-root user
USER gameserver
WORKDIR /app

# Expose port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Start server
CMD ["./start_server.sh"]
EOF

# Create docker-compose.yml
print_status "Creating docker-compose.yml..."
cat > $DEPLOY_DIR/docker-compose.yml << 'EOF'
version: '3.8'

services:
  game-server:
    build: .
    ports:
      - "8080:8080"
    volumes:
      - ./config:/app/config
      - ./plugins:/app/plugins
      - logs:/app/logs
    environment:
      - RUST_LOG=info
      - RUST_BACKTRACE=1
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 40s

volumes:
  logs:
EOF

# Create README for deployment
print_status "Creating deployment README..."
cat > $DEPLOY_DIR/README.md << 'EOF'
# Distributed Games Server - Production Deployment

This directory contains a production-ready deployment of the Distributed Games Server.

## Directory Structure

```
deploy/
├── bin/                    # Server executable
│   └── game-server
├── plugins/                # Plugin libraries
│   └── libhorizon.so      # (or .dylib/.dll depending on OS)
├── config/                 # Configuration files
│   ├── config.toml        # Main server configuration
│   └── game-server.service # Systemd service file
├── start_server.sh        # Startup script
├── Dockerfile             # Docker configuration
├── docker-compose.yml     # Docker Compose configuration
└── README.md              # This file
```

## Quick Start

### Local Deployment

1. Make the startup script executable (if not already):
   ```bash
   chmod +x start_server.sh
   ```

2. Start the server:
   ```bash
   ./start_server.sh
   ```

3. The server will start on `http://0.0.0.0:8080` by default.

### Docker Deployment

1. Build and start with Docker Compose:
   ```bash
   docker-compose up -d
   ```

2. View logs:
   ```bash
   docker-compose logs -f game-server
   ```

3. Stop the server:
   ```bash
   docker-compose down
   ```

### Systemd Service (Linux)

1. Copy files to system directory:
   ```bash
   sudo cp -r . /opt/game-server/
   sudo useradd -m -s /bin/false gameserver
   sudo chown -R gameserver:gameserver /opt/game-server
   ```

2. Install and start the service:
   ```bash
   sudo cp config/game-server.service /etc/systemd/system/
   sudo systemctl daemon-reload
   sudo systemctl enable game-server
   sudo systemctl start game-server
   ```

3. Check status:
   ```bash
   sudo systemctl status game-server
   ```

## Configuration

Edit `config/config.toml` to customize:

- **Server settings**: Listen address, max players, tick rate
- **Region bounds**: 3D space boundaries for this server instance
- **Plugin settings**: Which plugins to load automatically
- **Logging**: Log level and format

## Plugin Development

To add new plugins:

1. Create a new Rust library with `crate-type = ["cdylib"]`
2. Implement the `Plugin` trait from `shared-types`
3. Export `create_plugin()` and `destroy_plugin()` functions
4. Build as a dynamic library and place in the `plugins/` directory
5. Add to `auto_load` list in configuration

## Monitoring

The server provides structured logging via the `tracing` crate. Logs include:

- Player connections and disconnections
- Plugin loading and events
- Performance metrics
- Error conditions

## Security Notes

- The server runs as a non-privileged user when using systemd
- Docker container runs as non-root user
- Network access is limited to the configured port
- Plugins are isolated but share the process space (Rust memory safety applies)

## Performance Tuning

For high-load deployments:

1. Increase file descriptor limits in systemd service
2. Tune region bounds to match server capacity
3. Adjust tick rate based on game requirements
4. Monitor memory usage and adjust max players accordingly
5. Consider running multiple instances with different regions

## Troubleshooting

### Server won't start
- Check configuration file syntax
- Verify plugin libraries are present and compatible
- Check port availability
- Review logs for specific error messages

### Plugin loading fails
- Ensure plugin library architecture matches server
- Verify plugin implements required functions
- Check plugin dependencies are satisfied

### Connection issues
- Verify firewall settings
- Check listen address configuration
- Ensure WebSocket support is working

## Support

For issues and questions:
- Check logs first: `journalctl -u game-server -f`
- Verify configuration: `./bin/game-server --help`
- Test with minimal configuration
EOF

# Run tests
print_status "Running tests..."
cargo test --workspace

# Generate documentation
print_status "Generating documentation..."
cargo doc --workspace --no-deps

print_status "Build completed successfully!"
print_status "Deployment ready in: $DEPLOY_DIR"
print_status ""
print_status "Next steps:"
print_status "1. cd $DEPLOY_DIR"
print_status "2. Review config/config.toml"
print_status "3. Run ./start_server.sh"
print_status ""
print_status "For production deployment, see $DEPLOY_DIR/README.md"
EOF