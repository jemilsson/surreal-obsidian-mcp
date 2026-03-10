# Examples

This document provides concrete examples of using the Surreal Obsidian MCP server tools.

## Table of Contents

- [Read Operations](#read-operations)
- [Write Operations](#write-operations)
- [Graph Operations](#graph-operations)
- [Advanced Workflows](#advanced-workflows)

## Read Operations

### Basic Semantic Search

Find notes about a topic:

```json
{
  "tool": "search_similar",
  "arguments": {
    "query": "machine learning deployment strategies",
    "limit": 5,
    "expand": 0
  }
}
```

**Response**:
```json
{
  "results": [
    {
      "id": "block:abc123",
      "title": "ML Deployment Guide",
      "content": "Discusses various strategies for deploying ML models...",
      "file_path": "Tech/ML/Deployment.md",
      "score": 0.92
    },
    ...
  ]
}
```

### Semantic Search with Graph Expansion

Get semantically similar notes plus their connected notes:

```json
{
  "tool": "search_similar",
  "arguments": {
    "query": "async programming patterns",
    "limit": 3,
    "expand": 1
  }
}
```

**Response includes**:
- Core results (3 most similar blocks)
- Related blocks (linked to/from core results, parents, children)

### Get Specific Block

Retrieve a block by its ID:

```json
{
  "tool": "get_block",
  "arguments": {
    "id": "block:abc123"
  }
}
```

**Response**:
```json
{
  "id": "block:abc123",
  "title": "Overview",
  "content": "This is the overview section...",
  "file_path": "Projects/Website.md",
  "level": 2,
  "parent_id": "block:xyz789",
  "tags": ["project", "web"],
  "links": ["[[React]]", "[[TypeScript]]"],
  "created_at": "2024-01-15T10:30:00Z",
  "updated_at": "2024-01-16T14:20:00Z"
}
```

### List Files in Vault

Get all markdown files:

```json
{
  "tool": "list_files",
  "arguments": {}
}
```

Filter by path:

```json
{
  "tool": "list_files",
  "arguments": {
    "path_filter": "Projects/"
  }
}
```

### Get All Tags

List all unique tags in the vault:

```json
{
  "tool": "get_tags",
  "arguments": {}
}
```

**Response**:
```json
{
  "tags": [
    "project",
    "meeting",
    "idea",
    "status/todo",
    "status/done",
    "tech/rust",
    "tech/python"
  ]
}
```

### Get Blocks by Tag

Find all blocks with a specific tag:

```json
{
  "tool": "get_blocks_by_tag",
  "arguments": {
    "tag": "meeting",
    "limit": 10
  }
}
```

## Write Operations

### Create a New Note (File)

Create a new markdown file (level 0 block):

```json
{
  "tool": "create_block",
  "arguments": {
    "file_path": "Meetings/2024-03-15-Standup.md",
    "title": "Standup 2024-03-15",
    "content": "## Attendees\n- Alice\n- Bob\n\n## Topics\n- Sprint planning\n- Bug review",
    "level": 0
  }
}
```

**Response**:
```json
{
  "id": "block:new123",
  "file_path": "Meetings/2024-03-15-Standup.md",
  "message": "Block created successfully"
}
```

### Create a Section (Heading)

Add a new section to an existing note:

```json
{
  "tool": "create_block",
  "arguments": {
    "parent_id": "block:abc123",
    "title": "Next Steps",
    "content": "- [ ] Research deployment options\n- [ ] Create prototype\n- [ ] Schedule review meeting",
    "level": 2
  }
}
```

Creates a `## Next Steps` section in the parent note.

### Update Block Content

Modify existing block:

```json
{
  "tool": "update_block",
  "arguments": {
    "id": "block:abc123",
    "content": "Updated content goes here...\n\nWith multiple paragraphs."
  }
}
```

Update just the title:

```json
{
  "tool": "update_block",
  "arguments": {
    "id": "block:abc123",
    "title": "New Title"
  }
}
```

### Append Content to Block

Add content to the end of a block without replacing:

```json
{
  "tool": "append_to_block",
  "arguments": {
    "id": "block:abc123",
    "content": "\n\n## Update 2024-03-15\n\nAdded new findings from testing..."
  }
}
```

### Delete a Block

Remove a block and optionally its children:

```json
{
  "tool": "delete_block",
  "arguments": {
    "id": "block:abc123"
  }
}
```

**Note**: Deleting a file block (level 0) deletes the entire file and all its sections.

## Graph Operations

### Get Linked Blocks (Outgoing Links)

Find blocks that this block links to via wiki-links:

```json
{
  "tool": "get_linked_blocks",
  "arguments": {
    "block_id": "block:abc123"
  }
}
```

If the block contains `[[Python]]` and `[[Django]]`, this returns those blocks.

### Get Backlinks (Incoming Links)

Find blocks that link to this block:

```json
{
  "tool": "get_backlinks",
  "arguments": {
    "block_id": "block:abc123"
  }
}
```

Shows all blocks that mention this block with `[[...]]` syntax.

### Find Shortest Path Between Blocks

Discover how two concepts are connected:

```json
{
  "tool": "find_path",
  "arguments": {
    "from_id": "block:abc123",
    "to_id": "block:xyz789",
    "max_depth": 4
  }
}
```

**Response**:
```json
{
  "path": [
    {
      "id": "block:abc123",
      "title": "Rust Programming",
      "file_path": "Tech/Rust.md"
    },
    {
      "id": "block:mid456",
      "title": "Web Frameworks",
      "file_path": "Tech/Web.md"
    },
    {
      "id": "block:xyz789",
      "title": "Actix Web",
      "file_path": "Tech/Actix.md"
    }
  ],
  "depth": 2
}
```

## Advanced Workflows

### Research Assistant Pattern

**Scenario**: User asks "Research neural networks and create a summary note"

**Step 1** - Search existing knowledge:
```json
{
  "tool": "search_similar",
  "arguments": {
    "query": "neural networks deep learning architecture",
    "limit": 10,
    "expand": 1
  }
}
```

**Step 2** - Analyze results (done by LLM)

**Step 3** - Create summary note with findings:
```json
{
  "tool": "create_block",
  "arguments": {
    "file_path": "Summaries/Neural Networks Overview.md",
    "title": "Neural Networks Overview",
    "content": "## Summary\n\nBased on existing notes, neural networks are...\n\n## Key Concepts\n- [[Backpropagation]]\n- [[Activation Functions]]\n- [[Gradient Descent]]\n\n## Related Topics\n- [[Machine Learning]]\n- [[Deep Learning]]\n\n## References\nSee: [[Tech/ML/Intro.md]], [[Papers/CNN.md]]",
    "level": 0
  }
}
```

### Daily Journal Entry

**Scenario**: LLM adds daily accomplishments to journal

**Step 1** - Find today's journal:
```json
{
  "tool": "list_files",
  "arguments": {
    "path_filter": "Journal/2024/03/"
  }
}
```

**Step 2** - Get current content:
```json
{
  "tool": "get_block",
  "arguments": {
    "id": "block:journal_today"
  }
}
```

**Step 3** - Append new entry:
```json
{
  "tool": "append_to_block",
  "arguments": {
    "id": "block:journal_today",
    "content": "\n\n## Accomplishments\n\n- Completed [[Project X]] milestone\n- Reviewed pull requests for [[Team/Backend]]\n- Research on [[Neural Networks]]\n\n## Tomorrow\n\n- [ ] Start implementation of [[Feature Y]]\n- [ ] Meeting with [[Alice]] re: deployment"
  }
}
```

### Knowledge Graph Explorer

**Scenario**: Understand how "Python" connects to "Web Development"

**Step 1** - Find Python note:
```json
{
  "tool": "search_similar",
  "arguments": {
    "query": "Python programming language",
    "limit": 1,
    "expand": 0
  }
}
```

**Step 2** - Find Web Development note:
```json
{
  "tool": "search_similar",
  "arguments": {
    "query": "Web Development",
    "limit": 1,
    "expand": 0
  }
}
```

**Step 3** - Find connection path:
```json
{
  "tool": "find_path",
  "arguments": {
    "from_id": "block:python123",
    "to_id": "block:webdev456",
    "max_depth": 3
  }
}
```

**Step 4** - Explore intermediate nodes:
```json
{
  "tool": "get_linked_blocks",
  "arguments": {
    "block_id": "block:frameworks789"
  }
}
```

### Meeting Notes Organizer

**Scenario**: LLM organizes scattered meeting notes by topic

**Step 1** - Find all meeting notes:
```json
{
  "tool": "get_blocks_by_tag",
  "arguments": {
    "tag": "meeting",
    "limit": 100
  }
}
```

**Step 2** - Create topic-based index:
```json
{
  "tool": "create_block",
  "arguments": {
    "file_path": "Meetings/Index by Topic.md",
    "title": "Meeting Index by Topic",
    "content": "## Product Planning\n- [[Meetings/2024-03-01-Product.md]]\n- [[Meetings/2024-03-08-Product.md]]\n\n## Engineering\n- [[Meetings/2024-03-05-Backend.md]]\n- [[Meetings/2024-03-12-Architecture.md]]\n\n## Standups\n- [[Meetings/2024-03-15-Standup.md]]",
    "level": 0
  }
}
```

### Incremental Context Building

**Scenario**: Start with core results, expand as needed

**Query 1** - Get core semantic matches:
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

**Query 2** - Same query, expand to neighbors (uses cached embedding):
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

**Query 3** - Expand further if needed:
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

Benefits:
- Don't pay for re-embedding the same query
- Start narrow, expand only as needed
- Control context window size dynamically

### Literature Review Assistant

**Scenario**: LLM helps organize research papers

**Step 1** - Find all paper notes:
```json
{
  "tool": "get_blocks_by_tag",
  "arguments": {
    "tag": "paper",
    "limit": 50
  }
}
```

**Step 2** - For each topic cluster, create summary:
```json
{
  "tool": "create_block",
  "arguments": {
    "file_path": "Research/Computer Vision Papers.md",
    "title": "Computer Vision Papers",
    "content": "## Overview\n\nSummary of key papers in computer vision.\n\n## Foundational\n- [[Papers/AlexNet.md]] - Deep CNN for ImageNet\n- [[Papers/ResNet.md]] - Residual networks\n\n## Recent Advances\n- [[Papers/Vision Transformers.md]] - Attention for vision\n- [[Papers/CLIP.md]] - Vision-language models\n\n## Common Themes\n- Increasing model scale\n- Self-supervised learning\n- Multi-modal approaches",
    "level": 0
  }
}
```

**Step 3** - Link papers bidirectionally:
```json
{
  "tool": "update_block",
  "arguments": {
    "id": "block:alexnet_paper",
    "content": "Original content...\n\n## Related Papers\n- [[ResNet]] - Builds on this work\n- [[Computer Vision Papers]] - Overview"
  }
}
```

### Project Status Tracker

**Scenario**: LLM maintains project status across notes

**Step 1** - Find project notes:
```json
{
  "tool": "search_similar",
  "arguments": {
    "query": "Website Redesign Project",
    "limit": 5,
    "expand": 1
  }
}
```

**Step 2** - Create/update status section:
```json
{
  "tool": "create_block",
  "arguments": {
    "parent_id": "block:project_main",
    "title": "Status - 2024-03-15",
    "content": "## Progress\n- ✅ Design mockups completed\n- ✅ Frontend framework selected ([[React]])\n- 🔄 Backend API in progress (70%)\n- ⏳ Database migration pending\n\n## Blockers\n- Waiting on [[DevOps]] for staging environment\n- Performance testing needs [[QA]] resources\n\n## Next Week\n- Complete API endpoints\n- Begin integration testing\n- Schedule deployment review",
    "level": 2
  }
}
```

## Tips and Best Practices

### Efficient Search

1. **Start with `expand: 0`** for pure semantic search
2. **Use `expand: 1`** when you need immediate context
3. **Only use `expand: 2+`** for deep exploration
4. **Same query, different expand** = cached embedding, fast

### Writing Patterns

1. **Use wiki-links liberally** in created content - builds the graph
2. **Add tags** to categorize notes
3. **Create MOCs (Maps of Content)** to organize topics
4. **Use headings** for structure - each becomes a searchable block

### Graph Navigation

1. **`find_path`** to discover connections
2. **`get_backlinks`** to see what references a concept
3. **`get_linked_blocks`** to see what a concept references
4. **Combine with search** for hybrid semantic + graph queries

### Batch Operations

When creating multiple related blocks:

```json
// Create parent note
{"tool": "create_block", "arguments": {...}}

// Create child sections
{"tool": "create_block", "arguments": {"parent_id": "...", ...}}
{"tool": "create_block", "arguments": {"parent_id": "...", ...}}
```

The server handles maintaining the markdown structure automatically.
