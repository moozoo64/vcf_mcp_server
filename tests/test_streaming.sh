#!/bin/bash
# Test script for streaming query functionality

set -euo pipefail

VCF_FILE="sample_data/sample.compressed.vcf.gz"
SERVER="./target/release/vcf_mcp_server"
SERVER_IN="/tmp/vcf_streaming_in_$$"
SERVER_OUT="/tmp/vcf_streaming_out_$$"

# Build if needed
if [ ! -f "$SERVER" ]; then
    echo "Building server..."
    cargo build --release
fi

echo "Testing streaming query tools..."
echo ""

cleanup() {
    if [ -n "${SERVER_PID:-}" ]; then
        kill "$SERVER_PID" 2>/dev/null || true
    fi
    rm -f "$SERVER_IN" "$SERVER_OUT"
}
trap cleanup EXIT

mkfifo "$SERVER_IN" "$SERVER_OUT"

"$SERVER" "$VCF_FILE" < "$SERVER_IN" > "$SERVER_OUT" 2>/dev/null &
SERVER_PID=$!

exec 3>"$SERVER_IN"
exec 4<"$SERVER_OUT"

echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1.0"}}}' >&3
IFS= read -r -t 5 _init_response <&4 || {
    echo "ERROR: Failed to initialize MCP session"
    exit 1
}
echo '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' >&3
sleep 0.1

echo '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"start_region_query","arguments":{"chromosome":"20","start":14000,"end":18000,"filter":""}}}' >&3
IFS= read -r -t 5 stream_response <&4 || stream_response=""

if [ -z "$stream_response" ]; then
    echo "ERROR: No response from start_region_query"
    exit 1
fi

session_id=$(echo "$stream_response" | jq -r '.result.content[0].text | fromjson | .session_id' 2>/dev/null || echo "")
if [ -n "$session_id" ] && [ "$session_id" != "null" ]; then
    echo "✓ start_region_query created session: $session_id"
else
    echo "ERROR: start_region_query did not return a valid session_id"
    exit 1
fi

echo ""
echo "Streaming tools available:"
echo "  - start_region_query: Start a streaming query session"
echo "  - get_next_variant: Get next variant from session"
echo "  - close_query_session: Close active session"
echo ""
echo "Test complete! Use an MCP client (Claude Desktop, etc.) to test interactively."
