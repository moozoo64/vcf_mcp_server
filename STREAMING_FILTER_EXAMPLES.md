# Streaming Variant Filter Examples

**⚠️ Breaking Change in v0.2.0**: The filter system has been upgraded to use the [vcf-filter](https://github.com/moozoo64/vcf-filter) library. See [FILTER_EXAMPLES.md](FILTER_EXAMPLES.md) for syntax reference.

This document demonstrates using filters with the streaming variant query tools (`stream_region_query` and `get_next_variant`).

## Streaming Query Workflow

1. **Initialize stream** with `stream_region_query`:
   - Specify region (chromosome + range)
   - Optional filter expression
   - Returns `session_id` for subsequent queries

2. **Retrieve variants** with `get_next_variant`:
   - Use `session_id` from stream initialization
   - Optional `count` parameter (default 1)
   - Returns next matching variants or `no_more_variants: true`

## Basic Streaming Examples

### Stream All Variants in Region

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000
}
```

Response:
```json
{
  "session_id": "abc123",
  "filter": null,
  "message": "Stream initialized for region chr20:1000000-2000000"
}
```

### Stream High-Quality Variants

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000,
  "filter": "QUAL > 30"
}
```

### Stream PASS Variants with Depth

```json
{
  "chromosome": "17",
  "start": 43044295,
  "end": 43125483,
  "filter": "FILTER == \"PASS\" && DP >= 20"
}
```

## Fetching Next Variants

### Get Single Variant

```json
{
  "session_id": "abc123"
}
```

Response:
```json
{
  "variants": [{
    "chromosome": "20",
    "position": 1234567,
    "id": "rs123456",
    "reference": "A",
    "alternate": ["G"],
    "quality": 45.2,
    "filter": ["PASS"],
    "info": {"DP": 35, "AF": 0.5}
  }],
  "no_more_variants": false
}
```

### Get Multiple Variants

```json
{
  "session_id": "abc123",
  "count": 10
}
```

Returns up to 10 variants at once.

## INFO Field Filtering Examples

### Read Depth Filter

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000,
  "filter": "DP >= 30"
}
```

### Allele Frequency Range

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000,
  "filter": "AF >= 0.01 && AF <= 0.99"
}
```

### Multiple INFO Fields

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000,
  "filter": "DP >= 20 && AC >= 1 && AF > 0.05"
}
```

## Annotation Filtering Examples

### High Impact Variants

```json
{
  "chromosome": "17",
  "start": 43044295,
  "end": 43125483,
  "filter": "ANN[*].Annotation_Impact == \"HIGH\""
}
```

### Gene-Specific Stream

```json
{
  "chromosome": "17",
  "start": 43044295,
  "end": 43125483,
  "filter": "ANN[*].Gene_Name == \"BRCA1\""
}
```

### Variant Type Filter

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000,
  "filter": "ANN[*].Annotation contains \"frameshift\""
}
```

## Clinical Filtering Examples

### Pathogenic Variants

```json
{
  "chromosome": "17",
  "start": 43044295,
  "end": 43125483,
  "filter": "CLNSIG == \"Pathogenic\" || CLNSIG == \"Likely_pathogenic\""
}
```

### Disease Association

```json
{
  "chromosome": "17",
  "start": 43044295,
  "end": 43125483,
  "filter": "CLNDN contains \"cancer\" && FILTER == \"PASS\""
}
```

## Complex Streaming Filters

### Multi-Condition Clinical Query

```json
{
  "chromosome": "17",
  "start": 43044295,
  "end": 43125483,
  "filter": "(CLNSIG == \"Pathogenic\" || CLNSIG == \"Likely_pathogenic\") && QUAL > 30 && DP >= 20"
}
```

### Combined Gene and Impact Filter

```json
{
  "chromosome": "17",
  "start": 43000000,
  "end": 44000000,
  "filter": "(ANN[*].Gene_Name == \"BRCA1\" || ANN[*].Gene_Name == \"BRCA2\") && (ANN[*].Annotation_Impact == \"HIGH\" || ANN[*].Annotation_Impact == \"MODERATE\")"
}
```

### Quality + Annotation Stream

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000,
  "filter": "FILTER == \"PASS\" && QUAL >= 30 && DP >= 15 && ANN[*].Annotation_Impact == \"HIGH\""
}
```

## Paginated Query Pattern

### Initialize Stream

```json
// Tool: stream_region_query
{
  "chromosome": "20",
  "start": 1000000,
  "end": 5000000,
  "filter": "QUAL > 30 && DP >= 20"
}
```

Response:
```json
{
  "session_id": "session_xyz",
  "filter": "QUAL > 30 && DP >= 20",
  "message": "Stream initialized for region chr20:1000000-5000000 with filter"
}
```

### Fetch First Batch (100 variants)

```json
// Tool: get_next_variant
{
  "session_id": "session_xyz",
  "count": 100
}
```

Response:
```json
{
  "variants": [ /* 100 variants */ ],
  "no_more_variants": false
}
```

### Fetch Next Batch

```json
// Tool: get_next_variant
{
  "session_id": "session_xyz",
  "count": 100
}
```

Response:
```json
{
  "variants": [ /* 100 more variants */ ],
  "no_more_variants": false
}
```

### Detect End of Stream

```json
// Tool: get_next_variant
{
  "session_id": "session_xyz",
  "count": 100
}
```

Response:
```json
{
  "variants": [ /* remaining variants (could be < 100) */ ],
  "no_more_variants": true
}
```

## Performance Considerations

### Large Region Streaming

For very large regions or whole chromosomes, use streaming with filters to reduce memory:

```json
{
  "chromosome": "20",
  "start": 1,
  "end": 64444167,
  "filter": "QUAL > 50 && DP >= 30"
}
```

Then fetch in batches:
```json
{
  "session_id": "session_xyz",
  "count": 1000
}
```

### Targeted Gene Queries

For gene-specific analysis, use tight position ranges with annotation filters:

```json
{
  "chromosome": "17",
  "start": 43044295,
  "end": 43125483,
  "filter": "ANN[*].Gene_Name == \"BRCA1\" && ANN[*].Annotation_Impact == \"HIGH\""
}
```

## Real-World Use Cases

### Cancer Variant Screening

```json
{
  "chromosome": "17",
  "start": 1,
  "end": 83257441,
  "filter": "(CLNDN contains \"cancer\" || CLNDN contains \"carcinoma\") && (CLNSIG == \"Pathogenic\" || CLNSIG == \"Likely_pathogenic\") && FILTER == \"PASS\""
}
```

### Population Genetics Study

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 10000000,
  "filter": "AF >= 0.01 && AF <= 0.99 && DP >= 30 && QUAL > 30"
}
```

### Loss-of-Function Variants

```json
{
  "chromosome": "17",
  "start": 43000000,
  "end": 44000000,
  "filter": "exists(LOF) && ANN[*].Gene_Name == \"BRCA1\" && FILTER == \"PASS\""
}
```

### Rare Variant Analysis

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 5000000,
  "filter": "AF < 0.01 && DP >= 20 && QUAL > 30 && ANN[*].Annotation_Impact == \"HIGH\""
}
```

## Error Handling

### Invalid Filter Syntax

Request:
```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000,
  "filter": "QUAL > 30 AND DP >= 10"
}
```

Response:
```json
{
  "error": "Filter parse error: expected '&&' not 'AND'"
}
```

### Unknown Field

Request:
```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000,
  "filter": "UNKNOWN_FIELD > 30"
}
```

Response:
```json
{
  "error": "Unknown field: UNKNOWN_FIELD not found in VCF header"
}
```

### Invalid Session ID

Request:
```json
{
  "session_id": "invalid_session_xyz"
}
```

Response:
```json
{
  "error": "Invalid session ID: invalid_session_xyz"
}
```

## Migration from v0.1.0

### Old Syntax (v0.1.0)

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000,
  "filter": "QUAL > 30 AND FILTER == PASS"
}
```

### New Syntax (v0.2.0)

```json
{
  "chromosome": "20",
  "start": 1000000,
  "end": 2000000,
  "filter": "QUAL > 30 && FILTER == \"PASS\""
}
```

Key changes:
- `AND` → `&&`
- `OR` → `||`
- String values must be quoted: `"PASS"` not `PASS`
- New capabilities: INFO fields, annotations, parentheses

## Best Practices

1. **Use specific filters**: Narrow results at stream initialization to reduce network transfer
2. **Batch fetches**: Use `count` parameter to fetch multiple variants per request
3. **Check `no_more_variants`**: Stop fetching when stream is exhausted
4. **Validate filters first**: Test filter syntax on small regions before large queries
5. **Combine filters**: Use `&&` to combine quality, depth, and annotation filters
6. **Use parentheses**: Group complex logic for clarity and correct precedence
7. **Quote strings**: Always quote string values to avoid parse errors
