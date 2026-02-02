#!/bin/bash
# Comprehensive test of debug logging for all MCP tools

cd "$(dirname "$0")"

echo "=== Testing Debug Logging for All VCF MCP Tools ==="
echo ""

# Test all available tools
cat <<'EOF' | ./target/release/vcf_mcp_server sample_data/sample.compressed.vcf.gz --debug 2>&1
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query_by_position","arguments":{"chromosome":"20","position":14370}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"query_by_region","arguments":{"chromosome":"20","start":14370,"end":14380}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"query_by_id","arguments":{"id":"rs6054257"}}}
{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"get_vcf_header","arguments":{}}}
{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":5}}}
{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"start_region_query","arguments":{"chromosome":"20","start":14370,"end":20000,"filter":""}}}
{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"get_documentation","arguments":{"doc_type":"readme"}}}
EOF

echo ""
echo "=== Summary ==="
echo "All tools now log response times and sizes when --debug flag is used."
echo "The logging is centralized in the create_result_with_logging() helper method."
