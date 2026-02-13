#!/bin/bash
# Simple non-hanging test for get_statistics

set -euo pipefail

VCF_FILE="sample_data/sample.compressed.vcf.gz"
BINARY="./target/release/vcf_mcp_server"
SERVER_IN="/tmp/vcf_simple_stats_in_$$"
SERVER_OUT="/tmp/vcf_simple_stats_out_$$"
failures=0

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo "Testing get_statistics with max_chromosomes parameter..."

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

# Initialize
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' >&3
IFS= read -r -t 5 _init <&4 || {
    echo -e "${RED}✗ Failed to initialize MCP session${NC}"
    exit 1
}
echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' >&3
sleep 0.1

# Query default statistics
echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_statistics","arguments":{}}}' >&3
IFS= read -r -t 5 stats_default <&4 || stats_default=""

# Query limited statistics
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":1}}}' >&3
IFS= read -r -t 5 stats_limited <&4 || stats_limited=""

# Check if we got responses
if echo "$stats_default" | jq -e '.id == 2 and .result.content[0].text' >/dev/null 2>&1; then
    echo -e "${GREEN}✓ get_statistics (default) works${NC}"
else
    echo -e "${RED}✗ get_statistics (default) failed${NC}"
    failures=$((failures + 1))
fi

if echo "$stats_limited" | jq -e '.id == 3 and .result.content[0].text' >/dev/null 2>&1; then
    echo -e "${GREEN}✓ get_statistics (max_chromosomes) works${NC}"
    
    # Verify chromosome count
    chr_count=$(echo "$stats_limited" | jq -r '.result.content[0].text | fromjson | .variants_per_chromosome | length' 2>/dev/null || echo "0")
    if [ "$chr_count" = "1" ]; then
        echo -e "${GREEN}✓ Chromosome limiting verified (1 chromosome as expected)${NC}"
    else
        echo -e "${RED}✗ Expected 1 chromosome, got $chr_count${NC}"
        failures=$((failures + 1))
    fi
else
    echo -e "${RED}✗ get_statistics (max_chromosomes) failed${NC}"
    failures=$((failures + 1))
fi

echo ""
echo "Test complete!"

if [ "$failures" -gt 0 ]; then
    exit 1
fi
