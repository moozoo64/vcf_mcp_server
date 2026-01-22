# Streaming Query API

The VCF MCP Server now supports **stateful streaming queries** that return variants one at a time, ideal for large genomic regions or memory-constrained environments.

## Why Streaming?

Traditional `query_by_region` returns all variants at once, which can be problematic for:
- Large genomic regions (e.g., entire chromosomes)
- Memory-constrained environments
- Interactive LLM workflows where you want to process results incrementally

Streaming queries solve this by maintaining server-side session state and returning **one variant per call**.

## How It Works

### 1. Start a Query Session

```javascript
// LLM calls: start_region_query
{
  "chromosome": "20",
  "start": 60000,
  "end": 70000,
  "filter": "QUAL > 30 AND FILTER == PASS"  // Optional filter
}

// Response:
{
  "variant": {
    "chromosome": "20",
    "position": 60001,
    "id": "rs123",
    "ref": "A",
    "alt": ["G"],
    // ... full variant data
  },
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "has_more": true,
  "reference_genome": "GRCh38",
  "matched_chromosome": "20"
}
```

**Note**: The `filter` parameter is optional. If omitted or empty, all variants are returned.

### 2. Get Next Variants

```javascript
// LLM calls: get_next_variant
{
  "session_id": "550e8400-e29b-41d4-a716-446655440000"
}

// Response:
{
  "variant": {
    "chromosome": "20",
    "position": 60150,
    "id": "rs456",
    // ...
  },
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "has_more": true,
  "reference_genome": "GRCh38",
  "matched_chromosome": "20"
}
```

### 3. End of Results

```javascript
// LLM calls: get_next_variant (when no more variants)
{
  "session_id": "550e8400-e29b-41d4-a716-446655440000"
}

// Response:
{
  "variant": null,
  "session_id": null,
  "has_more": false,
  "reference_genome": "GRCh38",
  "matched_chromosome": "20"
}
```

## Available Tools

### `start_region_query`

Start a new streaming query session for a genomic region.

**Parameters:**
- `chromosome` (string): Chromosome name (e.g., "1", "chr1", "X")
- `start` (u64): Start position (1-based, inclusive)
- `end` (u64): End position (1-based, inclusive)
- `filter` (string, optional): Filter expression (e.g., "QUAL > 30 AND FILTER == PASS"). Empty/omitted = no filtering. See [FILTER_EXAMPLES.md](FILTER_EXAMPLES.md) for syntax.

**Returns:**
- `variant`: First variant in region matching filter (or null if none found)
- `session_id`: UUID for subsequent calls (or null if no variants)
- `has_more`: Whether more variants exist
- `reference_genome`: Genome build (GRCh37/GRCh38/etc.)
- `matched_chromosome`: Actual chromosome name used

**Errors:**
- Chromosome not found → suggests alternate names (chr1 ↔ 1)
- No variants match filter → descriptive error message

### `get_next_variant`

Get the next variant from an active session.

**Parameters:**
- `session_id` (string): Session ID from `start_region_query` or previous `get_next_variant`

**Returns:**
- `variant`: Next variant (or null if exhausted)
- `session_id`: Same ID (or null if exhausted)
- `has_more`: Whether more variants exist
- `reference_genome`: Genome build
- `matched_chromosome`: Chromosome name

**Errors:**
- Session not found → start new query
- Session expired (>5 minutes) → start new query

### `close_query_session`

Explicitly close a session and free resources.

**Parameters:**
- `session_id` (string): Session ID to close

**Returns:**
- `closed` (bool): Whether session existed
- `message` (string): Status message

**Note:** Sessions are automatically closed when:
- All variants are retrieved (`has_more: false`)
- Session expires (5 minutes of inactivity)
- Server restarts

## Usage Examples

### Example 1: LLM Processing Variants Incrementally

```javascript
// LLM starts query
const init = await start_region_query({
  chromosome: "chr1",
  start: 1000000,
  end: 2000000
});

// Process first variant
if (init.variant) {
  processVariant(init.variant);
}

// Get remaining variants one by one
let session_id = init.session_id;
while (session_id) {
  const next = await get_next_variant({ session_id });
  
  if (next.variant) {
    processVariant(next.variant);
  }
  
  // Update session_id (becomes null when done)
  session_id = next.session_id;
}
```

### Example 2: LLM Finding First N Pathogenic Variants

```javascript
const session = await start_region_query({
  chromosome: "17",
  start: 43044295,  // BRCA1 region
  end: 43125483
});

const pathogenic = [];
let current = session;

while (current.session_id && pathogenic.length < 5) {
  if (current.variant && isPathogenic(current.variant)) {
    pathogenic.push(current.variant);
  }
  
  if (current.has_more) {
    current = await get_next_variant({ session_id: current.session_id });
  } else {
    break;
  }
}

// LLM can stop early without fetching all variants
if (current.session_id) {
  await close_query_session({ session_id: current.session_id });
}
```

### Example 3: Handling Chromosome Name Mismatches

```javascript
// Try with "chr1"
const result = await start_region_query({
  chromosome: "chr1",
  start: 1000,
  end: 2000
});
// Error: "Chromosome 'chr1' not found. Try '1'?"

// Retry with suggestion
const retry = await start_region_query({
  chromosome: "1",  // Use suggested name
  start: 1000,
  end: 2000
});
// Success!
```

## Session Management

### Session State

Each session stores:
- Chromosome name (normalized)
- Start/end positions
- Last returned position (for resumption)
- Creation timestamp

### Session Lifecycle

1. **Created**: `start_region_query` generates UUID, returns first variant
2. **Active**: `get_next_variant` retrieves subsequent variants
3. **Completed**: When `has_more: false`, session auto-closes
4. **Expired**: After 5 minutes, session auto-closes
5. **Closed**: Explicitly via `close_query_session`

### Memory Efficiency

- **O(1) memory** per session (only metadata, no cached variants)
- **O(k) memory** total where k = active sessions
- Each variant retrieved fresh from VCF file via tabix index

## Performance Characteristics

| Operation | Time Complexity | Notes |
|-----------|----------------|-------|
| `start_region_query` | O(log n) | Tabix index lookup |
| `get_next_variant` | O(log n) | Tabix query from last position |
| `close_query_session` | O(1) | HashMap removal |

**Best for:**
- Regions with unknown variant counts
- Scenarios where you may stop early (first N variants)
- Memory-constrained environments

**Not ideal for:**
- Small regions (<100 variants) → use `query_by_region`
- Batch processing where you need all variants → use `query_by_region`

## Comparison: Streaming vs Batch Queries

| Feature | `query_by_region` | Streaming (`start_region_query` + `get_next_variant`) |
|---------|-------------------|-------------------------------------------------------|
| Return type | All variants at once | One variant per call |
| Memory usage | O(n) variants | O(1) per session |
| Total API calls | 1 | k + 1 (k = variant count) |
| Can stop early | No | Yes |
| Session management | Stateless | Stateful (5 min timeout) |
| Best for | Small regions | Large regions, incremental processing |

## Claude Desktop Integration

```jsonc
// claude_desktop_config.json
{
  "mcpServers": {
    "vcf": {
      "command": "/path/to/vcf_mcp_server",
      "args": ["/path/to/sample.vcf.gz"]
    }
  }
}
```

Claude can now:
- Call `start_region_query` to begin streaming
- Iteratively call `get_next_variant` until exhausted
- Process variants incrementally in conversation
- Stop early if criteria met (e.g., "find first 3 pathogenic variants")

## Implementation Details

### Session Storage

Sessions stored in `Arc<Mutex<HashMap<String, QuerySession>>>`:
- Thread-safe across async handlers
- Survives between MCP tool calls
- Cleared on server restart

### Position Tracking

- `last_position` tracks last returned variant position
- Next query starts from `last_position + 1`
- Handles multi-allelic variants (multiple ALT alleles at same position)

### Error Handling

- **Chromosome not found**: Returns suggestions (chr1 ↔ 1)
- **Session not found**: Prompt to start new query
- **Session expired**: Auto-remove after 5 minutes
- **No variants**: Returns `variant: null` immediately

## Limitations

1. **Server restart** clears all sessions (not persisted to disk)
2. **5-minute timeout** for inactive sessions
3. **No backward iteration** (can't go to previous variants)
4. **No random access** (can't jump to arbitrary positions within session)

For these use cases, use the batch `query_by_region` tool instead.

## Security Considerations

- **Session IDs are UUIDs** (v4, cryptographically random)
- **No session sharing** between MCP clients (each gets unique ID)
- **Auto-expiry** prevents indefinite resource consumption
- **Read-only access** to VCF files (no writes)

## Future Enhancements

Possible additions (not yet implemented):
- Pagination with configurable page size
- Cursor-based navigation (forward/backward)
- Session persistence across server restarts
- Session usage metrics (variants retrieved, time active)
- Concurrent session limits per client
