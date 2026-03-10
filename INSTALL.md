# Installation Guide

This guide covers different ways to install and run the Surreal Obsidian MCP server.

> **⚠️ Pre-release Software (v0.1.0)**
>
> This is pre-release software. **Backup your Obsidian vault** before use, especially if using write operations. Report issues at [GitHub Issues](https://github.com/jemilsson/surreal-obsidian-mcp/issues).

## Table of Contents

- [Pre-built Binaries (Recommended)](#pre-built-binaries-recommended)
- [Build from Source](#build-from-source)
- [Using with Claude Desktop](#using-with-claude-desktop)
- [Platform-Specific Notes](#platform-specific-notes)

## Pre-built Binaries (Recommended)

Download pre-built binaries from the [GitHub Releases](https://github.com/jemilsson/surreal-obsidian-mcp/releases) page.

### Linux

**x86_64 (glibc)**:
```bash
# Download the binary
wget https://github.com/jemilsson/surreal-obsidian-mcp/releases/latest/download/surreal-obsidian-mcp-linux-x86_64

# Make it executable
chmod +x surreal-obsidian-mcp-linux-x86_64

# Move to PATH
sudo mv surreal-obsidian-mcp-linux-x86_64 /usr/local/bin/surreal-obsidian-mcp

# Verify installation
surreal-obsidian-mcp --version
```

**x86_64 (musl, static binary)**:

For older Linux distributions or systems without glibc:

```bash
wget https://github.com/jemilsson/surreal-obsidian-mcp/releases/latest/download/surreal-obsidian-mcp-linux-x86_64-musl
chmod +x surreal-obsidian-mcp-linux-x86_64-musl
sudo mv surreal-obsidian-mcp-linux-x86_64-musl /usr/local/bin/surreal-obsidian-mcp
```

### macOS

**Intel (x86_64)**:
```bash
# Download the binary
curl -L https://github.com/jemilsson/surreal-obsidian-mcp/releases/latest/download/surreal-obsidian-mcp-macos-x86_64 -o surreal-obsidian-mcp

# Make it executable
chmod +x surreal-obsidian-mcp

# Move to PATH
sudo mv surreal-obsidian-mcp /usr/local/bin/

# First run - remove quarantine attribute
sudo xattr -d com.apple.quarantine /usr/local/bin/surreal-obsidian-mcp

# Verify installation
surreal-obsidian-mcp --version
```

**Apple Silicon (ARM64)**:
```bash
curl -L https://github.com/jemilsson/surreal-obsidian-mcp/releases/latest/download/surreal-obsidian-mcp-macos-arm64 -o surreal-obsidian-mcp
chmod +x surreal-obsidian-mcp
sudo mv surreal-obsidian-mcp /usr/local/bin/
sudo xattr -d com.apple.quarantine /usr/local/bin/surreal-obsidian-mcp
```

### Windows

**x86_64**:

1. Download `surreal-obsidian-mcp-windows-x86_64.exe` from [Releases](https://github.com/jemilsson/surreal-obsidian-mcp/releases)
2. Rename to `surreal-obsidian-mcp.exe`
3. Move to a directory in your PATH (e.g., `C:\Program Files\surreal-obsidian-mcp\`)
4. Or add the directory to your PATH:
   - Search for "Environment Variables" in Windows Settings
   - Edit PATH and add the directory containing the executable

**Verify installation**:
```powershell
surreal-obsidian-mcp --version
```

## Build from Source

### Prerequisites

- **Rust**: Install from [rustup.rs](https://rustup.rs/)
- **Git**: For cloning the repository

### Standard Build

```bash
# Clone the repository
git clone https://github.com/jemilsson/surreal-obsidian-mcp
cd surreal-obsidian-mcp

# Build release binary (without embedded reranking)
cargo build --release

# Binary will be at: target/release/surreal-obsidian-mcp
```

### Build with Embedded Reranking

For local reranking with Candle:

```bash
# Build with embedded feature
cargo build --release --features embedded

# Binary will be at: target/release/surreal-obsidian-mcp
```

### Install Locally

```bash
# Install to ~/.cargo/bin (must be in PATH)
cargo install --path .

# With embedded feature
cargo install --path . --features embedded
```

### Platform-Specific Builds

**Cross-compile for Linux musl**:
```bash
# Install musl target
rustup target add x86_64-unknown-linux-musl

# Install musl tools (Ubuntu/Debian)
sudo apt-get install musl-tools

# Build
cargo build --release --target x86_64-unknown-linux-musl --features embedded
```

**Cross-compile for Windows (from Linux)**:
```bash
# Install Windows target
rustup target add x86_64-pc-windows-gnu

# Install MinGW (Ubuntu/Debian)
sudo apt-get install mingw-w64

# Build
cargo build --release --target x86_64-pc-windows-gnu --features embedded
```

## Configuration

### Create Configuration File

Choose a template based on your needs:

```bash
# Local with Ollama (recommended)
cp config.example.json config.json

# OpenAI
cp config.openai.json config.json

# Venice.ai
cp config.venice.json config.json

# Together.ai
cp config.together.json config.json
```

Edit the config file:
```bash
# Linux/macOS
nano config.json

# Windows
notepad config.json
```

**Minimum required changes**:
1. Set `vault.path` to your Obsidian vault path
2. For cloud providers, add your API key

### Example Configuration

```json
{
  "vault": {
    "path": "/home/user/Documents/MyVault"
  },
  "database": {
    "path": "./obsidian.db"
  },
  "embedding": {
    "provider": "ollama",
    "model": "nomic-embed-text",
    "dimensions": 768,
    "api_base": "http://localhost:11434"
  },
  "reranking": {
    "enabled": true,
    "provider": "embedded",
    "model": "BAAI/bge-reranker-base",
    "model_cache": "./model_cache",
    "top_n": 20
  },
  "sync": {
    "watch_for_changes": true,
    "initial_indexing": true,
    "batch_size": 100
  },
  "graph": {
    "extract_links": true,
    "extract_backlinks": true,
    "extract_tags": true,
    "extract_mentions": true
  }
}
```

## Using with Claude Desktop

### Configuration

Add to your Claude Desktop configuration:

**macOS**: `~/Library/Application Support/Claude/claude_desktop_config.json`

**Windows**: `%APPDATA%\Claude\claude_desktop_config.json`

**Linux**: `~/.config/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "obsidian": {
      "command": "/usr/local/bin/surreal-obsidian-mcp",
      "args": ["--config", "/path/to/your/config.json"]
    }
  }
}
```

**Windows example**:
```json
{
  "mcpServers": {
    "obsidian": {
      "command": "C:\\Program Files\\surreal-obsidian-mcp\\surreal-obsidian-mcp.exe",
      "args": ["--config", "C:\\Users\\YourName\\obsidian-config.json"]
    }
  }
}
```

### Restart Claude Desktop

After updating the configuration, restart Claude Desktop for changes to take effect.

### Verify Connection

In Claude Desktop, you should see the Obsidian MCP server tools available. Try:

> "List all files in my vault"

or

> "Search my notes for machine learning"

## Setting Up Ollama (Local Embeddings)

For privacy and zero API costs:

### Install Ollama

**Linux**:
```bash
curl -fsSL https://ollama.com/install.sh | sh
```

**macOS**:
```bash
# Download from https://ollama.com/download/mac
# Or use Homebrew
brew install ollama
```

**Windows**:
- Download installer from https://ollama.com/download/windows

### Pull Embedding Model

```bash
# Start Ollama service (Linux only, macOS/Windows auto-start)
ollama serve

# In a new terminal, pull the embedding model
ollama pull nomic-embed-text
```

### Verify Ollama

```bash
# List installed models
ollama list

# Test embedding
ollama run nomic-embed-text "test"
```

## Platform-Specific Notes

### Linux

**systemd service** (optional, for auto-start):

Create `/etc/systemd/system/surreal-obsidian-mcp.service`:

```ini
[Unit]
Description=Surreal Obsidian MCP Server
After=network.target

[Service]
Type=simple
User=youruser
WorkingDirectory=/home/youruser
ExecStart=/usr/local/bin/surreal-obsidian-mcp --config /home/youruser/config.json
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl daemon-reload
sudo systemctl enable surreal-obsidian-mcp
sudo systemctl start surreal-obsidian-mcp
sudo systemctl status surreal-obsidian-mcp
```

### macOS

**Gatekeeper**: First run may require removing quarantine:

```bash
sudo xattr -d com.apple.quarantine /usr/local/bin/surreal-obsidian-mcp
```

**launchd service** (optional):

Create `~/Library/LaunchAgents/com.surreal-obsidian-mcp.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.surreal-obsidian-mcp</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/surreal-obsidian-mcp</string>
        <string>--config</string>
        <string>/Users/youruser/config.json</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardErrorPath</key>
    <string>/tmp/surreal-obsidian-mcp.err</string>
    <key>StandardOutPath</key>
    <string>/tmp/surreal-obsidian-mcp.out</string>
</dict>
</plist>
```

Load service:
```bash
launchctl load ~/Library/LaunchAgents/com.surreal-obsidian-mcp.plist
```

### Windows

**Windows Defender**: May flag the binary. Add an exception if needed:

1. Open Windows Security
2. Go to Virus & threat protection
3. Click "Manage settings"
4. Scroll to "Exclusions" and add the binary

**Run as Service** (optional, using NSSM):

1. Download NSSM from https://nssm.cc/download
2. Install service:
   ```powershell
   nssm install SurrealObsidianMCP "C:\Program Files\surreal-obsidian-mcp\surreal-obsidian-mcp.exe"
   nssm set SurrealObsidianMCP AppParameters "--config C:\path\to\config.json"
   nssm start SurrealObsidianMCP
   ```

## Troubleshooting

### "command not found"

**Solution**: Ensure the binary is in your PATH or use the full path to the binary.

```bash
# Check PATH
echo $PATH

# Add to PATH temporarily (Linux/macOS)
export PATH=$PATH:/path/to/directory

# Add to PATH permanently
echo 'export PATH=$PATH:/path/to/directory' >> ~/.bashrc
source ~/.bashrc
```

### Permission denied (Linux/macOS)

**Solution**: Make the binary executable:

```bash
chmod +x /path/to/surreal-obsidian-mcp
```

### Library errors (Linux)

**For glibc binary**: Ensure your system has recent glibc:
```bash
ldd --version
```

**Solution**: Use the musl static binary instead (no dependencies).

### macOS quarantine

**Solution**: Remove the quarantine attribute:
```bash
sudo xattr -d com.apple.quarantine /usr/local/bin/surreal-obsidian-mcp
```

### Database locked error

**Cause**: Another instance is already running.

**Solution**:
- Stop other instances
- Or use a different database path in config

### Embedding API errors

See [USAGE.md - Troubleshooting](USAGE.md#troubleshooting) for embedding provider issues.

## Updating

### Pre-built Binary

Download the latest release and replace the old binary:

```bash
# Backup old version (optional)
sudo mv /usr/local/bin/surreal-obsidian-mcp /usr/local/bin/surreal-obsidian-mcp.old

# Download and install new version
# (same steps as installation)
```

### From Source

```bash
cd surreal-obsidian-mcp
git pull
cargo build --release --features embedded
cargo install --path . --features embedded
```

## Uninstallation

### Remove Binary

```bash
# Linux/macOS
sudo rm /usr/local/bin/surreal-obsidian-mcp

# Windows
# Delete from Program Files or wherever installed
```

### Remove Data

```bash
# Remove database
rm -rf ./obsidian.db

# Remove model cache (if using embedded reranking)
rm -rf ./model_cache

# Remove config (if desired)
rm config.json
```

### Remove from Claude Desktop

Edit `claude_desktop_config.json` and remove the `obsidian` entry from `mcpServers`.

## Getting Help

- **Documentation**: See [USAGE.md](USAGE.md) and [EXAMPLES.md](EXAMPLES.md)
- **Issues**: https://github.com/jemilsson/surreal-obsidian-mcp/issues
- **Discussions**: https://github.com/jemilsson/surreal-obsidian-mcp/discussions
