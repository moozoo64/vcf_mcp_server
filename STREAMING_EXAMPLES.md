# Streaming Query Examples

> **Related Documentation:**
> - [Streaming API Guide](STREAMING.md) - Complete streaming API documentation
> - [Streaming with Filters](STREAMING_FILTER_EXAMPLES.md) - Filter examples for streaming queries
> - [Filter Syntax Reference](FILTER_EXAMPLES.md) - Complete filter syntax documentation

## Example 1: Find First 5 Variants in a Region

```javascript
// Start the query - returns up to 5 variants immediately
const session = await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000
});

console.log("First batch:", session.variants);

// If more exist, keep fetching until we have 5 total
const all = [...session.variants];
let current_session_id = session.session_id;

while (current_session_id && all.length < 5) {
  const next = await get_next_variant({ session_id: current_session_id });
  all.push(...next.variants);
  current_session_id = next.session_id;
}

// Clean up if session still active
if (current_session_id) {
  await close_query_session({ session_id: current_session_id });
}
```

## Example 2: Process All Variants One Batch at a Time

```javascript
let response = await start_region_query({
  chromosome: "chr1",
  start: 1000000,
  end: 2000000
});

let count = 0;

while (true) {
  for (const v of response.variants) {
    count++;
    processVariant(v);
  }

  if (!response.session_id) break;
  response = await get_next_variant({ session_id: response.session_id });
}

console.log(`Processed ${count} variants`);
```

## Example 3: Stop Early When Condition Met

```javascript
// Find first pathogenic variant in BRCA1 region
const session = await start_region_query({
  chromosome: "17",
  start: 43044295,
  end: 43125483
});

let pathogenic = null;
let current = session;

outer: while (true) {
  for (const v of current.variants) {
    if (v.info?.CLNSIG === "Pathogenic") {
      pathogenic = v;
      break outer;
    }
  }

  if (!current.session_id) break;
  current = await get_next_variant({ session_id: current.session_id });
}

// Clean up - important when stopping early!
if (current.session_id) {
  await close_query_session({ session_id: current.session_id });
}

if (pathogenic) {
  console.log("Found pathogenic variant:", pathogenic);
} else {
  console.log("No pathogenic variants found in region");
}
```

## Example 4: Handle Chromosome Name Variations

```javascript
// Try with "chr" prefix
try {
  const session = await start_region_query({
    chromosome: "chr20",
    start: 60000,
    end: 70000
  });
  console.log("Success with chr20");
} catch (error) {
  // Error might suggest trying "20" instead
  console.log("Error:", error.message);
  
  // Retry without prefix
  const retry = await start_region_query({
    chromosome: "20",
    start: 60000,
    end: 70000
  });
  console.log("Success with 20");
  console.log("Matched chromosome:", retry.matched_chromosome);
}
```

## Example 6: Session Timeout Handling

```javascript
const session = await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000
});

// Simulate long delay (>5 minutes)
await new Promise(resolve => setTimeout(resolve, 301000));

try {
  // This will fail - session expired
  const next = await get_next_variant({ session_id: session.session_id });
} catch (error) {
  console.log("Session expired:", error.message);
  
  // Start new session
  const newSession = await start_region_query({
    chromosome: "20",
    start: 60000,
    end: 70000
  });
  console.log("New session started:", newSession.session_id);
}
```

## Example 7: Multiple Concurrent Sessions

```javascript
// LLM can manage multiple regions simultaneously
const session1 = await start_region_query({
  chromosome: "1",
  start: 100000,
  end: 200000
});

const session2 = await start_region_query({
  chromosome: "2", 
  start: 300000,
  end: 400000
});

// Process from both regions
const v1 = await get_next_variant({ session_id: session1.session_id });
const v2 = await get_next_variant({ session_id: session2.session_id });

console.log("Variant from chr1:", v1.variant);
console.log("Variant from chr2:", v2.variant);

// Clean up both sessions
await close_query_session({ session_id: session1.session_id });
await close_query_session({ session_id: session2.session_id });
```

## Example 8: Error Recovery

```javascript
async function robustStreamingQuery(chr, start, end) {
  try {
    let response = await start_region_query({
      chromosome: chr,
      start: start,
      end: end
    });
    
    const variants = [];

    while (true) {
      for (const v of response.variants) {
        variants.push(v);
      }

      if (!response.session_id) break;

      try {
        response = await get_next_variant({ 
          session_id: response.session_id 
        });
      } catch (error) {
        console.error("Error getting next variant:", error);

        // Try to close session on error
        try {
          await close_query_session({ session_id: response.session_id });
        } catch (closeError) {
          // Session might already be closed
        }
        break;
      }
    }
    
    return variants;
    
  } catch (error) {
    console.error("Error starting query:", error);
    return [];
  }
}

// Usage
const variants = await robustStreamingQuery("20", 60000, 70000);
console.log(`Retrieved ${variants.length} variants`);
```

## When to Use Streaming vs Batch

### Use Streaming (`start_region_query` + `get_next_variant`) when:
- ✅ Querying large regions (>1000 variants)
- ✅ Need to stop early (e.g., "find first N matching variants")
- ✅ Processing variants incrementally
- ✅ Memory constrained environment
- ✅ Interactive LLM workflows


