# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- `query_by_id` now accepts a comma-separated list of variant IDs and/or `chrom:pos` coordinates (e.g., `'rs1234,chr11:46352,rs46352,chr2:74635'`). Entries with a `:` are treated as `chromosome:position` lookups; all others are looked up by variant ID. Returns a flat, deduplicated list of all matching variants. Response field renamed from `query.id` to `query.ids` (array).

## [0.2.3] - 2026

### Changed
- Replaced `bincode`-backed `.idx` and `.stats` cache persistence with `serde_json` for easier inspection and better version resilience
- Updated code paths and docs to match the current `rmcp` 1.3 / `noodles` 0.109 behavior
- Refreshed release documentation for the new package version

## [0.2.2] - 2026

### Changed
- Upgraded crate metadata to Rust edition 2024
- Bumped crate version to 0.2.2
- Updated `vcf-filter` to latest Git revision (includes DP filter behavior fix observed in tests)

## [0.2.1] - 2026

### Changed
- Documentation synchronized with current MCP API behavior and response schemas
- Streaming examples updated to reflect one-variant-per-call semantics (`variant`, `session_id`, `has_more`)
- Test scripts and testing docs aligned with current timeout-safe non-hanging workflow

## [0.2.0-fork] - 2024

**Note**: This version represents enhancements made in this fork by Michael Simmons, built on top of Jade Auer's v0.1.0 release.

### Added
- **Streaming Query API**: New stateful streaming tools for processing large genomic regions
  - `start_region_query`: Initialize a streaming session for a genomic region
  - `get_next_variant`: Retrieve next variant from an active session
  - `close_query_session`: Close an active session and free resources
  - Session management with 5-minute timeout and automatic cleanup
  - UUID-based session IDs for secure session tracking
- **Statistics Tool** (`get_statistics`): Comprehensive VCF file statistics including:
  - Variant counts by type (SNPs, insertions, deletions, MNPs, complex)
  - Quality score statistics (min, max, mean)
  - Filter status distribution
  - Chromosome-specific variant counts
- **Documentation Tool** (`get_documentation`): Access embedded documentation
  - Six documentation types: readme, streaming, filters, streaming-filters, filterlib, all
  - Documentation embedded at compile time (~50KB added to binary)
- **MCP Resource**: `vcf://metadata` resource for accessing VCF header metadata
  - File format version, reference genome, contigs, samples
  - Accessible without querying variants
- **Advanced Variant Filtering**: Integration with [vcf-filter](https://github.com/moozoo64/vcf-filter) library
  - Support for complex filter expressions with `&&` and `||` operators
  - Available on `query_by_region` and `start_region_query` tools
  - Field access for QUAL, DP, FILTER, and INFO fields
- **10kb Region Size Limit**: Added performance constraint on `query_by_region`
  - Prevents memory issues with very large regions
  - Streaming API recommended for regions >10kb

### Changed
- **⚠️ BREAKING: Filter Syntax Migration**
  - Old syntax (v0.1.0): `AND`/`OR` operators, unquoted strings
  - New syntax (v0.2.0): `&&`/`||` operators, quoted strings
  - See [FILTER_EXAMPLES.md](FILTER_EXAMPLES.md) for migration guide
- **Dependency Updates**:
  - `noodles`: 0.101.0 → 0.104.0
  - `rmcp`: 0.8 → 0.13.0
- **Enhanced Error Messages**: Better chromosome mismatch handling with suggestions

### Fixed
- Race condition in index file creation (atomic write via .tmp rename)
- Memory efficiency improvements in statistics calculation

## [0.1.0] - 2024

**Note**: Original release by Jade Auer. The following is documentation of the original v0.1.0 features for reference.

### Added
- Initial release of VCF MCP Server
- **Core Query Tools**:
  - `query_by_position`: Query variants at a specific genomic position
  - `query_by_region`: Query variants in a genomic region
  - `query_by_id`: Query variants by variant ID (e.g., rsID) — supports a single ID or comma-separated list
  - `get_vcf_header`: Retrieve raw VCF header text
- **Dual Index Support**:
  - Tabix (.tbi) index support for genomic queries
  - CSI (.csi) index support for very large chromosomes (>512 Mbp)
  - ID index (.idx) for rsID lookups
  - Automatic index detection and loading
  - In-memory index building with optional disk persistence
- **Chromosome Name Normalization**: Automatic handling of "chr1" vs "1" naming
- **Genome Build Detection**: 
  - Extract from VCF header `##reference=` line
  - Infer from contig lengths if missing
  - Include in all query responses
- **Dual Transport Support**:
  - stdio transport (default) for desktop integration
  - HTTP/SSE transport for web clients
- **Command-line Options**:
  - `--sse <ADDR:PORT>`: Run HTTP server
  - `--debug`: Enable debug logging
  - `--never-save-index`: Read-only mode for ephemeral environments
- **Comprehensive Test Suite**:
  - Integration tests via MCP protocol
  - Performance benchmarks
  - Sample data for human (1000 Genomes) and plant (Arabidopsis) genomes

### Technical Details
- Built with Rust 2021 edition
- Uses Noodles bioinformatics library for VCF/tabix/CSI parsing
- Implements MCP protocol via rmcp crate
- Async architecture with tokio runtime
- Thread-safe with Arc<Mutex<>> for concurrent access

---

## Version Numbering

- **Major version** (X.0.0): Breaking API changes or major architectural changes
- **Minor version** (0.X.0): New features, non-breaking changes
- **Patch version** (0.0.X): Bug fixes, documentation updates

## Upgrade Notes

### Migrating from v0.1.0 to v0.2.0-fork

**Filter Syntax Changes**:
```javascript
// Old (v0.1.0)
{
  "filter": "QUAL > 30 AND FILTER = PASS"
}

// New (v0.2.0)
{
  "filter": "QUAL > 30 && FILTER == \"PASS\""
}
```

Key changes:
- `AND` → `&&`
- `OR` → `||`
- `=` → `==`
- String values must be quoted: `PASS` → `"PASS"`

**New Features to Adopt**:
- Use streaming API (`start_region_query`) for large regions instead of `query_by_region`
- Access `vcf://metadata` resource for file structure information
- Use `get_statistics` tool for comprehensive variant analysis
- Access embedded docs via `get_documentation` tool

[0.2.0-fork]: https://github.com/jda/vcf_mcp_server/compare/v0.1.0...develop
[0.2.3]: https://github.com/jda/vcf_mcp_server/compare/v0.2.2...develop
[0.2.2]: https://github.com/jda/vcf_mcp_server/compare/v0.2.1...develop
[0.2.1]: https://github.com/jda/vcf_mcp_server/compare/v0.2.0-fork...develop
[0.1.0]: https://github.com/jda/vcf_mcp_server/releases/tag/v0.1.0
