# Usage Guide

This guide covers common usage patterns and examples for the Surreal Obsidian MCP server.

## Table of Contents

- [Configuration](#configuration)
- [MCP Tools Overview](#mcp-tools-overview)
- [Common Workflows](#common-workflows)
- [Example Queries](#example-queries)
- [Troubleshooting](#troubleshooting)

## Configuration

### Local Setup (Recommended)

For privacy and no API costs, use Ollama for embeddings:

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
  }
}
```

**Setup steps**:
1. Install Ollama: `curl -fsSL https://ollama.com/install.sh | sh`
2. Pull the embedding model: `ollama pull nomic-embed-text`
3. Build with reranking support: `cargo build --release --features embedded`

### OpenAI Setup

For cloud-based embeddings with OpenAI:

```json
{
  "embedding": {
    "provider": "open-ai",
    "model": "text-embedding-3-small",
    "dimensions": 1536,
    "api_key": "sk-your-api-key-here",
    "api_base": "https://api.openai.com/v1"
  },
  "reranking": {
    "enabled": false
  }
}
```

### Venice.ai Setup

For privacy-focused cloud embeddings with Venice.ai:

```json
{
  "embedding": {
    "provider": "open-ai-compatible",
    "model": "text-embedding-bge-m3",
    "dimensions": 1024,
    "api_key": "your-venice-api-key",
    "api_base": "https://api.venice.ai/api/v1"
  },
  "reranking": {
    "enabled": true,
    "provider": "embedded",
    "model": "BAAI/bge-reranker-base",
    "model_cache": "./model_cache",
    "top_n": 20
  }
}
```

**Benefits**:

- Privacy-focused (no data retention)
- Uncensored models
- OpenAI-compatible API
- Combine with local reranking for best of both worlds

### OpenAI-Compatible Setup

For services like Together.ai, Fireworks, or local inference servers:

```json
{
  "embedding": {
    "provider": "open-ai-compatible",
    "model": "BAAI/bge-base-en-v1.5",
    "dimensions": 768,
    "api_key": "your-api-key",
    "api_base": "https://api.together.xyz/v1"
  }
}
```

## MCP Tools Overview

### Read Operations

| Tool | Purpose | Example Use Case |
|------|---------|------------------|
| `search_similar` | Semantic search with optional graph expansion | "Find notes about machine learning" |
| `get_block` | Retrieve specific block by ID | Get exact note content |
| `list_files` | List all markdown files in vault | Browse vault structure |
| `find_path` | Find shortest path between blocks | "How are 'Python' and 'Django' connected?" |
| `get_tags` | List all unique tags | Explore tag taxonomy |
| `get_blocks_by_tag` | Get blocks with specific tag | "Show all #meeting notes" |

### Write Operations

| Tool | Purpose | Example Use Case |
|------|---------|------------------|
| `create_block` | Create new heading or file | LLM creates summary note |
| `update_block` | Update title or content | LLM edits existing note |
| `delete_block` | Remove block | Clean up old content |
| `append_to_block` | Add content to end | LLM adds daily journal entry |

### Graph Operations

| Tool | Purpose | Example Use Case |
|------|---------|------------------|
| `get_linked_blocks` | Get outgoing wiki-links | "What does this note reference?" |
| `get_backlinks` | Get incoming links | "What links to this note?" |
| `find_path` | BFS shortest path | Graph navigation |
| `get_blocks_by_tag` | Tag-based retrieval | Topic clustering |

## Common Workflows

### 1. Semantic Search with Graph Expansion

**Use case**: Find relevant notes and their immediate context.

```json
{
  "tool": "search_similar",
  "arguments": {
    "query": "machine learning deployment strategies",
    "limit": 5,
    "expand": 1
  }
}
```

**What happens**:
1. Finds 5 most semantically similar blocks
2. Expands each result to include:
   - Blocks it links to (wiki-links)
   - Blocks that link to it (backlinks)
   - Parent block (if it's a heading)
   - Child blocks (if it has sub-headings)
3. Returns results organized by "Core Results" and "Related via Graph Expansion"

**Benefits**:
- Get not just the answer, but surrounding context
- Discover related notes through the knowledge graph
- Cache the query vector - increment `expand` to explore deeper without re-embedding

### 2. LLM Communication via Notes

**Use case**: Let the LLM create notes to communicate insights.

**Example flow**:
1. User asks: "Summarize my meeting notes from this week and create a summary"
2. LLM searches: `search_similar` with query "meeting notes"
3. LLM filters by tag: `get_blocks_by_tag` with tag "meeting"
4. LLM analyzes content
5. LLM creates summary: `create_block` with title "Weekly Meeting Summary"

```json
{
  "tool": "create_block",
  "arguments": {
    "file_path": "Summaries/Weekly Meeting Summary 2024-W12.md",
    "title": "Weekly Meeting Summary",
    "content": "## Key Decisions\n- Decided to use Rust for backend\n\n## Action Items\n- [[John]] to research database options\n- [[Sarah]] to draft API specs",
    "level": 0
  }
}
```

### 3. Knowledge Graph Navigation

**Use case**: Discover connections between concepts.

**Example**:
```json
{
  "tool": "find_path",
  "arguments": {
    "from_id": "block:abc123",
    "to_id": "block:def456",
    "max_depth": 3
  }
}
```

**Returns**:
```json
{
  "path": [
    {"id": "block:abc123", "title": "Python"},
    {"id": "block:xyz789", "title": "Web Frameworks"},
    {"id": "block:def456", "title": "Django"}
  ]
}
```

### 4. Tag-Based Organization

**Use case**: Explore notes by topic.

**List all tags**:
```json
{"tool": "get_tags"}
```

**Get notes with specific tag**:
```json
{
  "tool": "get_blocks_by_tag",
  "arguments": {
    "tag": "project/website",
    "limit": 20
  }
}
```

### 5. Incremental Graph Exploration

**Use case**: Start narrow, expand gradually using cached embeddings.

**Step 1** - Find core results:
```json
{
  "tool": "search_similar",
  "arguments": {
    "query": "rust async programming",
    "limit": 3,
    "expand": 0
  }
}
```

**Step 2** - Expand one level (uses cached query vector):
```json
{
  "tool": "search_similar",
  "arguments": {
    "query": "rust async programming",
    "limit": 3,
    "expand": 1
  }
}
```

**Step 3** - Expand further if needed:
```json
{
  "tool": "search_similar",
  "arguments": {
    "query": "rust async programming",
    "limit": 3,
    "expand": 2
  }
}
```

## Example Queries

### Research Assistant

**User**: "Find everything about neural networks and create a summary note"

**LLM Actions**:
1. `search_similar` with query "neural networks", limit 10, expand 1
2. Analyze results and graph connections
3. `create_block` to create summary note with wiki-links to source notes

### Daily Journaling

**User**: "Add today's accomplishments to my journal"

**LLM Actions**:
1. `list_files` filtered by path "Journal/"
2. Find today's journal file
3. `get_block` to retrieve current content
4. `append_to_block` to add new entry

### Knowledge Graph Exploration

**User**: "How are my Rust notes connected to my Web Development notes?"

**LLM Actions**:
1. `search_similar` for "Rust programming" to find Rust note ID
2. `search_similar` for "Web Development" to find web dev note ID
3. `find_path` between the two notes
4. Explain the connection path

### Content Organization

**User**: "Show me all my unfinished project notes"

**LLM Actions**:
1. `get_tags` to list all tags
2. `get_blocks_by_tag` with tag "status/todo" or "project"
3. Filter and present results

## Troubleshooting

### "Embedding API error"

**Cause**: Embedding service not running or incorrect configuration.

**Solutions**:
- **Ollama**: Check if running with `ollama list`, start with `ollama serve`
- **OpenAI**: Verify API key is valid
- **Check API base URL**: Ensure `api_base` is correct

### "Failed to parse embedding response"

**Cause**: Model doesn't support the dimensions you specified.

**Solution**:
- For Ollama models, check supported dimensions in model card
- For OpenAI, text-embedding-3-small supports 1536 dimensions
- Update `dimensions` in config to match model output

### "Vault path does not exist"

**Cause**: Invalid vault path in configuration.

**Solution**: Use absolute path to your Obsidian vault:
```json
{
  "vault": {
    "path": "/home/user/Documents/ObsidianVault"
  }
}
```

### Reranking not improving results

**Possible causes**:
1. Not enough initial results (increase search limit)
2. Model not downloaded (check `model_cache` directory)
3. Feature not enabled (rebuild with `--features embedded`)

**Solutions**:
```json
{
  "reranking": {
    "enabled": true,
    "provider": "embedded",
    "model": "BAAI/bge-reranker-base",
    "model_cache": "./model_cache",
    "top_n": 20
  }
}
```

Build with: `cargo build --release --features embedded`

### Graph expansion returning too many results

**Solution**: Use smaller expand depth
- `expand: 0` - No expansion (semantic only)
- `expand: 1` - Direct neighbors only (recommended)
- `expand: 2` - Neighbors of neighbors (can be large)

### Performance issues with large vaults

**Solutions**:
1. Increase batch size for initial indexing:
   ```json
   {"sync": {"batch_size": 200}}
   ```
2. Disable file watching if not needed:
   ```json
   {"sync": {"watch_for_changes": false}}
   ```
3. Use database on SSD
4. Consider increasing `RUST_LOG=warn` to reduce logging overhead

## Best Practices

### 1. Start with Local Setup

Use Ollama for privacy and cost-free operation:
- No API costs
- Private - data never leaves your machine
- Fast for smaller models
- Easy to experiment

### 2. Use Graph Expansion Wisely

- Start with `expand: 0` for pure semantic search
- Use `expand: 1` for most queries (direct context)
- Only use `expand: 2+` when you need deep exploration
- Remember: Same query with different expand depths uses cached vector

### 3. Leverage Reranking

Enable reranking for better accuracy:
- Especially useful when initial results are noisy
- Cross-encoder reranking is more accurate than embeddings alone
- Set `top_n` based on your needs (10-20 is usually good)

### 4. Let the LLM Create Structure

Use write operations to let the LLM help organize:
- Create summary notes
- Generate index pages with [[wiki-links]]
- Add tags automatically
- Maintain MOCs (Maps of Content)

### 5. Tag Hierarchies

Use hierarchical tags for better organization:
- `#project/website`
- `#project/app`
- `#status/todo`
- `#status/done`

The server preserves the full tag including slashes.

## Integration Examples

### Claude Desktop Integration

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "obsidian": {
      "command": "/path/to/surreal-obsidian-mcp",
      "args": ["--config", "/path/to/config.json"]
    }
  }
}
```

### Using with MCP Inspector

For testing and development:

```bash
npx @modelcontextprotocol/inspector /path/to/surreal-obsidian-mcp --config config.json
```

## Performance Tips

1. **Initial Indexing**: Can take time on large vaults. Watch logs with `RUST_LOG=info`
2. **Database Location**: Put on SSD for faster queries
3. **Batch Size**: Tune based on vault size and memory
4. **Reranking**: Adds latency but improves accuracy - enable for quality, disable for speed
5. **Graph Expansion**: Depth 0-1 is fast, depth 2+ can be slow on highly connected notes
