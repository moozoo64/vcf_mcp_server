#!/bin/bash
cargo run sample_data/sample.compressed.vcf.gz 2>/dev/null << 'INPUT' | tail -1 | python3 -m json.tool 2>/dev/null
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"query_by_id","arguments":{"id":"rs670874"}}}
INPUT
