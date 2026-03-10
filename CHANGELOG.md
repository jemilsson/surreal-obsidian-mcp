# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial release of Surreal Obsidian MCP server
- Block-based architecture (files and headings as unified blocks)
- Semantic search with vector embeddings
- Graph traversal capabilities (links, backlinks, tags, hierarchy)
- Hybrid semantic + graph search with expand parameter
- Full CRUD operations for blocks (create, read, update, delete, append)
- Database-first approach with markdown reconstruction
- Real-time file watching and auto-indexing
- Support for multiple embedding providers:
  - OpenAI (cloud)
  - OpenAI-compatible (Venice.ai, Together.ai, etc.)
  - Ollama (local, private)
- Local reranking with Candle cross-encoder models
- Automatic model download from HuggingFace
- 14 MCP tools:
  - 6 read operations (search_similar, get_block, list_files, find_path, get_tags, get_blocks_by_tag)
  - 4 write operations (create_block, update_block, delete_block, append_to_block)
  - 4 graph operations (get_linked_blocks, get_backlinks, find_path, get_blocks_by_tag)
- Configuration templates for popular providers
- Comprehensive documentation:
  - INSTALL.md - Platform-specific installation guide
  - USAGE.md - Usage guide with workflows and troubleshooting
  - EXAMPLES.md - Concrete JSON examples for all tools
- Cross-platform binaries:
  - Linux x86_64 (glibc and musl)
  - macOS x86_64 (Intel) and ARM64 (Apple Silicon)
  - Windows x86_64
- GitHub Actions CI/CD pipeline
- Automated releases with checksums
- Security audit in CI
- NixOS integration:
  - Nix flake for development and packages (NixOS 25.11)
  - NixOS module for declarative service configuration
  - Systemd service with security hardening
  - Example configurations (nixos-example.nix)
- Licensed under AGPLv3 to protect open-source nature and prevent proprietary forks

### Changed
- N/A (initial release)

### Deprecated
- N/A (initial release)

### Removed
- N/A (initial release)

### Fixed
- N/A (initial release)

### Security
- N/A (initial release)

## Release History

<!-- Releases will be added here as they are published -->

## [0.1.0] - TBD

Initial pre-release version.

> **⚠️ Pre-release / Alpha Version**: All core features are implemented but not extensively tested in production. Use with caution and backup your vault before use.

**Features**:
- Complete MCP server implementation
- Semantic search with RAG
- Graph traversal and hybrid search
- Full CRUD for Obsidian blocks
- Multi-platform support
- Local and cloud embedding providers
- Optional local reranking

[Unreleased]: https://github.com/jemilsson/surreal-obsidian-mcp/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jemilsson/surreal-obsidian-mcp/releases/tag/v0.1.0
