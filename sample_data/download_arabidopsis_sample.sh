#!/bin/bash
# Download and extract a small region from the 1001 Genomes Arabidopsis thaliana dataset
#
# This script downloads a subset of variants from Chromosome 1 to create a small
# test file for non-human genome testing.
#
# Source: 1001 Genomes Project (https://1001genomes.org/)
# Dataset: GMI-MPI v3.1 release
# Reference: TAIR10 (The Arabidopsis Information Resource)
#
# Arabidopsis thaliana genome:
#   - 5 chromosomes (Chr1-Chr5)
#   - ~135 Mb total genome size
#   - Chr1 is ~30 Mb

set -euo pipefail

# Configuration
REMOTE_VCF="https://1001genomes.org/data/GMI-MPI/releases/v3.1/1001genomes_snp-short-indel_only_ACGTN.vcf.gz"
REMOTE_INDEX="https://1001genomes.org/data/GMI-MPI/releases/v3.1/1001genomes_snp-short-indel_only_ACGTN.vcf.gz.tbi"
OUTPUT_FILE="arabidopsis_thaliana_chr1_subset.vcf"
OUTPUT_GZ="${OUTPUT_FILE}.gz"
REGION="Chr1:1-100000"  # First 100kb of Chr1
TEMP_DIR=$(mktemp -d)

echo "Downloading Arabidopsis thaliana VCF data..."
echo "Region: ${REGION}"
echo "Source: ${REMOTE_VCF}"
echo ""

# Check if bcftools is available
if ! command -v bcftools &> /dev/null; then
    echo "ERROR: bcftools is required but not installed."
    echo "Install with: brew install bcftools (macOS) or apt-get install bcftools (Debian-based Linux distros)"
    exit 1
fi

# Check if tabix is available
if ! command -v tabix &> /dev/null; then
    echo "ERROR: tabix is required but not installed."
    echo "Install with: brew install htslib (macOS) or apt-get install tabix (Debian-based Linux distros)"
    exit 1
fi

cleanup() {
    echo "Cleaning up temporary files..."
    rm -rf "${TEMP_DIR}"
}
trap cleanup EXIT

# Download just the first 10MB of the file to get header + initial variants
# This is much faster than streaming the entire 18GB file
echo "Downloading first 10MB of VCF file..."
curl -s -r 0-10485760 "${REMOTE_VCF}" | zcat 2>/dev/null | head -5000 > "${OUTPUT_FILE}"

# Check if we got any variants
VARIANT_COUNT=$(grep -v "^#" "${OUTPUT_FILE}" | wc -l | tr -d ' ')
if [ "$VARIANT_COUNT" -eq 0 ]; then
    echo "ERROR: No variants found in downloaded portion"
    exit 1
fi
echo "Downloaded ${VARIANT_COUNT} variants from the file"

# Compress with bgzip
echo "Compressing with bgzip..."
bgzip -f "${OUTPUT_FILE}"

# Create tabix index
echo "Creating tabix index..."
tabix -p vcf "${OUTPUT_GZ}"

# Print summary
echo ""
echo "Success! Created the following files:"
echo "  - ${OUTPUT_GZ} (compressed VCF)"
echo "  - ${OUTPUT_GZ}.tbi (tabix index)"
echo ""
echo "File statistics:"
bcftools stats "${OUTPUT_GZ}" | grep "^SN" | head -10
echo ""
echo "Sample variants:"
bcftools view "${OUTPUT_GZ}" | grep -v "^#" | head -5
