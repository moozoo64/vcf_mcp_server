use noodles::bgzf;
use noodles::core::{Position, Region};
use noodles::csi::{self, BinningIndex};
use noodles::tabix;
use noodles::vcf;
use noodles::vcf::variant::record::{AlternateBases, Filters, Ids};
use std::collections::HashMap;
use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use vcf_filter::FilterEngine;

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
    #[serde(skip_serializing)]
    pub raw_row: String,
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

// VCF summary statistics structures
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VcfStatistics {
    pub file_format: String,
    pub reference_genome: String,
    pub chromosome_count: usize,
    pub sample_count: usize,
    pub chromosomes: Vec<String>,
    pub total_variants: u64,
    pub variants_per_chromosome: HashMap<String, u64>,
    pub unique_ids: u64,
    pub missing_ids: u64,
    pub quality_stats: Option<QualityStats>,
    pub filter_counts: HashMap<String, u64>,
    pub variant_types: VariantTypeStats,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityStats {
    pub min: f32,
    pub max: f32,
    pub mean: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VariantTypeStats {
    pub snps: u64,
    pub insertions: u64,
    pub deletions: u64,
    pub mnps: u64,
    pub complex: u64,
}

// VCF index structure - supports both tabix (.tbi) and CSI (.csi) indices for efficient queries
pub struct VcfIndex {
    #[allow(dead_code)]
    path: PathBuf,
    index: GenomicIndex,
    header: vcf::Header,
    reader: Mutex<vcf::io::Reader<bgzf::io::Reader<File>>>,
    id_index: HashMap<String, Vec<(String, u64)>>, // ID -> [(chromosome, position)]
    filter_engine: Arc<FilterEngine>,              // Thread-safe filter engine
    statistics: VcfStatistics,                     // Cached statistics computed at load time
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

    pub fn get_header_string(&self, search: Option<&str>) -> String {
        let mut buffer = Vec::new();
        let mut writer = vcf::io::Writer::new(&mut buffer);
        if writer.write_header(&self.header).is_ok() {
            let full_header = String::from_utf8_lossy(&buffer).to_string();

            // Apply search filter if provided, otherwise exclude ##contig lines by default
            if let Some(search_str) = search {
                full_header
                    .lines()
                    .filter(|line| line.contains(search_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                // Exclude ##contig lines by default (noodles doesn't include them anyway)
                // This prevents clutter while keeping useful metadata
                full_header
                    .lines()
                    .filter(|line| !line.starts_with("##contig"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        } else {
            "Error formatting header".to_string()
        }
    }

    // Get reference to the filter engine for evaluating filters
    pub fn filter_engine(&self) -> Arc<FilterEngine> {
        Arc::clone(&self.filter_engine)
    }

    // Compute comprehensive statistics about the VCF file
    pub fn compute_statistics(&self) -> std::io::Result<VcfStatistics> {
        // Return cached statistics (computed at load time)
        Ok(self.statistics.clone())
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
    // Serialize record to VCF row string for filtering
    let mut raw_row = Vec::new();
    {
        let mut writer = vcf::io::Writer::new(&mut raw_row);
        writer
            .write_record(header, record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    }
    let raw_row_string = String::from_utf8(raw_row)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
        .trim_end()
        .to_string();

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
        raw_row: raw_row_string,
    })
}

// Helper function to save ID index to disk
// Helper function to atomically save statistics to disk
fn save_statistics_to_disk(
    statistics: &VcfStatistics,
    stats_path: &PathBuf,
    debug: bool,
) -> std::io::Result<()> {
    use std::fs;
    use std::io::Write;

    // Create temporary file with .tmp extension
    let tmp_path = PathBuf::from(format!("{}.tmp", stats_path.display()));

    if debug {
        eprintln!(
            "Writing statistics to temporary file: {}",
            tmp_path.display()
        );
    }

    // Serialize and write to temp file
    {
        let encoded = bincode::serialize(statistics)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut tmp_file = fs::File::create(&tmp_path)?;
        tmp_file.write_all(&encoded)?;
        tmp_file.flush()?;
        tmp_file.sync_all()?; // Force OS to write to disk
    }

    // Check if .stats file was created by another process (race condition)
    if stats_path.exists() {
        if debug {
            eprintln!("Statistics file appeared during write, removing temporary file");
        }
        fs::remove_file(&tmp_path)?;
        return Ok(());
    }

    // Atomically rename temp file to final .stats file
    fs::rename(&tmp_path, stats_path)?;

    Ok(())
}

// Helper function to load statistics from disk
fn load_statistics_from_disk(stats_path: &PathBuf, debug: bool) -> std::io::Result<VcfStatistics> {
    use std::fs;
    use std::io::Read;

    if debug {
        eprintln!("Loading statistics from: {}", stats_path.display());
    }

    let mut file = fs::File::open(stats_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;

    let statistics: VcfStatistics = bincode::deserialize(&buffer)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    Ok(statistics)
}

// Helper function to compute statistics by scanning all variants
fn compute_statistics_from_vcf(
    path: &PathBuf,
    header: &vcf::Header,
    id_index: &HashMap<String, Vec<(String, u64)>>,
    debug: bool,
) -> std::io::Result<VcfStatistics> {
    if debug {
        eprintln!("Computing VCF statistics...");
    }

    // Extract metadata using existing helper function
    let metadata = extract_metadata(header);

    // Get chromosomes from header contigs (fallback to index if empty)
    let mut chromosomes: Vec<String> = header.contigs().keys().map(|k| k.to_string()).collect();

    if chromosomes.is_empty() {
        // Fall back to variants_per_chromosome keys collected during scan
        // We'll populate this after the scan
    }

    // Unique IDs from existing id_index (no scan needed)
    let unique_ids = id_index.len() as u64;

    // Counters for single-pass scan
    let mut total_variants = 0u64;
    let mut variants_per_chromosome: HashMap<String, u64> = HashMap::new();
    let mut missing_ids = 0u64;
    let mut filter_counts: HashMap<String, u64> = HashMap::new();

    // Quality statistics (running calculations)
    let mut qual_min = f32::INFINITY;
    let mut qual_max = f32::NEG_INFINITY;
    let mut qual_sum = 0.0;
    let mut qual_count = 0u64;

    // Variant type counters
    let mut snps = 0u64;
    let mut insertions = 0u64;
    let mut deletions = 0u64;
    let mut mnps = 0u64;
    let mut complex = 0u64;

    // Single-pass scan through all variants
    let file = File::open(path)?;
    let mut reader = vcf::io::Reader::new(bgzf::io::Reader::new(file));
    let _ = reader.read_header()?; // Skip header

    for record in reader.records().flatten() {
        if let Ok(variant) = parse_variant_record(&record, header) {
            total_variants += 1;

            // Count per chromosome
            *variants_per_chromosome
                .entry(variant.chromosome.clone())
                .or_insert(0) += 1;

            // Count missing IDs
            if variant.id == "." {
                missing_ids += 1;
            }

            // Track quality stats
            if let Some(qual) = variant.quality {
                qual_min = qual_min.min(qual);
                qual_max = qual_max.max(qual);
                qual_sum += qual as f64;
                qual_count += 1;
            }

            // Count filter categories
            for filter in &variant.filter {
                *filter_counts.entry(filter.clone()).or_insert(0) += 1;
            }

            // Classify variant type
            let ref_len = variant.reference.len();
            if variant.alternate.len() == 1 {
                let alt_len = variant.alternate[0].len();
                if ref_len == 1 && alt_len == 1 {
                    snps += 1;
                } else if ref_len < alt_len {
                    insertions += 1;
                } else if ref_len > alt_len {
                    deletions += 1;
                } else if ref_len == alt_len && ref_len > 1 {
                    mnps += 1;
                } else {
                    complex += 1;
                }
            } else {
                // Multiple alternates or complex
                complex += 1;
            }
        }
    }

    // Compute quality statistics
    let quality_stats = if qual_count > 0 {
        Some(QualityStats {
            min: qual_min,
            max: qual_max,
            mean: (qual_sum / qual_count as f64) as f32,
        })
    } else {
        None
    };

    // If header had no contigs, use chromosomes from actual variants
    if chromosomes.is_empty() {
        chromosomes = variants_per_chromosome.keys().cloned().collect();
        chromosomes.sort(); // Sort for consistent ordering
    }

    // Get reference genome using existing helper
    let reference_genome_info = extract_reference_genome(header);
    let reference_genome = format!(
        "{} ({})",
        reference_genome_info.build,
        match reference_genome_info.source {
            ReferenceGenomeSource::HeaderLine => "from header",
            ReferenceGenomeSource::InferredFromContigLengths => "inferred from contigs",
            ReferenceGenomeSource::Unknown => "unknown source",
        }
    );

    if debug {
        eprintln!(
            "Statistics computed: {} total variants, {} chromosomes",
            total_variants,
            chromosomes.len()
        );
    } else {
        eprintln!("Statistics computed ({} total variants)", total_variants);
    }

    Ok(VcfStatistics {
        file_format: metadata.file_format,
        reference_genome,
        chromosome_count: chromosomes.len(),
        sample_count: metadata.samples.len(),
        chromosomes,
        total_variants,
        variants_per_chromosome,
        unique_ids,
        missing_ids,
        quality_stats,
        filter_counts,
        variant_types: VariantTypeStats {
            snps,
            insertions,
            deletions,
            mnps,
            complex,
        },
    })
}

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
        tmp_file.flush()?;
        tmp_file.sync_all()?; // Force OS to write to disk
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
    // Check for existing indices: TBI first (for compatibility), then CSI
    let csi_path = PathBuf::from(format!("{}.csi", path.display()));
    let tbi_path = PathBuf::from(format!("{}.tbi", path.display()));

    let genomic_index = if tbi_path.exists() {
        // Use existing tabix index (prefer TBI if it exists for compatibility)
        if debug {
            eprintln!("Found tabix index: {}", tbi_path.display());
        }
        eprintln!("Loading VCF file with existing tabix index...");
        GenomicIndex::Tabix(tabix::fs::read(&tbi_path)?)
    } else if csi_path.exists() {
        // Use existing CSI index
        if debug {
            eprintln!("Found CSI index: {}", csi_path.display());
        }
        eprintln!("Loading VCF file with existing CSI index...");
        GenomicIndex::Csi(csi::fs::read(&csi_path)?)
    } else {
        // Build tabix index on the fly (fallback - CSI requires external bcftools)
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

    // Initialize filter engine with VCF header
    let header_string = {
        let mut buffer = Vec::new();
        let mut writer = vcf::io::Writer::new(&mut buffer);
        if writer.write_header(&header).is_ok() {
            String::from_utf8_lossy(&buffer).to_string()
        } else {
            String::new() // Empty header if write fails
        }
    };

    let filter_engine = Arc::new(FilterEngine::new(&header_string).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to create filter engine: {}", e),
        )
    })?);

    // Load or compute statistics
    let stats_path = PathBuf::from(format!("{}.stats", path.display()));

    let statistics = if stats_path.exists() {
        // Load existing statistics
        if debug {
            eprintln!("Found statistics file: {}", stats_path.display());
        }
        eprintln!("Loading VCF statistics from cache...");
        match load_statistics_from_disk(&stats_path, debug) {
            Ok(stats) => {
                eprintln!(
                    "Statistics loaded ({} total variants)",
                    stats.total_variants
                );
                stats
            }
            Err(e) => {
                eprintln!("Warning: Failed to load statistics: {}", e);
                eprintln!("Recomputing statistics...");
                let stats = compute_statistics_from_vcf(path, &header, &id_index, debug)?;

                // Try to save the recomputed statistics
                if save_index {
                    match save_statistics_to_disk(&stats, &stats_path, debug) {
                        Ok(()) => eprintln!("Statistics saved to {}", stats_path.display()),
                        Err(e) => {
                            eprintln!("Warning: Failed to save statistics: {}", e);
                            eprintln!("Continuing with in-memory statistics...");
                        }
                    }
                }

                stats
            }
        }
    } else {
        // Compute statistics from scratch
        let stats = compute_statistics_from_vcf(path, &header, &id_index, debug)?;

        // Try to save statistics to disk if requested
        if save_index {
            match save_statistics_to_disk(&stats, &stats_path, debug) {
                Ok(()) => eprintln!("Statistics saved to {}", stats_path.display()),
                Err(e) => {
                    eprintln!("Warning: Failed to save statistics to disk: {}", e);
                    eprintln!("Continuing with in-memory statistics...");
                }
            }
        } else if debug {
            eprintln!("Skipping statistics save (--never-save-index flag set)");
        }

        stats
    };

    Ok(VcfIndex {
        path: path.clone(),
        index: genomic_index,
        header,
        reader: Mutex::new(reader),
        id_index,
        filter_engine,
        statistics,
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
// Save CSI index to disk with atomic write (currently unused but provided for future functionality)
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

//
// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     fn create_test_variant(
//         chromosome: &str,
//         position: u64,
//         id: &str,
//         reference: &str,
//         alternate: Vec<&str>,
//     ) -> Variant {
//         let mut info = HashMap::new();
//         info.insert("NS".to_string(), serde_json::Value::Number(3.into()));
//         info.insert("DP".to_string(), serde_json::Value::Number(14.into()));
//         info.insert(
//             "AF".to_string(),
//             serde_json::Number::from_f64(0.5)
//                 .map(serde_json::Value::Number)
//                 .unwrap(),
//         );
//
//         Variant {
//             chromosome: chromosome.to_string(),
//             position,
//             id: id.to_string(),
//             reference: reference.to_string(),
//             alternate: alternate.iter().map(|s| s.to_string()).collect(),
//             quality: Some(29.0),
//             filter: vec!["PASS".to_string()],
//             info,
//         }
//     }
//
//     fn test_format_variant_basic() {
//         let variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
//         let dto = format_variant(variant.clone());
//
//         assert_eq!(dto.chromosome, "20");
//         assert_eq!(dto.position, 14370);
//         assert_eq!(dto.id, "rs6054257");
//         assert_eq!(dto.reference, "G");
//         assert_eq!(dto.alternate, vec!["A"]);
//         assert_eq!(dto.quality, Some(29.0));
//         assert_eq!(dto.filter, vec!["PASS"]);
//         assert_eq!(dto.info.get("NS"), Some(&serde_json::json!(3)));
//         assert_eq!(dto.info.get("DP"), Some(&serde_json::json!(14)));
//         assert_eq!(dto.info.get("AF"), Some(&serde_json::json!(0.5)));
//     }
//     #[test]
//
//     fn test_format_variant_multiple_alternates() {
//         let variant = create_test_variant("20", 1110696, "rs6040355", "A", vec!["G", "T"]);
//         let dto = format_variant(variant.clone());
//
//         assert_eq!(dto.alternate, vec!["G", "T"]);
//     }
//
//     fn test_format_variant_no_quality() {
//     #[test]
//         let mut variant = create_test_variant("20", 14370, "rs6054257", "G", vec!["A"]);
//         variant.quality = None;
//         let dto = format_variant(variant);
//
//         assert!(dto.quality.is_none());
//     }
//
//
//
//
//
//
//
//
//
//
//
