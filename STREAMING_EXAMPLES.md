# Streaming Query Examples

> **Related Documentation:**
> - [Streaming API Guide](STREAMING.md) - Complete streaming API documentation
> - [Streaming with Filters](STREAMING_FILTER_EXAMPLES.md) - Filter examples for streaming queries
> - [Filter Syntax Reference](FILTER_EXAMPLES.md) - Complete filter syntax documentation

## Example 1: Find First 5 Variants in a Region

```javascript
// Start the query
const session = await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000
});

console.log("First variant:", session.variant);

// Get next 4 variants
for (let i = 0; i < 4 && session.session_id; i++) {
  const next = await get_next_variant({ session_id: session.session_id });
  if (next.variant) {
    console.log(`Variant ${i + 2}:`, next.variant);
  }
  session.session_id = next.session_id;
}

// Clean up if session still active
if (session.session_id) {
  await close_query_session({ session_id: session.session_id });
}
```

## Example 2: Process All Variants One by One

```javascript
let response = await start_region_query({
  chromosome: "chr1",
  start: 1000000,
  end: 2000000
});

let count = 0;

while (response.session_id) {
  if (response.variant) {
    count++;
    processVariant(response.variant);
  }
  
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

while (current.session_id && !pathogenic) {
  if (current.variant && current.variant.info?.CLNSIG === "Pathogenic") {
    pathogenic = current.variant;
    break;
  }
  
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

## Example 4: Compare Streaming vs Batch

```javascript
// Batch approach (all at once)
const batch = await query_by_region({
  chromosome: "20",
  start: 60000,
  end: 70000
});
console.log(`Batch: ${batch.result.count} variants`);

// Streaming approach (one at a time)
let stream = await start_region_query({
  chromosome: "20",
  start: 60000,
  end: 70000
});

let streamCount = 0;
while (stream.session_id) {
  if (stream.variant) streamCount++;
  stream = await get_next_variant({ session_id: stream.session_id });
}
console.log(`Streaming: ${streamCount} variants`);

// Should be the same count!
```

## Example 5: Handle Chromosome Name Variations

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
    
    while (response.session_id) {
      if (response.variant) {
        variants.push(response.variant);
      }
      
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

### Use Batch (`query_by_region`) when:
- ✅ Small regions (<100 variants)
- ✅ Need all variants at once for analysis
- ✅ Calculating statistics (need complete set)
- ✅ Simpler code (one API call)
- ✅ Faster for small result sets
