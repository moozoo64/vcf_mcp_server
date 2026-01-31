#!/bin/bash

# Simple E2E test for all VCF MCP Server tools
# Uses echo/heredoc instead of named pipes for simplicity

set -e

VCF_FILE="sample_data/sample.compressed.vcf.gz"
BINARY="./target/release/vcf_mcp_server"

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

# Test with timeout to prevent hanging
response=$(timeout 5 $BINARY "$VCF_FILE" 2>/dev/null <<'EOF' || true
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query_by_position","arguments":{"chromosome":"20","position":14370}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query_by_region","arguments":{"chromosome":"20","start":14000,"end":18000}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"query_by_id","arguments":{"id":"rs6054257"}}}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"get_vcf_header","arguments":{}}}
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"start_region_query","arguments":{"chromosome":"20","start":14000,"end":18000,"filter":""}}}
{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"get_documentation","arguments":{"doc_type":"readme"}}}
{"jsonrpc":"2.0","id":9,"method":"resources/list","params":{}}
{"jsonrpc":"2.0","id":10,"method":"resources/read","params":{"uri":"vcf://header"}}
EOF
)

# Filter out non-JSON lines (loading messages)
json_lines=$(echo "$response" | grep -E '^\{' || true)

# Count the number of JSON responses (should be 10: init + 9 requests)
response_count=$(echo "$json_lines" | wc -l)

echo -e "\n${BLUE}Test: JSON Response Count${NC}"
if [ "$response_count" -eq "10" ]; then
    echo -e "${GREEN}✓ Received 10 JSON responses${NC}"
else
    echo -e "${RED}✗ Expected 10 responses, got $response_count${NC}"
fi

# Extract the tools list response (id:2)
tools_response=$(echo "$json_lines" | grep '"id":2' || true)
if [ ! -z "$tools_response" ]; then
    tool_count=$(echo "$tools_response" | jq -r '.result.tools | length' 2>/dev/null || echo "0")
    echo -e "\n${BLUE}Test: Tool Count${NC}"
    if [ "$tool_count" -eq "9" ]; then
        echo -e "${GREEN}✓ Found 9 tools${NC}"
        echo "$tools_response" | jq -r '.result.tools[].name' | while read -r tool; do
            echo "  - $tool"
        done
    else
        echo -e "${RED}✗ Expected 9 tools, found $tool_count${NC}"
    fi
fi

# Check query_by_position response (id:3)
position_response=$(echo "$json_lines" | grep '"id":3' || true)
if [ ! -z "$position_response" ]; then
    echo -e "\n${BLUE}Test: query_by_position${NC}"
    if echo "$position_response" | jq -e '.result.content[0].text' | jq -e '.result.count > 0' >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Found variant at position${NC}"
    else
        echo -e "${RED}✗ No variant found at position${NC}"
    fi
fi

# Check query_by_region response (id:4)
region_response=$(echo "$json_lines" | grep '"id":4' || true)
if [ ! -z "$region_response" ]; then
    echo -e "\n${BLUE}Test: query_by_region${NC}"
    if echo "$region_response" | jq -e '.result.content[0].text' | jq -e '.result.count > 0' >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Found variants in region${NC}"
    else
        echo -e "${RED}✗ No variants found in region${NC}"
    fi
fi

# Check query_by_id response (id:5)
id_response=$(echo "$json_lines" | grep '"id":5' || true)
if [ ! -z "$id_response" ]; then
    echo -e "\n${BLUE}Test: query_by_id${NC}"
    if echo "$id_response" | jq -e '.result.content[0].text' | jq -e '.result.count > 0' >/dev/null 2>&1; then
        echo -e "${GREEN}✓ Found variant by ID${NC}"
    else
        echo -e "${RED}✗ No variant found by ID${NC}"
    fi
fi

# Check get_vcf_header response (id:6)
header_response=$(echo "$json_lines" | grep '"id":6' || true)
if [ ! -z "$header_response" ]; then
    echo -e "\n${BLUE}Test: get_vcf_header${NC}"
    if echo "$header_response" | jq -e '.result.content[0].text' | grep -q "##fileformat=VCF" 2>/dev/null; then
        echo -e "${GREEN}✓ VCF header retrieved${NC}"
    else
        echo -e "${RED}✗ Failed to get VCF header${NC}"
    fi
fi

# Check start_region_query response (id:7)
stream_response=$(echo "$json_lines" | grep '"id":7' || true)
if [ ! -z "$stream_response" ]; then
    echo -e "\n${BLUE}Test: start_region_query${NC}"
    session_id=$(echo "$stream_response" | jq -r '.result.content[0].text' | jq -r '.session_id' 2>/dev/null || echo "")
    if [ ! -z "$session_id" ] && [ "$session_id" != "null" ]; then
        echo -e "${GREEN}✓ Streaming session created${NC}"
    else
        echo -e "${RED}✗ Failed to create streaming session${NC}"
    fi
fi

# Check get_documentation response (id:8)
doc_response=$(echo "$json_lines" | grep '"id":8' || true)
if [ ! -z "$doc_response" ]; then
    echo -e "\n${BLUE}Test: get_documentation${NC}"
    if echo "$doc_response" | jq -e '.result.content[0].text' | grep -q "VCF MCP Server" 2>/dev/null; then
        echo -e "${GREEN}✓ Documentation retrieved${NC}"
    else
        echo -e "${RED}✗ Failed to get documentation${NC}"
    fi
fi

# Check resources/list response (id:9)
resources_response=$(echo "$json_lines" | grep '"id":9' || true)
if [ ! -z "$resources_response" ]; then
    echo -e "\n${BLUE}Test: resources/list${NC}"
    resource_count=$(echo "$resources_response" | jq -r '.result.resources | length' 2>/dev/null || echo "0")
    if [ "$resource_count" -ge "1" ]; then
        echo -e "${GREEN}✓ Found $resource_count resource(s)${NC}"
    else
        echo -e "${RED}✗ No resources found${NC}"
    fi
fi

# Check resources/read response (id:10)
read_response=$(echo "$json_lines" | grep '"id":10' || true)
if [ ! -z "$read_response" ]; then
    echo -e "\n${BLUE}Test: resources/read (vcf://header)${NC}"
    if echo "$read_response" | jq -e '.result.contents[0].text' | grep -q "##fileformat=VCF" 2>/dev/null; then
        echo -e "${GREEN}✓ Resource read successfully${NC}"
    else
        echo -e "${RED}✗ Failed to read resource${NC}"
    fi
fi

echo -e "\n${BLUE}======================================${NC}"
echo -e "${GREEN}Tool testing complete!${NC}"
echo -e "${BLUE}======================================${NC}"
