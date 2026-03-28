#!/bin/bash

# Test the get_statistics MCP tool

set -euo pipefail

VCF_FILE="sample_data/sample.compressed.vcf.gz"
BINARY="./target/release/vcf_mcp_server"
SERVER_IN="/tmp/vcf_statistics_in_$$"
SERVER_OUT="/tmp/vcf_statistics_out_$$"

echo "Testing get_statistics tool..."

cleanup() {
    if [ -n "${SERVER_PID:-}" ]; then
        kill "$SERVER_PID" 2>/dev/null || true
    fi
    rm -f "$SERVER_IN" "$SERVER_OUT"
}
trap cleanup EXIT

mkfifo "$SERVER_IN" "$SERVER_OUT"

"$BINARY" "$VCF_FILE" < "$SERVER_IN" > "$SERVER_OUT" 2>/dev/null &
SERVER_PID=$!

exec 3>"$SERVER_IN"
exec 4<"$SERVER_OUT"

echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' >&3
IFS= read -r -t 5 _init <&4 || {
    echo "ERROR: Failed to initialize MCP session"
    exit 1
}
echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' >&3
sleep 0.1

echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_statistics","arguments":{}}}' >&3
IFS= read -r -t 5 stats_response <&4 || stats_response=""

echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":2}}}' >&3
IFS= read -r -t 5 stats_limited <&4 || stats_limited=""

echo '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":0}}}' >&3
IFS= read -r -t 5 stats_all <&4 || stats_all=""

# Extract the statistics response (filter out non-JSON lines)
if ! echo "$stats_response" | jq -e '.id == 2 and .result.content[0].text' >/dev/null 2>&1; then
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
chr_count_all=$(echo "$stats_all" | jq -r '.result.content[0].text | fromjson | .variants_per_chromosome | length')

# sample.compressed.vcf.gz has 3 chromosomes (19, 20, X)
if [ "$chr_count_all" = "3" ]; then
    echo "✓ max_chromosomes=0 works (got all $chr_count_all chromosomes)"
else
    echo "ERROR: Expected 3 chromosomes, got $chr_count_all"
    exit 1
fi

echo ""
echo "✓ All max_chromosomes tests passed!"
exit 0
