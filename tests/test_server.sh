#!/bin/bash

# Test script for VCF MCP Server - Tests MCP protocol functionality

set -e  # Exit on error

VCF_FILE="sample_data/sample.compressed.vcf.gz"
BINARY="./target/release/vcf_mcp_server"
TEST_LOG="test_output.log"
SERVER_IN="/tmp/vcf_mcp_test_in_$$"
SERVER_OUT="/tmp/vcf_mcp_test_out_$$"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Cleanup function
cleanup() {
    if [ ! -z "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
    fi
    rm -f "$SERVER_IN" "$SERVER_OUT"
}
trap cleanup EXIT

echo -e "${BLUE}Building server...${NC}"
cargo build --release --quiet

if [ ! -f "$VCF_FILE" ]; then
    echo -e "${RED}Error: VCF file not found: $VCF_FILE${NC}"
    exit 1
fi

# Create named pipes
mkfifo "$SERVER_IN" "$SERVER_OUT"

# Start server in background
$BINARY "$VCF_FILE" < "$SERVER_IN" > "$SERVER_OUT" 2>/dev/null &
SERVER_PID=$!

# Keep the input pipe open by redirecting from fd 3
exec 3>"$SERVER_IN"

# Give server time to start
sleep 0.5

# Function to send MCP request and get response via the running server
send_mcp_request() {
    local request="$1"
    local description="$2"

    echo -e "\n${BLUE}Test: $description${NC}" >&2

    # Send request to server
    echo "$request" >&3

    # Read response from server (with timeout)
    local response
    if ! response=$(head -1 < "$SERVER_OUT"); then
        echo -e "${RED}✗ Failed to read response${NC}" >&2
        exit 1
    fi

    # Check if response is valid JSON
    if ! echo "$response" | jq empty 2>/dev/null; then
        echo -e "${RED}✗ Invalid JSON response${NC}" >&2
        echo "Response: $response" >&2
        exit 1
    fi

    echo -e "${GREEN}✓ Valid JSON response${NC}" >&2

    # Return the response for further processing (to stdout)
    echo "$response"
}

# Function to validate response has no error
check_no_error() {
    local response="$1"
    if echo "$response" | jq -e '.error' >/dev/null 2>&1; then
        echo -e "${RED}✗ Response contains error${NC}"
        echo "$response" | jq '.error'
        return 1
    else
        echo -e "${GREEN}✓ No error in response${NC}"
    fi
}

echo -e "\n${BLUE}======================================${NC}"
echo -e "${BLUE}  MCP Protocol Tests${NC}"
echo -e "${BLUE}======================================${NC}"

# Test 1: Initialize
response=$(send_mcp_request \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test-client","version":"1.0"}}}' \
    "Initialize handshake")
check_no_error "$response"

# Test 2: List tools
response=$(send_mcp_request \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
    "List available tools")
check_no_error "$response"

# Check that we have 3 tools
tool_count=$(echo "$response" | jq '.result.tools | length')
if [ "$tool_count" == "3" ]; then
    echo -e "${GREEN}✓ Found 3 tools${NC}"
else
    echo -e "${RED}✗ Expected 3 tools, found $tool_count${NC}"
fi

# Display tool names
echo "Available tools:"
echo "$response" | jq -r '.result.tools[].name' | while read -r tool; do
    echo "  - $tool"
done

# Test 3: Call tool - query_by_position
response=$(send_mcp_request \
    '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query_by_position","arguments":{"chromosome":"20","position":14370}}}' \
    "Query by position (chr20:14370)")
check_no_error "$response"

# Check if we found variants
if echo "$response" | jq -e '.result.content[0].text' | grep -q "Found"; then
    echo -e "${GREEN}✓ Found variant(s)${NC}"
else
    echo -e "${RED}✗ No variants found${NC}"
fi

# Test 4: Call tool - query_by_region
response=$(send_mcp_request \
    '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query_by_region","arguments":{"chromosome":"20","start":14000,"end":18000}}}' \
    "Query by region (chr20:14000-18000)")
check_no_error "$response"

# Test 5: Call tool - query_by_id
response=$(send_mcp_request \
    '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"query_by_id","arguments":{"id":"rs6054257"}}}' \
    "Query by ID (rs6054257)")
check_no_error "$response"

# Test 6: Query with no results
response=$(send_mcp_request \
    '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"query_by_position","arguments":{"chromosome":"99","position":99999}}}' \
    "Query with no results (chr99:99999)")
check_no_error "$response"

if echo "$response" | jq -e '.result.content[0].text' | grep -q "No variants found"; then
    echo -e "${GREEN}✓ Correctly reports no variants found${NC}"
else
    echo -e "${RED}✗ Expected 'No variants found' message${NC}"
fi

echo -e "\n${BLUE}======================================${NC}"
echo -e "${GREEN}All tests completed!${NC}"
echo -e "${BLUE}======================================${NC}"
