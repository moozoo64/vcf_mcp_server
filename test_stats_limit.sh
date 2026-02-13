#!/bin/bash
# Test statistics chromosome limiting

set -euo pipefail

extract_json_rpc() {
python3 -c 'import json,sys
text=sys.stdin.read()
decoder=json.JSONDecoder()
i=0
while i < len(text):
  while i < len(text) and text[i] not in "[{":
    i += 1
  if i >= len(text):
    break
  try:
    obj,end = decoder.raw_decode(text, i)
    if isinstance(obj, dict) and obj.get("jsonrpc") == "2.0":
      print(json.dumps(obj))
    i = end
  except Exception:
    i += 1'
}

VCF_FILE="sample_data/sample.compressed.vcf.gz"
SERVER="./target/release/vcf_mcp_server"

echo "Testing statistics with default limit (25 chromosomes)..."
run_stats_count() {
  local request_id="$1"
  local args_json="$2"

  local output
  output=$(timeout 10 "$SERVER" "$VCF_FILE" 2>/dev/null <<EOF || true
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":$request_id,"method":"tools/call","params":{"name":"get_statistics","arguments":$args_json}}
EOF
)

    local json_lines
      json_lines=$(echo "$output" | extract_json_rpc || true)

  echo "$json_lines" | jq -r --argjson id "$request_id" 'select(.id == $id) | .result.content[0].text | fromjson | .variants_per_chromosome | length' 2>/dev/null || echo "Parse error"
}

run_stats_count 2 '{}'

echo ""
echo "Testing statistics with max_chromosomes=10..."
run_stats_count 3 '{"max_chromosomes":10}'

echo ""
echo "Testing statistics with max_chromosomes=0 (all)..."
run_stats_count 4 '{"max_chromosomes":0}'
