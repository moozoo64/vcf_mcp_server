# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

VCF MCP Server is a Rust-based Model Context Protocol (MCP) server that exposes Variant Calling Format (VCF) files to LLMs for analysis. It supports looking up variants by chromosome, position, and ID.

## Technology Stack

- **Language**: Rust (2021 edition)
- **Key Dependencies**:
  - `noodles` (v0.101.0): VCF parsing and tabix indexing support
  - `rmcp` (v0.8): MCP server implementation

## Commands

### Build
```bash
cargo build
```

### Run
```bash
cargo run
```

### Build for release
```bash
cargo build --release
```

### Run tests
```bash
cargo test
```

### Run a single test
```bash
cargo test <test_name>
```

### Check code without building
```bash
cargo check
```

### Format code
```bash
cargo fmt
```

### Lint
```bash
cargo clippy
```

### Run benchmarks
```bash
cargo bench
```

Benchmarks use the Criterion framework to measure performance of VCF query operations. This helps detect performance regressions in:
- `query_by_position`: Lookup variants at specific genomic positions
- `query_by_region`: Lookup variants in genomic regions
- `query_by_id`: Lookup variants by their ID (e.g., rsID)

Criterion automatically compares new runs against saved baselines and reports statistical differences in the terminal output.

**Important**: Always run benchmarks before and after performance-sensitive changes to ensure no regressions.

## Architecture

This is an MCP server that bridges VCF genomic data files with LLMs. The server uses:

- **noodles**: A bioinformatics library for parsing VCF files and handling tabix-indexed files for efficient genomic coordinate queries
- **rmcp**: Provides the MCP server infrastructure to expose VCF data operations as tools/resources that LLMs can invoke

The MCP protocol allows LLMs to query genomic variants by:
- Chromosome location
- Genomic position
- Variant ID

This enables LLMs to perform analysis on genomic variation data without needing to understand the VCF file format directly.
