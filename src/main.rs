mod vcf;

use clap::Parser;
use rmcp::{
    ErrorData as McpError,
    RoleServer,
    ServerHandler,
    ServiceExt,
    handler::server::{
        router::tool::ToolRouter,
        tool::ToolCallContext,
        wrapper::Parameters,
    },
    model::*,
    schemars,
    service::RequestContext,
    tool,
    tool_router,
};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use vcf::{VcfIndex, format_variant, load_vcf};

// CLI arguments
#[derive(Parser, Debug)]
#[command(name = "vcf_mcp_server")]
#[command(about = "VCF MCP Server - expose VCF files via MCP protocol", long_about = None)]
struct Args {
    /// Path to the VCF file
    vcf_file: PathBuf,

    /// Run HTTP server on specified address (e.g., 0.0.0.0:8090)
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

// MCP Server implementation
#[derive(Clone)]
struct VcfServer {
    index: Arc<Mutex<VcfIndex>>,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    debug: bool,
}

#[tool_router]
impl VcfServer {
    fn new(index: VcfIndex, debug: bool) -> Self {
        VcfServer {
            index: Arc::new(Mutex::new(index)),
            tool_router: Self::tool_router(),
            debug,
        }
    }

    #[tool(description = "Query variants at a specific genomic position")]
    async fn query_by_position(
        &self,
        Parameters(QueryByPositionParams { chromosome, position }): Parameters<QueryByPositionParams>,
    ) -> Result<CallToolResult, McpError> {
        let index = self.index.lock().await;
        let (variants, matched_chr) = index.query_by_position(&chromosome, position);

        let content = if variants.is_empty() {
            if matched_chr.is_some() {
                // Chromosome was found but no variants at this position
                format!("No variants found at {}:{}", chromosome, position)
            } else {
                // Chromosome name not recognized - provide helpful error
                let available = index.get_available_chromosomes();
                let sample_chroms: Vec<String> = available.iter().take(5).cloned().collect();

                let alternate_suggestion = if chromosome.starts_with("chr") {
                    chromosome.strip_prefix("chr").unwrap_or(&chromosome).to_string()
                } else {
                    format!("chr{}", chromosome)
                };

                if sample_chroms.is_empty() {
                    format!("No variants found at {}:{}\n\nChromosome '{}' not found in VCF file (no chromosomes available).", chromosome, position, chromosome)
                } else {
                    let chr_format = if sample_chroms[0].starts_with("chr") { "chr-prefixed" } else { "numeric" };
                    format!(
                        "No variants found at {}:{}\n\nChromosome '{}' not found in VCF file. Available chromosomes include: {}\n\nNote: This file uses '{}' format. Try using '{}' instead of '{}'.",
                        chromosome, position, chromosome,
                        sample_chroms.join(", "),
                        chr_format,
                        alternate_suggestion,
                        chromosome
                    )
                }
            }
        } else {
            let variant_json: Vec<String> = variants.iter().map(format_variant).collect();
            format!(
                "Found {} variant(s):\n[{}]",
                variants.len(),
                variant_json.join(",\n")
            )
        };

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Query variants in a genomic region")]
    async fn query_by_region(
        &self,
        Parameters(QueryByRegionParams { chromosome, start, end }): Parameters<QueryByRegionParams>,
    ) -> Result<CallToolResult, McpError> {
        let index = self.index.lock().await;
        let (variants, matched_chr) = index.query_by_region(&chromosome, start, end);

        let content = if variants.is_empty() {
            if matched_chr.is_some() {
                // Chromosome was found but no variants in this region
                format!(
                    "No variants found in region {}:{}-{}",
                    chromosome, start, end
                )
            } else {
                // Chromosome name not recognized - provide helpful error
                let available = index.get_available_chromosomes();
                let sample_chroms: Vec<String> = available.iter().take(5).cloned().collect();

                let alternate_suggestion = if chromosome.starts_with("chr") {
                    chromosome.strip_prefix("chr").unwrap_or(&chromosome).to_string()
                } else {
                    format!("chr{}", chromosome)
                };

                if sample_chroms.is_empty() {
                    format!("No variants found in region {}:{}-{}\n\nChromosome '{}' not found in VCF file (no chromosomes available).", chromosome, start, end, chromosome)
                } else {
                    let chr_format = if sample_chroms[0].starts_with("chr") { "chr-prefixed" } else { "numeric" };
                    format!(
                        "No variants found in region {}:{}-{}\n\nChromosome '{}' not found in VCF file. Available chromosomes include: {}\n\nNote: This file uses '{}' format. Try using '{}' instead of '{}'.",
                        chromosome, start, end, chromosome,
                        sample_chroms.join(", "),
                        chr_format,
                        alternate_suggestion,
                        chromosome
                    )
                }
            }
        } else {
            let variant_json: Vec<String> = variants.iter().map(format_variant).collect();
            format!(
                "Found {} variant(s):\n[{}]",
                variants.len(),
                variant_json.join(",\n")
            )
        };

        Ok(CallToolResult::success(vec![Content::text(content)]))
    }

    #[tool(description = "Query variants by variant ID (e.g., rsID)")]
    async fn query_by_id(
        &self,
        Parameters(QueryByIdParams { id }): Parameters<QueryByIdParams>,
    ) -> Result<CallToolResult, McpError> {
        let index = self.index.lock().await;
        let variants = index.query_by_id(&id);

        let content = if variants.is_empty() {
            format!("No variants found with ID '{}'", id)
        } else {
            let variant_json: Vec<String> = variants.iter().map(format_variant).collect();
            format!(
                "Found {} variant(s):\n[{}]",
                variants.len(),
                variant_json.join(",\n")
            )
        };

        Ok(CallToolResult::success(vec![Content::text(content)]))
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
                "This server provides VCF variant query tools (query_by_position, query_by_region, query_by_id) and a metadata resource (vcf://metadata)".to_string()
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
                },
                None
            )],
            next_cursor: None,
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
            let metadata_json = serde_json::to_string_pretty(&metadata)
                .map_err(|e| McpError::internal_error(format!("Failed to serialize metadata: {}", e), None))?;

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
                None
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
        })
    }

    async fn initialize(
        &self,
        request: InitializeRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        if self.debug {
            eprintln!("[DEBUG] Initialize request: {}", serde_json::to_string_pretty(&request).unwrap_or_else(|_| format!("{:?}", request)));
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
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        if self.debug {
            eprintln!("[DEBUG] Tool call: {}", serde_json::to_string_pretty(&request).unwrap_or_else(|_| format!("{:?}", request)));
        }
        let tool_ctx = ToolCallContext::new(self, request, ctx);
        self.tool_router.call(tool_ctx).await
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
        eprintln!("VCF MCP Server ready. Starting SSE transport on {}...", addr);
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
    use rmcp::transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService,
        session::local::LocalSessionManager,
    };
    use axum::{
        Router,
        middleware::{self, Next},
        extract::Request,
        response::Response,
    };

    let bind_addr: std::net::SocketAddr = addr.parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;

    let config = StreamableHttpServerConfig {
        sse_keep_alive: Some(std::time::Duration::from_secs(15)),
        stateful_mode: false,
    };

    let session_manager = Arc::new(LocalSessionManager::default());

    let debug = server.debug;
    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        session_manager,
        config,
    );

    // Logging middleware
    async fn log_request(
        req: Request,
        next: Next,
        debug: bool,
    ) -> Response {
        if debug {
            eprintln!("[DEBUG] HTTP {} {}", req.method(), req.uri());
            eprintln!("[DEBUG] Headers: {:?}", req.headers());
        }
        next.run(req).await
    }

    let app = Router::new()
        .fallback_service(service)
        .layer(middleware::from_fn(move |req, next| log_request(req, next, debug)));

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;

    eprintln!("Streamable HTTP MCP server listening on http://{}", bind_addr);

    axum::serve(listener, app)
        .await
        .map_err(std::io::Error::other)
}
