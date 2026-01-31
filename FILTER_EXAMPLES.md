# Variant Filter Examples

**⚠️ Breaking Change in v0.2.0**: The filter system has been upgraded to use the [vcf-filter](https://github.com/moozoo64/vcf-filter) library, which provides a more powerful expression language with INFO field access, annotation queries, and proper operator precedence.

## Filter Syntax

Filters use a SQL-like expression language to query variants based on VCF fields.

## Supported Fields

| Field Category | Examples | Description |
|---------------|----------|-------------|
| **Built-in VCF Columns** | `CHROM`, `POS`, `ID`, `REF`, `ALT`, `QUAL`, `FILTER` | Standard VCF columns |
| **INFO Fields** | `DP`, `AF`, `AC`, `AN`, etc. | Any INFO field from VCF header |
| **Annotations** | `ANN[0].Gene_Name`, `ANN[*].Annotation_Impact` | Structured annotations (SnpEff) |
| **LOF/NMD** | `LOF[0].Gene_Name`, `NMD[*].Percent_affected` | Loss-of-function annotations |

## Comparison Operators

| Operator | Example | Description |
|----------|---------|-------------|
| `==` | `FILTER == "PASS"` | Equal (strings must be quoted) |
| `!=` | `CLNSIG != "Benign"` | Not equal |
| `>` | `QUAL > 30` | Greater than |
| `<` | `DP < 100` | Less than |
| `>=` | `QUAL >= 30` | Greater than or equal |
| `<=` | `DP <= 50` | Less than or equal |
| `contains` | `CLNDN contains "cancer"` | Substring match |

## Logical Operators

| Operator | Example | Description |
|----------|---------|-------------|
| `&&` | `QUAL > 30 && DP >= 10` | Logical AND (both must be true) |
| `\|\|` | `CLNSIG == "Pathogenic" \|\| CLNSIG == "Likely_pathogenic"` | Logical OR (either must be true) |
| `!` | `!exists(LOF)` | Logical NOT |
| `()` | `(A \|\| B) && C` | Grouping for precedence |

## Functions

| Function | Example | Description |
|----------|---------|-------------|
| `exists()` | `exists(CLNSIG)` | True if field is present and not missing |

## Basic Examples

### Quality and Depth Filtering

```
QUAL > 30
QUAL >= 20 && DP >= 10
```

### Position Filtering

```
POS > 1000000
POS >= 43044295 && POS <= 43125483
```

### Chromosome Filtering

```
CHROM == "chr1"
CHROM == "20"
```

### ID and Allele Matching

```
ID contains "rs"
REF == "A"
ALT contains "G"
```

### Filter Status

```
FILTER == "PASS"
FILTER != "FAIL"
```

## INFO Field Examples

### Read Depth

```
DP >= 30
DP > 10 && DP < 100
```

### Allele Frequency

```
AF > 0.01
AF >= 0.05 && AF <= 0.95
```

### Allele Count

```
AC >= 1
AN == 2
```

## Annotation Examples

### Gene-Specific Queries

```
ANN[0].Gene_Name == "BRCA1"
ANN[*].Gene_Name == "TP53"
```

### Impact Filtering

```
ANN[*].Annotation_Impact == "HIGH"
ANN[*].Annotation_Impact == "HIGH" || ANN[*].Annotation_Impact == "MODERATE"
```

### Annotation Type

```
ANN[0].Annotation == "missense_variant"
ANN[*].Annotation contains "frameshift"
```

## Clinical Significance Examples

### ClinVar Filtering

```
CLNSIG == "Pathogenic"
CLNSIG == "Pathogenic" || CLNSIG == "Likely_pathogenic"
CLNDN contains "cancer"
CLNDN contains "BRCA" || CLNDN contains "breast"
```

## Complex Combinations

### Multi-Gene Filtering

```
QUAL > 30 && FILTER == "PASS" && (ANN[*].Gene_Name == "BRCA1" || ANN[*].Gene_Name == "BRCA2")
```

### Quality + Annotation

```
QUAL >= 30 && DP >= 10 && ANN[*].Annotation_Impact == "HIGH"
```

### Disease Association

```
FILTER == "PASS" && (CLNDN contains "cancer" || CLNDN contains "carcinoma")
```

### Position Range with Quality

```
CHROM == "17" && POS >= 43044295 && POS <= 43125483 && QUAL > 20
```

### Complex Clinical Query

```
(CLNSIG == "Pathogenic" || CLNSIG == "Likely_pathogenic") && 
QUAL > 30 && 
DP >= 20 && 
(ANN[*].Annotation_Impact == "HIGH" || ANN[*].Annotation_Impact == "MODERATE")
```

## Field Existence Checking

```
exists(CLNSIG)
exists(DP)
!exists(LOF)
```

## Important Notes

1. **String values must be quoted**: Use `"PASS"` not `PASS`
2. **Use `&&` and `||`**: Not `AND` and `OR` (breaking change from v0.1.0)
3. **Operator precedence**: Use parentheses `()` for complex logic
4. **Array access**: Use `[0]` for first element, `[*]` to match any element
5. **Case sensitivity**: Field names are case-sensitive, string comparisons respect quotes
6. **INFO fields**: Auto-detected from VCF header metadata
7. **Empty filter**: An empty filter expression (`""`) passes all variants

## Migration from v0.1.0

| Old Syntax (v0.1.0) | New Syntax (v0.2.0) |
|---------------------|---------------------|
| `QUAL > 30 AND FILTER == PASS` | `QUAL > 30 && FILTER == "PASS"` |
| `CHROM == chr1 OR CHROM == chr2` | `CHROM == "chr1" \|\| CHROM == "chr2"` |
| `FILTER in PASS,LowQual` | `FILTER == "PASS" \|\| FILTER == "LowQual"` |
| Not supported | `DP >= 30` (INFO fields) |
| Not supported | `ANN[*].Gene_Name == "BRCA1"` (annotations) |
| Not supported | `(A \|\| B) && C` (parentheses) |

## Error Handling

Invalid filter expressions will be rejected with descriptive error messages:

- **Unknown field**: Field not in VCF header
- **Parse error**: Invalid syntax
- **Type mismatch**: Comparing incompatible types
- **Invalid index**: Array index out of bounds
