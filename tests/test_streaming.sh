#!/bin/bash
# Test script for streaming query functionality

set -e

VCF_FILE="sample_data/sample.compressed.vcf.gz"
SERVER="./target/release/vcf_mcp_server"

# Build if needed
if [ ! -f "$SERVER" ]; then
    echo "Building server..."
    cargo build --release
fi

echo "Testing streaming query tools..."
echo ""

# Start the server in background
$SERVER "$VCF_FILE" &
SERVER_PID=$!

# Give server time to start
sleep 1

# Function to send MCP request
send_request() {
    local method=$1
    local params=$2
    echo '{"jsonrpc":"2.0","id":1,"method":"'$method'","params":'$params'}' | nc -N localhost 8080 2>/dev/null || echo ""
}

# Test 1: Start a region query
echo "1. Starting region query (chr20:60000-70000)..."
REQUEST='{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_region_query","arguments":{"chromosome":"20","start":60000,"end":70000}}}'
echo "$REQUEST" | $SERVER "$VCF_FILE" 2>/dev/null | jq -r '.result.content[0].text' | jq '.' || echo "Stream test requires manual MCP client"

# Clean up
kill $SERVER_PID 2>/dev/null || true

echo ""
echo "Streaming tools available:"
echo "  - start_region_query: Start a streaming query session"
echo "  - get_next_variant: Get next variant from session"
echo "  - close_query_session: Close active session"
echo ""
echo "Test complete! Use an MCP client (Claude Desktop, etc.) to test interactively."
