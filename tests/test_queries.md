# VCF MCP Server Test Queries

## Sample Data Summary
The server loaded 9 variants from `sample_data/sample.compressed.vcf.gz`

Based on the sample VCF content we saw earlier, here are test queries:

### 1. Query by Position
- **Chromosome 20, Position 14370**: Should find variant rs6054257 (G->A)
- **Chromosome 20, Position 1110696**: Should find variant rs6040355 (A->G,T)
- **Chromosome 19, Position 111**: Should find variant at 19:111 (A->C)

### 2. Query by Region
- **Chromosome 20, Range 14000-18000**: Should find variants at positions 14370 and 17330
- **Chromosome 20, Range 1110000-1240000**: Should find variants at 1110696, 1230237, 1234567, 1235237

### 3. Query by ID
- **ID rs6054257**: Should find variant at 20:14370
- **ID rs6040355**: Should find variant at 20:1110696
- **ID microsat1**: Should find variant at 20:1234567
- **ID rsTest**: Should find variant at X:10

## Expected Variant Fields
Each variant should return:
- chromosome
- position
- id
- reference allele
- alternate allele(s)
- quality score
- filter status
- info fields

## Integration with MCP
This server can be integrated with Claude Desktop or other MCP clients by adding to the MCP config:

```json
{
  "mcpServers": {
    "vcf": {
      "command": "/path/to/vcf_mcp_server",
      "args": ["/path/to/your/file.vcf.gz"]
    }
  }
}
```

Then the LLM can query variants using natural language like:
- "What variants are at position 20:14370?"
- "Show me all variants in the region chr20:1110000-1240000"
- "Find variant with ID rs6054257"
