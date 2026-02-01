#!/bin/bash
# Simple non-hanging test for get_statistics

set -e

VCF_FILE="sample_data/sample.compressed.vcf.gz"
BINARY="./target/release/vcf_mcp_server"

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo "Testing get_statistics with max_chromosomes parameter..."

# Create temp file with all requests
REQUESTS=$(cat <<'EOFREQ'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_statistics","arguments":{}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":1}}}
EOFREQ
)

# Run with timeout and capture output, then kill
response=$(echo "$REQUESTS" | timeout --kill-after=1 5 $BINARY "$VCF_FILE" 2>&1 | grep '^{' || true)

# Check if we got responses
if echo "$response" | grep -q '"id":2'; then
    echo -e "${GREEN}✓ get_statistics (default) works${NC}"
else
    echo -e "${RED}✗ get_statistics (default) failed${NC}"
fi

if echo "$response" | grep -q '"id":3'; then
    echo -e "${GREEN}✓ get_statistics (max_chromosomes) works${NC}"
    
    # Verify chromosome count
    chr_count=$(echo "$response" | grep '"id":3' | jq -r '.result.content[0].text | fromjson | .variants_per_chromosome | length' 2>/dev/null || echo "0")
    if [ "$chr_count" = "1" ]; then
        echo -e "${GREEN}✓ Chromosome limiting verified (1 chromosome as expected)${NC}"
    else
        echo -e "${RED}✗ Expected 1 chromosome, got $chr_count${NC}"
    fi
else
    echo -e "${RED}✗ get_statistics (max_chromosomes) failed${NC}"
fi

echo ""
echo "Test complete!"
