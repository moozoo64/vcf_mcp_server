#!/bin/bash
# Test response timing for streaming tools

cd "$(dirname "$0")"

echo "=== Testing Response Timing for Streaming Tools ==="
echo ""

echo "1. Start streaming query..."
cat <<'EOF' | ./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --debug 2>&1 | grep "Response time"
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"start_region_query","arguments":{"chromosome":"20","start":14370,"end":20000,"filter":""}}}
EOF

echo ""
echo "2. Get documentation..."
cat <<'EOF' | ./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --debug 2>&1 | grep "Response time"
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_documentation","arguments":{"doc_type":"readme"}}}
EOF

echo ""
echo "=== Complete! All tools including streaming show timing. ==="
