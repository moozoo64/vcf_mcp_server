#!/bin/bash

# Test script to verify filter error handling via MCP protocol

set -e

VCF_PATH="sample_data/sample.compressed.vcf.gz"
SERVER_BIN="./target/release/vcf_mcp_server"
SERVER_IN="/tmp/vcf_filter_test_in_$$"
SERVER_OUT="/tmp/vcf_filter_test_out_$$"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Cleanup function
cleanup() {
    if [ ! -z "$SERVER_PID" ]; then
        kill $SERVER_PID 2>/dev/null || true
    fi
    rm -f "$SERVER_IN" "$SERVER_OUT"
}
trap cleanup EXIT

echo -e "${YELLOW}Testing Filter Error Handling${NC}\n"

# Create named pipes
mkfifo "$SERVER_IN" "$SERVER_OUT"

# Start server in background
$SERVER_BIN "$VCF_PATH" < "$SERVER_IN" > "$SERVER_OUT" 2>/dev/null &
SERVER_PID=$!

# Keep the input pipe open
exec 3>"$SERVER_IN"

# Give server time to start
sleep 0.5

# Initialize the MCP session
echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}' >&3
head -1 < "$SERVER_OUT" > /dev/null  # Consume init response

# Send initialized notification
echo '{"jsonrpc":"2.0","method":"notifications/initialized"}' >&3
sleep 0.1  # Give server a moment to process

# Helper function to send request and check for error
test_filter_error() {
    local filter="$1"
    local expected_error="$2"
    local description="$3"
    
    echo -e "${YELLOW}Test:${NC} $description"
    echo -e "${YELLOW}Filter:${NC} '$filter'"
    
    # Create MCP request
    local request=$(cat <<EOF
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_region_query","arguments":{"chromosome":"20","start":14370,"end":17330,"filter":"$filter"}}}
EOF
)
    
    # Send request
    echo "$request" >&3
    
    # Read response
    local response=$(head -1 < "$SERVER_OUT")
    
    # Check if response contains error
    if echo "$response" | jq -e '.error' >/dev/null 2>&1; then
        local error_msg=$(echo "$response" | jq -r '.error.message')
        if echo "$error_msg" | grep -q "$expected_error"; then
            echo -e "${GREEN}✓ PASS${NC} - Got expected error: $error_msg"
        else
            echo -e "${RED}✗ FAIL${NC} - Got error but not the expected one:"
            echo "  Expected: $expected_error"
            echo "  Got: $error_msg"
        fi
    else
        echo -e "${RED}✗ FAIL${NC} - Expected error but got success response"
    fi
    echo ""
}

# Test 1: Invalid field name
test_filter_error \
    "CHROMOSOME == 20" \
    "Unsupported field" \
    "Invalid field name (CHROMOSOME instead of CHROM)"

# Test 2: Missing operator
test_filter_error \
    "QUAL 30" \
    "Unsupported operator" \
    "Missing comparison operator"

# Test 3: Invalid field in AND expression
test_filter_error \
    "QUAL > 20 AND CHROMSOME == 20" \
    "Unsupported field" \
    "Typo in field name within AND expression (CHROMSOME)"

# Test 4: Invalid field in complex expression
test_filter_error \
    "POSITION > 14000 AND FILTER == PASS" \
    "Unsupported field" \
    "Invalid field name POSITION (should be POS)"

echo -e "${GREEN}Filter error testing complete!${NC}"
