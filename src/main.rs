mod vcf;

use rmcp::{
    ErrorData as McpError,
    RoleServer,
    ServerHandler,
    ServiceExt,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::Parameters,
    },
    model::*,
    schemars,
    service::RequestContext,
    tool,
    tool_router,
};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use vcf::{VcfIndex, format_variant, load_vcf};

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
}

#[tool_router]
impl VcfServer {
    fn new(index: VcfIndex) -> Self {
        VcfServer {
            index: Arc::new(Mutex::new(index)),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Query variants at a specific genomic position")]
    async fn query_by_position(
        &self,
        Parameters(QueryByPositionParams { chromosome, position }): Parameters<QueryByPositionParams>,
    ) -> Result<CallToolResult, McpError> {
        let index = self.index.lock().await;
        let variants = index.query_by_position(&chromosome, position);

        let content = if variants.is_empty() {
            format!("No variants found at {}:{}", chromosome, position)
        } else {
            let variant_json: Vec<String> = variants.iter().map(|v| format_variant(v)).collect();
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
        let variants = index.query_by_region(&chromosome, start, end);

        let content = if variants.is_empty() {
            format!(
                "No variants found in region {}:{}-{}",
                chromosome, start, end
            )
        } else {
            let variant_json: Vec<String> = variants.iter().map(|v| format_variant(v)).collect();
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
            let variant_json: Vec<String> = variants.iter().map(|v| format_variant(v)).collect();
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
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "This server provides VCF variant query tools: query_by_position, query_by_region, query_by_id".to_string()
            ),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        Ok(ListResourcesResult {
            resources: vec![],
            next_cursor: None,
        })
    }

    async fn read_resource(
        &self,
        _request: ReadResourceRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        Err(McpError::resource_not_found("No resources available", None))
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
        _request: InitializeRequestParam,
        _: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        Ok(self.get_info())
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <vcf_file_path>", args[0]);
        std::process::exit(1);
    }

    let vcf_path = PathBuf::from(&args[1]);

    if !vcf_path.exists() {
        eprintln!("Error: VCF file not found: {}", vcf_path.display());
        std::process::exit(1);
    }

    // Load and index the VCF file
    let index = load_vcf(&vcf_path)?;

    // Create and run the MCP server
    let server = VcfServer::new(index);

    println!("VCF MCP Server ready. Starting stdio transport...");

    // Run the server using stdio transport
    let service = server
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    service
        .waiting()
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    Ok(())
}
