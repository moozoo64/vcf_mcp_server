#!/bin/bash
# Test debug logging functionality

cd "$(dirname "$0")"

echo "Testing debug mode response size logging..."
echo ""

# Create a test input that does a full MCP handshake and query
cat <<EOF | ./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --debug 2>&1 | head -60
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test-client","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query_by_position","arguments":{"chromosome":"20","position":14370}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":5}}}
EOF

echo ""
echo "Test complete. Look for '[DEBUG] Response size:' lines above."
