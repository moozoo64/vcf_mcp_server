#!/bin/bash

# Test script to verify filter error handling via MCP protocol

set -euo pipefail

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
    if [ -n "${SERVER_PID:-}" ]; then
        kill "$SERVER_PID" 2>/dev/null || true
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
exec 4<"$SERVER_OUT"

# Give server time to start
sleep 0.5

# Initialize the MCP session
echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}' >&3
IFS= read -r -t 5 _init_response <&4 || {
    echo -e "${RED}✗ FAIL${NC} - Timed out waiting for initialize response"
    exit 1
}

# Send initialized notification
echo '{"jsonrpc":"2.0","method":"notifications/initialized"}' >&3
sleep 0.1  # Give server a moment to process

failures=0
request_id=1

test_expect_parse_error() {
    local filter="$1"
    local description="$2"
    
    echo -e "${YELLOW}Test:${NC} $description"
    echo -e "${YELLOW}Filter:${NC} '$filter'"
    
    local request
    request=$(jq -cn \
        --argjson id "$request_id" \
        --arg filter "$filter" \
        '{jsonrpc:"2.0",id:$id,method:"tools/call",params:{name:"start_region_query",arguments:{chromosome:"20",start:14370,end:17330,filter:$filter}}}')
    request_id=$((request_id + 1))
    
    # Send request
    echo "$request" >&3
    
    local response
    if ! IFS= read -r -t 5 response <&4; then
        echo -e "${RED}✗ FAIL${NC} - Timed out waiting for response"
        failures=$((failures + 1))
        echo ""
        return
    fi
    
    if echo "$response" | jq -e '.error' >/dev/null 2>&1; then
        local error_msg=$(echo "$response" | jq -r '.error.message')
        if echo "$error_msg" | grep -Eiq 'parse error|expected|invalid filter expression'; then
            echo -e "${GREEN}✓ PASS${NC} - Got expected error: $error_msg"
        else
            echo -e "${RED}✗ FAIL${NC} - Got error but not the expected one:"
            echo "  Expected parse error"
            echo "  Got: $error_msg"
            failures=$((failures + 1))
        fi
    else
        echo -e "${RED}✗ FAIL${NC} - Expected parse error but got success response"
        failures=$((failures + 1))
    fi
    echo ""
}

test_expect_handled_response() {
    local filter="$1"
    local description="$2"

    echo -e "${YELLOW}Test:${NC} $description"
    echo -e "${YELLOW}Filter:${NC} '$filter'"

    local request
    request=$(jq -cn \
        --argjson id "$request_id" \
        --arg filter "$filter" \
        '{jsonrpc:"2.0",id:$id,method:"tools/call",params:{name:"start_region_query",arguments:{chromosome:"20",start:14370,end:17330,filter:$filter}}}')
    request_id=$((request_id + 1))

    echo "$request" >&3

    local response
    if ! IFS= read -r -t 5 response <&4; then
        echo -e "${RED}✗ FAIL${NC} - Timed out waiting for response"
        failures=$((failures + 1))
        echo ""
        return
    fi

    if echo "$response" | jq -e '.error or .result' >/dev/null 2>&1; then
        echo -e "${GREEN}✓ PASS${NC} - Request handled without crash"
    else
        echo -e "${RED}✗ FAIL${NC} - Invalid JSON-RPC response"
        failures=$((failures + 1))
    fi
    echo ""
}

test_expect_handled_response \
    "CHROMOSOME == \"20\"" \
    "Invalid field name (CHROMOSOME instead of CHROM)"

test_expect_parse_error \
    "QUAL 30" \
    "Missing comparison operator"

test_expect_handled_response \
    "QUAL > 20 && CHROMSOME == \"20\"" \
    "Typo in field name within && expression (CHROMSOME)"

test_expect_handled_response \
    "POSITION > 14000 && FILTER == \"PASS\"" \
    "Invalid field name POSITION (should be POS)"

if [ "$failures" -gt 0 ]; then
    echo -e "${RED}Filter error testing complete: $failures failure(s).${NC}"
    exit 1
fi

echo -e "${GREEN}Filter error testing complete!${NC}"
