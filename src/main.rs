mod response_budget;
mod shutdown;
mod vcf;

use clap::Parser;
use response_budget::{DEFAULT_MAX_RESPONSE_BYTES, FittedVariants, fit_variants, variant_budget};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext, wrapper::Parameters},
    model::*,
    schemars,
    service::RequestContext,
    tool, tool_router,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use vcf::{Variant, VcfIndex, format_variant, load_vcf};
use vcf_filter::docs as filter_docs;

// Embed documentation at compile time
const README_DOCS: &str = include_str!("../README.md");
const STREAMING_DOCS: &str = include_str!("../STREAMING.md");
const FILTER_DOCS: &str = include_str!("../FILTER_EXAMPLES.md");
const STREAMING_FILTER_DOCS: &str = include_str!("../STREAMING_FILTER_EXAMPLES.md");

// CLI arguments
#[derive(Parser, Debug)]
#[command(name = "vcf_mcp_server")]
#[command(about = "VCF MCP Server - expose VCF files via MCP protocol", long_about = None)]
struct Args {
    /// Path to the VCF file
    vcf_file: PathBuf,

    /// Run HTTP server on specified address (e.g., 127.0.0.1:8090)
    #[arg(long, value_name = "ADDR:PORT")]
    sse: Option<String>,

    /// Disable Host header validation so any host may connect (HTTP mode only).
    /// By default only localhost, 127.0.0.1 and ::1 are accepted.
    #[arg(long, conflicts_with = "allowed_hosts")]
    allow_all_hosts: bool,

    /// Allowed Host header value (HTTP mode only); may be repeated.
    /// Replaces the default localhost allow-list when given.
    #[arg(long = "allowed-host", value_name = "HOST")]
    allowed_hosts: Vec<String>,

    /// Enable debug logging
    #[arg(long)]
    debug: bool,

    /// Never save the built tabix index to disk (for read-only/ephemeral environments)
    #[arg(long)]
    never_save_index: bool,

    /// Maximum size in bytes of the JSON returned by variant queries
    /// (query_by_position, query_by_id, start_region_query, get_next_variant).
    /// Larger results are cut short and flagged with `truncated: true`; single
    /// records over the limit are shortened and flagged `record_truncated: true`.
    #[arg(long, value_name = "BYTES", default_value_t = DEFAULT_MAX_RESPONSE_BYTES)]
    max_response_bytes: usize,
}

// Parameter structs for MCP tools
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct QueryByPositionParams {
    /// Chromosome name (e.g., '1', '2', 'X', 'chr1')
    chromosome: String,
    /// Genomic position (1-based)
    position: u64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct QueryByIdParams {
    /// Comma-separated list of variant IDs (e.g., 'rs6054257' or 'rs6054257,rs6040355,microsat1')
    id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetHeaderParams {
    /// Optional search string to filter header lines (e.g., '##INFO', '##contig', '##FILTER'). If provided, only lines containing this string will be returned.
    #[serde(default)]
    search: Option<String>,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetStatisticsParams {
    /// Maximum number of chromosomes to include in variants_per_chromosome. Default is 25 (top chromosomes by variant count). Set to 0 to include all chromosomes.
    #[serde(default = "default_max_chromosomes")]
    max_chromosomes: usize,
}

fn default_max_chromosomes() -> usize {
    25
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct StreamRegionParams {
    /// Chromosome name (e.g., '1', '2', 'X', 'chr1')
    chromosome: String,
    /// Start position (1-based, inclusive)
    start: u64,
    /// End position (1-based, inclusive)
    end: u64,
    /// Optional filter expression, e.g. `QUAL > 30 && FILTER == "PASS"`. Combine conditions with `&&` / `||` (not AND/OR) and put string values in double quotes; an unquoted value such as PASS matches nothing. Call get_documentation with doc_type "filters" for the full syntax. Empty or omitted means no filtering.
    #[serde(default)]
    filter: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct NextVariantParams {
    /// Session ID from start_region_query or get_next_variant response
    region_query_session_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct CloseSessionParams {
    /// Session ID to close
    region_query_session_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetDocumentationParams {
    /// Which documentation to retrieve: "readme", "streaming", "filters", "streaming-filters", "filterlib", or "all"
    #[serde(default = "default_doc_type")]
    doc_type: String,
}

fn default_doc_type() -> String {
    "readme".to_string()
}

#[derive(Debug, serde::Serialize)]
struct QueryResult {
    /// Total number of matching records
    count: usize,
    /// Number of records in `items` (less than `count` when truncated)
    returned_count: usize,
    /// True when records were left out to stay under the response size limit
    truncated: bool,
    /// Number of returned records shortened to fit (`record_truncated: true`)
    #[serde(skip_serializing_if = "is_zero")]
    records_shrunk: usize,
    /// What to do about truncated or shortened records
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
    items: Vec<serde_json::Value>,
}

impl QueryResult {
    fn new(count: usize, fitted: FittedVariants, max_response_bytes: usize) -> Self {
        let hint = size_limit_hint(&fitted, max_response_bytes, HintContext::Lookup, count);
        QueryResult {
            count,
            returned_count: fitted.items.len(),
            truncated: fitted.truncated,
            records_shrunk: fitted.records_shrunk,
            hint,
            items: fitted.items,
        }
    }
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

enum HintContext {
    /// query_by_position / query_by_id: no way to fetch the rest except a region query
    Lookup,
    /// start_region_query / get_next_variant: the rest comes with get_next_variant
    Stream,
}

/// Explain a size-limited response to the caller, or None if nothing was cut.
fn size_limit_hint(
    fitted: &FittedVariants,
    max_response_bytes: usize,
    context: HintContext,
    total_count: usize,
) -> Option<String> {
    let mut parts = Vec::new();
    if fitted.truncated {
        parts.push(match context {
            HintContext::Lookup => format!(
                "Response size limit ({max_response_bytes} bytes) reached: returned {} of {total_count} records. \
                 Use start_region_query on the region to page through all records, and add a filter \
                 (e.g. FILTER == \"PASS\") to skip records you do not need.",
                fitted.items.len()
            ),
            HintContext::Stream => format!(
                "Response size limit ({max_response_bytes} bytes) reached: this batch has fewer than 5 records. \
                 Call get_next_variant to continue."
            ),
        });
    }
    if fitted.records_shrunk > 0 {
        parts.push(format!(
            "{} record(s) were larger than the size limit on their own and were shortened \
             (record_truncated: true): large INFO values are replaced by \
             {{\"omitted\": true, \"entries\": N, \"bytes\": N}} and sample genotypes may be omitted \
             (samples_omitted). The omitted data is not available through this server.",
            fitted.records_shrunk
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum QueryStatus {
    Ok,
    ChromosomeNotFound,
    NotFound,
}

#[derive(Debug, serde::Serialize)]
struct PositionQuery {
    chromosome: String,
    position: u64,
}

#[derive(Debug, serde::Serialize)]
struct IdQuery {
    ids: Vec<String>,
}

#[derive(Debug, serde::Serialize)]
struct QueryByPositionResponse {
    status: QueryStatus,
    reference_genome: String,
    query: PositionQuery,
    matched_chromosome: Option<String>,
    available_chromosomes_sample: Option<Vec<String>>,
    alternate_chromosome_suggestion: Option<String>,
    result: QueryResult,
}

#[derive(Debug, serde::Serialize)]
struct QueryByIdResponse {
    status: QueryStatus,
    reference_genome: String,
    query: IdQuery,
    result: QueryResult,
}

#[derive(Debug, serde::Serialize)]
struct StreamQueryResponse {
    /// Variants in this batch (up to 5), or empty if exhausted
    variants: Vec<serde_json::Value>,
    /// Session ID for subsequent calls, or null if query complete
    region_query_session_id: Option<String>,
    /// Whether more variants exist in this region
    has_more: bool,
    /// True when this batch was cut short by the response size limit
    truncated: bool,
    /// Number of returned records shortened to fit (`record_truncated: true`)
    #[serde(skip_serializing_if = "is_zero")]
    records_shrunk: usize,
    /// What to do about truncated or shortened records
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
    reference_genome: String,
    matched_chromosome: Option<String>,
}

impl StreamQueryResponse {
    fn new(
        fitted: FittedVariants,
        region_query_session_id: Option<String>,
        has_more: bool,
        reference_genome: String,
        matched_chromosome: String,
        max_response_bytes: usize,
    ) -> Self {
        let hint = size_limit_hint(
            &fitted,
            max_response_bytes,
            HintContext::Stream,
            fitted.items.len(),
        );
        StreamQueryResponse {
            region_query_session_id,
            has_more,
            truncated: fitted.truncated,
            records_shrunk: fitted.records_shrunk,
            hint,
            variants: fitted.items,
            reference_genome,
            matched_chromosome: Some(matched_chromosome),
        }
    }
}

// Store iterator state for a streaming query
struct QuerySession {
    chromosome: String,
    start: u64,
    end: u64,
    // Last position returned to the client (used to resume pagination)
    last_position: Option<u64>,
    // Number of variants at last_position already sent to the client.
    // Required to correctly resume when a page boundary falls inside a group
    // of variants that all share the same genomic position.
    last_position_skip: usize,
    last_accessed_at: std::time::Instant,
    // Filter expression to apply to variants
    filter: String,
}

// MCP Server implementation
#[derive(Clone)]
struct VcfServer {
    index: Arc<Mutex<VcfIndex>>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    debug: bool,
    // Track active query sessions by session ID
    query_sessions: Arc<Mutex<HashMap<String, QuerySession>>>,
    // Size limit for variant-returning tool responses (see --max-response-bytes)
    max_response_bytes: usize,
}

#[tool_router]
impl VcfServer {
    fn new(index: VcfIndex, debug: bool) -> Self {
        VcfServer {
            index: Arc::new(Mutex::new(index)),
            tool_router: Self::tool_router(),
            debug,
            query_sessions: Arc::new(Mutex::new(HashMap::new())),
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
        }
    }

    fn with_max_response_bytes(mut self, max_response_bytes: usize) -> Self {
        self.max_response_bytes = max_response_bytes;
        self
    }

    /// Helper method to create a CallToolResult with optional debug logging
    fn create_result_with_logging(
        &self,
        content: ContentBlock,
        start_time: std::time::Instant,
    ) -> Result<CallToolResult, McpError> {
        if self.debug {
            let elapsed = start_time.elapsed();
            let size = serde_json::to_string(&content)
                .map(|s| s.len())
                .unwrap_or(0);
            eprintln!(
                "[DEBUG] Response time: {:.2}ms | Response size: {} bytes",
                elapsed.as_secs_f64() * 1000.0,
                size
            );
        }
        Ok(CallToolResult::success(vec![content]))
    }

    fn debug_log_filter_expression(&self, context: &str, filter_expression: &str) {
        if self.debug {
            eprintln!(
                "[DEBUG] Filter expression ({}) sent to filter engine: {:?}",
                context, filter_expression
            );
        }
    }

    fn debug_log_filter_evaluation(
        &self,
        context: &str,
        filter_expression: &str,
        variant: &Variant,
        passes: bool,
        eval_error: Option<&str>,
    ) {
        if !self.debug {
            return;
        }

        eprintln!(
            "[DEBUG] Filter evaluate ({}) | variant={}:{} id={} | passes={} | error={} | filter={:?}",
            context,
            variant.chromosome,
            variant.position,
            variant.id,
            passes,
            eval_error.unwrap_or(""),
            filter_expression
        );
    }

    #[tool(
        description = "Query variants at a specific genomic position. NOTE: Coordinates are genome build-specific (GRCh37 vs GRCh38). Check the reference_genome field in the response to verify which build is being queried. Returns every record overlapping the position, including long structural variants that start earlier. Responses are size-limited: if result.truncated is true, not every matching record was returned (see result.hint); a record with record_truncated: true had oversized fields (e.g. a large ANN list) replaced by {omitted: true, entries, bytes}."
    )]
    async fn query_by_position(
        &self,
        Parameters(QueryByPositionParams {
            chromosome: requested_chromosome,
            position,
        }): Parameters<QueryByPositionParams>,
    ) -> Result<CallToolResult, McpError> {
        let start_time = std::time::Instant::now();
        let query_context = PositionQuery {
            chromosome: requested_chromosome.clone(),
            position,
        };

        let response = {
            let index = self.index.lock().await;
            let (variants, matched_chr) = index.query_by_position(&requested_chromosome, position);
            let count = variants.len();
            let variants: Vec<Variant> = variants.into_iter().map(format_variant).collect();
            let fitted = fit_variants(&variants, variant_budget(self.max_response_bytes, 0));
            let result = QueryResult::new(count, fitted, self.max_response_bytes);

            let (status, available_sample, alternate_suggestion) =
                build_chromosome_response(&index, &requested_chromosome, &matched_chr);

            let reference_genome = index.get_reference_genome();

            QueryByPositionResponse {
                status,
                reference_genome,
                query: query_context,
                matched_chromosome: matched_chr,
                available_chromosomes_sample: available_sample,
                alternate_chromosome_suggestion: alternate_suggestion,
                result,
            }
        };

        let payload = serde_json::to_value(response).map_err(|e| {
            McpError::internal_error(
                format!("Failed to serialize query_by_position response: {}", e),
                None,
            )
        })?;

        let content = ContentBlock::json(payload)?;

        self.create_result_with_logging(content, start_time)
    }

    #[tool(
        description = "Query variants by variant ID or genomic position. Accepts a comma-separated list where each entry is either a variant ID (e.g., 'rs6054257') or a chromosome:position coordinate (e.g., 'chr11:46352' or '2:74635'). Returns a flat list of all matching variants. Check the reference_genome field in the response to verify which genome build the coordinates use. Responses are size-limited: if result.truncated is true, not every matching record was returned (see result.hint); a record with record_truncated: true had oversized fields (e.g. a large ANN list) replaced by {omitted: true, entries, bytes}."
    )]
    async fn query_by_id(
        &self,
        Parameters(QueryByIdParams { id: requested_id }): Parameters<QueryByIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let start_time = std::time::Instant::now();
        let response = {
            let index = self.index.lock().await;

            let parsed_ids: Vec<String> = requested_id
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            let mut seen = std::collections::HashSet::new();
            let mut all_items: Vec<Variant> = Vec::new();
            for token in &parsed_ids {
                let variants: Vec<Variant> = if let Some((chrom, pos_str)) = token.split_once(':') {
                    let chrom = chrom.trim();
                    let pos_str = pos_str.trim();
                    if let Ok(pos) = pos_str.parse::<u64>() {
                        let (v, _) = index.query_by_position(chrom, pos);
                        v
                    } else {
                        Vec::new()
                    }
                } else {
                    index.query_by_id(token)
                };
                for variant in variants {
                    let key = (
                        variant.chromosome.clone(),
                        variant.position,
                        variant.id.clone(),
                    );
                    if seen.insert(key) {
                        all_items.push(format_variant(variant));
                    }
                }
            }

            let count = all_items.len();
            // The requested IDs are echoed back, so leave room for them too.
            let echoed_ids_bytes = serde_json::to_string(&parsed_ids).map_or(0, |s| s.len());
            let fitted = fit_variants(
                &all_items,
                variant_budget(self.max_response_bytes, echoed_ids_bytes),
            );
            let result = QueryResult::new(count, fitted, self.max_response_bytes);

            let status = if result.count > 0 {
                QueryStatus::Ok
            } else {
                QueryStatus::NotFound
            };

            let reference_genome = index.get_reference_genome();

            QueryByIdResponse {
                status,
                reference_genome,
                query: IdQuery { ids: parsed_ids },
                result,
            }
        };

        let payload = serde_json::to_value(response).map_err(|e| {
            McpError::internal_error(
                format!("Failed to serialize query_by_id response: {}", e),
                None,
            )
        })?;

        let content = ContentBlock::json(payload)?;

        self.create_result_with_logging(content, start_time)
    }

    #[tool(
        description = "Get the raw VCF file header containing metadata and format definitions. By default, ##contig lines are excluded to reduce clutter. To include contig definitions, use the search parameter with '##contig'. To filter for specific header types, provide a search string (e.g., '##INFO' for INFO definitions, '##FILTER' for filter definitions, '##FORMAT' for format definitions)."
    )]
    async fn get_vcf_header(
        &self,
        Parameters(params): Parameters<GetHeaderParams>,
    ) -> Result<CallToolResult, McpError> {
        let start_time = std::time::Instant::now();
        let header_text = {
            let index = self.index.lock().await;
            index.get_header_string(params.search.as_deref())
        };

        let payload = serde_json::json!({
            "header": header_text,
            "line_count": header_text.lines().count(),
            "search_applied": params.search,
        });

        let content = ContentBlock::json(payload)?;
        self.create_result_with_logging(content, start_time)
    }

    #[tool(
        description = "Get comprehensive summary statistics for the VCF file. Returns variant counts, quality statistics, filter distributions, chromosome information, and variant type breakdown. By default, limits variants_per_chromosome to top 25 chromosomes to reduce response size. Set max_chromosomes=0 to include all chromosomes. Statistics are computed once at server startup and cached for instant retrieval."
    )]
    async fn get_statistics(
        &self,
        Parameters(params): Parameters<GetStatisticsParams>,
    ) -> Result<CallToolResult, McpError> {
        let start_time = std::time::Instant::now();
        let mut stats = {
            let index = self.index.lock().await;
            index.compute_statistics().map_err(|e| {
                McpError::internal_error(format!("Failed to compute statistics: {}", e), None)
            })?
        };

        // Limit variants_per_chromosome if requested
        if params.max_chromosomes > 0
            && stats.variants_per_chromosome.len() > params.max_chromosomes
        {
            // Sort chromosomes by variant count (descending) and keep top N
            let mut chr_counts: Vec<_> = stats.variants_per_chromosome.iter().collect();
            chr_counts.sort_by(|a, b| b.1.cmp(a.1));

            let limited: HashMap<String, u64> = chr_counts
                .into_iter()
                .take(params.max_chromosomes)
                .map(|(k, v)| (k.clone(), *v))
                .collect();

            stats.variants_per_chromosome = limited;
        }

        let payload = serde_json::to_value(stats).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize statistics: {}", e), None)
        })?;

        let content = ContentBlock::json(payload)?;
        self.create_result_with_logging(content, start_time)
    }

    #[tool(
        description = "Start a new streaming query session for a genomic region. Returns up to 5 variants per call. If has_more is true, use get_next_variant with the returned region_query_session_id to retrieve subsequent batches. Optionally filter variants using a filter expression (e.g., 'QUAL > 30 && FILTER == \"PASS\"'; use && / || and double-quote string values). A batch may hold fewer than 5 variants if the response size limit is reached (truncated: true); oversized records are shortened (record_truncated: true)."
    )]
    async fn start_region_query(
        &self,
        Parameters(StreamRegionParams {
            chromosome: requested_chromosome,
            start,
            end,
            filter,
        }): Parameters<StreamRegionParams>,
    ) -> Result<CallToolResult, McpError> {
        let start_time = std::time::Instant::now();
        const BATCH_SIZE: usize = 5;

        if start > end {
            return Err(McpError::invalid_params(
                format!("Invalid region: start ({}) must be <= end ({})", start, end),
                None,
            ));
        }

        // Validate filter expression before processing
        let index = self.index.lock().await;

        if !filter.trim().is_empty() {
            self.debug_log_filter_expression("start_region_query", &filter);
            let filter_engine = index.filter_engine();
            drop(index); // Drop lock before potentially expensive operation
            if let Err(e) = filter_engine.parse_filter(&filter) {
                return Err(McpError::invalid_params(
                    format!("Invalid filter expression: {}", e),
                    None,
                ));
            }
        } else {
            drop(index); // Drop lock if no validation needed
        }

        let index = self.index.lock().await;

        // Query the region and resolve matching chromosome (handles chr1 vs 1 normalization)
        let (region_variants, matched_chr) =
            index.query_by_region(&requested_chromosome, start, end);
        let matched_chr_name = if let Some(chr) = matched_chr {
            chr
        } else {
            let available_sample: Vec<String> = index
                .get_available_chromosomes()
                .into_iter()
                .take(5)
                .collect();
            let alternate = if requested_chromosome.starts_with("chr") {
                requested_chromosome
                    .strip_prefix("chr")
                    .unwrap_or(&requested_chromosome)
                    .to_string()
            } else {
                format!("chr{}", requested_chromosome)
            };
            return Err(McpError::invalid_params(
                format!(
                    "Chromosome '{}' not found in VCF file. Try '{}'. Available chromosomes (first 5): {:?}",
                    requested_chromosome, alternate, available_sample
                ),
                None,
            ));
        };

        let filter_engine = index.filter_engine();

        // Collect up to BATCH_SIZE+1 filtered variants; the extra one tells us if has_more is true
        let mut batch: Vec<Variant> = region_variants
            .into_iter()
            .map(format_variant)
            .filter(|v| {
                if filter.trim().is_empty() {
                    true
                } else {
                    let evaluation = filter_engine.evaluate(&filter, &v.raw_row);
                    let (passes, eval_error) = match evaluation {
                        Ok(p) => (p, None),
                        Err(e) => (false, Some(e.to_string())),
                    };
                    self.debug_log_filter_evaluation(
                        "start_region_query",
                        &filter,
                        v,
                        passes,
                        eval_error.as_deref(),
                    );
                    passes
                }
            })
            .take(BATCH_SIZE + 1)
            .collect();

        let mut has_more = batch.len() > BATCH_SIZE;
        if has_more {
            batch.truncate(BATCH_SIZE);
        }

        // Keep the batch under the response size limit. Records that do not fit
        // are dropped from this batch and returned by the next get_next_variant.
        let fitted = fit_variants(&batch, variant_budget(self.max_response_bytes, 0));
        batch.truncate(fitted.items.len());
        has_more |= fitted.truncated;

        let reference_genome = index.get_reference_genome();

        if batch.is_empty() {
            let response = StreamQueryResponse::new(
                fitted,
                None,
                false,
                reference_genome,
                matched_chr_name,
                self.max_response_bytes,
            );

            let payload = serde_json::to_value(response).map_err(|e| {
                McpError::internal_error(
                    format!("Failed to serialize start_region_query response: {}", e),
                    None,
                )
            })?;

            let content = ContentBlock::json(payload)?;
            return self.create_result_with_logging(content, start_time);
        }

        let last_position = batch.last().unwrap().position;
        // Count how many variants at last_position are included in this batch.
        // This is needed so get_next_variant can skip them on resume without
        // incrementing the position and missing same-position variants.
        let last_position_skip = batch.iter().filter(|v| v.position == last_position).count();

        // Create session only when there are more variants beyond this batch
        let region_query_session_id = if has_more {
            let id = Uuid::new_v4().to_string();
            let session = QuerySession {
                chromosome: matched_chr_name.clone(),
                start,
                end,
                last_position: Some(last_position),
                last_position_skip,
                last_accessed_at: std::time::Instant::now(),
                filter: filter.clone(),
            };

            drop(index); // Release lock before acquiring sessions lock
            let mut sessions = self.query_sessions.lock().await;
            sessions.insert(id.clone(), session);
            Some(id)
        } else {
            None
        };

        let response = StreamQueryResponse::new(
            fitted,
            region_query_session_id,
            has_more,
            reference_genome,
            matched_chr_name,
            self.max_response_bytes,
        );

        let payload = serde_json::to_value(response).map_err(|e| {
            McpError::internal_error(
                format!("Failed to serialize start_region_query response: {}", e),
                None,
            )
        })?;

        let content = ContentBlock::json(payload)?;
        self.create_result_with_logging(content, start_time)
    }

    #[tool(
        description = "Get the next batch of variants (up to 5) from an active streaming query session. When has_more is false, the session is complete and automatically closed."
    )]
    async fn get_next_variant(
        &self,
        Parameters(NextVariantParams {
            region_query_session_id,
        }): Parameters<NextVariantParams>,
    ) -> Result<CallToolResult, McpError> {
        let start_time = std::time::Instant::now();
        const BATCH_SIZE: usize = 5;

        let (chromosome, last_pos, last_pos_skip, end, filter) = {
            let mut sessions = self.query_sessions.lock().await;

            let session = sessions.get_mut(&region_query_session_id).ok_or_else(|| {
                McpError::internal_error(
                    "Session not found or expired. Start a new query with start_region_query.",
                    None,
                )
            })?;

            // Check session inactivity timeout (5 minutes)
            if session.last_accessed_at.elapsed().as_secs() > 300 {
                sessions.remove(&region_query_session_id);
                return Err(McpError::internal_error(
                    "Session expired. Start a new query.",
                    None,
                ));
            }

            session.last_accessed_at = std::time::Instant::now();

            (
                session.chromosome.clone(),
                session.last_position.unwrap_or(session.start),
                session.last_position_skip,
                session.end,
                session.filter.clone(),
            )
        };

        let index = self.index.lock().await;

        // Query from last_pos (not last_pos+1) so we include any remaining variants
        // at last_pos that were not returned on the previous page, then skip past
        // the ones that were already sent.
        let (variants, _) = index.query_by_region(&chromosome, last_pos, end);
        let filter_engine = index.filter_engine();

        if !filter.trim().is_empty() {
            self.debug_log_filter_expression("get_next_variant", &filter);
        }

        // Collect up to BATCH_SIZE+1 filtered variants; the extra one tells us if has_more is true
        let mut skip_remaining = last_pos_skip;
        let mut batch: Vec<Variant> = variants
            .into_iter()
            .map(format_variant)
            // The tabix query returns every record *overlapping* [last_pos, end],
            // including ones that start earlier (e.g. long deletions). Results are
            // sorted by start position, so anything starting before last_pos was
            // already sent on a previous page. Without this, such records are
            // re-sent on every page and the stream never advances.
            .filter(|v| v.position >= last_pos)
            .filter(|v| {
                if filter.trim().is_empty() {
                    true
                } else {
                    let evaluation = filter_engine.evaluate(&filter, &v.raw_row);
                    let (passes, eval_error) = match evaluation {
                        Ok(p) => (p, None),
                        Err(e) => (false, Some(e.to_string())),
                    };
                    self.debug_log_filter_evaluation(
                        "get_next_variant",
                        &filter,
                        v,
                        passes,
                        eval_error.as_deref(),
                    );
                    passes
                }
            })
            .skip_while(|v| {
                // Skip variants at last_pos that were already sent on the previous page.
                if v.position == last_pos && skip_remaining > 0 {
                    skip_remaining -= 1;
                    true
                } else {
                    false
                }
            })
            .take(BATCH_SIZE + 1)
            .collect();

        let mut has_more = batch.len() > BATCH_SIZE;
        if has_more {
            batch.truncate(BATCH_SIZE);
        }

        // Keep the batch under the response size limit. The session's resume
        // point is computed from the records actually sent (below), so records
        // that do not fit are returned by the next call.
        let fitted = fit_variants(&batch, variant_budget(self.max_response_bytes, 0));
        batch.truncate(fitted.items.len());
        has_more |= fitted.truncated;

        if batch.is_empty() {
            // No more variants - close session
            let reference_genome = index.get_reference_genome();
            drop(index);

            let mut sessions = self.query_sessions.lock().await;
            sessions.remove(&region_query_session_id);

            let response = StreamQueryResponse::new(
                fitted,
                None,
                false,
                reference_genome,
                chromosome,
                self.max_response_bytes,
            );

            let payload = serde_json::to_value(response).map_err(|e| {
                McpError::internal_error(
                    format!("Failed to serialize get_next_variant response: {}", e),
                    None,
                )
            })?;

            let content = ContentBlock::json(payload)?;
            return self.create_result_with_logging(content, start_time);
        }

        let last_position = batch.last().unwrap().position;
        // Compute how many variants at last_position are in this batch, so the
        // next page can skip past them if the boundary falls at that position again.
        let count_at_last = batch.iter().filter(|v| v.position == last_position).count();
        let new_skip = if last_position == last_pos {
            // Still ending at the same position — accumulate the skip count.
            last_pos_skip + count_at_last
        } else {
            count_at_last
        };
        let reference_genome = index.get_reference_genome();
        drop(index);

        // Update session with last position and skip count for this batch
        let mut sessions = self.query_sessions.lock().await;
        if let Some(session) = sessions.get_mut(&region_query_session_id) {
            session.last_position = Some(last_position);
            session.last_position_skip = new_skip;
        }

        if !has_more {
            sessions.remove(&region_query_session_id);
        }
        drop(sessions);

        let response = StreamQueryResponse::new(
            fitted,
            has_more.then_some(region_query_session_id),
            has_more,
            reference_genome,
            chromosome,
            self.max_response_bytes,
        );

        let payload = serde_json::to_value(response).map_err(|e| {
            McpError::internal_error(
                format!("Failed to serialize get_next_variant response: {}", e),
                None,
            )
        })?;

        let content = ContentBlock::json(payload)?;
        self.create_result_with_logging(content, start_time)
    }

    #[tool(
        description = "Close an active streaming query session and free resources. Sessions are automatically closed when exhausted or after 5 minutes of inactivity."
    )]
    async fn close_query_session(
        &self,
        Parameters(CloseSessionParams {
            region_query_session_id,
        }): Parameters<CloseSessionParams>,
    ) -> Result<CallToolResult, McpError> {
        let start_time = std::time::Instant::now();
        let mut sessions = self.query_sessions.lock().await;
        let existed = sessions.remove(&region_query_session_id).is_some();

        let payload = serde_json::json!({
            "closed": existed,
            "message": if existed { "Session closed" } else { "Session not found" }
        });

        let content = ContentBlock::json(payload)?;
        self.create_result_with_logging(content, start_time)
    }

    #[tool(
        description = "Get embedded documentation for the VCF MCP server. Available types: 'readme' (main documentation), 'streaming' (streaming query guide), 'filters' (filter syntax examples), 'streaming-filters' (streaming with filters guide), 'filterlib' (vcf-filter library syntax and operators), 'all' (complete documentation)."
    )]
    async fn get_documentation(
        &self,
        Parameters(GetDocumentationParams { doc_type }): Parameters<GetDocumentationParams>,
    ) -> Result<CallToolResult, McpError> {
        let start_time = std::time::Instant::now();
        let (content, doc_name) = match doc_type.to_lowercase().as_str() {
            "readme" | "main" => (README_DOCS, "README.md"),
            "streaming" => (STREAMING_DOCS, "STREAMING.md"),
            "filters" | "filter" => (FILTER_DOCS, "FILTER_EXAMPLES.md"),
            "streaming-filters" | "streaming_filters" => {
                (STREAMING_FILTER_DOCS, "STREAMING_FILTER_EXAMPLES.md")
            }
            "filterlib" | "filter_lib" | "filter-lib" => {
                let documentation = filter_docs();
                let payload = serde_json::json!({
                    "doc_type": "filterlib",
                    "document_name": "vcf-filter library",
                    "content": documentation,
                    "format": "markdown"
                });
                let content = ContentBlock::json(payload)?;
                return self.create_result_with_logging(content, start_time);
            }
            "all" => {
                let combined = format!(
                    "# VCF MCP Server - Complete Documentation\n\n\
                     ---\n\n\
                     # Main Documentation\n\n{}\n\n\
                     ---\n\n\
                     # Streaming Queries\n\n{}\n\n\
                     ---\n\n\
                     # Filter Examples\n\n{}\n\n\
                     ---\n\n\
                     # Streaming with Filters\n\n{}",
                    README_DOCS, STREAMING_DOCS, FILTER_DOCS, STREAMING_FILTER_DOCS
                );
                let payload = serde_json::json!({
                    "doc_type": "all",
                    "content": combined,
                    "format": "markdown",
                    "sections": ["README.md", "STREAMING.md", "FILTER_EXAMPLES.md", "STREAMING_FILTER_EXAMPLES.md"]
                });
                let content = ContentBlock::json(payload)?;
                return self.create_result_with_logging(content, start_time);
            }
            unknown => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown doc_type '{}'. Available: readme, streaming, filters, streaming-filters, filterlib, all",
                        unknown
                    ),
                    None,
                ));
            }
        };

        let payload = serde_json::json!({
            "doc_type": doc_type,
            "document_name": doc_name,
            "content": content,
            "format": "markdown"
        });

        let content = ContentBlock::json(payload)?;
        self.create_result_with_logging(content, start_time)
    }

    // Helper method for chromosome not found responses
    // fn build_chromosome_not_found_response(
    //     &self,
    //     index: &VcfIndex,
    //     requested_chromosome: &str,
    // ) -> Result<CallToolResult, McpError> {
    //     let sample_chroms: Vec<String> = index
    //         .get_available_chromosomes()
    //         .into_iter()
    //         .take(5)
    //         .collect();
    //     let alternate = if requested_chromosome.starts_with("chr") {
    //         requested_chromosome
    //             .strip_prefix("chr")
    //             .unwrap_or(requested_chromosome)
    //             .to_string()
    //     } else {
    //         format!("chr{}", requested_chromosome)
    //     };

    //     Err(McpError::internal_error(
    //         format!(
    //             "Chromosome '{}' not found. Try '{}'? Available chromosomes (first 5): {:?}",
    //             requested_chromosome, alternate, sample_chroms
    //         ),
    //         None,
    //     ))
    // }
}

// Helper function to build chromosome match response metadata
fn build_chromosome_response(
    index: &VcfIndex,
    requested_chromosome: &str,
    matched_chr: &Option<String>,
) -> (QueryStatus, Option<Vec<String>>, Option<String>) {
    match matched_chr {
        Some(_) => (QueryStatus::Ok, None, None),
        None => {
            let sample_chroms: Vec<String> = index
                .get_available_chromosomes()
                .into_iter()
                .take(5)
                .collect();
            let alternate = if requested_chromosome.starts_with("chr") {
                requested_chromosome
                    .strip_prefix("chr")
                    .unwrap_or(requested_chromosome)
                    .to_string()
            } else {
                format!("chr{}", requested_chromosome)
            };
            (
                QueryStatus::ChromosomeNotFound,
                Some(sample_chroms),
                Some(alternate),
            )
        }
    }
}

impl ServerHandler for VcfServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
        )
            .with_protocol_version(ProtocolVersion::V_2024_11_05)
            .with_server_info(Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "This server provides VCF variant query tools (query_by_position, query_by_id, start_region_query, get_next_variant, close_query_session) and a metadata resource (vcf://metadata). For large regions, use streaming tools (start_region_query + get_next_variant) to fetch variants one at a time. IMPORTANT: Genomic coordinates are specific to the reference genome build (GRCh37 vs GRCh38). Always check the reference_genome field in responses.".to_string()
            )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult::with_all_items(vec![
            Resource::new("vcf://metadata", "VCF Metadata")
                .with_description(
                    "Metadata from the VCF file header including file format, contigs, and samples",
                )
                .with_mime_type("application/json"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
        if request.uri.as_str() == "vcf://metadata" {
            let index = self.index.lock().await;
            let metadata = index.get_metadata();
            let metadata_json = serde_json::to_string_pretty(&metadata).map_err(|e| {
                McpError::internal_error(format!("Failed to serialize metadata: {}", e), None)
            })?;

            Ok(
                ReadResourceResult::new(vec![ResourceContents::TextResourceContents {
                    uri: request.uri.to_string(),
                    mime_type: Some("application/json".to_string()),
                    text: metadata_json,
                    meta: None,
                }])
                .into(),
            )
        } else {
            Err(McpError::resource_not_found(
                format!("Resource not found: {}", request.uri),
                None,
            ))
        }
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult::default())
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        if self.debug {
            eprintln!(
                "[DEBUG] Initialize request: {}",
                serde_json::to_string_pretty(&request).unwrap_or_else(|_| format!("{:?}", request))
            );
        }
        Ok(self.get_info())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items(self.tool_router.list_all()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if self.debug {
            eprintln!(
                "[DEBUG] Tool call: {}",
                serde_json::to_string_pretty(&request).unwrap_or_else(|_| format!("{:?}", request))
            );
        }
        let tool_ctx = ToolCallContext::new(self, request, ctx);
        let result = self.tool_router.call(tool_ctx).await;

        // Log errors in debug mode
        if self.debug
            && let Err(ref e) = result
        {
            eprintln!("[DEBUG] Tool call error: {:?}", e);
        }

        result
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args = Args::parse();

    if !args.vcf_file.exists() {
        eprintln!("Error: VCF file not found: {}", args.vcf_file.display());
        std::process::exit(1);
    }

    // Load and index the VCF file
    let save_index = !args.never_save_index;
    let index = load_vcf(&args.vcf_file, args.debug, save_index)?;

    // Create the MCP server
    let server = VcfServer::new(index, args.debug).with_max_response_bytes(args.max_response_bytes);

    // Ctrl+C / SIGTERM handling starts here, after the VCF has loaded. Being
    // killed during loading is safe: index and statistics caches are written to
    // a temporary file and renamed into place.
    let shutdown = shutdown::install()?;

    // Run server with appropriate transport
    if let Some(addr) = args.sse {
        eprintln!(
            "VCF MCP Server ready. Starting SSE transport on {}...",
            addr
        );
        run_sse_server(
            server,
            &addr,
            args.allow_all_hosts,
            &args.allowed_hosts,
            shutdown,
        )
        .await?;
        shutdown::log("Server stopped.");
    } else {
        eprintln!("VCF MCP Server ready. Starting stdio transport...");
        run_stdio_server(server, shutdown, args.debug).await?;
    }

    Ok(())
}

/// Serve MCP over stdin/stdout until the client closes stdin or a shutdown
/// signal arrives.
async fn run_stdio_server(
    server: VcfServer,
    shutdown: tokio_util::sync::CancellationToken,
    debug: bool,
) -> std::io::Result<()> {
    // serve() waits for the client's initialize handshake, so a signal can
    // arrive before it returns.
    let service = tokio::select! {
        service = server.serve(rmcp::transport::stdio()) => {
            service.map_err(|e| std::io::Error::other(e.to_string()))?
        }
        _ = shutdown.cancelled() => exit_after_stdio_shutdown(),
    };

    let service_token = service.cancellation_token();
    tokio::spawn({
        let shutdown = shutdown.clone();
        async move {
            shutdown.cancelled().await;
            service_token.cancel();
        }
    });

    let quit_reason = service
        .waiting()
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    if debug {
        shutdown::log(format_args!(
            "[DEBUG] stdio service stopped: {quit_reason:?}"
        ));
    }
    if shutdown.is_cancelled() {
        exit_after_stdio_shutdown();
    }
    Ok(())
}

/// Finish a signal-driven stdio shutdown.
///
/// tokio reads stdin on a blocking thread that only returns when a line
/// arrives, and the runtime waits for that thread when main returns, so a
/// normal return could hang. Nothing is left to clean up, so exit directly.
fn exit_after_stdio_shutdown() -> ! {
    shutdown::log("Server stopped.");
    std::process::exit(0);
}

async fn run_sse_server(
    server: VcfServer,
    addr: &str,
    allow_all_hosts: bool,
    allowed_hosts: &[String],
    shutdown: tokio_util::sync::CancellationToken,
) -> std::io::Result<()> {
    use axum::{
        Router,
        extract::Request,
        middleware::{self, Next},
        response::Response,
    };
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    };

    let bind_addr: std::net::SocketAddr = addr
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    // Cancelling this token closes every MCP stream still open.
    let sessions_token = tokio_util::sync::CancellationToken::new();

    let base_config = if allow_all_hosts {
        eprintln!("WARNING: Host header validation disabled; any host may connect");
        StreamableHttpServerConfig::default().disable_allowed_hosts()
    } else if allowed_hosts.is_empty() {
        StreamableHttpServerConfig::default()
    } else {
        StreamableHttpServerConfig::default().with_allowed_hosts(allowed_hosts.iter().cloned())
    };

    let config = base_config
        .with_sse_keep_alive(Some(std::time::Duration::from_secs(15)))
        .with_sse_retry(Some(std::time::Duration::from_secs(5)))
        .with_legacy_session_mode(false)
        .with_cancellation_token(sessions_token.clone());

    let session_manager = Arc::new(LocalSessionManager::default());

    let debug = server.debug;
    let service = StreamableHttpService::new(move || Ok(server.clone()), session_manager, config);

    // Logging middleware
    async fn log_request(req: Request, next: Next, debug: bool) -> Response {
        if debug {
            eprintln!("[DEBUG] HTTP {} {}", req.method(), req.uri());
            eprintln!("[DEBUG] Headers: {:?}", req.headers());
        }
        next.run(req).await
    }

    let app = Router::new()
        .fallback_service(service)
        .layer(middleware::from_fn(move |req, next| {
            log_request(req, next, debug)
        }));

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    eprintln!(
        "Streamable HTTP MCP server listening on http://{}",
        bind_addr
    );

    // Graceful shutdown: once a signal arrives, axum stops accepting
    // connections, closes idle keep-alive connections and lets in-flight
    // requests finish. Anything still open after REQUEST_DRAIN_PERIOD (e.g. a
    // long-lived SSE stream, if session mode is ever enabled) is closed by
    // cancelling the MCP token, so axum can return.
    tokio::spawn({
        let shutdown = shutdown.clone();
        async move {
            shutdown.cancelled().await;
            shutdown::log(format_args!(
                "No longer accepting connections; waiting up to {}s for in-flight requests...",
                shutdown::REQUEST_DRAIN_PERIOD.as_secs()
            ));
            tokio::time::sleep(shutdown::REQUEST_DRAIN_PERIOD).await;
            sessions_token.cancel();
        }
    });

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown.cancelled_owned())
        .await
        .map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use vcf_filter::FilterEngine;

    fn create_test_index() -> VcfIndex {
        let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
        load_vcf(&vcf_path, false, false).expect("Failed to load test VCF")
    }

    fn tool_text(result: CallToolResult) -> String {
        result.content[0]
            .as_text()
            .expect("tool result should be text")
            .text
            .clone()
    }

    fn tool_json(result: CallToolResult) -> serde_json::Value {
        serde_json::from_str(&tool_text(result)).expect("tool result should be JSON")
    }

    const LARGE_SV_VCF: &str = "sample_data/large-sv-annotation.vcf.gz";
    const KGP_VCF: &str = "sample_data/1kGP_high_coverage_Illumina.chr1.filtered.SNV_INDEL_SV_phased_panel-head-1000.vcf.gz";

    /// Every page of a streamed region query, as returned to the client.
    struct StreamedPages {
        /// (position, id) of every variant, across all pages, in order
        variants: Vec<(u64, String)>,
        /// Parsed JSON of each page
        pages: Vec<serde_json::Value>,
        /// Size in bytes of each page's JSON text
        page_bytes: Vec<usize>,
    }

    /// Run start_region_query then get_next_variant until has_more is false.
    async fn stream_region(
        server: &VcfServer,
        chromosome: &str,
        start: u64,
        end: u64,
    ) -> StreamedPages {
        let mut text = tool_text(
            server
                .start_region_query(Parameters(StreamRegionParams {
                    chromosome: chromosome.into(),
                    start,
                    end,
                    filter: String::new(),
                }))
                .await
                .unwrap(),
        );
        let mut out = StreamedPages {
            variants: Vec::new(),
            pages: Vec::new(),
            page_bytes: Vec::new(),
        };
        for _ in 0..1000 {
            let page: serde_json::Value = serde_json::from_str(&text).unwrap();
            out.page_bytes.push(text.len());
            for v in page["variants"].as_array().unwrap() {
                out.variants.push((
                    v["position"].as_u64().unwrap(),
                    v["id"].as_str().unwrap().to_string(),
                ));
            }
            let has_more = page["has_more"].as_bool().unwrap();
            let session = page["region_query_session_id"].as_str().map(str::to_string);
            out.pages.push(page);
            if !has_more {
                assert!(
                    session.is_none(),
                    "finished query must not return a session id"
                );
                return out;
            }
            text = tool_text(
                server
                    .get_next_variant(Parameters(NextVariantParams {
                        region_query_session_id: session.expect("has_more without a session id"),
                    }))
                    .await
                    .unwrap(),
            );
        }
        panic!("stream did not terminate within 1000 pages");
    }

    fn expected_region(
        index: &VcfIndex,
        chromosome: &str,
        start: u64,
        end: u64,
    ) -> Vec<(u64, String)> {
        let (variants, _) = index.query_by_region(chromosome, start, end);
        variants
            .iter()
            .map(|v| (v.position, v.id.clone()))
            .collect()
    }

    // Regression: resuming a stream re-queries tabix from last_pos, which also
    // returns earlier-starting records that overlap it (long deletions). Those
    // were re-sent on every page, so the stream never terminated. Region
    // chr1:1-10443 of the 1kGP sample starts with a 44bp deletion at 10390.
    // Each 1kGP record carries 3,202 samples, so this also exercises the
    // response size limit on every page.
    #[tokio::test]
    async fn test_streaming_with_overlapping_deletions_terminates() {
        let index =
            load_vcf(&PathBuf::from(KGP_VCF), false, false).expect("Failed to load 1kGP sample");
        let expected = expected_region(&index, "chr1", 1, 10443);
        assert!(expected.len() > 5, "region must span multiple pages");

        let server = VcfServer::new(index, false);
        let streamed = stream_region(&server, "chr1", 1, 10443).await;

        assert_eq!(
            streamed.variants, expected,
            "stream must return each variant once, in order"
        );
        for bytes in &streamed.page_bytes {
            assert!(
                *bytes <= DEFAULT_MAX_RESPONSE_BYTES,
                "page of {bytes} bytes exceeds the limit"
            );
        }
    }

    // Regression: a Manta deletion spanning chr1:789477-224012315 with 22,813
    // ANN entries made query_by_position return ~3 MB, which broke the client.
    #[tokio::test]
    async fn test_query_by_position_large_sv_is_shrunk_under_limit() {
        let index = load_vcf(&PathBuf::from(LARGE_SV_VCF), false, false)
            .expect("Failed to load large SV sample");
        let (raw, _) = index.query_by_position("chr1", 1207339);
        let raw_bytes = serde_json::to_string(&raw).unwrap().len();
        assert!(
            raw_bytes > 2_000_000,
            "test data should be multi-megabyte unshrunk, got {raw_bytes}"
        );

        let server = VcfServer::new(index, false);
        let text = tool_text(
            server
                .query_by_position(Parameters(QueryByPositionParams {
                    chromosome: "chr1".into(),
                    position: 1207339,
                }))
                .await
                .unwrap(),
        );
        assert!(
            text.len() <= DEFAULT_MAX_RESPONSE_BYTES,
            "response is {} bytes",
            text.len()
        );

        let response: serde_json::Value = serde_json::from_str(&text).unwrap();
        let result = &response["result"];
        assert_eq!(result["count"], 3, "deletion + two SNVs overlap 1207339");
        assert_eq!(result["returned_count"], 3);
        assert_eq!(result["truncated"], false);
        assert_eq!(result["records_shrunk"], 1);
        assert!(
            result["hint"]
                .as_str()
                .unwrap()
                .contains("record_truncated")
        );

        let items = result["items"].as_array().unwrap();
        let sv = items
            .iter()
            .find(|v| v["id"] == "MantaDEL:6:7:8:0:0:0")
            .expect("deletion returned");
        assert_eq!(sv["record_truncated"], true);
        assert_eq!(sv["info"]["ANN"]["omitted"], true);
        assert_eq!(sv["info"]["ANN"]["entries"], 22813);
        assert_eq!(sv["info"]["END"], 224012315);
        assert_eq!(sv["info"]["SVTYPE"], "DEL");
        assert_eq!(sv["filter"], serde_json::json!(["SV_HARD"]));

        let small = items
            .iter()
            .find(|v| v["id"] == "rsSmall1")
            .expect("small record returned");
        assert!(small.get("record_truncated").is_none());
        assert!(
            small["info"]["ANN"].is_array(),
            "small records keep their ANN"
        );
    }

    #[tokio::test]
    async fn test_query_by_id_truncates_at_limit() {
        let index = create_test_index();
        let (all_20, _) = index.query_by_region("20", 1, 100_000_000);
        let ids: Vec<String> = all_20
            .iter()
            .map(|v| format!("20:{}", v.position))
            .collect();
        let max_bytes = 5000;

        let server = VcfServer::new(index, false).with_max_response_bytes(max_bytes);
        let text = tool_text(
            server
                .query_by_id(Parameters(QueryByIdParams { id: ids.join(",") }))
                .await
                .unwrap(),
        );
        assert!(text.len() <= max_bytes, "response is {} bytes", text.len());

        let response: serde_json::Value = serde_json::from_str(&text).unwrap();
        let result = &response["result"];
        assert_eq!(result["count"], all_20.len());
        assert_eq!(result["truncated"], true);
        let returned = result["returned_count"].as_u64().unwrap() as usize;
        assert!(returned >= 1 && returned < all_20.len());
        assert_eq!(result["items"].as_array().unwrap().len(), returned);
        assert!(
            result.get("records_shrunk").is_none(),
            "small records are not shrunk"
        );
        assert!(
            result["hint"]
                .as_str()
                .unwrap()
                .contains("start_region_query")
        );
    }

    #[tokio::test]
    async fn test_query_by_position_under_limit_is_not_truncated() {
        let server = VcfServer::new(create_test_index(), false);
        let response = tool_json(
            server
                .query_by_position(Parameters(QueryByPositionParams {
                    chromosome: "20".into(),
                    position: 14370,
                }))
                .await
                .unwrap(),
        );
        let result = &response["result"];
        assert_eq!(result["count"], 1);
        assert_eq!(result["returned_count"], 1);
        assert_eq!(result["truncated"], false);
        assert!(result.get("hint").is_none());
        assert!(result.get("records_shrunk").is_none());
    }

    // With a small limit each batch holds fewer than 5 records; the resume point
    // must follow what was actually sent so nothing is skipped or repeated.
    #[tokio::test]
    async fn test_streaming_under_small_limit_returns_every_record_once() {
        let index = create_test_index();
        let expected = expected_region(&index, "20", 1, 100_000_000);
        let max_bytes = 5000;

        let server = VcfServer::new(index, false).with_max_response_bytes(max_bytes);
        let streamed = stream_region(&server, "20", 1, 100_000_000).await;

        assert_eq!(streamed.variants, expected);
        assert!(
            streamed.pages.iter().any(|p| p["truncated"] == true),
            "at least one batch should be cut short by the limit"
        );
        let first = &streamed.pages[0];
        assert!(
            first["variants"].as_array().unwrap().len() < 5 && first["has_more"] == true,
            "first batch should be cut below 5 records with more remaining"
        );
        for bytes in &streamed.page_bytes {
            assert!(
                *bytes <= max_bytes,
                "page of {bytes} bytes exceeds the limit"
            );
        }
    }

    #[tokio::test]
    async fn test_streaming_region_with_large_sv_stays_under_limit() {
        let index = load_vcf(&PathBuf::from(LARGE_SV_VCF), false, false)
            .expect("Failed to load large SV sample");
        let expected = expected_region(&index, "chr1", 1, 2_000_000);
        assert_eq!(expected.len(), 4);

        let server = VcfServer::new(index, false);
        let streamed = stream_region(&server, "chr1", 1, 2_000_000).await;

        assert_eq!(streamed.variants, expected);
        for bytes in &streamed.page_bytes {
            assert!(
                *bytes <= DEFAULT_MAX_RESPONSE_BYTES,
                "page of {bytes} bytes exceeds the limit"
            );
        }
        let first = &streamed.pages[0];
        assert_eq!(first["records_shrunk"], 1);
        assert_eq!(first["variants"][0]["info"]["ANN"]["entries"], 22813);
    }

    #[tokio::test]
    async fn test_close_query_session_on_live_session() {
        let server = VcfServer::new(create_test_index(), false);
        let (all, _) = server
            .index
            .lock()
            .await
            .query_by_region("20", 1, 100_000_000);
        assert!(
            all.len() > 5,
            "sample must have more than one page on chr 20"
        );
        let page = tool_json(
            server
                .start_region_query(Parameters(StreamRegionParams {
                    chromosome: "20".into(),
                    start: 1,
                    end: 100_000_000,
                    filter: String::new(),
                }))
                .await
                .unwrap(),
        );
        let region_query_session_id = page["region_query_session_id"]
            .as_str()
            .unwrap()
            .to_string();
        let closed = tool_json(
            server
                .close_query_session(Parameters(CloseSessionParams {
                    region_query_session_id: region_query_session_id.clone(),
                }))
                .await
                .unwrap(),
        );
        assert_eq!(closed["closed"], true);
        assert!(
            server
                .get_next_variant(Parameters(NextVariantParams {
                    region_query_session_id
                }))
                .await
                .is_err(),
            "closed session must not be resumable"
        );
    }

    #[test]
    fn test_build_chromosome_response_when_matched() {
        let index = create_test_index();
        let matched_chr = Some("20".to_string());

        let (status, available, alternate) = build_chromosome_response(&index, "20", &matched_chr);

        assert!(matches!(status, QueryStatus::Ok));
        assert_eq!(available, None);
        assert_eq!(alternate, None);
    }

    #[test]
    fn test_build_chromosome_response_when_not_found() {
        let index = create_test_index();
        let matched_chr = None;

        let (status, available, alternate) = build_chromosome_response(&index, "99", &matched_chr);

        assert!(matches!(status, QueryStatus::ChromosomeNotFound));
        assert!(available.is_some());
        assert!(alternate.is_some());
        assert_eq!(alternate, Some("chr99".to_string()));
    }

    #[test]
    fn test_build_chromosome_response_suggests_without_chr_prefix() {
        let index = create_test_index();
        let matched_chr = None;

        let (status, available, alternate) =
            build_chromosome_response(&index, "chr99", &matched_chr);

        assert!(matches!(status, QueryStatus::ChromosomeNotFound));
        assert!(available.is_some());
        assert_eq!(alternate, Some("99".to_string()));
    }

    #[test]
    fn test_build_chromosome_response_suggests_with_chr_prefix() {
        let index = create_test_index();
        let matched_chr = None;

        let (status, _available, alternate) = build_chromosome_response(&index, "99", &matched_chr);

        assert!(matches!(status, QueryStatus::ChromosomeNotFound));
        assert_eq!(alternate, Some("chr99".to_string()));
    }

    #[test]
    fn test_build_chromosome_response_includes_sample_chromosomes() {
        let index = create_test_index();
        let matched_chr = None;

        let (_status, available, _alternate) =
            build_chromosome_response(&index, "99", &matched_chr);

        assert!(available.is_some());
        let chroms = available.unwrap();
        assert!(!chroms.is_empty());
        assert!(chroms.len() <= 5, "Should limit to 5 chromosomes");
    }

    #[test]
    fn test_get_vcf_header() {
        let index = create_test_index();
        let header_string = index.get_header_string(None);

        // Header should not be empty
        assert!(!header_string.is_empty(), "Header should not be empty");

        // Header should start with ##fileformat
        assert!(
            header_string.starts_with("##fileformat="),
            "Header should start with ##fileformat="
        );

        // Header should contain column header line
        assert!(
            header_string.contains("#CHROM"),
            "Header should contain #CHROM column header"
        );

        // Count header lines (all lines starting with #)
        let line_count = header_string.lines().filter(|l| l.starts_with('#')).count();
        assert!(line_count > 0, "Header should have at least one line");
    }

    #[test]
    fn test_raw_row_filter_evaluation_matches_malformed_gt_input() {
        let mut sample_fields = HashMap::new();
        sample_fields.insert(
            "GT".to_string(),
            serde_json::Value::String("1/1".to_string()),
        );

        let mut samples = HashMap::new();
        samples.insert("SAMPLE".to_string(), sample_fields);

        let variant = Variant {
            chromosome: "chr1".to_string(),
            position: 977780,
            id: "rs2710875".to_string(),
            reference: "C".to_string(),
            alternate: vec!["T".to_string()],
            quality: None,
            filter: vec![],
            info: HashMap::new(),
            samples,
            raw_row: "chr1\t977780\trs2710875\tC\tT\t.\t.\t.\tGT\t/1/1".to_string(),
        };

        assert_eq!(
            variant.raw_row,
            "chr1\t977780\trs2710875\tC\tT\t.\t.\t.\tGT\t/1/1"
        );
    }

    #[test]
    fn test_filter_engine_with_rs2710875_one_row_vcf() {
        use noodles::bgzf;
        use std::io::Read;

        let file = fs::File::open("sample_data/rs2710875test.vcf.gz")
            .expect("Failed to open rs2710875test.vcf.gz");
        let mut bgzf_reader = bgzf::io::Reader::new(file);
        let mut content = String::new();
        bgzf_reader
            .read_to_string(&mut content)
            .expect("Failed to decompress rs2710875test.vcf.gz");

        let mut header_lines = Vec::new();
        let mut data_row: Option<String> = None;

        for line in content.lines() {
            if line.starts_with('#') {
                header_lines.push(line);
            } else if !line.trim().is_empty() {
                data_row = Some(line.to_string());
                break;
            }
        }

        let header = format!("{}\n", header_lines.join("\n"));
        let row = data_row.expect("Expected exactly one data row in rs2710875test.vcf");

        let filter_engine =
            FilterEngine::new(&header).expect("Failed to create filter engine from test header");

        assert!(
            filter_engine
                .evaluate("ID == \"rs2710875\"", &row)
                .expect("ID filter should evaluate")
        );

        assert!(
            filter_engine
                .evaluate("GT == \"1/1\"", &row)
                .expect("GT filter should evaluate")
        );

        assert!(
            filter_engine
                .evaluate("ID == \"rs2710875\" && GT == \"1/1\"", &row)
                .expect("Combined ID/GT filter should evaluate")
        );
    }
}
