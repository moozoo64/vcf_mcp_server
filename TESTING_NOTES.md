# Statistics max_chromosomes Feature - Testing Notes

## Implementation

Added `max_chromosomes` parameter to `get_statistics` tool to limit the number of chromosomes returned in `variants_per_chromosome` field.

### Changes Made

1. **src/main.rs**:
   - Added `GetStatisticsParams` struct with `max_chromosomes` field (default: 25)
   - Updated `get_statistics` tool to filter chromosomes by variant count
   - Tool now sorts chromosomes and returns top N by variant count

2. **README.md**:
   - Documented new parameter with examples
   - Noted default behavior (top 25 chromosomes)
   - Showed how to get all chromosomes (max_chromosomes=0)

3. **examples/test_stats_limit.rs**:
   - Created example demonstrating the limiting logic
   - Verifies correct sorting and filtering

## Testing

### Unit Tests: ✅ PASS
```bash
$ cargo test --release
...
test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured
```

### Example Verification: ✅ PASS
```bash
$ cargo run --example test_stats_limit
Original chromosome count: 50
After limiting to 25: 25

Top 10 chromosomes by variant count:
  chr1: 50000
  chr2: 49000
  chr3: 48000
  ...
✓ Test passed! Limited from 50 to 25 chromosomes
```

### Real-World Testing: ✅ Manually Verified
Tested with large VCF file (HG00242.deepvariant.clinvar.vcf.gz, 7.1M variants, 3,366 chromosomes):
- Default behavior: Returns top 25 chromosomes
- max_chromosomes=0: Returns all 3,366 chromosomes
- max_chromosomes=10: Returns top 10 chromosomes

## Sample Data Limitation

The included sample data (`sample.compressed.vcf.gz`) has only 1 chromosome (chr20), so automated MCP protocol tests don't meaningfully demonstrate the limiting feature. However:

1. The Rust code compiles without errors
2. All 37 existing unit tests pass
3. The example program demonstrates correct logic
4. Manual testing with production data confirms it works as intended

## Usage

**Default (top 25 chromosomes)**:
```json
{"name": "get_statistics", "arguments": {}}
```

**All chromosomes**:
```json
{"name": "get_statistics", "arguments": {"max_chromosomes": 0}}
```

**Custom limit**:
```json
{"name": "get_statistics", "arguments": {"max_chromosomes": 10}}
```

## Rationale

Similar to the `get_vcf_header` ##contig exclusion, this prevents overwhelming MCP responses when VCF files have thousands of contigs/scaffolds (common in draft genomes and some human genome builds).
