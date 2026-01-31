#!/bin/bash

# Test the get_statistics MCP tool

set -e

VCF_FILE="sample_data/sample.compressed.vcf.gz"
BINARY="./target/release/vcf_mcp_server"

echo "Testing get_statistics tool..."

response=$(timeout 5 $BINARY "$VCF_FILE" 2>/dev/null <<'EOF'
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_statistics","arguments":{}}}
EOF
)

# Extract the statistics response
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
    exit 0
else
    echo "ERROR: Invalid response format"
    echo "$stats_response"
    exit 1
fi
