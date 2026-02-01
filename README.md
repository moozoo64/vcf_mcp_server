# VCF MCP Server: Exposing Variant Calling Format files to LLMs for Analysis

**Original Author**: [Jade Auer (jda)](https://github.com/jda)  
**Original Repository**: https://github.com/jda/vcf_mcp_server  
**License**: MIT License

_Original disclaimer: No warranty express or implied. This was totally vibe-coded while chronicly sleep-deprived and watching Bluey and Tremors 5._ Use at your own risk. AFAICT it works and outputs line up with what I get through traditional tools.

**Current Version**: 0.2.0-fork | [Changelog](CHANGELOG.md)

---

**About This Fork**: This is a fork of Jade's original work with additional features vibe-coded by Michael Simmons using Claude Sonnet 4.5 via GitHub Copilot. Enhancements include streaming API, statistics tool, advanced filter support, comprehensive documentation, and more. See [CHANGELOG.md](CHANGELOG.md) for complete details. These changes are being prepared for potential contribution back to the upstream repository.

---

## Overview

This is an MCP server that exposes a VCF (Variant Calling Format) file to LLMs for genomic variant analysis. The server provides tools to query variants by genomic position, region, or variant ID.

It is not intended to serve up multiple VCF files at once. It is designed to be used with a single VCF file in a desktop or confidential compute VM setting.

## Prerequisites

- Rust 1.70 or later

## Build

You want to build in release mode for optimal performance:
```bash
cargo build --release
```

The binary will be at `./target/release/vcf_mcp_server`

## Usage

* stdio transport: ```./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz```
* HTTP/SSE transport: ```./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --sse 127.0.0.1:8090```

### Options

- `--sse <ADDR:PORT>` - Run HTTP server on specified address (e.g., 127.0.0.1:8090)
- `--debug` - Enable debug logging
- `--never-save-index` - Never save the built index to disk (for read-only/ephemeral environments)

## Available MCP Tools

### 1. `query_by_position`
Query variants at a specific genomic position.

**Parameters:**
- `chromosome` (string): Chromosome name (e.g., '1', '2', 'X', 'chr1')
- `position` (integer): Genomic position (1-based)

**Example:**
```json
{
  "name": "query_by_position",
  "arguments": {
    "chromosome": "20",
    "position": 14370
  }
}
```

### 2. `query_by_region`
Query variants in a genomic region. **Note: Region size is limited to 10,000 base pairs (10kb) for performance reasons.** For larger regions, use the streaming API (`start_region_query`).

**Parameters:**
- `chromosome` (string): Chromosome name (e.g., '1', '2', 'X', 'chr1')
- `start` (integer): Start position (1-based, inclusive)
- `end` (integer): End position (1-based, inclusive)
- `filter` (string, optional): Filter expression to select variants (see [FILTER_EXAMPLES.md](FILTER_EXAMPLES.md))

**Example:**
```json
{
  "name": "query_by_region",
  "arguments": {
    "chromosome": "20",
    "start": 14000,
    "end": 18000,
    "filter": "QUAL > 30 && FILTER == \"PASS\""
  }
}
```

### 3. `query_by_id`
Query variants by variant ID (e.g., rsID).

**Parameters:**
- `id` (string): Variant ID (e.g., 'rs6054257')

**Example:**
```json
{
  "name": "query_by_id",
  "arguments": {
    "id": "rs6054257"
  }
}
```

### 4. `start_region_query` (Streaming)
Start a streaming query session for a genomic region. Returns one variant at a time.

**Parameters:**
- `chromosome` (string): Chromosome name (e.g., '1', '2', 'X', 'chr1')
- `start` (integer): Start position (1-based, inclusive)
- `end` (integer): End position (1-based, inclusive)
- `filter` (string, optional): Filter expression to select variants (see [FILTER_EXAMPLES.md](FILTER_EXAMPLES.md))

**Returns:** First variant + session_id for subsequent calls

### 5. `get_next_variant` (Streaming)
Get the next variant from an active streaming session.

**Parameters:**
- `session_id` (string): Session ID from start_region_query

**Returns:** Next variant (or null if exhausted)

### 6. `close_query_session` (Streaming)
Close an active streaming session and free resources.

**Parameters:**
- `session_id` (string): Session ID to close

**See [STREAMING.md](STREAMING.md) for detailed streaming API documentation.**

### 7. `get_vcf_header`
Get the raw VCF file header text. **By default, `##contig` lines are excluded** to reduce clutter. Use the search parameter to filter for specific header types or to include contig definitions.

**Parameters:**
- `search` (string, optional): Filter string to match header lines (e.g., "##INFO", "##FILTER", "##FORMAT", "##contig")

**Returns:** Header text with line count and reference genome information

**Examples:**

Get header without contig lines (default):
```json
{
  "name": "get_vcf_header",
  "arguments": {}
}
```

Get only contig definitions:
```json
{
  "name": "get_vcf_header",
  "arguments": {
    "search": "##contig"
  }
}
```

Get only INFO definitions:
```json
{
  "name": "get_vcf_header",
  "arguments": {
    "search": "##INFO"
  }
}
```

Get only FILTER definitions:
```json
{
  "name": "get_vcf_header",
  "arguments": {
    "search": "##FILTER"
  }
}
```

### 8. `get_statistics`
Get comprehensive statistics about the VCF file including variant counts, quality metrics, and variant type distributions. **By default, `variants_per_chromosome` is limited to the top 25 chromosomes by variant count** to reduce response size.

**Parameters:**
- `max_chromosomes` (integer, optional): Maximum number of chromosomes to include in `variants_per_chromosome`. Default is 25. Set to 0 to include all chromosomes.

**Returns:** 
- Total variant count
- SNP/insertion/deletion/MNP/complex variant counts
- Quality score statistics (min, max, mean)
- Depth statistics
- Filter status distribution
- Chromosome-specific variant counts (limited to top N chromosomes)

**Examples:**

Get statistics with default limits (top 25 chromosomes):
```json
{
  "name": "get_statistics",
  "arguments": {}
}
```

Get statistics with all chromosomes included:
```json
{
  "name": "get_statistics",
  "arguments": {
    "max_chromosomes": 0
  }
}
```

Get statistics with top 10 chromosomes only:
```json
{
  "name": "get_statistics",
  "arguments": {
    "max_chromosomes": 10
  }
}
```

### 9. `get_documentation`
Get embedded documentation for this MCP server.

**Parameters:**
- `doc_type` (string): Type of documentation - "readme", "streaming", "filters", "streaming-filters", or "all"

**Example:**
```json
{
  "name": "get_documentation",
  "arguments": {
    "doc_type": "filters"
  }
}
```

## Filter Support

The server supports advanced variant filtering using the [vcf-filter](https://github.com/moozoo64/vcf-filter) library. Filters can be applied to `query_by_region` and `start_region_query` tools.

**Example filters:**
- `QUAL > 30` - Quality score greater than 30
- `FILTER == "PASS"` - Only variants that passed all filters
- `DP >= 10` - Read depth of at least 10
- `QUAL > 30 && FILTER == "PASS"` - Combined conditions

**See [FILTER_EXAMPLES.md](FILTER_EXAMPLES.md) for comprehensive filter syntax documentation and examples.**

## MCP Resources

The server exposes an MCP resource for accessing VCF metadata:

### `vcf://metadata`
Provides structured metadata from the VCF file header including:
- File format version
- Reference genome information
- Contig definitions (chromosome names and lengths)
- Sample IDs
- Filter definitions
- INFO and FORMAT field definitions

This resource can be accessed by MCP clients to understand the structure of the VCF file without querying variants.

## VCF File Requirements

### Compressed VCF Files (Recommended)

For efficient querying by position/region, use bgzip-compressed VCF files with genomic indices.
The server supports both **tabix (.tbi)** and **CSI (.csi)** indices:

- **CSI indices** are checked first (better support for very large chromosomes > 512 Mbp)
- **Tabix indices** are used as fallback (more widely compatible)

You probably already have these files if you're using a genome browser or other tools.
If not, you can create them using the following commands:

1. **Compress with bgzip:**
   ```bash
   bgzip myfile.vcf
   # Creates myfile.vcf.gz
   ```

2. **Create index (choose one):**
   
   **Tabix index (default, widely compatible):**
   ```bash
   tabix -p vcf myfile.vcf.gz
   # Creates myfile.vcf.gz.tbi
   ```
   
   **CSI index (for very large chromosomes):**
   ```bash
   bcftools index -c myfile.vcf.gz
   # Creates myfile.vcf.gz.csi
   ```

The server will automatically detect and use `.csi` or `.tbi` index files if present, or build an in-memory tabix index. The index will be saved alongside your VCF file if it doesn't already exist and `--never-save-index` was not used.

### Uncompressed VCF Files

Uncompressed VCF files are supported but will be indexed in-memory only. For large files, this can be slow and memory-intensive.

## Development

The usual for Rust development:
```bash
cargo run
cargo test
cargo fmt
cargo clippy
cargo bench  # Run performance benchmarks
```

## Test Data

Sample VCF files in the `sample_data/` directory are provided for testing and demonstration:

- **VCFlib samples** - Small test files from the VCFlib project (MIT License)
- **1000 Genomes Project data** - Subset of high-coverage sequencing data (CC BY-NC-SA 3.0)

See [sample_data/README.md](sample_data/README.md) for complete attribution and licensing information.

## License

MIT
