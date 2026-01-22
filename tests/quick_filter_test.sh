#!/bin/bash

# Quick test to verify filter error handling

cd "$(dirname "$0")/.."

echo "Testing filter error handling..."

# Test 1: Invalid field name
echo -e "\n1. Testing CHROMOSOME (should be CHROM):"
(echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}'; 
 sleep 0.2;
 echo '{"jsonrpc":"2.0","method":"notifications/initialized"}'; 
 sleep 0.2;
 echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_region_query","arguments":{"chromosome":"20","start":14370,"end":17330,"filter":"CHROMOSOME == 20"}}}';
 sleep 0.5) \
 | ./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz 2>&1 \
 | tail -1 | jq -r '.error.message' 2>/dev/null

# Test 2: Missing operator
echo -e "\n2. Testing missing operator (QUAL 30):"
(echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}'; 
 sleep 0.2;
 echo '{"jsonrpc":"2.0","method":"notifications/initialized"}'; 
 sleep 0.2;
 echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_region_query","arguments":{"chromosome":"20","start":14370,"end":17330,"filter":"QUAL 30"}}}';
 sleep 0.5) \
 | ./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz 2>&1 \
 | tail -1 | jq -r '.error.message' 2>/dev/null

# Test 3: Typo in AND expression
echo -e "\n3. Testing typo CHROMSOME in AND:"
(echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}'; 
 sleep 0.2;
 echo '{"jsonrpc":"2.0","method":"notifications/initialized"}'; 
 sleep 0.2;
 echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_region_query","arguments":{"chromosome":"20","start":14370,"end":17330,"filter":"QUAL > 20 AND CHROMSOME == 20"}}}';
 sleep 0.5) \
 | ./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz 2>&1 \
 | tail -1 | jq -r '.error.message' 2>/dev/null

# Test 4: Invalid POSITION
echo -e "\n4. Testing POSITION (should be POS):"
(echo '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}'; 
 sleep 0.2;
 echo '{"jsonrpc":"2.0","method":"notifications/initialized"}'; 
 sleep 0.2;
 echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"start_region_query","arguments":{"chromosome":"20","start":14370,"end":17330,"filter":"POSITION > 14000"}}}';
 sleep 0.5) \
 | ./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz 2>&1 \
 | tail -1 | jq -r '.error.message' 2>/dev/null

echo -e "\n✅ All filter error tests completed!"
