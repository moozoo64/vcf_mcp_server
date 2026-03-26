# Streaming Variant Filter Examples

> **Related Documentation:**
> - [Streaming API Guide](STREAMING.md) - Complete streaming API documentation
> - [Streaming Examples](STREAMING_EXAMPLES.md) - Basic streaming usage examples
> - [Filter Syntax Reference](FILTER_EXAMPLES.md) - Complete filter syntax documentation

This document demonstrates using filters with the streaming query tools: `start_region_query` and `get_next_variant`.

For the authoritative filter grammar/reference from the upstream library, use:

```json
{
  "name": "get_documentation",
  "arguments": {
    "doc_type": "filterlib"
  }
}
```

## Important API Notes

- Each call returns **up to 5 variants** in a `variants` array.
- `get_next_variant` accepts only `session_id` (no `count` parameter).
- Completion is indicated by:
  - `variants: []` (empty array)
  - `session_id: null`
  - `has_more: false`

## Workflow

1. Start a stream with `start_region_query` and an optional `filter`.
2. Process the returned `variants` array (up to 5 variants).
3. While `session_id` is present, keep calling `get_next_variant`.
4. Stop when `session_id` becomes `null` (or call `close_query_session` early).

## Start a Filtered Stream

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000,
  "filter": "QUAL > 30 && FILTER == \"PASS\""
}
```

Example response:

```json
{
  "variants": [
    {
      "chromosome": "20",
      "position": 1234567,
      "id": "rs123456"
    }
  ],
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "has_more": true,
  "reference_genome": "1000GenomesPilot-NCBI36 (from header)",
  "matched_chromosome": "20"
}
```

## Fetch Next Variant

```json
{
  "session_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

Possible in-progress response:

```json
{
  "variants": [
    {
      "chromosome": "20",
      "position": 1235237,
      "id": "microsat1"
    }
  ],
  "session_id": "550e8400-e29b-41d4-a716-446655440000",
  "has_more": true,
  "reference_genome": "1000GenomesPilot-NCBI36 (from header)",
  "matched_chromosome": "20"
}
```

End-of-stream response:

```json
{
  "variants": [],
  "session_id": null,
  "has_more": false,
  "reference_genome": "1000GenomesPilot-NCBI36 (from header)",
  "matched_chromosome": "20"
}
```

## Filter Examples

### INFO field filters

```json
{
  "filter": "DP >= 20 && AF > 0.05"
}
```

### Annotation filters

```json
{
  "filter": "ANN[*].Gene_Name == \"BRCA1\" && ANN[*].Annotation_Impact == \"HIGH\""
}
```

### Clinical-significance filters

```json
{
  "filter": "CLNSIG == \"Pathogenic\" || CLNSIG == \"Likely_pathogenic\""
}
```

### Combined filter

```json
{
  "filter": "(CLNSIG == \"Pathogenic\" || CLNSIG == \"Likely_pathogenic\") && QUAL > 30 && FILTER == \"PASS\""
}
```

## Error Examples

### Invalid filter syntax

Request:

```json
{
  "filter": "QUAL > 30 AND DP >= 10"
}
```

Typical error:

```json
{
  "error": {
    "message": "Invalid filter expression: Filter parse error: ..."
  }
}
```

### Invalid session

```json
{
  "session_id": "invalid-session-id"
}
```

Typical error:

```json
{
  "error": {
    "message": "Session not found or expired. Start a new query with start_region_query."
  }
}
```

## Best Practices

1. Use selective filters at stream start to reduce total calls.
2. Always check `has_more` and `session_id` on every response.
3. Call `close_query_session` if stopping early.
4. Use `&&` / `||` and quote string literals (e.g., `"PASS"`).
5. Test complex filters on a small region first.
