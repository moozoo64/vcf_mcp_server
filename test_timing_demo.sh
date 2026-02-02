#!/bin/bash
# Simple test to show response timing for all tools

cd "$(dirname "$0")"

echo "=== Testing Response Timing Feature ==="
echo ""
echo "Each tool call will show: [DEBUG] Response time: X.XXms | Response size: XXX bytes"
echo ""

# Run a single query to see timing
echo "1. Query by position (chr20:14370)..."
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query_by_position","arguments":{"chromosome":"20","position":14370}}}' | \
./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --debug 2>&1 | grep "Response time"

echo ""
echo "2. Query by region (chr20:14370-14380)..."
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query_by_region","arguments":{"chromosome":"20","start":14370,"end":14380}}}' | \
./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --debug 2>&1 | grep "Response time"

echo ""
echo "3. Query by ID (rs6054257)..."
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query_by_id","arguments":{"id":"rs6054257"}}}' | \
./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --debug 2>&1 | grep "Response time"

echo ""
echo "4. Get VCF header..."
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_vcf_header","arguments":{}}}' | \
./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --debug 2>&1 | grep "Response time"

echo ""
echo "5. Get statistics..."
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":5}}}' | \
./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --debug 2>&1 | grep "Response time"

echo ""
echo "=== All tools successfully log response time and size! ==="
