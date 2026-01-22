# VCF MCP Server - AI Coding Agent Instructions

## Project Overview

This is a **Model Context Protocol (MCP) server** that exposes VCF (Variant Calling Format) files to LLMs for genomic variant analysis. Built in Rust, it provides efficient genomic data querying via the MCP protocol over stdio or HTTP/SSE transports.

**Core Purpose**: Enable LLMs to query genomic variants from VCF files by position, region, or variant ID without requiring the LLM to parse VCF files directly.

## Architecture

### Three-Layer Design

1. **VCF Layer** ([src/vcf.rs](src/vcf.rs))
   - `VcfIndex`: Core data structure using genomic indexing for O(log n) queries
   - `GenomicIndex` enum: Wraps both `tabix::Index` (.tbi) and `csi::Index` (.csi)
   - Handles chromosome name normalization (e.g., "chr1" ↔ "1")
   - Manages two indices: genomic (position/region queries) and hash map (ID queries)
   - Index persistence: `.tbi`/`.csi` files for genomic, `.idx` files for ID lookups

2. **MCP Server Layer** ([src/main.rs](src/main.rs))
   - `VcfServer`: Implements MCP protocol using `rmcp` crate
   - Exposes 4 tools: `query_by_position`, `query_by_region`, `query_by_id`, `get_vcf_header`
   - Uses `#[tool_router]` macro from rmcp to auto-generate tool schema
   - Wraps `VcfIndex` in `Arc<Mutex<>>` for async access

3. **Transport Layer** ([src/main.rs](src/main.rs))
   - stdio transport (default): JSON-RPC over stdin/stdout
   - HTTP/SSE transport: `--sse` flag for web clients
   - Uses Axum for HTTP server

### Key Data Flow

```
LLM → MCP Tool Call → VcfServer → VcfIndex → Tabix Query → VCF File → Results → JSON Response
```

## Critical Developer Workflows

### Build & Run

```bash
# Always build in release mode for performance (tabix queries are I/O intensive)
cargo build --release

# Run with stdio transport (for Claude Desktop, VS Code, etc.)
./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz

# Run with HTTP/SSE transport (for web clients)
./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --sse 127.0.0.1:8090

# Debug mode (verbose logging)
./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --debug

# Read-only mode (never save indices)
./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --never-save-index
```

### Testing

```bash
# Unit tests (fast)
cargo test

# Integration tests via MCP protocol (requires release build)
./tests/test_server.sh

# Benchmark queries (requires sample data)
cargo bench
```

### Adding a New MCP Tool

1. Add parameter struct with `#[derive(serde::Deserialize, schemars::JsonSchema)]`
2. Add response struct with `#[derive(serde::Serialize)]`
3. Implement method on `VcfServer` with `#[tool(description = "...")]` attribute
4. Method signature: `async fn tool_name(&self, Parameters(params): Parameters<ParamType>) -> Result<CallToolResult, McpError>`
5. Lock index: `let index = self.index.lock().await;`
6. Query data, serialize to JSON via `Content::json()`, return `CallToolResult::success(vec![content])`

**Example**: See `query_by_position` in [src/main.rs](src/main.rs#L143-L193)

## Project-Specific Conventions

### Chromosome Naming Normalization

The VCF spec allows "1", "chr1", or custom names. This server **automatically tries both variants**:

- Query for "chr1" checks "chr1" and "1"
- Query for "1" checks "1" and "chr1"
- See `VcfIndex::find_matching_chromosome()` in [src/vcf.rs](src/vcf.rs#L102-L109)

When chromosome not found, responses include:

- `available_chromosomes_sample`: First 5 chromosomes in file
- `alternate_chromosome_suggestion`: Alternate naming (chr1 → 1 or 1 → chr1)

### Index Management Strategy

Two types of genomic indices, both with disk persistence:

1. **Genomic Positional Index** (`.vcf.gz.tbi` or `.vcf.gz.csi` file)
   - **CSI (.csi)**: Checked first, supports chromosomes > 512 Mbp
   - **Tabix (.tbi)**: Fallback, more widely compatible
   - For position/region queries
   - Auto-loaded if exists, else tabix index built in-memory
   - Saved to disk after build unless `--never-save-index` flag
   - Atomic write via `.tmp` rename to prevent corruption
   - Abstracted via `GenomicIndex` enum wrapping both types

2. **ID Index** (`.vcf.gz.idx` file)
   - HashMap of variant IDs → `[(chromosome, position)]`
   - Required because genomic indices can't query by ID
   - Binary format via `bincode` crate
   - Same save/load logic as genomic indices

**Race condition handling**: If index appears during build, discard in-progress build (see [src/vcf.rs](src/vcf.rs#L673-L680))

### Genome Build Awareness

Genomic coordinates are build-specific (GRCh37 vs GRCh38). All tool responses include `reference_genome` field:

- Extracted from VCF header `##reference=` line
- If missing, inferred from contig lengths (see `infer_genome_build_from_contigs` in [src/vcf.rs](src/vcf.rs#L245-L271))
- LLMs **must** check this field to avoid coordinate mismatches

### Error Handling Pattern

- Use `anyhow::Result` for VCF layer (simple error propagation)
- Convert to `McpError` at MCP boundary using `McpError::internal_error(msg, None)`
- Never panic in async handlers - all errors become MCP error responses

### Noodles Crate Usage

This project uses **Noodles** (Rust bioinformatics I/O):

- `noodles::vcf` for VCF parsing
- `noodles::bgzf` for block-gzip decompression
- `noodles::tabix` for tabix indexing (.tbi files)
- `noodles::csi` for CSI indexing (.csi files)
- Both implement `BinningIndex` trait for polymorphic queries
- Version pinned in Cargo.toml (breaking changes between releases)

**Key Gotcha**: Noodles uses 1-based, fully-closed intervals `[start, end]`, same as VCF spec. No off-by-one conversions needed.

## Testing Data

- **sample_data/sample.compressed.vcf.gz**: Small human genome subset (chromosome 20)
- **sample_data/ARABIDOPSIS_EXAMPLE.md**: Non-human genome example (validates cross-species support)
- Tests assume this data exists; missing files cause silent benchmark skips (see [benches/vcf_queries.rs](benches/vcf_queries.rs#L13-L18))

## Dependencies

- **rmcp**: MCP protocol implementation (use `#[tool_router]` macro, `Parameters<T>` wrapper)
- **noodles**: Genomic file I/O (VCF, tabix, CSI, bgzf)
- **tokio**: Async runtime (use `#[tokio::main]` on main, `.await` in handlers)
- **axum**: HTTP server for SSE transport
- **serde/serde_json**: Serialization (use `#[serde(rename_all = "snake_case")]` for consistency)

## Common Pitfalls

1. **Forgetting `cargo build --release`**: Debug builds are 10-100x slower for I/O
2. **Not handling missing chromosomes**: Always check `matched_chromosome` in responses
3. **Assuming chr prefix**: Different organisms use different naming (humans often "chr1", Arabidopsis uses "1")
4. **Mixing genome builds**: Coordinates from GRCh37 won't match GRCh38 - always verify `reference_genome`
5. **Not locking index**: `VcfIndex` is behind `Arc<Mutex<>>` - must `.lock().await` before use

## Debug Workflow

```bash
# Enable debug logging
./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --debug

# Test MCP protocol manually
./tests/test_server.sh

# Check query performance
cargo bench

# Verify index files created
ls -lh sample_data/*.{tbi,csi,idx}

# Create CSI index for testing (requires bcftools)
bcftools index -c sample.vcf.gz
```
