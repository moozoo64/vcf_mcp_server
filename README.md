# VCF MCP Server: Exposing Variant Calling Format files to LLMs for Analysis

## Overview

This is an MCP server that exposes a VCF (Variant Calling Format) file to LLMs for genomic variant analysis. The server provides tools to query variants by genomic position, region, or variant ID.

It is not intended to serve up multiple VCF files at once. It is designed to be used with a single VCF file in a desktop or confidential compute VM setting.

## Prerequisites

- Rust 1.70 or later

## Installation

```bash
cargo build --release
```

The binary will be at `./target/release/vcf_mcp_server`

## Usage

* stdio transport: ```./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz```
* HTTP/SSE transport: ```./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --sse 0.0.0.0:8090```

### Options

- `--sse <ADDR:PORT>` - Run HTTP server on specified address (e.g., 0.0.0.0:8090)
- `--debug` - Enable debug logging
- `--never-save-index` - Never save the built tabix index to disk (for read-only/ephemeral environments)

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
Query variants in a genomic region.

**Parameters:**
- `chromosome` (string): Chromosome name (e.g., '1', '2', 'X', 'chr1')
- `start` (integer): Start position (1-based, inclusive)
- `end` (integer): End position (1-based, inclusive)

**Example:**
```json
{
  "name": "query_by_region",
  "arguments": {
    "chromosome": "20",
    "start": 14000,
    "end": 18000
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

## VCF File Requirements

### Compressed VCF Files (Recommended)

For efficient querying by position/region, use bgzip-compressed VCF files with tabix indexes.
You probably already have these files if you're using a genome browser or other tools.
If not, you can create them using the following commands:

1. **Compress with bgzip:**
   ```bash
   bgzip myfile.vcf
   # Creates myfile.vcf.gz
   ```

2. **Create tabix index:**
   ```bash
   tabix -p vcf myfile.vcf.gz
   # Creates myfile.vcf.gz.tbi
   ```

The server will automatically use the `.tbi` index file if present, or build an in-memory index. The tabix file will be saved it alongside your VCF file if it doesn't already exist and `--never-save-index` was not used.

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
