use noodles::bgzf;
use noodles::core::{Position, Region};
use noodles::csi::{self, BinningIndex};
use noodles::tabix;
use noodles::vcf;
use noodles::vcf::variant::record::{AlternateBases, Filters, Ids};
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::Mutex;

// Genomic index enum - supports both tabix (.tbi) and CSI (.csi) indices
#[derive(Debug)]
pub enum GenomicIndex {
    Tabix(tabix::Index),
    Csi(csi::Index),
}

impl GenomicIndex {
    // Get reference to index header (works for both types via BinningIndex trait)
    fn header(&self) -> Option<&csi::binning_index::index::Header> {
        match self {
            Self::Tabix(idx) => idx.header(),
            Self::Csi(idx) => idx.header(),
        }
    }
}

// Variant structure - used both internally and exposed via MCP responses
#[derive(Debug, Clone, serde::Serialize)]
pub struct Variant {
    pub chromosome: String,
    pub position: u64,
    pub id: String,
    pub reference: String,
    pub alternate: Vec<String>,
    pub quality: Option<f32>,
    pub filter: Vec<String>,
    pub info: HashMap<String, serde_json::Value>,
}

// VCF metadata structure extracted from header
#[derive(Debug, Clone, serde::Serialize)]
pub struct VcfMetadata {
    pub file_format: String,
    pub reference_genome: ReferenceGenomeInfo,
    pub contigs: Vec<ContigInfo>,
    pub samples: Vec<String>,
}

// Information about the reference genome build
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReferenceGenomeInfo {
    pub build: String,
    pub source: ReferenceGenomeSource,
}

// Source of reference genome information
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceGenomeSource {
    HeaderLine,
    InferredFromContigLengths,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContigInfo {
    pub id: String,
}

// VCF index structure - supports both tabix (.tbi) and CSI (.csi) indices for efficient queries
pub struct VcfIndex {
    index: GenomicIndex,
    header: vcf::Header,
    reader: Mutex<vcf::io::Reader<bgzf::io::Reader<File>>>,
    id_index: HashMap<String, Vec<(String, u64)>>, // ID -> [(chromosome, position)]
}

impl VcfIndex {
    // Helper to get alternate chromosome name
    fn get_chromosome_variants(chromosome: &str) -> Vec<String> {
        let mut variants = vec![chromosome.to_string()];
        if let Some(stripped) = chromosome.strip_prefix("chr") {
            variants.push(stripped.to_string());
        } else {
            variants.push(format!("chr{}", chromosome));
        }
        variants
    }

    // Get list of chromosomes present in the VCF file
    pub fn get_available_chromosomes(&self) -> Vec<String> {
        // Try to get chromosomes from VCF header contigs first
        let from_header: Vec<String> = self
            .header
            .contigs()
            .keys()
            .map(|k| k.to_string())
            .collect();

        if !from_header.is_empty() {
            return from_header;
        }

        // Fall back to index if header has no contigs
        if let Some(header) = self.index.header() {
            header
                .reference_sequence_names()
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        }
    }

    // Check if a chromosome (or its variant) exists in the header
    fn find_matching_chromosome(&self, chromosome: &str) -> Option<String> {
        let variants = Self::get_chromosome_variants(chromosome);
        let available = self.get_available_chromosomes();

        variants
            .into_iter()
            .find(|variant| available.contains(variant))
    }

    pub fn query_by_position(
        &self,
        chromosome: &str,
        position: u64,
    ) -> (Vec<Variant>, Option<String>) {
        // Try to find the matching chromosome format
        if let Some(matching_chr) = self.find_matching_chromosome(chromosome) {
            let mut reader = self.reader.lock().unwrap();
            let results = match &self.index {
                GenomicIndex::Tabix(idx) => query_indexed_region(
                    &mut reader,
                    idx,
                    &self.header,
                    &matching_chr,
                    position,
                    position,
                ),
                GenomicIndex::Csi(idx) => query_indexed_region(
                    &mut reader,
                    idx,
                    &self.header,
                    &matching_chr,
                    position,
                    position,
                ),
            };
            return (results, Some(matching_chr));
        }
        (Vec::new(), None)
    }

    pub fn query_by_region(
        &self,
        chromosome: &str,
        start: u64,
        end: u64,
    ) -> (Vec<Variant>, Option<String>) {
        // Try to find the matching chromosome format
        if let Some(matching_chr) = self.find_matching_chromosome(chromosome) {
            let mut reader = self.reader.lock().unwrap();
            let results = match &self.index {
                GenomicIndex::Tabix(idx) => {
                    query_indexed_region(&mut reader, idx, &self.header, &matching_chr, start, end)
                }
                GenomicIndex::Csi(idx) => {
                    query_indexed_region(&mut reader, idx, &self.header, &matching_chr, start, end)
                }
            };
            return (results, Some(matching_chr));
        }
        (Vec::new(), None)
    }

    pub fn query_by_id(&self, id: &str) -> Vec<Variant> {
        // Use the ID index for O(1) lookup
        if let Some(locations) = self.id_index.get(id) {
            let mut results = Vec::new();
            let mut reader = self.reader.lock().unwrap();

            for (chromosome, position) in locations {
                let variants = match &self.index {
                    GenomicIndex::Tabix(idx) => query_indexed_region(
                        &mut reader,
                        idx,
                        &self.header,
                        chromosome,
                        *position,
                        *position,
                    ),
                    GenomicIndex::Csi(idx) => query_indexed_region(
                        &mut reader,
                        idx,
                        &self.header,
                        chromosome,
                        *position,
                        *position,
                    ),
                };
                results.extend(variants);
            }

            results
        } else {
            Vec::new()
        }
    }

    pub fn get_metadata(&self) -> VcfMetadata {
        extract_metadata(&self.header)
    }

    pub fn get_reference_genome(&self) -> String {
        let metadata = self.get_metadata();
        format!(
            "{} ({})",
            metadata.reference_genome.build,
            match metadata.reference_genome.source {
                ReferenceGenomeSource::HeaderLine => "from header",
                ReferenceGenomeSource::InferredFromContigLengths => "inferred from contigs",
                ReferenceGenomeSource::Unknown => "unknown source",
            }
        )
    }

    pub fn get_header_string(&self) -> String {
        let mut buffer = Vec::new();
        let mut writer = vcf::io::Writer::new(&mut buffer);
        if writer.write_header(&self.header).is_ok() {
            String::from_utf8_lossy(&buffer).to_string()
        } else {
            "Error formatting header".to_string()
        }
    }
}

// Helper function to query indexed VCF by region (generic over BinningIndex trait)
fn query_indexed_region<I: BinningIndex>(
    reader: &mut vcf::io::Reader<bgzf::io::Reader<File>>,
    index: &I,
    header: &vcf::Header,
    chromosome: &str,
    start: u64,
    end: u64,
) -> Vec<Variant> {
    let mut results = Vec::new();

    // Create region with Position types
    let start_pos = match Position::try_from(start as usize) {
        Ok(p) => p,
        Err(_) => return results,
    };
    let end_pos = match Position::try_from(end as usize) {
        Ok(p) => p,
        Err(_) => return results,
    };
    let region = Region::new(chromosome, start_pos..=end_pos);

    let query_result = match reader.query(header, index, &region) {
        Ok(q) => q,
        Err(_) => return results,
    };

    for record in query_result.records().flatten() {
        if let Ok(variant) = parse_variant_record(&record, header) {
            results.push(variant);
        }
    }

    results
}

// Helper function to infer genome build from contig lengths
// GRCh37/hg19: chr1 = 249,250,621 bp
// GRCh38/hg38: chr1 = 248,956,422 bp
fn infer_genome_build_from_contigs(header: &vcf::Header) -> Option<String> {
    const CHR1_GRCH37_LENGTH: usize = 249_250_621;
    const CHR1_GRCH38_LENGTH: usize = 248_956_422;
    const TOLERANCE: usize = 1000; // Allow small differences

    // Try both "chr1" and "1" naming conventions
    for chr_name in ["chr1", "1"] {
        if let Some(contig) = header.contigs().get(chr_name) {
            if let Some(length) = contig.length() {
                let diff_grch37 =
                    (length as i64 - CHR1_GRCH37_LENGTH as i64).unsigned_abs() as usize;
                let diff_grch38 =
                    (length as i64 - CHR1_GRCH38_LENGTH as i64).unsigned_abs() as usize;

                if diff_grch37 < TOLERANCE {
                    return Some("GRCh37".to_string());
                } else if diff_grch38 < TOLERANCE {
                    return Some("GRCh38".to_string());
                }
            }
        }
    }

    None
}

// Helper function to extract reference genome from VCF header
fn extract_reference_genome(header: &vcf::Header) -> ReferenceGenomeInfo {
    use vcf::header::record::value::Collection;

    // Try to get ##reference line from header
    // Collection can be Unstructured (Vec<String>) or Structured (IndexMap)
    // ##reference is typically unstructured with a single string value
    if let Some(Collection::Unstructured(values)) = header.get("reference") {
        if let Some(reference_value) = values.first() {
            return ReferenceGenomeInfo {
                build: reference_value.clone(),
                source: ReferenceGenomeSource::HeaderLine,
            };
        }
    }

    // Fall back to inferring from contig lengths
    if let Some(inferred_build) = infer_genome_build_from_contigs(header) {
        return ReferenceGenomeInfo {
            build: inferred_build,
            source: ReferenceGenomeSource::InferredFromContigLengths,
        };
    }

    // Unknown
    ReferenceGenomeInfo {
        build: "Unknown".to_string(),
        source: ReferenceGenomeSource::Unknown,
    }
}

// Helper function to extract metadata from VCF header
fn extract_metadata(header: &vcf::Header) -> VcfMetadata {
    // Extract file format version
    let file_format = format!("{:?}", header.file_format());

    // Extract reference genome information
    let reference_genome = extract_reference_genome(header);

    // Extract contig information
    let contigs: Vec<ContigInfo> = header
        .contigs()
        .keys()
        .map(|id| ContigInfo { id: id.to_string() })
        .collect();

    // Extract sample names
    let samples: Vec<String> = header
        .sample_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

    VcfMetadata {
        file_format,
        reference_genome,
        contigs,
        samples,
    }
}

// Helper function to convert debug-formatted info values to JSON
// Converts: Integer(123) -> 123, Float(1.23) -> 1.23, String("foo") -> "foo", etc.
fn convert_info_value(debug_str: &str) -> serde_json::Value {
    let s = debug_str;

    // Handle common patterns from noodles VCF library:
    // Integer(123) -> JSON number
    // Float(1.23) -> JSON number
    // String("foo") -> JSON string
    // Array([Ok(Some(1)), Ok(Some(2))]) -> JSON array
    // Flag -> JSON true

    if s == "Flag" {
        return serde_json::Value::Bool(true);
    }

    // Match Integer(value)
    if let Some(inner) = s.strip_prefix("Integer(").and_then(|s| s.strip_suffix(')')) {
        if let Ok(num) = inner.parse::<i64>() {
            return serde_json::Value::Number(num.into());
        }
    }

    // Match Float(value)
    if let Some(inner) = s.strip_prefix("Float(").and_then(|s| s.strip_suffix(')')) {
        if let Ok(num) = inner.parse::<f64>() {
            if let Some(json_num) = serde_json::Number::from_f64(num) {
                return serde_json::Value::Number(json_num);
            }
        }
    }

    // Match Character(value)
    if let Some(inner) = s
        .strip_prefix("Character(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return serde_json::Value::String(inner.trim_matches('\'').to_string());
    }

    // Match String("value")
    if let Some(inner) = s
        .strip_prefix("String(\"")
        .and_then(|s| s.strip_suffix("\")"))
    {
        return serde_json::Value::String(inner.to_string());
    }

    // Match Array([...])
    if let Some(inner) = s.strip_prefix("Array([").and_then(|s| s.strip_suffix("])")) {
        // Extract Ok(Some(value)) patterns
        let values: Vec<serde_json::Value> = inner
            .split("), ")
            .filter_map(|part| {
                let part = part.trim_end_matches(')');
                if let Some(val_str) = part.strip_prefix("Ok(Some(") {
                    let val_str = val_str.trim_matches('"');
                    // Try to parse as number first, otherwise string
                    if let Ok(num) = val_str.parse::<i64>() {
                        return Some(serde_json::Value::Number(num.into()));
                    }
                    if let Ok(num) = val_str.parse::<f64>() {
                        if let Some(json_num) = serde_json::Number::from_f64(num) {
                            return Some(serde_json::Value::Number(json_num));
                        }
                    }
                    return Some(serde_json::Value::String(val_str.to_string()));
                }
                None
            })
            .collect();
        return serde_json::Value::Array(values);
    }

    // Fall back to string if no pattern matched
    serde_json::Value::String(s.to_string())
}

// Helper function to parse a VCF record into a Variant
fn parse_variant_record(record: &vcf::Record, header: &vcf::Header) -> std::io::Result<Variant> {
    Ok(Variant {
        chromosome: record.reference_sequence_name().to_string(),
        position: usize::from(
            record
                .variant_start()
                .transpose()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, "Missing position")
                })?,
        ) as u64,
        id: record.ids().iter().next().unwrap_or(".").to_string(),
        reference: record.reference_bases().to_string(),
        alternate: record
            .alternate_bases()
            .iter()
            .map(|alt| {
                alt.map(|a| a.to_string())
                    .unwrap_or_else(|_| ".".to_string())
            })
            .collect(),
        quality: record
            .quality_score()
            .transpose()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        filter: record
            .filters()
            .iter(header)
            .filter_map(|f| f.ok())
            .map(|filter| filter.to_string())
            .collect(),
        info: record
            .info()
            .iter(header)
            .map(|item| {
                item.map(|(key, value)| {
                    if let Some(val) = value {
                        let debug_str = format!("{:?}", val);
                        let json_value = convert_info_value(&debug_str);
                        (key.to_string(), json_value)
                    } else {
                        // Flag with no value - just the key is present
                        (key.to_string(), serde_json::Value::Bool(true))
                    }
                })
            })
            .filter_map(|item| item.ok())
            .collect(),
    })
}

// Helper function to save ID index to disk
fn save_id_index_to_disk(
    id_index: &HashMap<String, Vec<(String, u64)>>,
    idx_path: &PathBuf,
    debug: bool,
) -> std::io::Result<()> {
    use std::fs;
    use std::io::Write;

    // Create temporary file with .tmp extension
    let tmp_path = PathBuf::from(format!("{}.tmp", idx_path.display()));

    if debug {
        eprintln!("Writing ID index to temporary file: {}", tmp_path.display());
    }

    // Serialize and write to temp file
    {
        let encoded = bincode::serialize(id_index)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut tmp_file = fs::File::create(&tmp_path)?;
        tmp_file.write_all(&encoded)?;
    }

    // Check if .idx file was created by another process (race condition)
    if idx_path.exists() {
        if debug {
            eprintln!("ID index file appeared during write, removing temporary file");
        }
        fs::remove_file(&tmp_path)?;
        return Ok(());
    }

    // Atomically rename temp file to final .idx file
    fs::rename(&tmp_path, idx_path)?;

    Ok(())
}

// Helper function to load ID index from disk
fn load_id_index_from_disk(
    idx_path: &PathBuf,
    debug: bool,
) -> std::io::Result<HashMap<String, Vec<(String, u64)>>> {
    use std::fs;
    use std::io::Read;

    if debug {
        eprintln!("Loading ID index from: {}", idx_path.display());
    }

    let mut file = fs::File::open(idx_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let id_index: HashMap<String, Vec<(String, u64)>> = bincode::deserialize(&buffer)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(id_index)
}

// Helper function to build ID index by scanning all variants
fn build_id_index(
    path: &PathBuf,
    header: &vcf::Header,
    debug: bool,
) -> std::io::Result<HashMap<String, Vec<(String, u64)>>> {
    let mut id_index: HashMap<String, Vec<(String, u64)>> = HashMap::new();

    if debug {
        eprintln!("Building ID index...");
    }

    let file = File::open(path)?;
    let mut reader = vcf::io::Reader::new(bgzf::io::Reader::new(file));
    let _ = reader.read_header()?; // Skip header

    let mut count = 0;
    for record in reader.records().flatten() {
        if let Ok(variant) = parse_variant_record(&record, header) {
            // Skip "." (missing ID)
            if variant.id != "." {
                id_index
                    .entry(variant.id.clone())
                    .or_default()
                    .push((variant.chromosome.clone(), variant.position));
            }
            count += 1;
        }
    }

    if debug {
        eprintln!(
            "ID index built: {} variants scanned, {} unique IDs indexed",
            count,
            id_index.len()
        );
    } else {
        eprintln!("ID index built ({} unique IDs)", id_index.len());
    }

    Ok(id_index)
}

// Load and index VCF file
pub fn load_vcf(path: &PathBuf, debug: bool, save_index: bool) -> std::io::Result<VcfIndex> {
    // Check for CSI index first (supports larger chromosomes), then TBI
    let csi_path = PathBuf::from(format!("{}.csi", path.display()));
    let tbi_path = PathBuf::from(format!("{}.tbi", path.display()));

    let genomic_index = if csi_path.exists() {
        // Use existing CSI index
        if debug {
            eprintln!("Found CSI index: {}", csi_path.display());
        }
        eprintln!("Loading VCF file with existing CSI index...");
        GenomicIndex::Csi(csi::fs::read(&csi_path)?)
    } else if tbi_path.exists() {
        // Use existing tabix index
        if debug {
            eprintln!("Found tabix index: {}", tbi_path.display());
        }
        eprintln!("Loading VCF file with existing tabix index...");
        GenomicIndex::Tabix(tabix::fs::read(&tbi_path)?)
    } else {
        // Build tabix index on the fly (default for VCF)
        eprintln!("No index found. Building tabix index...");
        let index = vcf::fs::index(path)?;
        eprintln!("Tabix index built successfully");

        // Try to save index to disk if requested
        if save_index {
            match save_tabix_index_to_disk(&index, &tbi_path, debug) {
                Ok(()) => eprintln!("Tabix index saved to {}", tbi_path.display()),
                Err(e) => {
                    eprintln!("Warning: Failed to save tabix index to disk: {}", e);
                    eprintln!("Continuing with in-memory index...");
                }
            }
        } else if debug {
            eprintln!("Skipping index save (--never-save-index flag set)");
        }

        GenomicIndex::Tabix(index)
    };

    // Create reader for queries
    let file = File::open(path)?;
    let mut reader = vcf::io::Reader::new(bgzf::io::Reader::new(file));
    let header = reader.read_header()?;

    // Check if ID index file exists
    let idx_path = PathBuf::from(format!("{}.idx", path.display()));

    let id_index = if idx_path.exists() {
        // Load existing ID index
        if debug {
            eprintln!("Found ID index: {}", idx_path.display());
        }
        eprintln!("Loading VCF file with existing ID index...");
        match load_id_index_from_disk(&idx_path, debug) {
            Ok(index) => {
                eprintln!("ID index loaded ({} unique IDs)", index.len());
                index
            }
            Err(e) => {
                eprintln!("Warning: Failed to load ID index: {}", e);
                eprintln!("Rebuilding ID index...");
                let index = build_id_index(path, &header, debug)?;

                // Try to save the rebuilt index
                if save_index {
                    match save_id_index_to_disk(&index, &idx_path, debug) {
                        Ok(()) => eprintln!("ID index saved to {}", idx_path.display()),
                        Err(e) => {
                            eprintln!("Warning: Failed to save ID index: {}", e);
                            eprintln!("Continuing with in-memory index...");
                        }
                    }
                }

                index
            }
        }
    } else {
        // Build ID index from scratch
        let index = build_id_index(path, &header, debug)?;

        // Try to save index to disk if requested
        if save_index {
            match save_id_index_to_disk(&index, &idx_path, debug) {
                Ok(()) => eprintln!("ID index saved to {}", idx_path.display()),
                Err(e) => {
                    eprintln!("Warning: Failed to save ID index to disk: {}", e);
                    eprintln!("Continuing with in-memory index...");
                }
            }
        } else if debug {
            eprintln!("Skipping ID index save (--never-save-index flag set)");
        }

        index
    };

    eprintln!("VCF loaded (indexed mode)");

    Ok(VcfIndex {
        index: genomic_index,
        header,
        reader: Mutex::new(reader),
        id_index,
    })
}

// Helper function to atomically save tabix index to disk
fn save_tabix_index_to_disk(
    index: &tabix::Index,
    tbi_path: &PathBuf,
    debug: bool,
) -> std::io::Result<()> {
    use std::fs;
    use std::io::BufWriter;

    // Create temporary file with .tmp extension
    let tmp_path = PathBuf::from(format!("{}.tmp", tbi_path.display()));

    if debug {
        eprintln!(
            "Writing tabix index to temporary file: {}",
            tmp_path.display()
        );
    }

    // Write index to temporary file
    {
        let tmp_file = fs::File::create(&tmp_path)?;
        let mut writer = tabix::io::Writer::new(BufWriter::new(tmp_file));
        writer.write_index(index)?;
    }

    // Check again if .tbi file was created by another process (race condition)
    if tbi_path.exists() {
        if debug {
            eprintln!("Index file appeared during write, removing temporary file");
        }
        fs::remove_file(&tmp_path)?;
        return Ok(());
    }

    // Atomically rename temp file to final .tbi file
    fs::rename(&tmp_path, tbi_path)?;

    Ok(())
}

// Helper function to atomically save CSI index to disk
// Currently unused but provided for future functionality (e.g., building CSI indices on-the-fly)
#[allow(dead_code)]
fn save_csi_index_to_disk(
    index: &csi::Index,
    csi_path: &PathBuf,
    debug: bool,
) -> std::io::Result<()> {
    use std::fs;
    use std::io::BufWriter;

    // Create temporary file with .tmp extension
    let tmp_path = PathBuf::from(format!("{}.tmp", csi_path.display()));

    if debug {
        eprintln!(
            "Writing CSI index to temporary file: {}",
            tmp_path.display()
        );
    }

    // Write index to temporary file
    {
        let tmp_file = fs::File::create(&tmp_path)?;
        let mut writer = csi::io::Writer::new(BufWriter::new(tmp_file));
        writer.write_index(index)?;
    }

    // Check again if .csi file was created by another process (race condition)
    if csi_path.exists() {
        if debug {
            eprintln!("Index file appeared during write, removing temporary file");
        }
        fs::remove_file(&tmp_path)?;
        return Ok(());
    }

    // Atomically rename temp file to final .csi file
    fs::rename(&tmp_path, csi_path)?;

    Ok(())
}

// Format variant for MCP response (no-op now that types are unified)
pub fn format_variant(variant: Variant) -> Variant {
    variant
}

// Filter expression errors
#[derive(Debug, Clone)]
pub enum FilterError {
    UnsupportedField(String),
    UnsupportedOperator(String),
    #[allow(dead_code)]
    InvalidSyntax(String),
}

impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterError::UnsupportedField(field) => {
                write!(
                    f,
                    "Unsupported field '{}'. Supported fields: CHROM, POS, ID, REF, ALT, QUAL, FILTER",
                    field
                )
            }
            FilterError::UnsupportedOperator(op) => {
                write!(
                    f,
                    "Unsupported operator '{}'. Supported operators: ==, !=, <, >, <=, >=, contains, in",
                    op
                )
            }
            FilterError::InvalidSyntax(msg) => write!(f, "Invalid filter syntax: {}", msg),
        }
    }
}

impl std::error::Error for FilterError {}

// Filter expression evaluation
// Supported operators: ==, !=, <, >, <=, >=, contains, in
// Supported fields: CHROM, POS, ID, REF, ALT, QUAL, FILTER
// Examples:
//   - "QUAL > 30"
//   - "FILTER == PASS"
//   - "CHROM == chr1"
//   - "POS >= 1000 AND POS <= 2000"
//   - "ALT contains A"
//   - "FILTER in PASS,LowQual"
pub fn evaluate_filter(variant: &Variant, filter_expr: &str) -> Result<bool, FilterError> {
    if filter_expr.trim().is_empty() {
        return Ok(true); // Empty filter passes all variants
    }

    // Handle AND/OR logic (simple left-to-right evaluation)
    if let Some(and_pos) = filter_expr.to_uppercase().find(" AND ") {
        let left = &filter_expr[..and_pos];
        let right = &filter_expr[and_pos + 5..];
        return Ok(evaluate_filter(variant, left)? && evaluate_filter(variant, right)?);
    }

    if let Some(or_pos) = filter_expr.to_uppercase().find(" OR ") {
        let left = &filter_expr[..or_pos];
        let right = &filter_expr[or_pos + 4..];
        return Ok(evaluate_filter(variant, left)? || evaluate_filter(variant, right)?);
    }

    // Parse single condition
    evaluate_condition(variant, filter_expr.trim())
}

fn evaluate_condition(variant: &Variant, condition: &str) -> Result<bool, FilterError> {
    // Try different operators in order of specificity
    if let Some(result) = try_operator(variant, condition, "contains")? {
        return Ok(result);
    }
    if let Some(result) = try_operator(variant, condition, "in")? {
        return Ok(result);
    }
    if let Some(result) = try_operator(variant, condition, "==")? {
        return Ok(result);
    }
    if let Some(result) = try_operator(variant, condition, "!=")? {
        return Ok(result);
    }
    if let Some(result) = try_operator(variant, condition, "<=")? {
        return Ok(result);
    }
    if let Some(result) = try_operator(variant, condition, ">=")? {
        return Ok(result);
    }
    if let Some(result) = try_operator(variant, condition, "<")? {
        return Ok(result);
    }
    if let Some(result) = try_operator(variant, condition, ">")? {
        return Ok(result);
    }

    // No valid operator found - return error
    Err(FilterError::UnsupportedOperator(condition.to_string()))
}

fn try_operator(
    variant: &Variant,
    condition: &str,
    operator: &str,
) -> Result<Option<bool>, FilterError> {
    let parts: Vec<&str> = condition.splitn(2, operator).collect();
    if parts.len() != 2 {
        return Ok(None);
    }

    let field = parts[0].trim().to_uppercase();
    let value = parts[1].trim();

    // Validate that we actually split on the operator (field and value should both be non-empty)
    if field.is_empty() || value.is_empty() {
        return Ok(None); // Not a valid use of this operator
    }

    // Validate field name
    if !is_valid_field(&field) {
        return Err(FilterError::UnsupportedField(field));
    }

    Ok(Some(match operator {
        "==" => compare_equal(variant, &field, value),
        "!=" => !compare_equal(variant, &field, value),
        ">" => compare_numeric(variant, &field, value, |a, b| a > b),
        "<" => compare_numeric(variant, &field, value, |a, b| a < b),
        ">=" => compare_numeric(variant, &field, value, |a, b| a >= b),
        "<=" => compare_numeric(variant, &field, value, |a, b| a <= b),
        "contains" => compare_contains(variant, &field, value),
        "in" => compare_in(variant, &field, value),
        _ => false,
    }))
}

fn is_valid_field(field: &str) -> bool {
    matches!(
        field,
        "CHROM" | "POS" | "ID" | "REF" | "ALT" | "QUAL" | "FILTER"
    )
}

fn get_field_value(variant: &Variant, field: &str) -> Option<String> {
    match field {
        "CHROM" => Some(variant.chromosome.clone()),
        "POS" => Some(variant.position.to_string()),
        "ID" => Some(variant.id.clone()),
        "REF" => Some(variant.reference.clone()),
        "ALT" => Some(variant.alternate.join(",")),
        "QUAL" => variant.quality.map(|q| q.to_string()),
        "FILTER" => Some(variant.filter.join(",")),
        _ => None,
    }
}

fn compare_equal(variant: &Variant, field: &str, expected: &str) -> bool {
    if let Some(actual) = get_field_value(variant, field) {
        actual.to_lowercase() == expected.to_lowercase()
    } else {
        false
    }
}

fn compare_numeric<F>(variant: &Variant, field: &str, value: &str, comparator: F) -> bool
where
    F: Fn(f64, f64) -> bool,
{
    if let Some(actual_str) = get_field_value(variant, field) {
        if let (Ok(actual), Ok(expected)) = (actual_str.parse::<f64>(), value.parse::<f64>()) {
            return comparator(actual, expected);
        }
    }
    false
}

fn compare_contains(variant: &Variant, field: &str, substring: &str) -> bool {
    if let Some(actual) = get_field_value(variant, field) {
        actual.to_lowercase().contains(&substring.to_lowercase())
    } else {
        false
    }
}

fn compare_in(variant: &Variant, field: &str, values: &str) -> bool {
    if let Some(actual) = get_field_value(variant, field) {
        let value_list: Vec<&str> = values.split(',').map(|s| s.trim()).collect();
        value_list
            .iter()
            .any(|v| v.to_lowercase() == actual.to_lowercase())
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_variant(
        chromosome: &str,
        position: u64,
        id: &str,
        reference: &str,
        alternate: Vec<&str>,
    ) -> Variant {
        let mut info = HashMap::new();
        info.insert("NS".to_string(), serde_json::Value::Number(3.into()));
        info.insert("DP".to_string(), serde_json::Value::Number(14.into()));
        info.insert(
            "AF".to_string(),
            serde_json::Number::from_f64(0.5)
                .map(serde_json::Value::Number)
                .unwrap(),
        );

        Variant {
            chromosome: chromosome.to_string(),
            position,
            id: id.to_string(),
            reference: reference.to_string(),
            alternate: alternate.iter().map(|s| s.to_string()).collect(),
            quality: Some(29.0),
            filter: vec!["PASS".to_string()],
            info,
        }
    }

    #[test]
    fn test_format_variant_basic() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        let dto = format_variant(variant.clone());

        assert_eq!(dto.chromosome, "20");
        assert_eq!(dto.position, 14370);
        assert_eq!(dto.id, "rs6054257");
        assert_eq!(dto.reference, "G");
        assert_eq!(dto.alternate, vec!["A"]);
        assert_eq!(dto.quality, Some(29.0));
        assert_eq!(dto.filter, vec!["PASS"]);
        assert_eq!(dto.info.get("NS"), Some(&serde_json::json!(3)));
        assert_eq!(dto.info.get("DP"), Some(&serde_json::json!(14)));
        assert_eq!(dto.info.get("AF"), Some(&serde_json::json!(0.5)));
    }

    #[test]
    fn test_format_variant_multiple_alternates() {
        let variant = create_test_variant("20", 1110696, "rs6040355", "A", vec!["G", "T"]);
        let dto = format_variant(variant.clone());

        assert_eq!(dto.alternate, vec!["G", "T"]);
    }

    #[test]
    fn test_format_variant_no_quality() {
        let mut variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        variant.quality = None;
        let dto = format_variant(variant);

        assert!(dto.quality.is_none());
    }

    #[test]
    fn test_filter_qual_greater_than() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        assert!(evaluate_filter(&variant, "QUAL > 20").unwrap());
        assert!(!evaluate_filter(&variant, "QUAL > 30").unwrap());
        assert!(evaluate_filter(&variant, "QUAL >= 29").unwrap());
    }

    #[test]
    fn test_filter_position_range() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        assert!(evaluate_filter(&variant, "POS >= 14000 AND POS <= 15000").unwrap());
        assert!(!evaluate_filter(&variant, "POS < 14000 OR POS > 15000").unwrap());
    }

    #[test]
    fn test_filter_chromosome_equals() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        assert!(evaluate_filter(&variant, "CHROM == 20").unwrap());
        assert!(!evaluate_filter(&variant, "CHROM == 1").unwrap());
    }

    #[test]
    fn test_filter_contains() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        assert!(evaluate_filter(&variant, "ID contains rs").unwrap());
        assert!(evaluate_filter(&variant, "ID contains 6054").unwrap());
        assert!(!evaluate_filter(&variant, "ID contains xyz").unwrap());
    }

    #[test]
    fn test_filter_in_operator() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        assert!(evaluate_filter(&variant, "FILTER in PASS,LowQual").unwrap());
        assert!(evaluate_filter(&variant, "FILTER in PASS").unwrap());
        assert!(!evaluate_filter(&variant, "FILTER in LowQual,FAIL").unwrap());
    }

    #[test]
    fn test_filter_alt_contains() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A", "T"]);
        assert!(evaluate_filter(&variant, "ALT contains A").unwrap());
        assert!(evaluate_filter(&variant, "ALT contains T").unwrap());
        assert!(!evaluate_filter(&variant, "ALT contains G").unwrap());
    }

    #[test]
    fn test_filter_ref_equals() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        assert!(evaluate_filter(&variant, "REF == G").unwrap());
        assert!(evaluate_filter(&variant, "REF != A").unwrap());
    }

    #[test]
    fn test_filter_complex_and() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        assert!(evaluate_filter(&variant, "QUAL > 20 AND FILTER == PASS").unwrap());
        assert!(!evaluate_filter(&variant, "QUAL > 30 AND FILTER == PASS").unwrap());
    }

    #[test]
    fn test_filter_complex_or() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        assert!(evaluate_filter(&variant, "QUAL > 30 OR FILTER == PASS").unwrap());
        assert!(evaluate_filter(&variant, "QUAL > 20 OR CHROM == 1").unwrap());
        assert!(!evaluate_filter(&variant, "QUAL > 30 OR CHROM == 1").unwrap());
    }

    #[test]
    fn test_filter_empty_expression() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        assert!(evaluate_filter(&variant, "").unwrap());
        assert!(evaluate_filter(&variant, "   ").unwrap());
    }

    #[test]
    fn test_filter_less_than() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        assert!(evaluate_filter(&variant, "QUAL < 30").unwrap());
        assert!(!evaluate_filter(&variant, "QUAL < 29").unwrap());
        assert!(evaluate_filter(&variant, "QUAL <= 29").unwrap());
    }

    #[test]
    fn test_filter_not_equal() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        assert!(evaluate_filter(&variant, "CHROM != 1").unwrap());
        assert!(!evaluate_filter(&variant, "CHROM != 20").unwrap());
        assert!(evaluate_filter(&variant, "REF != A").unwrap());
    }

    #[test]
    fn test_filter_unsupported_field() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        let result = evaluate_filter(&variant, "CHROMOSOME == 20");
        assert!(result.is_err());
        if let Err(FilterError::UnsupportedField(field)) = result {
            assert_eq!(field, "CHROMOSOME");
        } else {
            panic!("Expected UnsupportedField error");
        }
    }

    #[test]
    fn test_filter_unsupported_operator() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        let result = evaluate_filter(&variant, "QUAL 30");
        assert!(result.is_err());
        assert!(matches!(result, Err(FilterError::UnsupportedOperator(_))));
    }

    #[test]
    fn test_filter_invalid_field_in_and() {
        let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
        let result = evaluate_filter(&variant, "QUAL > 20 AND CHROMSOME == 20");
        assert!(result.is_err());
        if let Err(FilterError::UnsupportedField(field)) = result {
            assert_eq!(field, "CHROMSOME");
        } else {
            panic!("Expected UnsupportedField error");
        }
    }
}
