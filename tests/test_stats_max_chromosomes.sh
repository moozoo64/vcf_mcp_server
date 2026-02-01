#!/bin/bash
# Simple test for get_statistics with max_chromosomes parameter

set -e

VCF_FILE="sample_data/sample.compressed.vcf.gz"
BINARY="./target/release/vcf_mcp_server"

GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}Testing get_statistics tool with sample data...${NC}"
echo ""

# Start server in background and communicate via named pipe
FIFO=$(mktemp -u)
mkfifo "$FIFO"

# Start server with output redirected
$BINARY "$VCF_FILE" > "$FIFO" 2>&1 &
SERVER_PID=$!

# Give server time to start
sleep 1

# Send requests
exec 3>"$FIFO"
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' >&3
echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' >&3
echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_statistics","arguments":{}}}' >&3
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":1}}}' >&3

# Wait a bit for responses
sleep 2

# Kill server
kill $SERVER_PID 2>/dev/null || true
wait $SERVER_PID 2>/dev/null || true

# Read responses
exec 3<"$FIFO"
responses=$(cat <&3)
rm "$FIFO"

# Check responses
echo "$responses" | grep -q '"id":2' && echo -e "${GREEN}✓ Default get_statistics works${NC}" || echo -e "${RED}✗ Default get_statistics failed${NC}"
echo "$responses" | grep -q '"id":3' && echo -e "${GREEN}✓ get_statistics with max_chromosomes works${NC}" || echo -e "${RED}✗ get_statistics with max_chromosomes failed${NC}"

chr_count=$(echo "$responses" | grep '"id":3' | jq -r '.result.content[0].text | fromjson | .variants_per_chromosome | length' 2>/dev/null || echo "0")
if [ "$chr_count" = "1" ]; then
    echo -e "${GREEN}✓ Chromosome limiting verified (got $chr_count chromosome)${NC}"
else
    echo -e "${RED}✗ Expected 1 chromosome, got $chr_count${NC}"
fi

echo ""
echo -e "${GREEN}Testing complete!${NC}"
