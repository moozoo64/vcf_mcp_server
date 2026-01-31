mod vcf;

use clap::Parser;
use rmcp::{
    handler::server::{router::tool::ToolRouter, tool::ToolCallContext, wrapper::Parameters},
    model::*,
    schemars,
    service::RequestContext,
    tool, tool_router, ErrorData as McpError, RoleServer, ServerHandler, ServiceExt,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;
use vcf::{format_variant, load_vcf, Variant, VcfIndex};

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

    /// Enable debug logging
    #[arg(long)]
    debug: bool,

    /// Never save the built tabix index to disk (for read-only/ephemeral environments)
    #[arg(long)]
    never_save_index: bool,
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
struct QueryByRegionParams {
    /// Chromosome name (e.g., '1', '2', 'X', 'chr1')
    chromosome: String,
    /// Start position (1-based, inclusive)
    start: u64,
    /// End position (1-based, inclusive)
    end: u64,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct QueryByIdParams {
    /// Variant ID (e.g., 'rs6054257')
    id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct StreamRegionParams {
    /// Chromosome name (e.g., '1', '2', 'X', 'chr1')
    chromosome: String,
    /// Start position (1-based, inclusive)
    start: u64,
    /// End position (1-based, inclusive)
    end: u64,
    /// Optional filter expression (e.g., "QUAL > 30 AND FILTER == PASS"). Empty or omitted means no filtering.
    #[serde(default)]
    filter: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct NextVariantParams {
    /// Session ID from start_region_query or get_next_variant response
    session_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct CloseSessionParams {
    /// Session ID to close
    session_id: String,
}

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
struct GetDocumentationParams {
    /// Which documentation to retrieve: "readme", "streaming", "filters", "streaming-filters", or "all"
    #[serde(default = "default_doc_type")]
    doc_type: String,
}

fn default_doc_type() -> String {
    "readme".to_string()
}

#[derive(Debug, serde::Serialize)]
struct QueryResult<T>
where
    T: serde::Serialize,
{
    count: usize,
    items: Vec<T>,
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
struct RegionQuery {
    chromosome: String,
    start: u64,
    end: u64,
}

#[derive(Debug, serde::Serialize)]
struct IdQuery {
    id: String,
}

#[derive(Debug, serde::Serialize)]
struct QueryByPositionResponse {
    status: QueryStatus,
    reference_genome: String,
    query: PositionQuery,
    matched_chromosome: Option<String>,
    available_chromosomes_sample: Option<Vec<String>>,
    alternate_chromosome_suggestion: Option<String>,
    result: QueryResult<Variant>,
}

#[derive(Debug, serde::Serialize)]
struct QueryByRegionResponse {
    status: QueryStatus,
    reference_genome: String,
    query: RegionQuery,
    matched_chromosome: Option<String>,
    available_chromosomes_sample: Option<Vec<String>>,
    alternate_chromosome_suggestion: Option<String>,
    result: QueryResult<Variant>,
}

#[derive(Debug, serde::Serialize)]
struct QueryByIdResponse {
    status: QueryStatus,
    reference_genome: String,
    query: IdQuery,
    result: QueryResult<Variant>,
}

#[derive(Debug, serde::Serialize)]
struct StreamQueryResponse {
    /// Next variant in region, or null if exhausted
    variant: Option<Variant>,
    /// Session ID for subsequent calls, or null if query complete
    session_id: Option<String>,
    /// Whether more variants exist in this region
    has_more: bool,
    reference_genome: String,
    matched_chromosome: Option<String>,
}

// Store iterator state for a streaming query
struct QuerySession {
    chromosome: String,
    start: u64,
    end: u64,
    // Last position returned (to resume from next position)
    last_position: Option<u64>,
    created_at: std::time::Instant,
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
}

#[tool_router]
impl VcfServer {
    fn new(index: VcfIndex, debug: bool) -> Self {
        VcfServer {
            index: Arc::new(Mutex::new(index)),
            tool_router: Self::tool_router(),
            debug,
            query_sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[tool(
        description = "Query variants at a specific genomic position. NOTE: Coordinates are genome build-specific (GRCh37 vs GRCh38). Check the reference_genome field in the response to verify which build is being queried."
    )]
    async fn query_by_position(
        &self,
        Parameters(QueryByPositionParams {
            chromosome: requested_chromosome,
            position,
        }): Parameters<QueryByPositionParams>,
    ) -> Result<CallToolResult, McpError> {
        let query_context = PositionQuery {
            chromosome: requested_chromosome.clone(),
            position,
        };

        let response = {
            let index = self.index.lock().await;
            let (variants, matched_chr) = index.query_by_position(&requested_chromosome, position);
            let count = variants.len();
            let items: Vec<Variant> = variants.into_iter().map(format_variant).collect();
            let result = QueryResult { count, items };

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

        let content = Content::json(payload)?;

        Ok(CallToolResult::success(vec![content]))
    }

    #[tool(
        description = "Query variants in a genomic region. Maximum region size is 10,000 bp (10 kb). Requests exceeding this limit will be rejected. NOTE: Coordinates are genome build-specific (GRCh37 vs GRCh38). Check the reference_genome field in the response to verify which build is being queried."
    )]
    async fn query_by_region(
        &self,
        Parameters(QueryByRegionParams {
            chromosome: requested_chromosome,
            start,
            end,
        }): Parameters<QueryByRegionParams>,
    ) -> Result<CallToolResult, McpError> {
        const MAX_WINDOW: u64 = 10000; // 10 kb maximum region size

        // Validate region size
        if end > start && (end - start) > MAX_WINDOW {
            return Err(McpError::invalid_params(
                format!(
                    "Requested region too large ({} bp). Maximum window is {} bp.",
                    end - start,
                    MAX_WINDOW
                ),
                None,
            ));
        }

        let query_context = RegionQuery {
            chromosome: requested_chromosome.clone(),
            start,
            end,
        };

        let response = {
            let index = self.index.lock().await;
            let (variants, matched_chr) = index.query_by_region(&requested_chromosome, start, end);
            let count = variants.len();
            let items: Vec<Variant> = variants.into_iter().map(format_variant).collect();
            let result = QueryResult { count, items };

            let (status, available_sample, alternate_suggestion) =
                build_chromosome_response(&index, &requested_chromosome, &matched_chr);

            let reference_genome = index.get_reference_genome();

            QueryByRegionResponse {
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
                format!("Failed to serialize query_by_region response: {}", e),
                None,
            )
        })?;

        let content = Content::json(payload)?;

        Ok(CallToolResult::success(vec![content]))
    }

    #[tool(
        description = "Query variants by variant ID (e.g., rsID). Check the reference_genome field in the response to verify which genome build the coordinates use."
    )]
    async fn query_by_id(
        &self,
        Parameters(QueryByIdParams { id: requested_id }): Parameters<QueryByIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let response = {
            let index = self.index.lock().await;
            let variants = index.query_by_id(&requested_id);

            let count = variants.len();
            let items: Vec<Variant> = variants.into_iter().map(format_variant).collect();
            let result = QueryResult { count, items };

            let status = if result.count > 0 {
                QueryStatus::Ok
            } else {
                QueryStatus::NotFound
            };

            let reference_genome = index.get_reference_genome();

            QueryByIdResponse {
                status,
                reference_genome,
                query: IdQuery {
                    id: requested_id.clone(),
                },
                result,
            }
        };

        let payload = serde_json::to_value(response).map_err(|e| {
            McpError::internal_error(
                format!("Failed to serialize query_by_id response: {}", e),
                None,
            )
        })?;

        let content = Content::json(payload)?;

        Ok(CallToolResult::success(vec![content]))
    }

    #[tool(
        description = "Get the raw VCF file header containing all metadata, format definitions, and contig information"
    )]
    async fn get_vcf_header(&self) -> Result<CallToolResult, McpError> {
        let header_text = {
            let index = self.index.lock().await;
            index.get_header_string()
        };

        let payload = serde_json::json!({
            "header": header_text,
            "line_count": header_text.lines().count(),
        });

        let content = Content::json(payload)?;
        Ok(CallToolResult::success(vec![content]))
    }

    #[tool(
        description = "Get comprehensive summary statistics for the VCF file. Returns variant counts, quality statistics, filter distributions, chromosome information, and variant type breakdown. This requires scanning the entire VCF file and may take a few seconds for large files."
    )]
    async fn get_statistics(&self) -> Result<CallToolResult, McpError> {
        let stats = {
            let index = self.index.lock().await;
            index.compute_statistics().map_err(|e| {
                McpError::internal_error(format!("Failed to compute statistics: {}", e), None)
            })?
        };

        let payload = serde_json::to_value(stats).map_err(|e| {
            McpError::internal_error(format!("Failed to serialize statistics: {}", e), None)
        })?;

        let content = Content::json(payload)?;
        Ok(CallToolResult::success(vec![content]))
    }

    #[tool(
        description = "Start a new streaming query session for a genomic region. Returns the first variant and a session_id for subsequent calls. Use get_next_variant to retrieve remaining variants one at a time. Optionally filter variants using a filter expression (e.g., 'QUAL > 30 AND FILTER == PASS')."
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
        // Validate filter expression before processing
        let index = self.index.lock().await;

        if !filter.trim().is_empty() {
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

        // Find matching chromosome (handles chr1 vs 1 normalization)
        let matched_chr = index.get_available_chromosomes().into_iter().find(|chr| {
            chr.to_lowercase() == requested_chromosome.to_lowercase()
                || chr.to_lowercase() == format!("chr{}", requested_chromosome).to_lowercase()
                || chr.to_lowercase()
                    == requested_chromosome
                        .strip_prefix("chr")
                        .unwrap_or(&requested_chromosome)
                        .to_lowercase()
        });

        let matched_chr_name = matched_chr.ok_or_else(|| {
            McpError::internal_error(
                format!(
                    "Chromosome '{}' not found in VCF file",
                    requested_chromosome
                ),
                None,
            )
        })?;

        // Query the region and find first variant that passes filter
        let (region_variants, _) = index.query_by_region(&matched_chr_name, start, end);
        let filter_engine = index.filter_engine();

        let first_variant = region_variants.into_iter().map(format_variant).find(|v| {
            // Use vcf-filter to evaluate filter expression
            filter_engine.evaluate(&filter, &v.raw_row).unwrap_or(false)
        });

        // If no variants found, return graceful response (consistent with get_next_variant)
        if first_variant.is_none() {
            let reference_genome = index.get_reference_genome();
            let response = StreamQueryResponse {
                variant: None,
                session_id: None,
                has_more: false,
                reference_genome,
                matched_chromosome: Some(matched_chr_name),
            };

            let payload = serde_json::to_value(response).map_err(|e| {
                McpError::internal_error(
                    format!("Failed to serialize start_region_query response: {}", e),
                    None,
                )
            })?;

            let content = Content::json(payload)?;
            return Ok(CallToolResult::success(vec![content]));
        }

        let first_variant = first_variant.unwrap();

        // Create session
        let session_id = Uuid::new_v4().to_string();
        let session = QuerySession {
            chromosome: matched_chr_name.clone(),
            start,
            end,
            last_position: Some(first_variant.position),
            created_at: std::time::Instant::now(),
            filter: filter.clone(),
        };

        drop(index); // Release lock before acquiring sessions lock
        let mut sessions = self.query_sessions.lock().await;
        sessions.insert(session_id.clone(), session);
        drop(sessions);

        let index = self.index.lock().await;
        let reference_genome = index.get_reference_genome();

        let response = StreamQueryResponse {
            variant: Some(first_variant),
            session_id: Some(session_id),
            has_more: true, // Assume yes until we check
            reference_genome,
            matched_chromosome: Some(matched_chr_name),
        };

        let payload = serde_json::to_value(response).map_err(|e| {
            McpError::internal_error(
                format!("Failed to serialize start_region_query response: {}", e),
                None,
            )
        })?;

        let content = Content::json(payload)?;
        Ok(CallToolResult::success(vec![content]))
    }

    #[tool(
        description = "Get the next variant from an active streaming query session. Returns one variant at a time. When has_more is false, the session is complete and automatically closed."
    )]
    async fn get_next_variant(
        &self,
        Parameters(NextVariantParams { session_id }): Parameters<NextVariantParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut sessions = self.query_sessions.lock().await;

        let session = sessions.get(&session_id).ok_or_else(|| {
            McpError::internal_error(
                "Session not found or expired. Start a new query with start_region_query.",
                None,
            )
        })?;

        // Check session timeout (5 minutes)
        if session.created_at.elapsed().as_secs() > 300 {
            sessions.remove(&session_id);
            return Err(McpError::internal_error(
                "Session expired. Start a new query.",
                None,
            ));
        }

        // Get session details before releasing lock
        let chromosome = session.chromosome.clone();
        let last_pos = session.last_position.unwrap_or(session.start);
        let end = session.end;
        let filter = session.filter.clone();
        drop(sessions);

        let index = self.index.lock().await;

        // Query from next position after last returned variant
        let next_pos = last_pos + 1;
        let (variants, _) = index.query_by_region(&chromosome, next_pos, end);
        let filter_engine = index.filter_engine();

        // Find next variant that passes filter
        let next_variant = variants.into_iter().map(format_variant).find(|v| {
            filter_engine.evaluate(&filter, &v.raw_row).unwrap_or(false) // Treat filter errors as non-match
        });

        if next_variant.is_none() {
            // No more variants - close session
            drop(index);
            let mut sessions = self.query_sessions.lock().await;
            sessions.remove(&session_id);

            let index = self.index.lock().await;
            let reference_genome = index.get_reference_genome();

            let response = StreamQueryResponse {
                variant: None,
                session_id: None,
                has_more: false,
                reference_genome,
                matched_chromosome: Some(chromosome),
            };

            let payload = serde_json::to_value(response).map_err(|e| {
                McpError::internal_error(
                    format!("Failed to serialize get_next_variant response: {}", e),
                    None,
                )
            })?;

            let content = Content::json(payload)?;
            return Ok(CallToolResult::success(vec![content]));
        }

        // Get next variant
        let next_variant_data = next_variant.unwrap();
        let new_position = next_variant_data.position;

        // Check if there are more variants after this one that pass the filter
        let (peek_variants, _) = index.query_by_region(&chromosome, new_position + 1, end);
        let has_more = peek_variants.into_iter().map(format_variant).any(|v| {
            filter_engine.evaluate(&filter, &v.raw_row).unwrap_or(false) // Treat filter errors as non-match
        });

        let reference_genome = index.get_reference_genome();
        drop(index);

        // Update session with new position
        let mut sessions = self.query_sessions.lock().await;
        if let Some(session) = sessions.get_mut(&session_id) {
            session.last_position = Some(new_position);
        }

        // If no more variants, remove session
        if !has_more {
            sessions.remove(&session_id);
        }
        drop(sessions);

        let response = StreamQueryResponse {
            variant: Some(next_variant_data),
            session_id: if has_more { Some(session_id) } else { None },
            has_more,
            reference_genome,
            matched_chromosome: Some(chromosome),
        };

        let payload = serde_json::to_value(response).map_err(|e| {
            McpError::internal_error(
                format!("Failed to serialize get_next_variant response: {}", e),
                None,
            )
        })?;

        let content = Content::json(payload)?;
        Ok(CallToolResult::success(vec![content]))
    }

    #[tool(
        description = "Close an active streaming query session and free resources. Sessions are automatically closed when exhausted or after 5 minutes of inactivity."
    )]
    async fn close_query_session(
        &self,
        Parameters(CloseSessionParams { session_id }): Parameters<CloseSessionParams>,
    ) -> Result<CallToolResult, McpError> {
        let mut sessions = self.query_sessions.lock().await;
        let existed = sessions.remove(&session_id).is_some();

        let payload = serde_json::json!({
            "closed": existed,
            "message": if existed { "Session closed" } else { "Session not found" }
        });

        let content = Content::json(payload)?;
        Ok(CallToolResult::success(vec![content]))
    }

    #[tool(
        description = "Get embedded documentation for the VCF MCP server. Available types: 'readme' (main documentation), 'streaming' (streaming query guide), 'filters' (filter syntax examples), 'streaming-filters' (streaming with filters guide), 'all' (complete documentation)."
    )]
    async fn get_documentation(
        &self,
        Parameters(GetDocumentationParams { doc_type }): Parameters<GetDocumentationParams>,
    ) -> Result<CallToolResult, McpError> {
        let (content, doc_name) = match doc_type.to_lowercase().as_str() {
            "readme" | "main" => (README_DOCS, "README.md"),
            "streaming" => (STREAMING_DOCS, "STREAMING.md"),
            "filters" | "filter" => (FILTER_DOCS, "FILTER_EXAMPLES.md"),
            "streaming-filters" | "streaming_filters" => {
                (STREAMING_FILTER_DOCS, "STREAMING_FILTER_EXAMPLES.md")
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
                let content = Content::json(payload)?;
                return Ok(CallToolResult::success(vec![content]));
            }
            unknown => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown doc_type '{}'. Available: readme, streaming, filters, streaming-filters, all",
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

        let content = Content::json(payload)?;
        Ok(CallToolResult::success(vec![content]))
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
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "This server provides VCF variant query tools (query_by_position, query_by_region, query_by_id, start_region_query, get_next_variant, close_query_session) and a metadata resource (vcf://metadata). For large regions, use streaming tools (start_region_query + get_next_variant) to fetch variants one at a time. IMPORTANT: Genomic coordinates are specific to the reference genome build (GRCh37 vs GRCh38). Always check the reference_genome field in responses.".to_string()
            ),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![Annotated::new(
                RawResource {
                    uri: "vcf://metadata".to_string(),
                    name: "VCF Metadata".to_string(),
                    title: None,
                    description: Some(
                        "Metadata from the VCF file header including file format, contigs, and samples".to_string()
                    ),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                    icons: None,
                    meta: None,
                },
                None
            )],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        if request.uri.as_str() == "vcf://metadata" {
            let index = self.index.lock().await;
            let metadata = index.get_metadata();
            let metadata_json = serde_json::to_string_pretty(&metadata).map_err(|e| {
                McpError::internal_error(format!("Failed to serialize metadata: {}", e), None)
            })?;

            Ok(ReadResourceResult {
                contents: vec![ResourceContents::TextResourceContents {
                    uri: request.uri.to_string(),
                    mime_type: Some("application/json".to_string()),
                    text: metadata_json,
                    meta: None,
                }],
            })
        } else {
            Err(McpError::resource_not_found(
                format!("Resource not found: {}", request.uri),
                None,
            ))
        }
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult {
            next_cursor: None,
            resource_templates: Vec::new(),
            meta: None,
        })
    }

    async fn initialize(
        &self,
        request: InitializeRequestParam,
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
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        if self.debug {
            eprintln!(
                "[DEBUG] Tool call: {}",
                serde_json::to_string_pretty(&request).unwrap_or_else(|_| format!("{:?}", request))
            );
        }
        let tool_ctx = ToolCallContext::new(self, request, ctx);
        let result = self.tool_router.call(tool_ctx).await;

        // Log errors in debug mode
        if self.debug {
            if let Err(ref e) = result {
                eprintln!("[DEBUG] Tool call error: {:?}", e);
            }
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
    let server = VcfServer::new(index, args.debug);

    // Run server with appropriate transport
    if let Some(addr) = args.sse {
        eprintln!(
            "VCF MCP Server ready. Starting SSE transport on {}...",
            addr
        );
        run_sse_server(server, &addr).await?;
    } else {
        eprintln!("VCF MCP Server ready. Starting stdio transport...");

        // Run the server using stdio transport
        let service = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        service
            .waiting()
            .await
            .map_err(|e| std::io::Error::other(e.to_string()))?;
    }

    Ok(())
}

async fn run_sse_server(server: VcfServer, addr: &str) -> std::io::Result<()> {
    use axum::{
        extract::Request,
        middleware::{self, Next},
        response::Response,
        Router,
    };
    use rmcp::transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    };

    let bind_addr: std::net::SocketAddr = addr
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    let config = StreamableHttpServerConfig {
        sse_keep_alive: Some(std::time::Duration::from_secs(15)),
        sse_retry: Some(std::time::Duration::from_secs(5)),
        stateful_mode: false,
        cancellation_token: tokio_util::sync::CancellationToken::new(),
    };

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

    axum::serve(listener, app)
        .await
        .map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_index() -> VcfIndex {
        let vcf_path = PathBuf::from("sample_data/sample.compressed.vcf.gz");
        load_vcf(&vcf_path, false, false).expect("Failed to load test VCF")
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
        let header_string = index.get_header_string();

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
}
