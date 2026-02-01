#!/bin/bash
# Test statistics chromosome limiting

VCF_FILE="sample_data/sample.compressed.vcf.gz"
SERVER="./target/release/vcf_mcp_server"

echo "Testing statistics with default limit (25 chromosomes)..."
(
  echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}'
  echo '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_statistics","arguments":{}}}'
  sleep 1
) | $SERVER "$VCF_FILE" 2>&1 | grep -o '"id":2' -A 5000 | jq -s '.[0].result.content[0].text | fromjson | .variants_per_chromosome | length' 2>/dev/null || echo "Parse error"

echo ""
echo "Testing statistics with max_chromosomes=10..."
(
  echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}'
  echo '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":10}}}'
  sleep 1
) | $SERVER "$VCF_FILE" 2>&1 | grep -o '"id":3' -A 5000 | jq -s '.[0].result.content[0].text | fromjson | .variants_per_chromosome | length' 2>/dev/null || echo "Parse error"

echo ""
echo "Testing statistics with max_chromosomes=0 (all)..."
(
  echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}'
  echo '{"jsonrpc":"2.0","method":"notifications/initialized"}'
  echo '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"get_statistics","arguments":{"max_chromosomes":0}}}'
  sleep 1
) | $SERVER "$VCF_FILE" 2>&1 | grep -o '"id":4' -A 5000 | jq -s '.[0].result.content[0].text | fromjson | .variants_per_chromosome | length' 2>/dev/null || echo "Parse error"
