# Streaming Query Filter Examples

The streaming query tools (`start_region_query` and `get_next_variant`) now support filtering variants based on VCF field expressions.

## Basic Usage

### No Filter (All Variants)

```javascript
// Returns all variants in the region
const session = await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000
});

// Or explicitly empty filter
const session = await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000,
  filter: ""
});
```

### With Filter

```javascript
// Only high-quality passing variants
const session = await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000,
  filter: "QUAL > 30 AND FILTER == PASS"
});

// First variant returned already matches filter
console.log(session.variant); // { quality: 35, filter: ["PASS"], ... }

// Continue getting filtered variants
let next = await get_next_variant({ session_id: session.session_id });
// All subsequent variants also match the filter
```

## Filter Examples

### Quality-Based Filtering

```javascript
// High quality variants only
await start_region_query({
  chromosome: "1",
  start: 1000000,
  end: 2000000,
  filter: "QUAL > 50"
});

// Quality range
await start_region_query({
  chromosome: "chr1",
  start: 1000000,
  end: 2000000,
  filter: "QUAL >= 20 AND QUAL <= 100"
});
```

### Position-Based Filtering

```javascript
// First half of region only
await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000,
  filter: "POS < 65000"
});

// Exclude specific position range
await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000,
  filter: "POS < 62000 OR POS > 68000"
});
```

### Filter Status

```javascript
// Only passing variants
await start_region_query({
  chromosome: "17",
  start: 43044295,
  end: 43125483,
  filter: "FILTER == PASS"
});

// Multiple acceptable filter values
await start_region_query({
  chromosome: "17",
  start: 43044295,
  end: 43125483,
  filter: "FILTER in PASS,LowQual"
});
```

### ID-Based Filtering

```javascript
// Only known variants (has rsID)
await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000,
  filter: "ID contains rs"
});

// Specific variant ID pattern
await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000,
  filter: "ID contains 6054"
});
```

### Allele Filtering

```javascript
// SNPs with specific alternate allele
await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000,
  filter: "ALT contains A"
});

// Specific reference allele
await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000,
  filter: "REF == G AND ALT contains A"
});
```

## Complex Filters

### Multiple Criteria

```javascript
// High-quality SNPs only
await start_region_query({
  chromosome: "1",
  start: 1000000,
  end: 2000000,
  filter: "QUAL > 30 AND FILTER == PASS AND REF in A,T,G,C AND ALT in A,T,G,C"
});

// BRCA1 region, high-quality variants
await start_region_query({
  chromosome: "17",
  start: 43044295,
  end: 43125483,
  filter: "QUAL > 50 AND FILTER == PASS AND ID contains rs"
});
```

### OR Logic for Chromosome Sets

```javascript
// Variants on chromosome 13 or 17 (BRCA genes)
await start_region_query({
  chromosome: "13",
  start: 1,
  end: 115169878,
  filter: "CHROM == 13 OR CHROM == 17"
});
```

## Complete Workflow Examples

### Example 1: Find High-Quality Variants Only

```javascript
// Start session with filter
const session = await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000,
  filter: "QUAL > 30 AND FILTER == PASS"
});

const highQualityVariants = [];

if (session.variant) {
  highQualityVariants.push(session.variant);
}

// Get all remaining high-quality variants
let sid = session.session_id;
while (sid) {
  const next = await get_next_variant({ session_id: sid });
  if (next.variant) {
    highQualityVariants.push(next.variant);
  }
  sid = next.session_id;
}

console.log(`Found ${highQualityVariants.length} high-quality variants`);
// All variants in the array match the filter
```

### Example 2: Early Stopping with Filter

```javascript
// Find first 5 known SNPs with high quality
const session = await start_region_query({
  chromosome: "1",
  start: 1000000,
  end: 2000000,
  filter: "QUAL > 50 AND ID contains rs AND REF in A,T,G,C"
});

const knownSNPs = [];
let current = session;

while (current.session_id && knownSNPs.length < 5) {
  if (current.variant) {
    knownSNPs.push(current.variant);
  }
  
  if (knownSNPs.length < 5 && current.has_more) {
    current = await get_next_variant({ session_id: current.session_id });
  } else {
    break;
  }
}

// Clean up if we stopped early
if (current.session_id) {
  await close_query_session({ session_id: current.session_id });
}

console.log(`Found ${knownSNPs.length} matching SNPs`);
```

### Example 3: Compare Filtered vs Unfiltered

```javascript
// Count all variants
const allSession = await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000
});

let allCount = 0;
let current = allSession;
while (current.session_id) {
  if (current.variant) allCount++;
  current = await get_next_variant({ session_id: current.session_id });
}

// Count high-quality variants
const filteredSession = await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000,
  filter: "QUAL > 30 AND FILTER == PASS"
});

let filteredCount = 0;
current = filteredSession;
while (current.session_id) {
  if (current.variant) filteredCount++;
  current = await get_next_variant({ session_id: current.session_id });
}

console.log(`Total variants: ${allCount}`);
console.log(`High-quality variants: ${filteredCount}`);
console.log(`Filtered out: ${allCount - filteredCount}`);
```

## Error Handling

### No Variants Match Filter

```javascript
try {
  const session = await start_region_query({
    chromosome: "20",
    start: 60000,
    end: 61000,
    filter: "QUAL > 1000" // Unrealistic quality threshold
  });
} catch (error) {
  console.error(error);
  // Error: "No variants matching filter 'QUAL > 1000' found in region 20:60000-61000"
}
```

### Invalid Filter Expression

```javascript
try {
  const session = await start_region_query({
    chromosome: "20",
    start: 60000,
    end: 70000,
    filter: "INVALID SYNTAX"
  });
} catch (error) {
  // Will return no variants (filter doesn't match any)
  console.log("Filter might be invalid or too strict");
}
```

## Performance Considerations

### Filter at Query Time vs Post-Processing

**Using streaming filters** (recommended):
```javascript
// Server-side filtering - only matching variants returned
const session = await start_region_query({
  chromosome: "1",
  start: 1000000,
  end: 10000000, // Large region
  filter: "QUAL > 50 AND FILTER == PASS"
});

// Network transfer: Only ~100s of variants
// Memory usage: O(1) - one variant at a time
```

**Post-processing** (less efficient):
```javascript
// Get all variants, filter client-side
const session = await start_region_query({
  chromosome: "1",
  start: 1000000,
  end: 10000000
});

const filtered = [];
let current = session;
while (current.session_id) {
  if (current.variant && current.variant.quality > 50) {
    filtered.push(current.variant);
  }
  current = await get_next_variant({ session_id: current.session_id });
}

// Network transfer: All ~1000s of variants
// More API calls and data transfer
```

### Memory Efficiency

```javascript
// Process large region with complex filter - still O(1) memory
const session = await start_region_query({
  chromosome: "1",
  start: 1,
  end: 248956422, // Entire chromosome 1
  filter: "QUAL > 50 AND FILTER == PASS AND ID contains rs"
});

// Each call returns one variant, doesn't load entire chromosome
let count = 0;
let current = session;
while (current.session_id) {
  if (current.variant) {
    processVariant(current.variant); // Process incrementally
    count++;
  }
  current = await get_next_variant({ session_id: current.session_id });
}
```

## Supported Filter Syntax

See [FILTER_EXAMPLES.md](FILTER_EXAMPLES.md) for complete filter syntax documentation.

**Quick reference:**
- **Fields**: CHROM, POS, ID, REF, ALT, QUAL, FILTER
- **Operators**: `==`, `!=`, `<`, `>`, `<=`, `>=`, `contains`, `in`
- **Logic**: `AND`, `OR`
- **Case-insensitive**: All string comparisons

## Tips

1. **Empty filter passes all variants** - omit `filter` parameter or use `""`
2. **Filters are case-insensitive** - `"CHROM == chr1"` same as `"chrom == CHR1"`
3. **Filters persist in session** - same filter applies to all `get_next_variant` calls
4. **No variants error** - descriptive message indicates filter vs no variants in region
5. **Performance** - Server-side filtering is more efficient than client-side for large regions
