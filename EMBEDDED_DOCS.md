# Embedded Documentation Tool

The VCF MCP Server now includes a `get_documentation` tool that provides access to all documentation embedded in the binary.

## Available Documentation

| Doc Type | Description | File |
|----------|-------------|------|
| `readme` | Main documentation and overview | README.md |
| `streaming` | Streaming query guide | STREAMING.md |
| `filters` | Filter syntax and examples | FILTER_EXAMPLES.md |
| `streaming-filters` | Streaming with filter examples | STREAMING_FILTER_EXAMPLES.md |
| `all` | Complete documentation (all above combined) | Combined |

## Usage

### Get Main Documentation

```javascript
const result = await get_documentation({
  doc_type: "readme"
});

console.log(result.content); // Full README.md as markdown string
// {
//   "doc_type": "readme",
//   "document_name": "README.md",
//   "content": "# VCF MCP Server...",
//   "format": "markdown"
// }
```

### Get Streaming Guide

```javascript
const result = await get_documentation({
  doc_type: "streaming"
});

console.log(result.content); // STREAMING.md content
// Learn about start_region_query, get_next_variant, etc.
```

### Get Filter Examples

```javascript
const result = await get_documentation({
  doc_type: "filters"
});

console.log(result.content); // FILTER_EXAMPLES.md content
// Learn filter syntax: QUAL > 30, FILTER == PASS, etc.
```

### Get Streaming + Filters Guide

```javascript
const result = await get_documentation({
  doc_type: "streaming-filters"
});

console.log(result.content); // STREAMING_FILTER_EXAMPLES.md content
// Learn how to use filters with streaming queries
```

### Get Complete Documentation

```javascript
const result = await get_documentation({
  doc_type: "all"
});

console.log(result.content); // All docs combined with separators
// {
//   "doc_type": "all",
//   "content": "# VCF MCP Server - Complete Documentation...",
//   "format": "markdown",
//   "sections": ["README.md", "STREAMING.md", "FILTER_EXAMPLES.md", "STREAMING_FILTER_EXAMPLES.md"]
// }
```

### Default Behavior

```javascript
// Omitting doc_type defaults to "readme"
const result = await get_documentation({});
// Same as doc_type: "readme"
```

## Claude Desktop Integration

Claude can now discover documentation on its own:

```
User: "How do I use streaming queries in this VCF server?"

Claude: Let me check the documentation...
[Calls get_documentation({doc_type: "streaming"})]

Claude: "Based on the documentation, here's how to use streaming queries:
1. Call start_region_query with chromosome, start, and end...
2. Use get_next_variant to retrieve subsequent variants...
..."
```

## Benefits

1. **Self-Documenting** - Server carries its own documentation
2. **Version-Synced** - Docs always match the binary version
3. **Offline Access** - No external files or network needed
4. **Zero Runtime Cost** - Docs embedded at compile time (no I/O)
5. **LLM-Accessible** - Claude can query docs via MCP tool
6. **Minimal Size Impact** - Adds ~50KB to 8.1MB binary (~0.6%)

## Error Handling

```javascript
try {
  const result = await get_documentation({
    doc_type: "invalid-type"
  });
} catch (error) {
  console.error(error);
  // Error: Unknown doc_type 'invalid-type'. 
  // Available: readme, streaming, filters, streaming-filters, all
}
```

## Implementation Details

### Compile-Time Embedding

Documentation is embedded using Rust's `include_str!` macro:

```rust
const README_DOCS: &str = include_str!("../README.md");
const STREAMING_DOCS: &str = include_str!("../STREAMING.md");
const FILTER_DOCS: &str = include_str!("../FILTER_EXAMPLES.md");
const STREAMING_FILTER_DOCS: &str = include_str!("../STREAMING_FILTER_EXAMPLES.md");
```

This happens at **compile time**, so:
- ✅ No file I/O at runtime
- ✅ Docs stored in binary's `.rodata` section
- ✅ Always available, even if original .md files deleted
- ✅ No runtime performance penalty

### Response Format

All responses include:

```json
{
  "doc_type": "streaming",           // Requested type
  "document_name": "STREAMING.md",   // Source filename (except "all")
  "content": "...",                   // Full markdown content
  "format": "markdown"                // Content format
}
```

The `all` type also includes:

```json
{
  "sections": ["README.md", "STREAMING.md", "FILTER_EXAMPLES.md", "STREAMING_FILTER_EXAMPLES.md"]
}
```

## Use Cases

### 1. LLM Self-Help

```javascript
// Claude encounters an error about filters
const filterDocs = await get_documentation({doc_type: "filters"});
// Claude reads the docs and corrects its approach
```

### 2. Interactive Tutorial

```javascript
// User asks for help
const guide = await get_documentation({doc_type: "streaming"});
// Display guide to user
```

### 3. API Discovery

```javascript
// New user connects to server
const readme = await get_documentation({doc_type: "readme"});
// Learn about all available tools and features
```

### 4. Version Verification

```javascript
// Check what features this version supports
const all = await get_documentation({doc_type: "all"});
// Search for specific feature mentions
```

## Comparison with Resources

### `get_documentation` Tool
- ✅ Embedded in binary
- ✅ Multiple doc sections
- ✅ Returns as tool response
- ✅ Works without file system access

### `vcf://metadata` Resource  
- ✅ VCF-specific metadata
- ✅ Header information
- ✅ Contig/sample data
- ✅ Runtime-generated from VCF file

Both approaches serve different purposes and complement each other.

## File Sizes

```bash
# Check documentation file sizes
$ ls -lh *.md
-rw-r--r-- README.md                     ~8 KB
-rw-r--r-- STREAMING.md                  ~12 KB
-rw-r--r-- FILTER_EXAMPLES.md            ~15 KB
-rw-r--r-- STREAMING_FILTER_EXAMPLES.md  ~18 KB
Total: ~53 KB embedded in 8.1 MB binary (~0.6% increase)
```

## Future Enhancements

Possible additions:
- Search within documentation (e.g., `search: "chromosome normalization"`)
- Document sections/table of contents
- HTML-formatted output option
- Changelog/version history
- Code examples extraction
