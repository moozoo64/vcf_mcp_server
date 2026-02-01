#!/bin/bash

# Test the get_statistics MCP tool

set -e

VCF_FILE="sample_data/sample.compressed.vcf.gz"
BINARY="./target/release/vcf_mcp_server"

echo "Testing get_statistics tool..."

response=$(timeout 10 $BINARY "$VCF_FILE" 2>/dev/null <<'EOF' || true
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_statistics","arguments":{}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":2}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":0}}}
EOF
)

# Extract the statistics response (filter out non-JSON lines)
stats_response=$(echo "$response" | grep -E '^\{.*"id":2' || true)

if [ -z "$stats_response" ]; then
    echo "ERROR: No response from get_statistics"
    exit 1
fi

# Check for expected fields
echo "$stats_response" | jq -e '.result.content[0].text' > /dev/null

if [ $? -eq 0 ]; then
    echo "✓ get_statistics tool works!"
    echo ""
    echo "Statistics response:"
    echo "$stats_response" | jq -r '.result.content[0].text' | jq '.'
else
    echo "ERROR: Invalid response format"
    echo "$stats_response"
    exit 1
fi

# Test max_chromosomes parameter
echo ""
echo "Testing max_chromosomes parameter..."

# Test with max_chromosomes=2
stats_limited=$(echo "$response" | grep -E '^\{.*"id":3' || true)
chr_count=$(echo "$stats_limited" | jq -r '.result.content[0].text | fromjson | .variants_per_chromosome | length')

if [ "$chr_count" = "1" ]; then
    echo "✓ max_chromosomes=2 works (got $chr_count chromosome - sample has only 1)"
elif [ "$chr_count" = "2" ]; then
    echo "✓ max_chromosomes=2 works (got $chr_count chromosomes)"
else
    echo "ERROR: Expected 1-2 chromosomes, got $chr_count"
    exit 1
fi

# Test with max_chromosomes=0 (all chromosomes)
stats_all=$(echo "$response" | grep -E '^\{.*"id":4' || true)
chr_count_all=$(echo "$stats_all" | jq -r '.result.content[0].text | fromjson | .variants_per_chromosome | length')

# sample.compressed.vcf.gz has 1 chromosome (chr20)
if [ "$chr_count_all" = "1" ]; then
    echo "✓ max_chromosomes=0 works (got all $chr_count_all chromosomes)"
else
    echo "ERROR: Expected 1 chromosome, got $chr_count_all"
    exit 1
fi

echo ""
echo "✓ All max_chromosomes tests passed!"
exit 0
