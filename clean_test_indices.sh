#!/bin/bash
# Clean up all generated VCF index files before running tests
# This ensures tests start with a clean slate and build indices from scratch

set -e

echo "Cleaning up VCF index files in sample_data/..."

# Remove all index files
rm -f sample_data/*.tbi
rm -f sample_data/*.csi
rm -f sample_data/*.idx
rm -f sample_data/*.stats

echo "✓ Index files removed"
echo ""
echo "Run 'cargo test' to regenerate indices during testing"
