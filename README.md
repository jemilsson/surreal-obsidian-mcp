# Surreal Obsidian MCP

An MCP (Model Context Protocol) server written in Rust that indexes your Obsidian vault into an embedded SurrealDB instance, providing AI assistants with powerful semantic search and graph traversal capabilities.

> **⚠️ Work in Progress**
>
> This project is in active development (v0.1.0 pre-release). While all core features are implemented and functional, it has not been extensively tested in production environments. Use with caution and expect potential bugs or breaking changes.
>
> **Backup your vault before use.** The server modifies markdown files when using write operations.
>
> Issues and feedback welcome at [GitHub Issues](https://github.com/jemilsson/surreal-obsidian-mcp/issues).

## Overview

This MCP server gives AI assistants the ability to:

- **Block Management**: Create, read, update, and delete blocks (notes are just root-level blocks)
- **Semantic Search**: Find blocks based on meaning using RAG with vector embeddings
- **Graph Queries**: Traverse block relationships through links, backlinks, tags, and hierarchy
- **Context Retrieval**: Pull relevant blocks to augment AI conversations
- **Real-time Sync**: Keep the index updated as your vault changes

**Everything is a Block**: Notes/files are simply root-level blocks (level 0). Headings create child blocks. This unified model makes the entire vault a hierarchical graph where every piece of content is a first-class block with its own ID, embedding, and relationships.

*Example*: A file `project.md` with sections becomes a tree of blocks:

```text
Block(level=0, title="project.md") [the file itself]
├── Block(level=2, title="Overview") [## Overview section]
├── Block(level=2, title="Requirements") [## Requirements section]
│   └── Block(level=3, title="Authentication") [### Authentication subsection]
└── Block(level=2, title="Tasks") [## Tasks section]
```

Each block has its own embedding, can be searched individually, and participates in the graph.

Built with Rust for performance and using embedded SurrealDB for zero-configuration setup.

## Documentation

- **[INSTALL.md](INSTALL.md)** - Installation guide for all platforms with pre-built binaries
- **[USAGE.md](USAGE.md)** - Comprehensive usage guide with workflows, examples, and troubleshooting
- **[EXAMPLES.md](EXAMPLES.md)** - Concrete JSON examples for all MCP tools
- **Configuration Templates**:
  - [config.example.json](config.example.json) - Ollama (local, private, recommended)
  - [config.openai.json](config.openai.json) - OpenAI (cloud, fast)
  - [config.venice.json](config.venice.json) - Venice.ai (cloud, private, uncensored)
  - [config.together.json](config.together.json) - Together.ai (cloud, cost-effective)

## Why Bidirectional Communication?

**Traditional AI chat is inefficient for knowledge work.** Conversations disappear, insights get lost, and you're constantly re-explaining context.

This MCP server enables **bidirectional communication through your notes**:

**AI → Notes (Write)**:

- Add analysis sections with insights and findings
- Create task lists from discussions
- Leave inline suggestions and improvements
- Update metadata (tags, status, properties)
- Create new linked notes with research results
- Maintain discussion/review sections for async collaboration

**Notes → AI (Read)**:

- Access full vault context and relationships
- Find relevant information across all notes
- Understand connections through graph traversal

**Benefits**:

- **Persistent**: Changes stay in your notes, not lost in chat history
- **Structured**: Use markdown headings, lists, frontmatter for organized communication
- **Contextual**: AI edits notes in-place where information belongs
- **Asynchronous**: AI works on notes, you review later at your own pace
- **Collaborative**: Like having a research assistant who reads, thinks, and writes in your knowledge base

*Example workflow*:

1. Ask AI to research a topic
2. AI searches your vault for existing knowledge
3. AI creates/updates note with findings in a "## Analysis" section
4. AI adds related links and tags
5. You review, refine, and continue the cycle

This turns your vault into a **shared workspace** between you and AI agents, not just a read-only reference.

## Why This Stack?

**SurrealDB (Embedded)**:

- Multi-model database combining documents, graphs, and vectors in one system
- Native vector support for semantic similarity search
- Graph capabilities perfect for both note relationships AND document structure (AST)
- Store document structure as graph nodes (headings, lists, blocks) alongside note relationships
- Embedded mode means no separate database server
- Fast queries even on large vaults

**Rust**:

- Native performance for indexing and queries
- Memory safety without garbage collection
- Efficient concurrent file watching and database operations
- Small binary footprint

## Features

### Retrieval Augmented Generation (RAG)

The server provides tools for semantic search over your vault:

- **Block-level Embeddings**: Each block (heading + content) gets its own vector embedding for precise retrieval
- **Granular Search**: Find specific blocks within notes, not just whole documents
- **Contextual Results**: Return relevant blocks with their parent note context
- **Two-stage Retrieval**: Initial vector search followed by optional reranking for higher precision
- **Reranking**: Use cross-encoder models to rerank top candidates for improved relevance
- **Similarity Search**: Find notes and blocks by semantic meaning, not just keywords
- **Natural Chunking**: Blocks are natural semantic boundaries (better than arbitrary character splits)
- **Flexible Embedding Providers**:
  - **OpenAI API**: Official OpenAI embeddings
  - **OpenAI-compatible APIs**: Venice.ai, OpenRouter, Together.ai, Anyscale, etc.
  - **Ollama**: Local models via Ollama (nomic-embed-text, mxbai-embed-large, etc.) - fully offline
- **Hybrid Search**: Combine vector similarity with keyword matching (BM25) for best results

*Example*: A 5000-word project note with blocks on "## Requirements", "## Architecture", and "## Testing" will have three separate embeddings. When you search for "testing strategies", only the relevant block is retrieved, not the entire note.

**How Reranking Works**:

1. **Initial retrieval**: Vector search finds top 20-100 candidate blocks quickly
2. **Reranking**: A more sophisticated model (cross-encoder) scores query-block pairs for true relevance
3. **Final results**: Return top N reranked results with higher precision

This two-stage approach balances speed (fast vector search) with accuracy (precise reranking).

**Reranking Providers**:

- **Cohere**: `rerank-english-v3.0` (English), `rerank-multilingual-v3.0` (100+ languages) - High quality, API-based
- **Jina AI**: `jina-reranker-v2-base-multilingual` - Multilingual, OpenAI-compatible API, good balance of speed/quality
- **Ollama**: `bge-reranker-v2-m3` - Local reranking via Ollama, fully offline, free but requires separate Ollama process
- **Embedded**: `BAAI/bge-reranker-base`, `cross-encoder/ms-marco-MiniLM-L-6-v2` - Models loaded directly into the binary using Rust ML frameworks (Candle or ONNX Runtime), zero external dependencies, fully self-contained
- **Disabled**: Set `enabled = false` to skip reranking (faster but less accurate)

Reranking typically improves accuracy by 10-30% but adds latency. For most use cases, Cohere or Jina provide the best quality-to-speed ratio. For fully self-contained local deployment without Ollama, use embedded rerankers.

**Embedding Providers**:

- **OpenAI**: `text-embedding-3-small` (1536d, fast), `text-embedding-3-large` (3072d, high quality)
- **Venice.ai**: `text-embedding-bge-m3` (1024d, privacy-focused, OpenAI-compatible API)
- **OpenAI-compatible**: Any service with OpenAI-style API (OpenRouter, Together.ai, Anyscale, etc.)
- **Ollama**: `nomic-embed-text` (768d), `mxbai-embed-large` (1024d), `snowflake-arctic-embed` (1024d) - runs locally, fully offline
- **Embedded**: `BAAI/bge-small-en-v1.5` (384d), `BAAI/bge-base-en-v1.5` (768d), `nomic-ai/nomic-embed-text-v1.5` (768d) - Models loaded directly into binary using Candle, zero external dependencies, fully self-contained

Choose based on your needs: OpenAI for quality and ease, Venice.ai for privacy with cloud convenience, Ollama for full privacy with local models, or embedded for single-binary deployment with zero dependencies.

**Available MCP Tools**:

- `search_blocks` - Semantic search across all blocks (filter by level: 0 for documents, 1-6 for headings)
- `get_similar_blocks` - Find blocks similar to a given block
- `get_block` - Retrieve any block's content with full context

### Graph Database

Navigate the relationships between your notes:

- **Automatic Relationship Extraction**: Links, backlinks, tags automatically indexed
- **AST-based Structure**: Markdown AST (headings, lists, code blocks, etc.) parsed and stored in the graph
- **Block Graph**: Each block is a node in the graph with parent/child relationships
- **Document Structure Queries**: Find notes by structure (e.g., all notes with a "## Tasks" block)
- **Intra-document Navigation**: Query and traverse within document structure
- **Graph Traversal**: Find notes connected through multiple relationships
- **Path Finding**: Discover how concepts connect
- **Community Detection**: Identify topic clusters
- **Temporal Tracking**: See how your knowledge graph evolves

**Available MCP Tools**:

- `traverse_graph` - Navigate relationships (links, backlinks, tags, hierarchy) with depth control
- `find_path` - Find connection paths between any two blocks
- `get_linked_blocks` - Get blocks that a block links to
- `get_backlinks` - Get blocks that link to a block
- `get_block_children` - Get child blocks in hierarchy
- `find_by_structure` - Find blocks with specific structural patterns
- `query_structure` - Complex AST graph queries

### Block Management (CRUD)

Everything in your vault is a block - files, headings, sections - all managed through a unified API:

**What is a block?**

- **Level 0 (Document/Root block)**: Represents a markdown file. Has frontmatter, filename, folder path.
- **Level 1-6 (Heading blocks)**: Markdown headings (`#` through `######`) and their content until the next heading of equal or higher level.
- All blocks have unique IDs, vector embeddings, and exist as nodes in the graph.
- Blocks maintain parent/child relationships forming a hierarchical tree per document.

**Operations**:

- **Create Blocks**: Create new files (level 0) or add headings to existing blocks
  - Without parent → creates a file (level 0)
  - With parent → creates child heading (parent.level + 1)
  - With explicit level → creates at that level
- **Read Blocks**: Retrieve any block's content and metadata
- **Update Blocks**: Modify block content, properties, or frontmatter (for level 0)
- **Delete Blocks**: Remove blocks and optionally their children
- **Move Blocks**: Reorder blocks within hierarchy or move between documents
- **Append**: Add content to any block
- **Bulk Operations**: Create or update multiple blocks atomically
- **Template Support**: Use templates when creating document blocks

*Examples*:

```typescript
// Create a new file
create_block({ title: "My Note", content: "Introduction..." })
// → Creates level 0 block (file)

// Add a heading to that file
create_block({ parent_id: "block_abc", title: "Overview", content: "..." })
// → Creates level 2 block (## Overview)

// Explicitly control level
create_block({ parent_id: "block_abc", level: 3, title: "Details" })
// → Creates level 3 block (### Details)
```

**Available MCP Tools**:

- `create_block` - Create a block (no parent = file, with parent = child heading, or specify level explicitly)
- `get_block` - Retrieve a specific block's content and metadata
- `update_block` - Update block content or properties
- `delete_block` - Delete a block and optionally its children
- `append_to_block` - Append content to a block
- `move_block` - Move a block within hierarchy or between documents
- `list_blocks` - List blocks with filtering (by level, parent, tags, etc.)
- `find_replace_in_block` - Find and replace text within a block
- `get_block_children` - Get all child blocks of a block
- `set_block_properties` - Set properties (frontmatter for level 0, metadata for others)

## Architecture

```text
┌─────────────────┐
│                 │
│  AI Assistant   │
│  (Claude, etc)  │
│                 │
└────────┬────────┘
         │ MCP Protocol
         │
┌────────▼────────────────────┐         ┌─────────────┐
│                             │         │             │
│   Surreal Obsidian MCP      │────────▶│  SurrealDB  │
│   (Rust Server)             │         │  (Embedded) │
│                             │         │             │
│  - Exposes MCP Tools        │         │  - Vectors  │
│  - Watches Vault            │         │  - Graph    │
│  - Indexes Content          │         │  - Docs     │
│  - Manages Notes (CRUD)     │         │             │
│                             │         │             │
└────────┬────────────────────┘         └─────────────┘
         │
         │ Read/Write
┌────────▼────────┐
│                 │
│  Obsidian Vault │
│  (Markdown)     │
│                 │
└─────────────────┘
```

## Quick Start

See [USAGE.md](USAGE.md) for comprehensive usage examples and common workflows.

```bash
# Clone the repository
git clone https://github.com/jemilsson/surreal-obsidian-mcp
cd surreal-obsidian-mcp

# Build with reranking support (recommended)
cargo build --release --features embedded

# Choose a config template based on your needs:
# - config.example.json (Ollama - local, private, free)
# - config.openai.json (OpenAI - cloud, fast, paid)
# - config.together.json (Together.ai - cloud, cost-effective)

cp config.example.json config.json
# Edit config.json with your vault path

# Run the server (for testing)
./target/release/surreal-obsidian-mcp --config config.json
```

## Installation

```bash
# Build from source
cargo build --release

# Copy binary to your PATH
cp target/release/surreal-obsidian-mcp ~/.local/bin/

# Or install with cargo
cargo install --path .
```

### Recommended: Local Setup with Ollama

For privacy and zero API costs, use Ollama for embeddings:

```bash
# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Pull an embedding model (768 dimensions, excellent quality)
ollama pull nomic-embed-text

# Build with embedded reranking support
cargo build --release --features embedded

# Use config.example.json as your starting point
cp config.example.json config.json
# Edit vault path in config.json
```

**Why this setup?**

- **Privacy**: All processing happens locally, nothing sent to external APIs
- **Cost**: Zero API costs, run as much as you want
- **Quality**: nomic-embed-text is excellent for semantic search
- **Speed**: Local inference is fast, embedded reranking improves accuracy

**Alternative**: For cloud-based setups, see `config.openai.json` or `config.together.json`.

### NixOS / Nix Flakes

For NixOS users, this project provides a NixOS module for declarative configuration:

**Using in your NixOS flake:**

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    surreal-obsidian-mcp.url = "github:jemilsson/surreal-obsidian-mcp";
  };

  outputs = { self, nixpkgs, surreal-obsidian-mcp }: {
    nixosConfigurations.yourhost = nixpkgs.lib.nixosSystem {
      modules = [
        surreal-obsidian-mcp.nixosModules.default
        {
          services.surreal-obsidian-mcp = {
            enable = true;
            vaultPath = "/home/user/Documents/ObsidianVault";
            embedding = {
              provider = "ollama";
              model = "nomic-embed-text";
              dimensions = 768;
              apiBase = "http://localhost:11434";
            };
            reranking.enable = true;
          };
        }
      ];
    };
  };
}
```

This automatically:

- Creates a systemd service
- Sets up proper permissions and security hardening
- Manages the database and model cache directories
- Generates the configuration file

See [nixos-example.nix](nixos-example.nix) for complete configuration examples.

**Using the package without the module:**

```nix
# In your home-manager or system packages
environment.systemPackages = [
  inputs.surreal-obsidian-mcp.packages.${system}.default
];
```

## Configuration

Add to your MCP client settings (e.g., Claude Desktop config):

```json
{
  "mcpServers": {
    "obsidian": {
      "command": "surreal-obsidian-mcp",
      "args": ["--config", "/path/to/config.json"]
    }
  }
}
```

Server configuration file (`config.json`):

```json
{
  "vault": {
    "path": "/path/to/your/obsidian/vault"
  },
  "database": {
    "path": "./obsidian.db"
  },
  "embedding": {
    "provider": "open-ai",
    "model": "text-embedding-3-small",
    "dimensions": 1536,
    "api_key": "sk-...",
    "api_base": "https://api.openai.com/v1"
  },
  "reranking": {
    "enabled": false
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

**Note**: Embeddings are now fully integrated! Configure your preferred embedding provider in `config.json`:

- **OpenAI** or **OpenAI-compatible APIs** (cloud-based, requires API key)
- **Ollama** (local, private, no API key required - recommended for privacy)

## MCP Tools Reference

Once running, AI assistants can use these tools:

### Read Operations

- `search_blocks(query: string, limit?: number)` - Search blocks by title or content (keyword search)
- `search_similar(query: string, limit?: number, expand?: number)` - Semantic similarity search using vector embeddings with optional graph expansion and reranking
- `get_block(id: string)` - Get a specific block by ID with full content and metadata
- `get_blocks_by_file(file_path: string)` - Get all blocks for a specific file path
- `get_all_files()` - Get all file blocks (level 0) in the vault
- `get_children(parent_id: string)` - Get child blocks of a specific block

### Write Operations

**LLMs can now communicate through your notes!**

- `create_block(file_path, title, content, level?, parent_id?)` - Create a new file or heading block
- `update_block(id, title?, content?)` - Update an existing block's title or content
- `delete_block(id)` - Delete a block (removes from database and updates/deletes the file)
- `append_to_block(id, content)` - Append content to an existing block

Write operations:

- Update the database first (source of truth)
- Reconstruct the markdown file from the block hierarchy
- Write changes to the filesystem
- Automatically regenerate embeddings for modified blocks

### Graph Traversal (NEW!)

**Navigate your knowledge graph through links and tags!**

- `get_linked_blocks(id: string)` - Get blocks that this block links to (outgoing wiki-links)
- `get_backlinks(id: string)` - Get blocks that link to this block (incoming links)
- `find_by_tag(tag: string, limit?: number)` - Find all blocks with a specific tag
- `find_connection_path(from_id, to_id, max_depth?: number)` - Find shortest path between two blocks via wiki-links (BFS search)

Graph tools enable:

- Discovering related notes through link relationships
- Finding all references to a concept (backlinks)
- Organizing notes by tags
- Understanding how ideas connect across your vault

## Using Semantic Search

The `search_similar` tool uses vector embeddings to find blocks that are semantically related to your query, even if they don't contain the exact keywords.

**Hybrid Semantic + Graph RAG:**

The `expand` parameter enables hybrid search that combines semantic similarity with graph traversal:

- `expand=0` (default): Pure semantic search - returns only vector similarity results
- `expand=1`: Includes direct neighbors (linked blocks, backlinks, parent, children)
- `expand=2`: Expands to neighbors of neighbors (2-hop connections)
- `expand=N`: Expands to N levels deep in the graph

**Benefits of graph expansion:**

- Query vector is cached - increasing depth doesn't re-embed the query
- Discover contextually related notes that aren't semantically similar
- Follow the knowledge graph structure around your core search results
- Combine semantic relevance with relational context

**Example use cases:**

- Find notes about "machine learning" when searching for "artificial intelligence"
- Discover related concepts across your knowledge base
- Find notes by describing what you're looking for, not just keywords
- Expand results to see the full context and related notes (`expand=1` or `expand=2`)

**How it works:**

1. During indexing, each block is converted to a vector embedding using your configured provider
2. When you search, your query is also converted to an embedding
3. SurrealDB performs cosine similarity search to find the most relevant blocks
4. Results are ranked by semantic relevance

**Configuration:**

```json
{
  "embedding": {
    "provider": "ollama",           // or "open-ai", "open-ai-compatible"
    "model": "nomic-embed-text",    // or "text-embedding-3-small" for OpenAI
    "dimensions": 768,               // 1536 for OpenAI models
    "api_base": "http://localhost:11434"  // Ollama default
  }
}
```

**For fully local/offline setup** (no API costs):

1. Install Ollama: `curl -fsSL https://ollama.com/install.sh | sh`
2. Pull an embedding model: `ollama pull nomic-embed-text`
3. Configure as shown above
4. All embeddings are generated locally!

## Local Reranking (Candle)

Reranking improves search quality by re-scoring the initial search results using a more accurate (but slower) cross-encoder model. This is optional but recommended for better results.

**What is reranking?**

- Initial search returns candidates (using fast vector similarity)
- Reranker scores each candidate more precisely (using a cross-encoder)
- Final results are sorted by reranker scores

**Configuration:**

```json
{
  "reranking": {
    "enabled": true,
    "provider": "embedded",
    "model": "cross-encoder/ms-marco-MiniLM-L-6-v2",
    "top_n": 10
  }
}
```

**Build with embedded reranking:**

```bash
# Build with the 'embedded' feature (includes Candle support)
cargo build --release --features embedded
```

**How it works:**

1. Search returns 3x more results than requested (e.g., 30 for `limit=10`)
2. Candle loads a cross-encoder model from HuggingFace
3. Each (query, document) pair is scored by the model
4. Results are re-sorted by score and top N are returned

**Recommended models:**

- `cross-encoder/ms-marco-MiniLM-L-6-v2` - Fast, good quality (default)
- `cross-encoder/ms-marco-MiniLM-L-12-v2` - Better quality, slower
- Any cross-encoder model from HuggingFace compatible with BERT architecture

**Benefits:**

- Fully local/offline (no API calls)
- Significantly improves search accuracy
- Works with any embedding provider
- Models are automatically downloaded and cached

## Development Environment

This project uses Nix for reproducible development environments:

- **Nix**: Package manager and build system
- **direnv**: Automatic environment loading when entering the directory
- **nix-flakes**: Modern Nix feature for dependency management

### Setup

```bash
# Install Nix with flakes support
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install

# Install direnv
# On NixOS: nix-env -iA nixpkgs.direnv
# On other systems: your package manager or https://direnv.net

# Clone the repository
git clone https://github.com/jemilsson/surreal-obsidian-mcp
cd surreal-obsidian-mcp

# Allow direnv (automatically loads Nix environment)
direnv allow

# Build and run
cargo build
cargo run -- --config config.json
```

The Nix environment provides:

- Rust toolchain
- Required system dependencies
- Development tools
- Consistent versions across machines

## Roadmap

**Completed**:

- [x] Project setup (Cargo + Nix flake)
- [x] MCP server scaffolding
- [x] SurrealDB embedded integration
- [x] Obsidian markdown parser (frontmatter, wiki-links, tags)
- [x] Block extraction system (files and headings)
- [x] File watcher for vault changes
- [x] Real-time vault synchronization
- [x] Basic MCP tools (search, get_block, get_blocks_by_file, get_all_files, get_children)
- [x] Graph extraction (links, backlinks, tags)
- [x] Test suite (16 tests passing)
- [x] Embedding generation integration (block-level)
  - [x] OpenAI provider
  - [x] OpenAI-compatible provider (Venice.ai, OpenRouter, etc.)
  - [x] Ollama provider (local, offline models)
- [x] Vector storage and similarity search
- [x] Semantic search MCP tool (search_similar)
- [x] Local reranking with Candle (cross-encoder models)
  - [x] Embedded reranker (Candle, fully local)
  - [x] Automatic model download from HuggingFace
  - [x] Integration with search_similar tool
- [x] Block write operations (CRUD)
  - [x] Create blocks (files and headings)
  - [x] Update block content and title
  - [x] Delete blocks (with file reconstruction)
  - [x] Append to blocks
  - [x] Database-first approach with markdown reconstruction
  - [x] Automatic embedding regeneration on changes
- [x] Graph traversal MCP tools
  - [x] Get linked blocks (outgoing wiki-links)
  - [x] Get backlinks (incoming links)
  - [x] Find blocks by tag
  - [x] Find connection paths between blocks (BFS)
  - [x] Hybrid semantic + graph search (expand parameter)

- [x] Documentation and examples
  - [x] INSTALL.md with platform-specific installation instructions
  - [x] USAGE.md with comprehensive usage guide
  - [x] EXAMPLES.md with concrete JSON examples for all tools
  - [x] Configuration templates (Ollama, OpenAI, Venice.ai, Together.ai)
  - [x] Common workflows and troubleshooting guide

- [x] Cross-platform binaries
  - [x] GitHub Actions CI/CD pipeline
  - [x] Linux x86_64 (glibc and musl)
  - [x] macOS x86_64 (Intel) and ARM64 (Apple Silicon)
  - [x] Windows x86_64
  - [x] Automated releases with checksums

- [x] NixOS integration
  - [x] Nix flake for development and packages
  - [x] NixOS module for declarative service configuration
  - [x] Systemd service with security hardening
  - [x] Example configurations

**Note on Local Embeddings**: Use Ollama for local, private embeddings without API dependencies. Ollama provides an easy-to-use local server with models like `nomic-embed-text`. The `embedded` feature flag is reserved for local cross-encoder reranking only.

## Contributing

Contributions are welcome! This is a fully functional project with room for improvements and new features.

**How to contribute**:

1. **Report Issues**: Found a bug? Open an issue with details and reproduction steps
2. **Suggest Features**: Have an idea? Open a discussion or issue to discuss it
3. **Submit PRs**: Fork, make changes, and submit a pull request
4. **Improve Docs**: Documentation improvements are always appreciated

**Development Setup**:

```bash
# Clone and build
git clone https://github.com/jemilsson/surreal-obsidian-mcp
cd surreal-obsidian-mcp
cargo build --features embedded

# Run tests
cargo test --all-features

# Check formatting and lints
cargo fmt --check
cargo clippy --all-features -- -D warnings
```

**Areas for Contribution**:

- Additional embedding providers
- More reranking model support
- Performance optimizations
- Additional MCP tools
- Documentation improvements
- Bug fixes

## Requirements

**Runtime**:

- Obsidian vault with markdown notes
- For embeddings, one of:
  - API key for cloud providers (OpenAI, Venice.ai, etc.)
  - Ollama installed locally (for free, offline local models)

**Development**:

- Nix with flakes enabled
- direnv (recommended)
- Rust 1.70+ (provided by Nix environment)

## License

This project is licensed under the **GNU Affero General Public License v3.0 or later (AGPL-3.0-or-later)**.

**What this means**:

- ✅ You can use this software freely for personal or commercial purposes
- ✅ You can modify and distribute it
- ✅ **But** any modified versions must also be open source under AGPL-3.0
- ✅ **Even if you run it as a service**, you must provide source code to users

This protects the open-source nature of the project and ensures improvements benefit everyone.

See [LICENSE](LICENSE) for the full license text.

## Acknowledgments

- [SurrealDB](https://surrealdb.com/) - Multi-model database
- [Obsidian](https://obsidian.md/) - Knowledge base application
- [Model Context Protocol](https://modelcontextprotocol.io/) - Protocol specification
- [Nix](https://nixos.org/) - Reproducible builds

---

**Status**: ✅ Production Ready - Full indexing, keyword search, semantic search (embeddings), graph extraction, and real-time file watching operational. Reranking and hybrid search coming next.

For questions or discussions, open an issue.
