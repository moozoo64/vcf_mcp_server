#!/bin/bash
set -euo pipefail

BINARY="./target/release/vcf_mcp_server"
VCF_FILE="sample_data/sample.compressed.vcf.gz"

if [ ! -x "$BINARY" ]; then
	cargo build --release --quiet
fi

timeout 10 "$BINARY" "$VCF_FILE" 2>/dev/null << 'INPUT' | grep '^{' | tail -1 | python3 -m json.tool
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query_by_id","arguments":{"id":"rs670874"}}}
INPUT
