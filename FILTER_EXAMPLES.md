# Variant Filter Examples

The `evaluate_filter()` function allows filtering variants based on VCF fields using simple expressions.

## Function Signature

```rust
pub fn evaluate_filter(variant: &Variant, filter_expr: &str) -> bool
```

## Supported Fields

| Field | Aliases | Description | Type |
|-------|---------|-------------|------|
| `CHROM` | `CHROMOSOME` | Chromosome name | String |
| `POS` | `POSITION` | Genomic position | Numeric |
| `ID` | - | Variant ID | String |
| `REF` | `REFERENCE` | Reference allele | String |
| `ALT` | `ALTERNATE` | Alternate alleles (comma-separated) | String |
| `QUAL` | `QUALITY` | Quality score | Numeric |
| `FILTER` | - | Filter status (comma-separated) | String |

## Supported Operators

### Comparison Operators
- `==` - Equals (case-insensitive)
- `!=` - Not equals
- `>` - Greater than (numeric)
- `<` - Less than (numeric)
- `>=` - Greater than or equal (numeric)
- `<=` - Less than or equal (numeric)

### String Operators
- `contains` - String contains substring (case-insensitive)
- `in` - Value is in comma-separated list (case-insensitive)

### Logical Operators
- `AND` - Both conditions must be true
- `OR` - Either condition must be true

## Usage Examples

### Basic Comparisons

```rust
use vcf::{Variant, evaluate_filter};

// Quality score filtering
evaluate_filter(&variant, "QUAL > 30");
evaluate_filter(&variant, "QUAL >= 20");
evaluate_filter(&variant, "QUAL < 50");

// Position filtering
evaluate_filter(&variant, "POS == 14370");
evaluate_filter(&variant, "POS > 1000000");

// Chromosome filtering
evaluate_filter(&variant, "CHROM == chr1");
evaluate_filter(&variant, "CHROM == 20");
```

### String Matching

```rust
// ID contains substring
evaluate_filter(&variant, "ID contains rs");
evaluate_filter(&variant, "ID contains 6054257");

// Reference/Alternate allele matching
evaluate_filter(&variant, "REF == A");
evaluate_filter(&variant, "ALT contains G");

// Filter status
evaluate_filter(&variant, "FILTER == PASS");
evaluate_filter(&variant, "FILTER != FAIL");
```

### In Operator

```rust
// Check if value is in list
evaluate_filter(&variant, "FILTER in PASS,LowQual");
evaluate_filter(&variant, "CHROM in 1,2,3,4,5");
evaluate_filter(&variant, "REF in A,T,G,C");
```

### Logical Combinations

```rust
// AND - both must be true
evaluate_filter(&variant, "QUAL > 30 AND FILTER == PASS");
evaluate_filter(&variant, "CHROM == 17 AND POS >= 43044295 AND POS <= 43125483");

// OR - either must be true
evaluate_filter(&variant, "CHROM == 13 OR CHROM == 17"); // BRCA1/BRCA2
evaluate_filter(&variant, "QUAL > 50 OR FILTER == PASS");

// Complex combinations (left-to-right evaluation)
evaluate_filter(&variant, "CHROM == 20 AND POS > 60000 AND QUAL > 20");
```

### Range Queries

```rust
// Position range
evaluate_filter(&variant, "POS >= 1000000 AND POS <= 2000000");

// Quality range
evaluate_filter(&variant, "QUAL > 20 AND QUAL < 100");

// Exclude range
evaluate_filter(&variant, "POS < 1000 OR POS > 5000");
```

### Real-World Examples

```rust
// High-quality passing variants
evaluate_filter(&variant, "QUAL > 30 AND FILTER == PASS");

// BRCA1 region variants (chromosome 17)
evaluate_filter(&variant, "CHROM == 17 AND POS >= 43044295 AND POS <= 43125483");

// SNPs only (single nucleotide)
evaluate_filter(&variant, "REF in A,T,G,C AND ALT in A,T,G,C");

// Known variants (has rsID)
evaluate_filter(&variant, "ID contains rs");

// Multi-allelic variants
evaluate_filter(&variant, "ALT contains ,");

// Specific chromosomes only
evaluate_filter(&variant, "CHROM in 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,X,Y");
```

### Edge Cases

```rust
// Empty filter (passes all variants)
evaluate_filter(&variant, "");
evaluate_filter(&variant, "   ");

// Case insensitive
evaluate_filter(&variant, "chrom == CHR1");  // Same as CHROM == chr1
evaluate_filter(&variant, "Filter == pass");  // Same as FILTER == PASS

// Missing quality (None)
// Returns false for numeric comparisons if quality is None
evaluate_filter(&variant, "QUAL > 30");  // false if quality is None
```

## Integration Example

```rust
use vcf::{VcfIndex, evaluate_filter};

fn filter_variants(index: &VcfIndex, chromosome: &str, start: u64, end: u64, filter: &str) -> Vec<Variant> {
    let (variants, _) = index.query_by_region(chromosome, start, end);
    
    variants
        .into_iter()
        .filter(|v| evaluate_filter(v, filter))
        .collect()
}

// Usage
let high_quality = filter_variants(
    &index, 
    "20", 
    60000, 
    70000, 
    "QUAL > 30 AND FILTER == PASS"
);

println!("Found {} high-quality variants", high_quality.len());
```

## Performance Notes

- **String comparisons** are case-insensitive (converted to lowercase)
- **Numeric comparisons** require both values to parse as f64
- **AND/OR evaluation** is left-to-right (no operator precedence)
- **Short-circuit evaluation**: AND stops on first false, OR stops on first true
- Empty filters always return `true` (pass all variants)

## Limitations

- No parentheses for grouping (e.g., `(A OR B) AND C` not supported)
- No operator precedence (AND and OR have same priority)
- No negation operator (use `!=` instead)
- INFO fields not yet supported (only base VCF fields)
- No regex matching (only exact match and contains)

## Future Enhancements

Possible additions:
- INFO field support (e.g., `INFO.DP > 10`)
- Parentheses for grouping
- Regex matching (`REF matches [ACGT]{3,}`)
- NOT operator
- EXISTS operator (e.g., `ID exists`)
- Array operations for multi-allelic variants
