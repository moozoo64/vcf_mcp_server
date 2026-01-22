# Using Arabidopsis VCF Data with VCF MCP Server

This directory contains a sample VCF file from *Arabidopsis thaliana* (thale cress), demonstrating that the VCF MCP Server works with non-human genomes.

## File Information

- **File**: `arabidopsis_thaliana_chr1_subset.vcf.gz`
- **Organism**: *Arabidopsis thaliana*
- **Reference Genome**: TAIR10
- **Chromosomes**: 1, 2, 3, 4, 5 (file contains data from chromosome 1 only)
- **Variants**: ~5,000 SNPs and short indels from the first 100kb of chromosome 1
- **Samples**: 1,135 accessions from the 1001 Genomes Project

## Usage Example

```bash
# Start the VCF MCP server with the Arabidopsis VCF file
./target/release/vcf_mcp_server sample_data/arabidopsis_thaliana_chr1_subset.vcf.gz
```

## Query Examples

### Query by Position

Look up variants at a specific genomic position on chromosome 1:

**Position 55**:

- Chromosome: 1
- Position: 55
- Reference: C
- Alternate: T

**Position 63**:

- Chromosome: 1
- Position: 63
- Reference: T
- Alternate: C

### Query by Region

Query a genomic region:

- **Region**: chromosome 1, positions 50-100
- Contains 7 variants in this 50bp window

### Chromosome Naming

Arabidopsis uses simple numeric chromosome naming (1-5), unlike human genomes which often use "chr" prefix (chr1, chr2, etc.). The server handles both conventions automatically.

## Key Differences from Human Genomes

| Feature | Human (GRCh38) | Arabidopsis (TAIR10) |
| --------- | ---------------- | ---------------------- |
| Genome Size | ~3.1 Gb | ~135 Mb |
| Chromosomes | 1-22, X, Y, M | 1-5 |
| Chr Naming | chr1, chr2... or 1, 2... | 1, 2, 3, 4, 5 |
| Reference | GRCh38 | TAIR10 |

## Regenerating the Dataset

To download fresh data from the source:

```bash
cd sample_data
./download_arabidopsis_sample.sh
```

This will download the first 100kb of chromosome 1 from the 1001 Genomes Project (v3.1 release) and create a new subset file.

## References

- **1001 Genomes Project**: [https://1001genomes.org/](https://1001genomes.org/)
- [**TAIR** (The Arabidopsis Information Resource)](https://www.arabidopsis.org/)
- **Paper**: 1,135 Genomes Reveal the Global Pattern of Polymorphism in Arabidopsis thaliana, Cell 166, 481–491 (2016)
