#!/bin/bash

# Simple E2E test for all VCF MCP Server tools
# Uses named pipes for deterministic stdio request/response ordering

set -euo pipefail

VCF_FILE="sample_data/sample.compressed.vcf.gz"
BINARY="./target/release/vcf_mcp_server"
SERVER_IN="/tmp/vcf_mcp_tools_in_$$"
SERVER_OUT="/tmp/vcf_mcp_tools_out_$$"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}Building server...${NC}"
cargo build --release --quiet 2>&1 | grep -v "warning:" || true

if [ ! -f "$VCF_FILE" ]; then
    echo -e "${RED}Error: VCF file not found: $VCF_FILE${NC}"
    exit 1
fi

echo -e "\n${BLUE}======================================${NC}"
echo -e "${BLUE}  Testing All MCP Tools${NC}"
echo -e "${BLUE}======================================${NC}"

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
failures=0

responses=()

send_and_read() {
    local request="$1"
    echo "$request" >&3
    local response
    IFS= read -r -t 5 response <&4 || {
        echo -e "${RED}✗ Timed out waiting for response${NC}"
        exit 1
    }
    responses+=("$response")
}

send_and_read '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}'
echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' >&3
sleep 0.1
send_and_read '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
send_and_read '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query_by_position","arguments":{"chromosome":"20","position":14370}}}'
send_and_read '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"query_by_id","arguments":{"id":"rs6054257"}}}'
send_and_read '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"get_vcf_header","arguments":{}}}'
send_and_read '{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"start_region_query","arguments":{"chromosome":"20","start":14000,"end":18000,"filter":""}}}'
send_and_read '{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"get_documentation","arguments":{"doc_type":"readme"}}}'
send_and_read '{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"get_statistics","arguments":{}}}'
send_and_read '{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":1}}}'
send_and_read '{"jsonrpc":"2.0","id":11,"method":"resources/list","params":{}}'
send_and_read '{"jsonrpc":"2.0","id":12,"method":"resources/read","params":{"uri":"vcf://metadata"}}'

json_lines=$(printf '%s\n' "${responses[@]}")

# Count the number of JSON responses (should be 12: init + 11 requests)
response_count=$(echo "$json_lines" | jq -r '.id // empty' | sort -nu | wc -l)

echo -e "\n${BLUE}Test: JSON Response Count${NC}"
if [ "$response_count" -eq "11" ]; then
    echo -e "${GREEN}✓ Received 11 JSON responses${NC}"
else
    echo -e "${RED}✗ Expected 11 responses, got $response_count${NC}"
    failures=$((failures + 1))
fi

# Extract the tools list response (id:2)
tools_response=$(echo "$json_lines" | jq -c 'select(.id == 2)' || true)
if [ ! -z "$tools_response" ]; then
    tool_count=$(echo "$tools_response" | jq -r '.result.tools | length' 2>/dev/null || echo "0")
    echo -e "\n${BLUE}Test: Tool Count${NC}"
    if [ "$tool_count" -eq "8" ]; then
        echo -e "${GREEN}✓ Found 8 tools${NC}"
        echo "$tools_response" | jq -r '.result.tools[].name' | while read -r tool; do
            echo "  - $tool"
        done
    else
        echo -e "${RED}✗ Expected 8 tools, found $tool_count${NC}"
        failures=$((failures + 1))
    fi
fi

# Check query_by_position response (id:3)
position_response=$(echo "$json_lines" | jq -c 'select(.id == 3)' || true)
if [ ! -z "$position_response" ]; then
    echo -e "\n${BLUE}Test: query_by_position${NC}"
    if echo "$position_response" | jq -e '.result.content[0].text | fromjson | .result.count > 0' >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Found variant at position${NC}"
    else
        echo -e "${RED}✗ No variant found at position${NC}"
        failures=$((failures + 1))
    fi
fi

# Check query_by_id response (id:5)
id_response=$(echo "$json_lines" | jq -c 'select(.id == 5)' || true)
if [ ! -z "$id_response" ]; then
    echo -e "\n${BLUE}Test: query_by_id${NC}"
    if echo "$id_response" | jq -e '.result.content[0].text | fromjson | .result.count > 0' >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Found variant by ID${NC}"
    else
        echo -e "${RED}✗ No variant found by ID${NC}"
        failures=$((failures + 1))
    fi
fi

# Check get_vcf_header response (id:6)
header_response=$(echo "$json_lines" | jq -c 'select(.id == 6)' || true)
if [ ! -z "$header_response" ]; then
    echo -e "\n${BLUE}Test: get_vcf_header${NC}"
    if echo "$header_response" | jq -r '.result.content[0].text | fromjson | .header' | grep -q "##fileformat=VCF" 2>/dev/null; then
        echo -e "${GREEN}✓ VCF header retrieved${NC}"
    else
        echo -e "${RED}✗ Failed to get VCF header${NC}"
        failures=$((failures + 1))
    fi
fi

# Check start_region_query response (id:7)
stream_response=$(echo "$json_lines" | jq -c 'select(.id == 7)' || true)
if [ ! -z "$stream_response" ]; then
    echo -e "\n${BLUE}Test: start_region_query${NC}"
    variant_count=$(echo "$stream_response" | jq -r '.result.content[0].text | fromjson | .variants | length' 2>/dev/null || echo "0")
    if [ "$variant_count" -gt "0" ]; then
        echo -e "${GREEN}✓ Streaming returned $variant_count variant(s)${NC}"
    else
        echo -e "${RED}✗ start_region_query returned no variants${NC}"
        failures=$((failures + 1))
    fi
fi

# Check get_documentation response (id:8)
doc_response=$(echo "$json_lines" | jq -c 'select(.id == 8)' || true)
if [ ! -z "$doc_response" ]; then
    echo -e "\n${BLUE}Test: get_documentation${NC}"
    if echo "$doc_response" | jq -r '.result.content[0].text | fromjson | .content' | grep -q "VCF MCP Server" 2>/dev/null; then
        echo -e "${GREEN}✓ Documentation retrieved${NC}"
    else
        echo -e "${RED}✗ Failed to get documentation${NC}"
        failures=$((failures + 1))
    fi
fi

# Check resources/list response (id:11)
resources_response=$(echo "$json_lines" | jq -c 'select(.id == 11)' || true)
if [ ! -z "$resources_response" ]; then
    echo -e "\n${BLUE}Test: resources/list${NC}"
    resource_count=$(echo "$resources_response" | jq -r '.result.resources | length' 2>/dev/null || echo "0")
    if [ "$resource_count" -ge "1" ]; then
        echo -e "${GREEN}✓ Found $resource_count resource(s)${NC}"
    else
        echo -e "${RED}✗ No resources found${NC}"
        failures=$((failures + 1))
    fi
fi

# Check resources/read response (id:12)
read_response=$(echo "$json_lines" | jq -c 'select(.id == 12)' || true)
if [ ! -z "$read_response" ]; then
    echo -e "\n${BLUE}Test: resources/read (vcf://metadata)${NC}"
    if echo "$read_response" | jq -e '.result.contents[0].text | fromjson | .file_format' >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Resource read successfully${NC}"
    else
        echo -e "${RED}✗ Failed to read resource${NC}"
        failures=$((failures + 1))
    fi
fi

# Check get_statistics response (id:9)
stats_response=$(echo "$json_lines" | jq -c 'select(.id == 9)' || true)
if [ ! -z "$stats_response" ]; then
    echo -e "\n${BLUE}Test: get_statistics (default)${NC}"
    total_variants=$(echo "$stats_response" | jq -r '.result.content[0].text | fromjson | .total_variants' 2>/dev/null || echo "0")
    if [ "$total_variants" -gt "0" ]; then
        echo -e "${GREEN}✓ Statistics returned ($total_variants variants)${NC}"
    else
        echo -e "${RED}✗ No statistics returned${NC}"
        failures=$((failures + 1))
    fi
fi

# Check get_statistics with max_chromosomes response (id:10)
stats_limited_response=$(echo "$json_lines" | jq -c 'select(.id == 10)' || true)
if [ ! -z "$stats_limited_response" ]; then
    echo -e "\n${BLUE}Test: get_statistics (max_chromosomes=1)${NC}"
    chr_count=$(echo "$stats_limited_response" | jq -r '.result.content[0].text | fromjson | .variants_per_chromosome | length' 2>/dev/null || echo "0")
    if [ "$chr_count" -eq "1" ]; then
        echo -e "${GREEN}✓ Chromosome limiting works (got $chr_count chromosome)${NC}"
    else
        echo -e "${RED}✗ Expected 1 chromosome, got $chr_count${NC}"
        failures=$((failures + 1))
    fi
fi

echo -e "\n${BLUE}======================================${NC}"
echo -e "${GREEN}Tool testing complete!${NC}"
echo -e "${BLUE}======================================${NC}"

if [ "$failures" -gt 0 ]; then
    exit 1
fi
